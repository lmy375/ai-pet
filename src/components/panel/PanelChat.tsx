import { useState, useRef, useEffect, useCallback, useMemo } from "react";
import { invoke, Channel } from "@tauri-apps/api/core";
import { ToolCallBlock } from "./ToolCallBlock";
import { TaskProposalCard, parseTaskProposal } from "./TaskProposalCard";
import { SlashCommandMenu } from "./SlashCommandMenu";
import {
  extractCommandPrefix,
  filterCommandsByPrefix,
  formatHelpText,
  parseSlashCommand,
  type SlashAction,
  type SlashCommand,
} from "./slashCommands";
import { parseUrls } from "../../utils/inlineMarkdown";

interface ToolCall {
  name: string;
  arguments: string;
  result?: string;
  isRunning: boolean;
}

interface ChatItem {
  type: "user" | "assistant" | "tool" | "error";
  content: string;
  toolCalls?: ToolCall[];
}

type StreamEvent =
  | { event: "chunk"; data: { text: string } }
  | { event: "toolStart"; data: { name: string; arguments: string } }
  | { event: "toolResult"; data: { name: string; result: string } }
  | { event: "done"; data: Record<string, never> }
  | { event: "error"; data: { message: string } };

interface SessionMeta {
  id: string;
  title: string;
  created_at: string;
  updated_at: string;
  /** R93: 会话内可见 chat item 总数（user / assistant / tool / error；不
   *  含 system）。`?` 守卫覆盖：旧前端读到迁移前 index.json 时为 undefined，
   *  此时 dropdown 隐藏 "(N 条)"，避免误显 "(0 条)" 让用户以为是空会话。 */
  item_count?: number;
}

/// 与后端 `commands::session::SearchHit` 对应。`match_start` 是 char 偏移
/// （而非 byte），用于在 snippet 中精准切片高亮。
interface SearchHit {
  session_id: string;
  session_title: string;
  session_updated_at: string;
  item_index: number;
  role: "user" | "assistant";
  snippet: string;
  match_start: number;
  match_len: number;
}

interface SessionIndex {
  active_id: string;
  sessions: SessionMeta[];
}

interface Session {
  id: string;
  title: string;
  created_at: string;
  updated_at: string;
  messages: any[];
  items: ChatItem[];
}

interface PanelChatProps {
  /// 让 panel chat 通过 `/tasks` 等 slash 命令请求父组件切 tab。父组件传入
  /// `setActiveTab` 即可。可选 —— 不传则 `/tasks` 走 Unknown 反馈路径。
  onRequestTab?: (tab: "设置" | "聊天" | "任务" | "记忆" | "人格") => void;
}

export function PanelChat({ onRequestTab }: PanelChatProps = {}) {
  const [items, setItems] = useState<ChatItem[]>([]);
  const [input, setInput] = useState("");
  const [isLoading, setIsLoading] = useState(false);
  const [currentResponse, setCurrentResponse] = useState("");
  const [currentToolCalls, setCurrentToolCalls] = useState<ToolCall[]>([]);
  const scrollRef = useRef<HTMLDivElement>(null);
  const messagesRef = useRef<any[]>([]);

  // Session state
  const [sessionId, setSessionId] = useState<string>("");
  const [sessionTitle, setSessionTitle] = useState("新会话");
  const [sessionList, setSessionList] = useState<SessionMeta[]>([]);
  const [showSessionList, setShowSessionList] = useState(false);
  const [loaded, setLoaded] = useState(false);

  // 跨会话搜索状态。searchMode 开启时盖掉 session 下拉；query 实时（无 debounce）
  // 调 search_sessions —— IO 廉价，~50 sessions × ~200 items 全扫 < 100ms。
  // pendingScroll 在切换会话后由 layout effect 消费，把对应 item 滚到中间并
  // 短暂高亮。
  const [searchMode, setSearchMode] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [searchResults, setSearchResults] = useState<SearchHit[]>([]);
  // R96: 搜索范围。"all" 跨全部会话；"current" 只搜当前打开会话。退出 search
  // 模式时复位到 "all"，下次进 search 默认全局直觉一致。
  const [searchScope, setSearchScope] = useState<"all" | "current">("all");
  // R101: dropdown 行内 rename 状态。renamingId 非 null = 该 id 处于 edit
  // 模式（input 替代标题）；commit/cancel 后回 null。draft 是用户编辑中的
  // title，commit 时 trim 后落库；空白等价 cancel。
  const [renamingId, setRenamingId] = useState<string | null>(null);
  const [renameDraft, setRenameDraft] = useState("");
  // R103: 长会话 scroll position > 200px 时浮 ↑ 按钮回顶。state 由 onScroll
  // 同步驱动；阈值 200 ≈ 2-3 条消息高度，少于此不显（用户接近顶部，没必要跳）。
  const [scrolledFromTop, setScrolledFromTop] = useState(false);
  // R106: 单 session 导出后的短反馈文案。显在 dropdown 顶部 3s 自清空；
  // 单状态串行（同时只显一条），多次点击末次覆盖。
  const [exportToast, setExportToast] = useState("");
  const [pendingScroll, setPendingScroll] = useState<number | null>(null);
  const [highlightedItemIdx, setHighlightedItemIdx] = useState<number | null>(null);
  // 复制按钮：刚被复制的 item idx（短暂展示"已复制"反馈），1.5s 自动清掉。
  // 用 idx 而非 boolean 让多条消息互不干扰（不会 A 复制后 B 也显示"已复制"）。
  const [copiedIdx, setCopiedIdx] = useState<number | null>(null);

  // Slash 命令菜单：当输入处于 slash 模式（首字符 `/` 且未敲到参数空格）时
  // 浮窗可见。selectedSlashIdx 由键盘上下 / Enter 控制；点击命令项也写它。
  const [selectedSlashIdx, setSelectedSlashIdx] = useState(0);

  // R129: shell-readline 风格多条历史召回。messageHistory 是 ring buffer
  // (cap 20, newest at end)；historyCursor null = 不在浏览模式，非 null =
  // 当前浏览到第几条历史。input 空 + ↑ 进入历史末（最新）；继续 ↑ 往前；
  // ↓ 往后或退出。slash 命令不入历史（panel 控制流不算 chat content）。
  const [messageHistory, setMessageHistory] = useState<string[]>([]);
  const [historyCursor, setHistoryCursor] = useState<number | null>(null);
  const slashPrefix = extractCommandPrefix(input);
  const filteredCommands: SlashCommand[] = useMemo(
    () => (slashPrefix === null ? [] : filterCommandsByPrefix(slashPrefix)),
    [slashPrefix],
  );
  const slashMenuVisible = slashPrefix !== null;
  // prefix 变化时把选中项 clamp 回 0（避免上次选第 5 条但现在只剩 2 条）
  useEffect(() => {
    setSelectedSlashIdx((idx) => {
      if (filteredCommands.length === 0) return 0;
      return Math.min(idx, filteredCommands.length - 1);
    });
  }, [filteredCommands.length]);

  // Load sessions on mount
  useEffect(() => {
    (async () => {
      try {
        const index = await invoke<SessionIndex>("list_sessions");
        setSessionList(index.sessions);

        if (index.active_id && index.sessions.some((s) => s.id === index.active_id)) {
          await loadSession(index.active_id);
        } else if (index.sessions.length > 0) {
          // Load the most recent session
          const last = index.sessions[index.sessions.length - 1];
          await loadSession(last.id);
        } else {
          // No sessions, create one
          await handleNewSession();
        }
      } catch (e) {
        console.error("Failed to load sessions:", e);
        await handleNewSession();
      }
      setLoaded(true);
    })();
  }, []);

  useEffect(() => {
    // pendingScroll 优先：跨会话跳转时不要把视图甩到底，而是停在命中行。
    // 只要有命中要跳就跳过自动滚到底；scroll-to-match 由下一段 effect 处理。
    if (pendingScroll !== null) return;
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [items, currentResponse, currentToolCalls, pendingScroll]);

  // 跨会话搜索 → 实时 fetch。空查询清空结果（不调命令，避免无意义 IO）。
  useEffect(() => {
    if (!searchMode) return;
    const q = searchQuery.trim();
    if (!q) {
      setSearchResults([]);
      return;
    }
    let cancelled = false;
    (async () => {
      try {
        // R96: scope === "current" 时把 sessionId 传给后端，后端会跳过其它
        // 会话；不在前端 post-filter 是因为 limit=50 可能被别的 session
        // 吃满，前端筛会丢当前会话的命中。
        const args: { keyword: string; sessionId?: string } = { keyword: q };
        if (searchScope === "current") args.sessionId = sessionId;
        const hits = await invoke<SearchHit[]>("search_sessions", args);
        if (!cancelled) setSearchResults(hits);
      } catch (e) {
        console.error("search_sessions failed:", e);
        if (!cancelled) setSearchResults([]);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [searchMode, searchQuery, searchScope, sessionId]);

  // 切到目标会话后把目标 item scrollIntoView + 短暂高亮（1.5s 后清掉）。
  // 通过 data-item-idx 选择器找到 DOM 节点；找不到就当作 noop。
  useEffect(() => {
    if (pendingScroll === null) return;
    const idx = pendingScroll;
    // 跨渲染 frame 等 items 落到 DOM
    const id = window.setTimeout(() => {
      if (!scrollRef.current) {
        setPendingScroll(null);
        return;
      }
      const target = scrollRef.current.querySelector<HTMLElement>(
        `[data-item-idx="${idx}"]`,
      );
      if (target) {
        target.scrollIntoView({ block: "center", behavior: "smooth" });
        setHighlightedItemIdx(idx);
        window.setTimeout(() => setHighlightedItemIdx(null), 1500);
      }
      setPendingScroll(null);
    }, 50);
    return () => window.clearTimeout(id);
  }, [pendingScroll, items]);

  const loadSession = async (id: string) => {
    try {
      const session = await invoke<Session>("load_session", { id });
      setSessionId(session.id);
      setSessionTitle(session.title);
      setItems(session.items || []);
      messagesRef.current = session.messages || [];
    } catch (e) {
      console.error("Failed to load session:", e);
    }
  };

  const saveCurrentSession = useCallback(
    async (newItems: ChatItem[]) => {
      if (!sessionId) return;

      // Auto-generate title from first user message
      let title = sessionTitle;
      if (title === "新会话") {
        const firstUser = newItems.find((i) => i.type === "user");
        if (firstUser) {
          title = firstUser.content.slice(0, 20) + (firstUser.content.length > 20 ? "..." : "");
          setSessionTitle(title);
        }
      }

      const now = new Date().toISOString();
      const session: Session = {
        id: sessionId,
        title,
        created_at: "", // preserved by backend
        updated_at: now,
        messages: messagesRef.current,
        items: newItems,
      };

      try {
        await invoke("save_session", { session });
        // Refresh session list to reflect updated title
        const index = await invoke<SessionIndex>("list_sessions");
        setSessionList(index.sessions);
      } catch (e) {
        console.error("Failed to save session:", e);
      }
    },
    [sessionId, sessionTitle],
  );

  const handleNewSession = async () => {
    try {
      const session = await invoke<Session>("create_session");
      setSessionId(session.id);
      setSessionTitle(session.title);
      setItems([]);
      messagesRef.current = session.messages;
      setShowSessionList(false);

      // Refresh session list
      const index = await invoke<SessionIndex>("list_sessions");
      setSessionList(index.sessions);
    } catch (e) {
      console.error("Failed to create session:", e);
    }
  };

  const switchSession = async (id: string) => {
    await loadSession(id);
    setShowSessionList(false);
  };

  /// 把一条本地"系统反馈" message 推到 items（不持久化、不发给 LLM），用于
  /// `/help` / `/clear` 后的提示 / 未知命令的错误反馈。type 选 assistant 让它
  /// 视觉上像宠物自己说话，与命令的"对话内 hint"语义对齐。
  const pushLocalAssistantNote = useCallback((text: string) => {
    setItems((prev) => [...prev, { type: "assistant", content: text }]);
  }, []);

  /// 执行已 parse 出的 slash action。命令在前端拦截，**不**走 LLM。`/clear`
  /// 与 `/sleep` 涉及后端持久化，其它纯 UI 切换。
  const executeSlash = useCallback(
    async (action: SlashAction) => {
      switch (action.kind) {
        case "clear": {
          // 清空当前 session 的 items / messages（保留 system soul），写盘。
          // 不删 session 文件，session id 不变；用户回看历史时该 session 会显示
          // 空内容。比"新建会话"少一次列表 churn。
          const soul = messagesRef.current.find((m) => m?.role === "system")?.content ?? "";
          const sysMsg = { role: "system", content: soul };
          messagesRef.current = [sysMsg];
          setItems([]);
          setCurrentResponse("");
          setCurrentToolCalls([]);
          await invoke("save_session", {
            session: {
              id: sessionId,
              title: sessionTitle,
              created_at: "",
              updated_at: new Date().toISOString(),
              messages: [sysMsg],
              items: [],
            },
          }).catch((e) => console.error("Failed to save cleared session:", e));
          break;
        }
        case "tasks": {
          if (onRequestTab) {
            onRequestTab("任务");
          } else {
            pushLocalAssistantNote("当前面板未提供切标签能力（onRequestTab 未注入）。");
          }
          break;
        }
        case "search": {
          setShowSessionList(false);
          setSearchMode(true);
          setSearchQuery("");
          setSearchResults([]);
          break;
        }
        case "sleep": {
          try {
            const until = await invoke<string>("set_mute_minutes", {
              minutes: action.minutes,
            });
            pushLocalAssistantNote(
              action.minutes === 0
                ? "已解除 mute。"
                : until
                  ? `已 mute 到 ${until.replace("T", " ").slice(0, 19)}（${action.minutes} 分钟）。`
                  : `已 mute ${action.minutes} 分钟。`,
            );
          } catch (e) {
            pushLocalAssistantNote(`mute 失败：${e}`);
          }
          break;
        }
        case "help": {
          pushLocalAssistantNote(formatHelpText());
          break;
        }
        case "incomplete":
          break; // 仅 `/` 还没敲完，菜单负责展示
        case "unknown": {
          pushLocalAssistantNote(`未知命令 /${action.name}。输入 /help 查看可用命令。`);
          break;
        }
      }
    },
    [sessionId, sessionTitle, onRequestTab, pushLocalAssistantNote],
  );

  /// 复制单条消息到剪贴板。成功 → 闪 1.5s "已复制"反馈；失败 → console.error
  /// 不弹 alert（剪贴板权限错误极少，不值得打断用户）。
  const handleCopy = useCallback(async (idx: number, text: string) => {
    try {
      await navigator.clipboard.writeText(text);
      setCopiedIdx(idx);
      window.setTimeout(() => {
        setCopiedIdx((prev) => (prev === idx ? null : prev));
      }, 1500);
    } catch (e) {
      console.error("clipboard write failed:", e);
    }
  }, []);

  /// 搜索结果点击 → 切到该会话 + 标记 pendingScroll，让下一帧 effect 滚到
  /// 命中 item 并高亮。如果当前已经在该 session，不重新 loadSession（避免
  /// 闪烁），直接触发 scroll。
  const handleSelectSearchHit = useCallback(
    async (hit: SearchHit) => {
      setSearchMode(false);
      setSearchQuery("");
      setSearchResults([]);
      setSearchScope("all");
      setShowSessionList(false);
      if (hit.session_id !== sessionId) {
        await loadSession(hit.session_id);
      }
      setPendingScroll(hit.item_index);
    },
    [sessionId],
  );

  // R106: 单 session 导出 markdown 到剪贴板。load_session 拉完整 items，
  // 跳过 tool / error 行（噪音多对"对话内容"复盘价值低），按 user/assistant
  // 时间序拼成 H2 段落 markdown。reuse copyMsg 通道做短反馈，3000ms 自清空。
  const handleExportSession = async (id: string, fallbackTitle: string) => {
    try {
      const session = await invoke<Session>("load_session", { id });
      const title = session.title || fallbackTitle;
      const md = exportSessionAsMarkdown(title, session.items as ChatItem[]);
      await navigator.clipboard.writeText(md);
      setExportToast(`已导出 "${title}" 到剪贴板`);
      setTimeout(() => setExportToast(""), 3000);
    } catch (e) {
      console.error("Failed to export session:", e);
      setExportToast(`导出失败: ${e}`);
      setTimeout(() => setExportToast(""), 3000);
    }
  };

  // R101: rename 三件套。startRename 进入 edit；commitRename load → 改 title
  // → save → refresh list（顶 bar 同步），失败时 console.error 后退出 edit；
  // cancelRename 直接退出。空标题视为 cancel（保留原 title）。
  const startRename = (s: SessionMeta) => {
    setRenamingId(s.id);
    setRenameDraft(s.title);
  };
  const commitRename = async () => {
    const id = renamingId;
    const newTitle = renameDraft.trim();
    if (!id) return;
    if (!newTitle) {
      setRenamingId(null);
      return;
    }
    try {
      const session = await invoke<Session>("load_session", { id });
      session.title = newTitle;
      await invoke("save_session", { session });
      const idx = await invoke<SessionIndex>("list_sessions");
      setSessionList(idx.sessions);
      if (id === sessionId) setSessionTitle(newTitle);
    } catch (e) {
      console.error("Failed to rename session:", e);
    } finally {
      setRenamingId(null);
    }
  };
  const cancelRename = () => setRenamingId(null);

  const handleDeleteSession = async (id: string) => {
    try {
      await invoke("delete_session", { id });
      const index = await invoke<SessionIndex>("list_sessions");
      setSessionList(index.sessions);

      // If deleted the current session, switch to another or create new
      if (id === sessionId) {
        if (index.sessions.length > 0) {
          const last = index.sessions[index.sessions.length - 1];
          await loadSession(last.id);
        } else {
          await handleNewSession();
        }
      }
    } catch (e) {
      console.error("Failed to delete session:", e);
    }
  };

  // 删除二次确认：第 1 次点击转 pending（按钮变红填充 + "确定？"），5s 内再
  // 点 → 真删；过期 / 切到别的 session → 自动 revert。同时只允许一个 session
  // 处于 pending（点 B 时 A 自动取消）。
  const [pendingDeleteId, setPendingDeleteId] = useState<string | null>(null);
  const pendingDeleteTimerRef = useRef<number | null>(null);
  const handleDeleteClick = (id: string) => {
    if (pendingDeleteId === id) {
      if (pendingDeleteTimerRef.current !== null) {
        window.clearTimeout(pendingDeleteTimerRef.current);
        pendingDeleteTimerRef.current = null;
      }
      setPendingDeleteId(null);
      void handleDeleteSession(id);
      return;
    }
    if (pendingDeleteTimerRef.current !== null) {
      window.clearTimeout(pendingDeleteTimerRef.current);
    }
    setPendingDeleteId(id);
    pendingDeleteTimerRef.current = window.setTimeout(() => {
      setPendingDeleteId((prev) => (prev === id ? null : prev));
      pendingDeleteTimerRef.current = null;
    }, 5000);
  };

  const sendMessage = useCallback(
    async (content: string) => {
      const userMsg = { role: "user", content };
      messagesRef.current = [...messagesRef.current, userMsg];
      const newItems = [...items, { type: "user" as const, content }];
      setItems(newItems);
      setIsLoading(true);
      setCurrentResponse("");
      setCurrentToolCalls([]);

      const onEvent = new Channel<StreamEvent>();
      let accumulated = "";
      let toolCalls: ToolCall[] = [];
      let finalItems = newItems;

      const flushToolCalls = () => {
        if (toolCalls.length > 0) {
          const snapshot = [...toolCalls];
          finalItems = [...finalItems, { type: "tool" as const, content: "", toolCalls: snapshot }];
          setItems(finalItems);
          toolCalls = [];
          setCurrentToolCalls([]);
        }
      };

      onEvent.onmessage = (event: StreamEvent) => {
        if (event.event === "chunk") {
          flushToolCalls();
          accumulated += event.data.text;
          setCurrentResponse(accumulated);
        } else if (event.event === "toolStart") {
          if (accumulated.trim()) {
            finalItems = [...finalItems, { type: "assistant", content: accumulated }];
            setItems(finalItems);
          }
          accumulated = "";
          setCurrentResponse("");
          const tc: ToolCall = {
            name: event.data.name,
            arguments: event.data.arguments,
            isRunning: true,
          };
          toolCalls = [...toolCalls, tc];
          setCurrentToolCalls([...toolCalls]);
        } else if (event.event === "toolResult") {
          toolCalls = toolCalls.map((tc) =>
            tc.name === event.data.name && tc.isRunning
              ? { ...tc, result: event.data.result, isRunning: false }
              : tc,
          );
          setCurrentToolCalls([...toolCalls]);
        } else if (event.event === "done") {
          flushToolCalls();
          if (accumulated.trim()) {
            finalItems = [...finalItems, { type: "assistant", content: accumulated }];
            setItems(finalItems);
            messagesRef.current = [
              ...messagesRef.current,
              { role: "assistant", content: accumulated },
            ];
          }
          setCurrentResponse("");
          setIsLoading(false);
          // Auto-save after completion
          saveCurrentSession(finalItems);
        } else if (event.event === "error") {
          finalItems = [...finalItems, { type: "error", content: event.data.message }];
          setItems(finalItems);
          setCurrentResponse("");
          setCurrentToolCalls([]);
          setIsLoading(false);
          saveCurrentSession(finalItems);
        }
      };

      try {
        await invoke("chat", { messages: messagesRef.current, onEvent });
      } catch (err) {
        finalItems = [...finalItems, { type: "error" as const, content: `${err}` }];
        setItems(finalItems);
        setIsLoading(false);
        saveCurrentSession(finalItems);
      }
    },
    [items, saveCurrentSession],
  );

  // R126: submit 主逻辑抽出来让 textarea Enter 路径与 form button click 路径
  // 共用。trim / slash 分支 / sendMessage / messageHistory 维护都在这里。
  const submitInput = useCallback(() => {
    const trimmed = input.trim();
    if (!trimmed || isLoading) return;
    if (trimmed.startsWith("/")) {
      const action = parseSlashCommand(trimmed);
      if (action) {
        executeSlash(action);
        setInput("");
        setSelectedSlashIdx(0);
        return;
      }
    }
    sendMessage(trimmed);
    // R129: push 到 messageHistory，cap 20。不去重相邻同内容（让用户连发
    // 两次相同 prompt 的 redo 场景仍能 ↑ ↑ 命中各自）。historyCursor 重置
    // null 让发送后下次 ↑ 从最末（即刚发的这条）开始。
    setMessageHistory((prev) => {
      const next = [...prev, trimmed];
      return next.length > 20 ? next.slice(-20) : next;
    });
    setHistoryCursor(null);
    setInput("");
  }, [input, isLoading, executeSlash, sendMessage]);

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    submitInput();
  };

  /// 命令菜单点击：把命令名回填到输入框。带参数命令保留 `/cmd ` 等空格让用户
  /// 接着填参；不带参数的直接把命令名 + 立即提交（视觉上等价于点了一下回车）。
  const handleSelectSlashCommand = useCallback(
    (cmd: SlashCommand) => {
      if (cmd.parametric) {
        setInput(`/${cmd.name} `);
      } else {
        // 直接执行（无参命令选了就执行，少一次按 Enter）
        executeSlash(parseSlashCommand(`/${cmd.name}`)!);
        setInput("");
        setSelectedSlashIdx(0);
      }
    },
    [executeSlash],
  );

  /// 输入框键盘事件：slash 模式下接管上下 / Enter / Esc / Tab。
  /// Enter 行为：
  /// - 如果当前 input 已经是一个完整命令名（如 `/clear`，能 parse 成功且非
  ///   Unknown / Incomplete）→ 不在 keydown 拦截，让 form onSubmit 执行。
  /// - 否则按 Tab 等价处理：用选中候选自动补全（参数化命令保留 prefix 等用户
  ///   再敲）。这避免了 `/cl` + Enter 触发"未知命令 /cl"。
  const handleInputKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (!slashMenuVisible) {
      // R126: textarea Enter 默认换行；在非-slash 模式 + Enter（无 shift）
      // → 显式提交（preventDefault 防换行）；Shift+Enter → 让默认行为换行。
      if (e.key === "Enter" && !e.shiftKey) {
        e.preventDefault();
        submitInput();
        return;
      }
      // R127: 非 slash 模式 + 非空 input + Esc → 清空（"我不发了"语义）。
      // 空 input 时不拦截，避免吞掉浏览器 / IME 默认 Esc 行为。
      // R129: 历史模式中按 Esc 也退出（设 cursor=null + 清空 input）。
      if (e.key === "Escape" && input.length > 0) {
        e.preventDefault();
        setInput("");
        setHistoryCursor(null);
        return;
      }
      // R129: shell-readline 风格 ↑ / ↓ 多条历史穿越。
      // ↑：历史模式中往前；空 input + 历史非空 → 进入历史末（最新）。
      // ↓：历史模式中往后；超过末尾 → 退出 + 清空。
      // 非空 input 且不在历史模式时不拦截 ↑（textarea 多行光标向上行为）。
      if (e.key === "ArrowUp") {
        if (historyCursor !== null) {
          e.preventDefault();
          const next = Math.max(0, historyCursor - 1);
          setHistoryCursor(next);
          setInput(messageHistory[next]);
          return;
        }
        if (input.length === 0 && messageHistory.length > 0) {
          e.preventDefault();
          const next = messageHistory.length - 1;
          setHistoryCursor(next);
          setInput(messageHistory[next]);
          return;
        }
        return;
      }
      if (e.key === "ArrowDown") {
        if (historyCursor !== null) {
          e.preventDefault();
          if (historyCursor < messageHistory.length - 1) {
            const next = historyCursor + 1;
            setHistoryCursor(next);
            setInput(messageHistory[next]);
          } else {
            setHistoryCursor(null);
            setInput("");
          }
        }
        return;
      }
      return;
    }
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setSelectedSlashIdx((i) =>
        filteredCommands.length === 0 ? 0 : Math.min(i + 1, filteredCommands.length - 1),
      );
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setSelectedSlashIdx((i) => Math.max(i - 1, 0));
    } else if (e.key === "Tab") {
      e.preventDefault();
      const cmd = filteredCommands[selectedSlashIdx];
      if (cmd) handleSelectSlashCommand(cmd);
    } else if (e.key === "Escape") {
      e.preventDefault();
      setInput("");
      setSelectedSlashIdx(0);
    } else if (e.key === "Enter") {
      // R126: textarea Enter 总不应换行（命令是单行）。complete known →
      // 显式 submitInput（不再依赖 form onSubmit fallthrough）；prefix →
      // autocomplete 选中候选。
      e.preventDefault();
      const action = parseSlashCommand(input);
      const isCompleteKnown =
        action !== null &&
        action.kind !== "unknown" &&
        action.kind !== "incomplete";
      if (isCompleteKnown) {
        submitInput();
      } else {
        const cmd = filteredCommands[selectedSlashIdx];
        if (cmd) handleSelectSlashCommand(cmd);
      }
    }
  };

  if (!loaded) {
    return (
      <div style={{ display: "flex", alignItems: "center", justifyContent: "center", height: "100%", color: "var(--pet-color-muted)" }}>
        加载中...
      </div>
    );
  }

  return (
    <div className="pet-panelchat-root" style={{ display: "flex", flexDirection: "column", height: "100%" }}>
      {/* Iter R47: focus ring audit — input had `outline: none` with no
          replacement (same accessibility issue R46 fixed in ChatPanel
          and R47 fixed in PanelSettings). Scoped descendant selector
          covers all input/textarea inside this panel. */}
      <style>{`
        .pet-panelchat-root input:focus,
        .pet-panelchat-root textarea:focus {
          border-color: #38bdf8;
          box-shadow: 0 0 0 2px rgba(56,189,248,0.18);
          transition: border-color 150ms ease-out, box-shadow 150ms ease-out;
        }
        /* 聊天消息复制按钮：默认隐藏，hover 整行时弱可见，hover 按钮自身时强化。
           已复制状态由内联 style 强制 opacity=1 与绿色，覆盖默认。 */
        .pet-chat-row .pet-copy-btn {
          opacity: 0;
          transition: opacity 120ms ease-out, color 120ms ease-out, border-color 120ms ease-out;
        }
        .pet-chat-row:hover .pet-copy-btn {
          opacity: 0.85;
        }
        .pet-chat-row .pet-copy-btn:hover {
          opacity: 1;
          color: var(--pet-color-accent);
          border-color: #7dd3fc;
        }
        /* R131: 会话列表行 hover 高亮，与 R122/R123/R130 同模式。!important
           反压 inline selected 蓝色（hover 时短暂换浅灰，移开恢复 — 用户
           操作流是"hover 看清 → 点击"，期间 selected 蓝退让可接受）。 */
        .pet-session-row {
          transition: background-color 0.12s ease;
        }
        .pet-session-row:hover {
          background: rgba(0, 0, 0, 0.04) !important;
        }
        /* R142: 发送按钮 hover 强化。filter brightness 跨色域生效（accent
           在 light/dark 不同色都自动加深）；scale 0.98 给 click 微触感；
           :not(:disabled) 保护 loading 灰态不被 hover 干扰。 */
        .pet-chat-send {
          transition: filter 0.12s ease, transform 0.08s ease;
        }
        .pet-chat-send:not(:disabled):hover {
          filter: brightness(0.92);
          transform: scale(0.98);
        }
      `}</style>
      {/* Session header bar */}
      <div style={sessionBarStyle}>
        <div
          style={{ display: "flex", alignItems: "center", gap: "8px", flex: 1, cursor: "pointer", minWidth: 0 }}
          onClick={() => {
            // 进 session 下拉时关掉 search mode（互斥 UI）
            setSearchMode(false);
            setShowSessionList(!showSessionList);
          }}
        >
          <span style={{ fontSize: "13px", fontWeight: 600, color: "var(--pet-color-fg)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
            {sessionTitle}
          </span>
          <span style={{ fontSize: "10px", color: "var(--pet-color-muted)" }}>{showSessionList ? "▲" : "▼"}</span>
        </div>
        <button
          onClick={() => {
            setShowSessionList(false);
            setSearchMode((prev) => !prev);
            setSearchQuery("");
            setSearchResults([]);
            // R96: 关闭 search 时复位 scope，下次进默认全局
            setSearchScope("all");
          }}
          style={{ ...newSessionBtnStyle, color: searchMode ? "#0369a1" : "var(--pet-color-fg)", background: searchMode ? "#e0f2fe" : "var(--pet-color-bg)" }}
          title="搜索消息（默认全部会话；进面板后可切换『本会话』）"
          aria-label="cross-session search"
        >
          🔍
        </button>
        <button onClick={handleNewSession} style={newSessionBtnStyle} title="新建会话">
          + 新会话
        </button>
      </div>

      {/* Search panel (取代 session dropdown 当 search mode 开启) */}
      {searchMode && (
        <div style={sessionDropdownStyle}>
          <div style={{ padding: "8px 12px", borderBottom: "1px solid #f1f5f9", display: "flex", gap: 6 }}>
            <input
              type="text"
              autoFocus
              placeholder={searchScope === "current" ? "搜本会话…" : "按关键字搜索全部会话…"}
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Escape") {
                  setSearchMode(false);
                  setSearchQuery("");
                  setSearchResults([]);
                  setSearchScope("all");
                }
              }}
              style={{ flex: 1, padding: "6px 10px", border: "1px solid var(--pet-color-border)", borderRadius: 4, fontSize: 13, color: "var(--pet-color-fg)", background: "var(--pet-color-card)" }}
            />
            {/* R96: scope 双键 pill toggle —— 全部 vs 本会话。active 态走 card
                fg，inactive 走 bg muted（与系统 toggle pill 配色一致）。 */}
            <div
              style={{
                display: "flex",
                gap: 0,
                alignItems: "center",
                padding: 1,
                background: "var(--pet-color-bg)",
                borderRadius: 4,
              }}
            >
              {(["all", "current"] as const).map((scope) => {
                const active = searchScope === scope;
                return (
                  <button
                    key={scope}
                    type="button"
                    onClick={() => setSearchScope(scope)}
                    style={{
                      padding: "2px 8px",
                      fontSize: 11,
                      border: "none",
                      borderRadius: 3,
                      background: active ? "var(--pet-color-card)" : "transparent",
                      color: active ? "var(--pet-color-fg)" : "var(--pet-color-muted)",
                      cursor: active ? "default" : "pointer",
                      fontWeight: active ? 600 : 400,
                    }}
                    title={
                      scope === "all"
                        ? "搜全部历史会话"
                        : "只搜当前打开会话（更精准，结果不被其它 session 抢限额）"
                    }
                  >
                    {scope === "all" ? "全部" : "本会话"}
                  </button>
                );
              })}
            </div>
            {searchQuery && (
              <button
                type="button"
                onClick={() => setSearchQuery("")}
                style={{ padding: "0 10px", border: "1px solid var(--pet-color-border)", borderRadius: 4, background: "var(--pet-color-card)", color: "var(--pet-color-muted)", cursor: "pointer", fontSize: 12 }}
                title="清空"
              >
                ✕
              </button>
            )}
          </div>
          {!searchQuery.trim() ? (
            <div style={{ padding: "12px", textAlign: "center", color: "var(--pet-color-muted)", fontSize: "12px" }}>
              {searchScope === "current"
                ? "输入关键字搜本会话内的消息"
                : "输入关键字查找跨会话消息（user / assistant）"}
            </div>
          ) : searchResults.length === 0 ? (
            <div style={{ padding: "12px", textAlign: "center", color: "var(--pet-color-muted)", fontSize: "12px" }}>
              {searchScope === "current" ? "本会话没有匹配的消息" : "没有匹配的消息"}
            </div>
          ) : (
            searchResults.map((hit) => (
              <SearchResultRow
                key={`${hit.session_id}-${hit.item_index}`}
                hit={hit}
                onSelect={handleSelectSearchHit}
              />
            ))
          )}
        </div>
      )}

      {/* Session list dropdown */}
      {showSessionList && !searchMode && (
        <div style={sessionDropdownStyle}>
          {/* R106: 导出 toast。3s 自清空；非空时贴顶横幅，accent 色提示。 */}
          {exportToast && (
            <div
              style={{
                padding: "6px 12px",
                fontSize: 11,
                color: "var(--pet-color-accent)",
                background: "var(--pet-color-bg)",
                borderBottom: "1px solid var(--pet-color-border)",
              }}
            >
              {exportToast}
            </div>
          )}
          {sessionList.length === 0 ? (
            <div style={{ padding: "12px", textAlign: "center", color: "var(--pet-color-muted)", fontSize: "12px" }}>
              暂无历史会话
            </div>
          ) : (
            [...sessionList].reverse().map((s) => (
              <div
                key={s.id}
                className="pet-session-row"
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: "8px",
                  padding: "8px 12px",
                  cursor: "pointer",
                  background: s.id === sessionId ? "#f0f9ff" : "transparent",
                  borderBottom: "1px solid #f1f5f9",
                }}
              >
                <div
                  style={{ flex: 1, minWidth: 0 }}
                  onClick={() => {
                    if (renamingId !== s.id) switchSession(s.id);
                  }}
                >
                  {renamingId === s.id ? (
                    /* R101: inline rename input。Enter / blur 提交；Esc 取消；
                       click stopPropagation 防 switchSession 触发。 */
                    <input
                      autoFocus
                      value={renameDraft}
                      onChange={(e) => setRenameDraft(e.target.value)}
                      onKeyDown={(e) => {
                        if (e.key === "Enter") {
                          e.preventDefault();
                          void commitRename();
                        } else if (e.key === "Escape") {
                          e.preventDefault();
                          cancelRename();
                        }
                      }}
                      onBlur={() => void commitRename()}
                      onClick={(e) => e.stopPropagation()}
                      style={{
                        width: "100%",
                        padding: "2px 6px",
                        fontSize: 13,
                        border: "1px solid var(--pet-color-accent)",
                        borderRadius: 3,
                        background: "var(--pet-color-card)",
                        color: "var(--pet-color-fg)",
                        outline: "none",
                        boxSizing: "border-box",
                      }}
                    />
                  ) : (
                    <>
                      <div style={{ fontSize: "13px", color: "var(--pet-color-fg)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", fontWeight: s.id === sessionId ? 600 : 400 }}>
                        {s.title}
                        {typeof s.item_count === "number" && (
                          <span
                            style={{ fontWeight: 400, color: "var(--pet-color-muted)", marginLeft: 6 }}
                            title="会话内可见对话条数（不含 system message）"
                          >
                            ({s.item_count} 条)
                          </span>
                        )}
                      </div>
                      <div style={{ fontSize: "11px", color: "var(--pet-color-muted)" }}>
                        {s.updated_at.split("T")[0]}
                      </div>
                    </>
                  )}
                </div>
                {renamingId !== s.id && (
                  <button
                    type="button"
                    onClick={(e) => {
                      e.stopPropagation();
                      void handleExportSession(s.id, s.title);
                    }}
                    style={{
                      padding: "2px 6px",
                      borderRadius: "4px",
                      border: "none",
                      background: "transparent",
                      color: "var(--pet-color-muted)",
                      fontSize: "12px",
                      cursor: "pointer",
                    }}
                    title="把会话全部 user / assistant 消息复制为 markdown 到剪贴板"
                    aria-label="export session"
                  >
                    📋
                  </button>
                )}
                {renamingId !== s.id && (
                  <button
                    type="button"
                    onClick={(e) => {
                      e.stopPropagation();
                      startRename(s);
                    }}
                    style={{
                      padding: "2px 6px",
                      borderRadius: "4px",
                      border: "none",
                      background: "transparent",
                      color: "var(--pet-color-muted)",
                      fontSize: "12px",
                      cursor: "pointer",
                    }}
                    title="重命名会话"
                    aria-label="rename session"
                  >
                    ✏️
                  </button>
                )}
                {renamingId !== s.id && (
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      handleDeleteClick(s.id);
                    }}
                    style={{
                      padding: "2px 6px",
                      borderRadius: "4px",
                      border: "none",
                      background: pendingDeleteId === s.id ? "#dc2626" : "#fee2e2",
                      color: pendingDeleteId === s.id ? "#fff" : "#dc2626",
                      fontSize: "11px",
                      cursor: "pointer",
                      fontWeight: pendingDeleteId === s.id ? 700 : 400,
                    }}
                    title={pendingDeleteId === s.id ? "再点一次确认删除（5 秒后自动撤回）" : "删除会话"}
                  >
                    {pendingDeleteId === s.id ? "确定？" : "删除"}
                  </button>
                )}
              </div>
            ))
          )}
        </div>
      )}

      {/* Message list — R103: 外层 relative 让 ↑ 浮动按钮锚定到视口右下，
          不被滚动卷走；内层 height:100% + overflowY:auto 保留原 scroll
          行为。 */}
      <div style={{ flex: 1, position: "relative", overflow: "hidden" }}>
      <div
        ref={scrollRef}
        onScroll={() => {
          const el = scrollRef.current;
          if (!el) return;
          setScrolledFromTop(el.scrollTop > 200);
        }}
        style={{ height: "100%", overflowY: "auto", padding: "16px" }}
      >
        {items.length === 0 && !currentResponse && (
          <div style={{ textAlign: "center", color: "var(--pet-color-muted)", marginTop: "40px", fontSize: "14px" }}>
            开始聊天吧~
          </div>
        )}

        {items.map((item, i) => {
          // 给每条消息挂 data-item-idx，跨会话搜索点击结果后能定位 + 高亮。
          const isHighlighted = highlightedItemIdx === i;
          const wrapperBase = (justify: "flex-end" | "flex-start"): React.CSSProperties => ({
            marginBottom: "12px",
            display: "flex",
            justifyContent: justify,
            transition: "background-color 200ms ease-out",
            background: isHighlighted ? "rgba(254, 243, 199, 0.7)" : "transparent",
            borderRadius: 8,
            padding: isHighlighted ? "4px 0" : 0,
          });
          if (item.type === "user") {
            return (
              <CopyableMessage
                key={i}
                role="user"
                content={item.content}
                itemIdx={i}
                copied={copiedIdx === i}
                onCopy={handleCopy}
                wrapperStyle={wrapperBase("flex-end")}
              />
            );
          }
          if (item.type === "assistant") {
            if (!item.content.trim()) return null;
            return (
              <CopyableMessage
                key={i}
                role="assistant"
                content={item.content}
                itemIdx={i}
                copied={copiedIdx === i}
                onCopy={handleCopy}
                wrapperStyle={wrapperBase("flex-start")}
              />
            );
          }
          if (item.type === "tool") {
            return (
              <div key={i} data-item-idx={i} style={{ marginBottom: "12px", maxWidth: "85%" }}>
                {item.toolCalls?.map((tc, j) => {
                  // 自然语言派单：propose_task 的结果走确认卡，而非普通工具块。
                  // 解析失败 / 工具返回 error → 回退普通展示，保留排查能见性。
                  if (tc.name === "propose_task" && tc.result) {
                    const proposal = parseTaskProposal(tc.result);
                    if (proposal) {
                      return <TaskProposalCard key={j} proposal={proposal} />;
                    }
                  }
                  return <ToolCallBlock key={j} name={tc.name} arguments={tc.arguments} result={tc.result} />;
                })}
              </div>
            );
          }
          if (item.type === "error") {
            return (
              <div key={i} data-item-idx={i} style={{ marginBottom: "12px", display: "flex", justifyContent: "flex-start" }}>
                <div style={{ ...bubbleStyle("assistant"), background: "var(--pet-tint-orange-bg)", color: "var(--pet-tint-orange-fg)" }}>
                  {item.content}
                </div>
              </div>
            );
          }
          return null;
        })}

        {/* Live tool calls */}
        {currentToolCalls.length > 0 && (
          <div style={{ marginBottom: "12px", maxWidth: "85%" }}>
            {currentToolCalls.map((tc, j) => {
              if (tc.name === "propose_task" && tc.result) {
                const proposal = parseTaskProposal(tc.result);
                if (proposal) {
                  return <TaskProposalCard key={j} proposal={proposal} />;
                }
              }
              return (
                <ToolCallBlock
                  key={j}
                  name={tc.name}
                  arguments={tc.arguments}
                  result={tc.result}
                  isRunning={tc.isRunning}
                />
              );
            })}
          </div>
        )}

        {/* Streaming response */}
        {currentResponse && (
          <div style={{ marginBottom: "12px", display: "flex", justifyContent: "flex-start" }}>
            <div style={bubbleStyle("assistant")}>
              {currentResponse}
              <span style={{ animation: "blink 1s infinite" }}>▌</span>
            </div>
          </div>
        )}
      </div>
      {/* R103: 浮动 ↑ 跳到顶按钮。仅在 scrollTop > 200 时显，accent 圆形按钮
          锚定外层 relative 容器右下，不被卷入滚动。 */}
      {scrolledFromTop && (
        <button
          type="button"
          onClick={() =>
            scrollRef.current?.scrollTo({ top: 0, behavior: "smooth" })
          }
          title="回到会话开头"
          aria-label="scroll to top"
          style={{
            position: "absolute",
            right: 16,
            bottom: 16,
            width: 36,
            height: 36,
            borderRadius: "50%",
            border: "none",
            background: "var(--pet-color-accent)",
            color: "#fff",
            fontSize: 18,
            cursor: "pointer",
            boxShadow: "0 2px 8px rgba(0,0,0,0.2)",
            opacity: 0.92,
          }}
        >
          ↑
        </button>
      )}
      </div>

      {/* Input bar */}
      <form
        onSubmit={handleSubmit}
        style={{
          display: "flex",
          gap: "8px",
          padding: "12px 16px",
          borderTop: "1px solid var(--pet-color-border)",
          background: "var(--pet-color-card)",
          position: "relative", // SlashCommandMenu 用 absolute / bottom: 100% 锚到这里
        }}
      >
        {slashMenuVisible && (
          <SlashCommandMenu
            commands={filteredCommands}
            selectedIdx={selectedSlashIdx}
            onSelect={handleSelectSlashCommand}
          />
        )}
        {/* R134: 输入框字数 counter。非空时浮 input bar 顶左（与 R132 历史
            hint 错开 — 顶右）。pointerEvents none 让它不挡按钮 / textarea。 */}
        {input.length > 0 && (
          <div
            style={{
              position: "absolute",
              top: -22,
              left: 16,
              fontSize: 10,
              color: "var(--pet-color-muted)",
              pointerEvents: "none",
              fontFamily: "'SF Mono', 'Menlo', monospace",
              whiteSpace: "nowrap",
            }}
            title="当前消息字符数（Unicode code units 计；含换行 / 空白）"
          >
            {input.length} 字
          </div>
        )}
        {/* R132: 历史模式视觉提示。historyCursor 非 null 时浮在 input bar
            顶部右侧；onChange 改写 / Esc / 发送 / ↓ 越过末尾 自然 cursor=null
            → hint 消失。pointerEvents none 让它不挡按钮。 */}
        {historyCursor !== null && (
          <div
            style={{
              position: "absolute",
              top: -22,
              right: 16,
              fontSize: 10,
              background: "var(--pet-color-card)",
              border: "1px solid var(--pet-color-border)",
              borderRadius: 4,
              padding: "2px 8px",
              color: "var(--pet-color-muted)",
              pointerEvents: "none",
              whiteSpace: "nowrap",
              fontFamily: "'SF Mono', 'Menlo', monospace",
            }}
            title={`当前浏览历史第 ${historyCursor + 1} / ${messageHistory.length} 条；↑ 往更早，↓ 往更新，Esc 退出`}
          >
            ↑ 历史 {historyCursor + 1} / {messageHistory.length}
          </div>
        )}
        {/* R126: 单行 input → auto-grow textarea。rows 用 \n 计数 + 1（cap 5），
            soft-wrap 不影响（用户主动换行才 grow）。Enter 提交 / Shift+Enter
            换行的语义在 handleInputKeyDown 里。 */}
        <textarea
          value={input}
          onChange={(e) => {
            const v = e.target.value;
            setInput(v);
            // R129: 用户改写 history 召回内容 → 自动退出历史模式让 free editing。
            // 再 ↑ 时从最新一条（历史末）重新进入；不会跳到 cursor 之前残留的位置。
            if (
              historyCursor !== null &&
              v !== messageHistory[historyCursor]
            ) {
              setHistoryCursor(null);
            }
          }}
          onKeyDown={handleInputKeyDown}
          placeholder='输入消息（Enter 发送 / Shift+Enter 换行；首字符 "/" 触发命令面板）'
          rows={Math.max(
            1,
            Math.min(5, (input.match(/\n/g)?.length ?? 0) + 1),
          )}
          style={{
            flex: 1,
            padding: "10px 14px",
            borderRadius: "10px",
            border: "1px solid var(--pet-color-border)",
            fontSize: "14px",
            outline: "none",
            color: "var(--pet-color-fg)",
            background: "var(--pet-color-card)",
            resize: "none",
            fontFamily: "inherit",
            lineHeight: 1.5,
            overflowY: "auto",
          }}
        />
        <button
          type="submit"
          className="pet-chat-send"
          disabled={isLoading}
          style={{
            padding: "10px 20px",
            borderRadius: "10px",
            border: "none",
            background: isLoading ? "#cbd5e1" : "var(--pet-color-accent)",
            color: "#fff",
            fontSize: "14px",
            fontWeight: 500,
            cursor: isLoading ? "default" : "pointer",
          }}
        >
          {isLoading ? "..." : "发送"}
        </button>
      </form>
    </div>
  );
}

/* ---------- Styles ---------- */

/// R106: 把 session.items 序列化成 markdown。H1=title + 摘要 → 按时间序
/// 渲染 user/assistant 段（H2 + emoji + content）；tool/error 跳过（噪音
/// 多对"对话内容"复盘价值低）。换行原样保留，markdown render 时自然 wrap。
function exportSessionAsMarkdown(
  title: string,
  items: ChatItem[],
): string {
  const lines: string[] = [];
  const visibleItems = items.filter(
    (it) => it.type === "user" || it.type === "assistant",
  );
  lines.push(`# ${title}`);
  lines.push(
    `> 导出时间: ${new Date().toLocaleString()} · 共 ${visibleItems.length} 条消息`,
  );
  lines.push("");
  for (const it of visibleItems) {
    const glyph = it.type === "user" ? "🧑" : "🐾";
    lines.push(`## ${glyph} ${it.type}`);
    lines.push("");
    lines.push(it.content);
    lines.push("");
  }
  return lines.join("\n");
}

function bubbleStyle(role: "user" | "assistant"): React.CSSProperties {
  return {
    maxWidth: "80%",
    padding: "10px 14px",
    borderRadius: role === "user" ? "16px 16px 4px 16px" : "16px 16px 16px 4px",
    background: role === "user" ? "var(--pet-color-accent)" : "var(--pet-color-card)",
    color: role === "user" ? "#fff" : "var(--pet-color-fg)",
    fontSize: "14px",
    lineHeight: "1.6",
    boxShadow: "0 1px 3px rgba(0,0,0,0.08)",
    wordBreak: "break-word",
    whiteSpace: "pre-wrap",
  };
}

const sessionBarStyle: React.CSSProperties = {
  display: "flex",
  alignItems: "center",
  gap: "8px",
  padding: "8px 16px",
  borderBottom: "1px solid var(--pet-color-border)",
  background: "var(--pet-color-card)",
  flexShrink: 0,
};

const newSessionBtnStyle: React.CSSProperties = {
  padding: "4px 12px",
  borderRadius: "6px",
  border: "1px solid var(--pet-color-border)",
  background: "var(--pet-color-bg)",
  color: "var(--pet-color-accent)",
  fontSize: "12px",
  fontWeight: 600,
  cursor: "pointer",
  whiteSpace: "nowrap",
};

const sessionDropdownStyle: React.CSSProperties = {
  maxHeight: "240px",
  overflowY: "auto",
  borderBottom: "1px solid var(--pet-color-border)",
  background: "var(--pet-color-card)",
  flexShrink: 0,
};

/**
 * 单条 user / assistant 消息的渲染容器。在原有 bubble 旁边挂一个 hover-only
 * 的「复制」按钮：assistant 在 bubble 右侧，user 在 bubble 左侧（与 bubble 对齐
 * 方向相反，避免按钮被屏幕边缘挤到看不见）。`data-item-idx` 留在最外层 row
 * 以保留跨会话搜索的 scrollIntoView 路径。
 *
 * 已复制状态 (`copied = true`) 用绿色 + 强制 opacity=1 覆盖默认 hover-only 显示，
 * 让用户在松开鼠标后还能看到 1.5s 的"已复制"反馈。
 */
function CopyableMessage({
  role,
  content,
  itemIdx,
  copied,
  onCopy,
  wrapperStyle,
}: {
  role: "user" | "assistant";
  content: string;
  itemIdx: number;
  copied: boolean;
  onCopy: (idx: number, text: string) => void;
  wrapperStyle: React.CSSProperties;
}) {
  const button = (
    <button
      type="button"
      className="pet-copy-btn"
      title={copied ? "已复制到剪贴板" : "复制这条消息"}
      onClick={() => onCopy(itemIdx, content)}
      style={{
        alignSelf: "flex-end",
        padding: "2px 6px",
        fontSize: "10px",
        lineHeight: 1.2,
        border: "1px solid var(--pet-color-border)",
        borderRadius: 4,
        background: "var(--pet-color-card)",
        color: copied ? "#16a34a" : "var(--pet-color-muted)",
        cursor: "pointer",
        whiteSpace: "nowrap",
        flexShrink: 0,
        opacity: copied ? 1 : undefined, // copied 状态强制可见，覆盖 CSS hover-only
      }}
    >
      {copied ? "已复制" : "复制"}
    </button>
  );
  // 仅识别 URL（不启用完整 markdown），避免历史里的散乱 `*` / `-` 误渲染。
  // 桌面气泡同 url 化路径但走 parseMarkdown 完整版（气泡是即时一句，无历史
  // 风险）。
  const bubble = <div style={bubbleStyle(role)}>{parseUrls(content)}</div>;
  return (
    <div className="pet-chat-row" data-item-idx={itemIdx} style={wrapperStyle}>
      {/* user 右对齐 → 按钮在 bubble 左侧；assistant 左对齐 → 按钮在 bubble 右侧 */}
      {role === "user" ? (
        <>
          <div style={{ display: "flex", alignItems: "flex-end", gap: 6 }}>
            {button}
            {bubble}
          </div>
        </>
      ) : (
        <>
          <div style={{ display: "flex", alignItems: "flex-end", gap: 6 }}>
            {bubble}
            {button}
          </div>
        </>
      )}
    </div>
  );
}

/**
 * 搜索结果单行。把 snippet 在匹配区段切三段（前 / 命中 / 后）渲染，命中段用
 * 浅黄背景与 priBadge 的同色系（与面板内其它"重点"色一致）。整行可点 → 触发
 * 跳转。
 */
function SearchResultRow({
  hit,
  onSelect,
}: {
  hit: SearchHit;
  onSelect: (hit: SearchHit) => void;
}) {
  // snippet 按 char 切三段：[0..match_start) + [match_start..match_start+match_len) +
  // [tail..]。Array.from 分 char 数组（中文友好；string slice 在 UTF-16 surrogate
  // pair 上不安全，但 char 视角全是单 codepoint 时也仅在 emoji 等极端场景才会有差，
  // 当前内容场景安全）。
  const chars = Array.from(hit.snippet);
  const head = chars.slice(0, hit.match_start).join("");
  const mid = chars.slice(hit.match_start, hit.match_start + hit.match_len).join("");
  const tail = chars.slice(hit.match_start + hit.match_len).join("");
  const roleGlyph = hit.role === "user" ? "🧑" : "🐾";
  return (
    <div
      onClick={() => onSelect(hit)}
      style={{
        padding: "8px 12px",
        cursor: "pointer",
        borderBottom: "1px solid #f1f5f9",
      }}
      title={`跳到「${hit.session_title}」第 ${hit.item_index + 1} 条消息`}
    >
      <div style={{ fontSize: "12px", color: "var(--pet-color-fg)", lineHeight: 1.5, display: "flex", gap: 6, alignItems: "flex-start" }}>
        <span style={{ flexShrink: 0 }}>{roleGlyph}</span>
        <span style={{ wordBreak: "break-word" }}>
          {head}
          <mark style={{ background: "#fef3c7", color: "#92400e", padding: "0 1px", borderRadius: 2 }}>
            {mid}
          </mark>
          {tail}
        </span>
      </div>
      <div style={{ fontSize: "11px", color: "var(--pet-color-muted)", marginTop: 2 }}>
        {hit.session_title} · {hit.session_updated_at.split("T")[0]}
      </div>
    </div>
  );
}
