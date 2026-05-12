import { useEffect, useMemo, useRef, useState } from "react";
import { bubbleStyle } from "./panel/panelChatBits";
import { ImageLightbox } from "./common/ImageLightbox";
import { ImageThumb } from "./common/ImageThumb";
import { parseMarkdown } from "../utils/inlineMarkdown";
import { extractImages, extractText, type MessageContent } from "../utils/messageContent";

interface ChatMessage {
  role: "user" | "assistant" | "system" | "tool";
  /// 多模态：可能是裸字符串或 OpenAI compatible parts 数组。统一走 extractText
  /// / extractImages 拆出渲染所需视图。
  content: MessageContent;
  /// 消息时间戳（ISO）。新发的 user / assistant 都有；老 session 加载回来的
  /// 可能缺 —— 显时间时退回"?"。
  ts?: string;
}

interface Props {
  /// 来自 useChat 的完整消息数组（含 system / tool）；本组件自己过滤展示。
  messages: ChatMessage[];
  /// 流式中的当前 chunk 累积。空串表示无 streaming。
  currentResponse: string;
  /// useChat 的 toolStatus：调到 tool 时 chunk 流停了，tool 结果回前先显
  /// "✅ X done" 让用户知道宠物在执行而非卡。空串 = 无 tool 状态。
  toolStatus?: string;
  isLoading: boolean;
  visible: boolean;
  /// 最新 assistant 行 👍 按钮的回调。写 Liked 反馈。流式中或 history 模式
  /// 不传，按钮不渲染（避免误触没读完的内容）。
  onLike?: () => void;
  /// 「最大化」按钮 → 打开 Panel chat 页。点击调用此回调；不传则按钮
  /// 不渲染。替代旧 ChatPanel 底栏的 💬 按钮，让用户从 mini chat 顶角直
  /// 接进入 panel。
  onOpenPanel?: () => void;
  /// streaming 中取消回复。仅在 isLoading 时挂 Esc keydown 监听调它；soft
  /// cancel 语义：把已累积文本 finalize 为 assistant 行 + 标 `[已取消]`。
  onCancel?: () => void;
  /// 复制对话历史时用户 / assistant 行的前缀。不传或空 → fallback 🧑 / 🐾。
  /// 由 App.tsx 从 settings.user_glyph / assistant_glyph 注入。
  userGlyph?: string;
  assistantGlyph?: string;
  /// 当前 NOW-marked 任务 title → 过期 ms 时戳。pet 端跨 panel 监听
  /// "task-now-mark" 事件后由 App.tsx 维护。hover 顶部 mini chat 出小卡列。
  nowTasks?: Map<string, number>;
}

/// 最近 N 条的硬上限。窗口很小，DOM 太长既不好读也耗渲染。
const MINI_CHAT_MAX_ITEMS = 20;

const MINI_CHAT_STYLES = `
@keyframes pet-mini-chat-fade-in {
  from { opacity: 0; transform: translateY(6px); }
  to   { opacity: 1; transform: translateY(0); }
}
.pet-mini-chat::-webkit-scrollbar {
  width: 6px;
}
.pet-mini-chat::-webkit-scrollbar-thumb {
  background: rgba(148, 163, 184, 0.55);
  border-radius: 3px;
}
.pet-mini-chat::-webkit-scrollbar-track {
  background: transparent;
}
/* ⛶ 最大化按钮：基态柔和，hover 时 scale + 提亮边框 + 加深 shadow，与
   PanelChat 顶部 ⛶ 行为一致。transform 比 width/height 改动便宜。 */
.pet-mini-maxbtn {
  transition: transform 120ms ease-out, border-color 120ms ease-out,
              box-shadow 120ms ease-out, color 120ms ease-out;
}
.pet-mini-maxbtn:hover {
  transform: scale(1.12);
  border-color: var(--pet-color-accent);
  color: var(--pet-color-accent);
  box-shadow: var(--pet-shadow-md);
}
.pet-mini-bubble-like-btn {
  border: none;
  background: transparent;
  color: var(--pet-color-muted);
  font-size: 11px;
  line-height: 1;
  padding: 0 2px;
  cursor: pointer;
  opacity: 0.55;
  transition: opacity 120ms ease-out, color 120ms ease-out, transform 120ms ease-out;
}
.pet-mini-bubble-like-btn:hover {
  opacity: 1;
  /* 粉色 like 反馈不是品牌主色但语义稳定 —— pink-500 light / dark 都能读，
     不主题化以免 hover 状态分裂出八种深浅。 */
  color: #ec4899;
  transform: scale(1.15);
}
/* 单条 bubble 复制按钮：默认完全隐，行级 hover 时弱可见，自身 hover 强化。
   与 PanelChat 的 .pet-copy-btn 同模式。已复制态由 inline opacity 强制保
   留 + 绿色色覆盖。 */
.pet-mini-row .pet-mini-row-copy {
  opacity: 0;
  transition: opacity 120ms ease-out, color 120ms ease-out;
  border: none;
  background: transparent;
  color: var(--pet-color-muted);
  font-size: 10px;
  line-height: 1;
  padding: 0 2px;
  cursor: pointer;
}
.pet-mini-row:hover .pet-mini-row-copy {
  opacity: 0.7;
}
.pet-mini-row .pet-mini-row-copy:hover {
  opacity: 1;
  color: var(--pet-color-accent);
}
/* 时间戳小角标：仅在 row hover 时弱可见。监控类信息，存在感低于复制 / 反
   馈按钮，所以默认完全隐 + hover 仅升 0.55（不像 copy 升 0.7）。无 ts
   的老 session 行 caller 自己不渲染本元素。 */
.pet-mini-row .pet-mini-row-time {
  opacity: 0;
  transition: opacity 120ms ease-out;
}
.pet-mini-row:hover .pet-mini-row-time {
  opacity: 0.55;
}
/* streaming 时的"宠物在思考"脉冲：opacity 0.4→1→0.4 循环 1.4s；首 chunk 到
   达前唯一可视提示，到达后与 streaming bubble 并列继续脉冲让用户感到"还在
   流"。reduced-motion 媒体查询下退化为常亮，避免对眩晕症用户挑战。 */
@keyframes pet-mini-thinking-pulse {
  0%   { opacity: 0.4; transform: scale(0.96); }
  50%  { opacity: 1;   transform: scale(1.04); }
  100% { opacity: 0.4; transform: scale(0.96); }
}
@keyframes pet-mini-thinking-dots {
  0%, 20%  { content: ""; }
  40%      { content: "."; }
  60%      { content: ".."; }
  80%, 100%{ content: "..."; }
}
.pet-mini-thinking-glyph {
  display: inline-block;
  animation: pet-mini-thinking-pulse 1.4s ease-in-out infinite;
  font-size: 14px;
  line-height: 1;
}
.pet-mini-thinking-dots::after {
  content: "";
  animation: pet-mini-thinking-dots 1.4s steps(4, end) infinite;
  margin-left: 1px;
  letter-spacing: 1px;
}
@media (prefers-reduced-motion: reduce) {
  .pet-mini-thinking-glyph { animation: none; opacity: 0.85; }
  .pet-mini-thinking-dots::after { animation: none; content: "…"; }
}
`;

/// 容器底部 8px 内视为"贴底"，用于决定 follow-tail 是否成立。给浮点偏差一
/// 点缓冲，避免微小量误判。
const FOLLOW_BOTTOM_THRESHOLD_PX = 8;

/// ChatMessage.ts → `[HH:MM]` 显示串。无 ts / 解析失败 → `[?]`。copyRecentN
/// 与 bubble hover tooltip 都用同一份格式。
function formatBubbleTimestamp(ts: string | undefined): string {
  if (!ts) return "[?]";
  const d = new Date(ts);
  if (Number.isNaN(d.getTime())) return "[?]";
  const hh = String(d.getHours()).padStart(2, "0");
  const mm = String(d.getMinutes()).padStart(2, "0");
  return `[${hh}:${mm}]`;
}

export function ChatMini({
  messages,
  currentResponse,
  toolStatus,
  isLoading,
  visible,
  onLike,
  onOpenPanel,
  onCancel,
  userGlyph,
  assistantGlyph,
  nowTasks,
}: Props) {
  // 空 / undefined fallback 内置默认；trim 去掉用户误打的空格。
  const effectiveUserGlyph = userGlyph?.trim() || "🧑";
  const effectiveAssistantGlyph = assistantGlyph?.trim() || "🐾";
  const scrollRef = useRef<HTMLDivElement>(null);
  // followTail：用户是否处于"自动跟随最新"状态。挂载时默认 true（贴底）。
  // 用 ref 让 auto-scroll effect 拿到最新值而不必加进 deps；同名 state
  // 仅供「跳到底浮标」按钮可见态用。两者由 onScroll 同步更新。
  const followTailRef = useRef(true);
  const [notAtBottom, setNotAtBottom] = useState(false);
  const [lightboxSrc, setLightboxSrc] = useState<string | null>(null);
  // ⌘+C 快捷复制反馈：1.5s 显"已复制最近一条"小气泡，自清。
  const [copyToast, setCopyToast] = useState<"none" | "done" | "err">("none");
  // 顶部 📋 复制最近 N 条按钮的弹出菜单状态。
  const [copyMenuOpen, setCopyMenuOpen] = useState(false);
  // 复制 N 条时是否带 [HH:MM] 时间前缀。开启后老消息（没 ts）显 "[?]"，提
  // 醒用户那条是 session 加载回来的旧条。
  const [copyIncludeTime, setCopyIncludeTime] = useState(false);
  // NOW 任务列表浮窗：hover ⚡ 角标时显当前 mark 队列 + 倒计时。tick 每秒
  // 刷一次让倒计时数字真正动起来；仅在有 mark 时启用 interval。
  const [nowOverlayHover, setNowOverlayHover] = useState(false);
  const [nowTick, setNowTick] = useState(0);
  useEffect(() => {
    if (!nowTasks || nowTasks.size === 0) return;
    const id = window.setInterval(() => setNowTick((t) => t + 1), 1000);
    return () => window.clearInterval(id);
  }, [nowTasks]);
  // 单条 bubble 复制反馈：刚被复制的 visibleItems idx，1.5s 自动清。
  // 与 copyToast（⌘+C 复制最近一条）分开，让两套语义各自独立显视觉反馈。
  const [bubbleCopyIdx, setBubbleCopyIdx] = useState<number | null>(null);
  const handleBubbleCopy = (idx: number, text: string) => {
    if (!text) return;
    navigator.clipboard
      .writeText(text)
      .then(() => {
        setBubbleCopyIdx(idx);
        window.setTimeout(
          () => setBubbleCopyIdx((cur) => (cur === idx ? null : cur)),
          1500,
        );
      })
      .catch((err) => console.error("bubble copy failed:", err));
  };
  // ⌘F inline 搜：搜索条显隐 + keyword + 当前 active hit 在 hits 数组中的
  // 下标。hits 是 visibleItems 内命中 keyword 的 idx 列表（重算靠 useMemo）。
  // Enter / Shift+Enter 在 hits 内循环跳；scrollIntoView 由 effect 在 active
  // 变化时触发，配合 bubble 的 data-mini-idx 选择器定位。
  const [searchOpen, setSearchOpen] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [searchActiveHitIdx, setSearchActiveHitIdx] = useState(0);
  const searchInputRef = useRef<HTMLInputElement>(null);

  /// 把最近 N 条 user / assistant 文本拼成 markdown 段落（带角色 glyph 区分）
  /// 写到剪贴板。N=0 / 无消息 → toast 错误。复用 copyToast 反馈通道。
  const copyRecentN = (n: number) => {
    setCopyMenuOpen(false);
    const slice = messages
      .filter((m) => m.role === "user" || m.role === "assistant")
      .slice(-n);
    if (slice.length === 0) {
      setCopyToast("err");
      window.setTimeout(() => setCopyToast("none"), 1500);
      return;
    }
    const text = slice
      .map((m) => {
        const glyph =
          m.role === "user" ? effectiveUserGlyph : effectiveAssistantGlyph;
        const prefix = copyIncludeTime ? `${formatBubbleTimestamp(m.ts)} ${glyph}` : glyph;
        return `${prefix} ${extractText(m.content)}`.trim();
      })
      .filter((s) => s.length > 0)
      .join("\n\n");
    navigator.clipboard
      .writeText(text)
      .then(() => {
        setCopyToast("done");
        window.setTimeout(() => setCopyToast("none"), 1500);
      })
      .catch((err) => {
        console.error("copy recent N failed:", err);
        setCopyToast("err");
        window.setTimeout(() => setCopyToast("none"), 1500);
      });
  };

  // 截到最近 N 条 + 只留 user / assistant。useMemo 防 messages 引用稳定时
  // 不必重算（useChat 在每次 setMessages 时返回新数组所以会变，但中间
  // 没变化的渲染仍命中 memo）。
  const visibleItems = useMemo(() => {
    const items = messages.filter(
      (m) => m.role === "user" || m.role === "assistant",
    );
    if (items.length <= MINI_CHAT_MAX_ITEMS) return items;
    return items.slice(items.length - MINI_CHAT_MAX_ITEMS);
  }, [messages]);

  /// 搜索命中：visibleItems 内 text 含 keyword 的 idx 列表。空 keyword 返
  /// 空数组（UI 自动隐藏 counter / 高亮）。case-insensitive。
  const searchHits = useMemo(() => {
    const q = searchQuery.trim().toLowerCase();
    if (!q) return [] as number[];
    const out: number[] = [];
    visibleItems.forEach((m, idx) => {
      const text = extractText(m.content).toLowerCase();
      if (text.includes(q)) out.push(idx);
    });
    return out;
  }, [searchQuery, visibleItems]);

  // hits 变化时 clamp active hit idx：keyword 改 / messages 流走老 hit
  // 后，active 不能落在 hits 之外。空 hits 仍归 0（counter 不显，无副作用）。
  useEffect(() => {
    if (searchHits.length === 0) {
      if (searchActiveHitIdx !== 0) setSearchActiveHitIdx(0);
      return;
    }
    if (searchActiveHitIdx >= searchHits.length) setSearchActiveHitIdx(0);
  }, [searchHits, searchActiveHitIdx]);

  // ⌘F / Ctrl+F → 打开搜索条 + autofocus input。Tauri WKWebView 没有原生
  // 浏览器查找页 UI，所以 preventDefault 仅是防止偶发的页内默认行为。
  useEffect(() => {
    if (!visible) return;
    const handler = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && (e.key === "f" || e.key === "F")) {
        e.preventDefault();
        setSearchOpen(true);
        window.setTimeout(() => searchInputRef.current?.focus(), 0);
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [visible]);

  // active hit 变化时把目标 bubble 滚到中间。followTail 单独逻辑不受影响
  // —— 搜索期间用户主动跳，followTail effect 不会反向把视图甩到底（其依
  // 赖在 visibleItems.length 等，不在 active hit）。
  useEffect(() => {
    if (!searchOpen) return;
    if (searchHits.length === 0) return;
    const targetItemIdx = searchHits[searchActiveHitIdx];
    if (targetItemIdx === undefined) return;
    const el = scrollRef.current?.querySelector<HTMLElement>(
      `[data-mini-idx="${targetItemIdx}"]`,
    );
    if (el) {
      el.scrollIntoView({ block: "center", behavior: "smooth" });
      // 搜索中跳到某条 = 不再 followTail（避免下一帧 streaming 来又被甩走）
      followTailRef.current = false;
    }
  }, [searchOpen, searchActiveHitIdx, searchHits]);

  const handleSearchInputKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter") {
      e.preventDefault();
      if (searchHits.length === 0) return;
      setSearchActiveHitIdx((cur) =>
        e.shiftKey
          ? (cur - 1 + searchHits.length) % searchHits.length
          : (cur + 1) % searchHits.length,
      );
    } else if (e.key === "Escape") {
      e.preventDefault();
      setSearchOpen(false);
      setSearchQuery("");
      setSearchActiveHitIdx(0);
    }
  };

  // 新消息或 streaming chunk 到达时滚到底 —— 仅在 followTail 成立时。否则
  // 用户在向上翻历史，强行滚到底会破坏阅读位置；浮标按钮承担"我要回到底"
  // 的显式选项。`requestAnimationFrame` 让滚动等到 DOM 已挂上新节点再设
  // scrollTop —— 否则 scrollHeight 还是旧值。
  useEffect(() => {
    if (!visible) return;
    if (!followTailRef.current) return;
    const el = scrollRef.current;
    if (!el) return;
    const id = requestAnimationFrame(() => {
      el.scrollTop = el.scrollHeight;
    });
    return () => cancelAnimationFrame(id);
  }, [visibleItems.length, currentResponse, isLoading, visible]);

  // 复制菜单 outside-click 关闭。任何不在 popover 内的鼠标按下 / Esc 都关；
  // popover 内自身的按钮 click 走 stopPropagation 防误触。
  useEffect(() => {
    if (!copyMenuOpen) return;
    const close = () => setCopyMenuOpen(false);
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setCopyMenuOpen(false);
    };
    window.addEventListener("mousedown", close);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", close);
      window.removeEventListener("keydown", onKey);
    };
  }, [copyMenuOpen]);

  // ⌘+C 全局快捷键：选区为空时拦截 ⌘+C 复制最近 assistant 消息。selection 非空
  // → 不拦截，让浏览器默认复制选词；从而"用户精确选了某段就照原意复制，否则
  // 一键拿最新回复"两条路径都顺手。仅在 visible（桌面态）下挂监听，避免 panel
  // 切到时误抢 ⌘+C。
  useEffect(() => {
    if (!visible) return;
    const handler = (e: KeyboardEvent) => {
      if (!(e.metaKey || e.ctrlKey)) return;
      if (e.key !== "c" && e.key !== "C") return;
      // 输入框 / 任何 textarea 聚焦 + 有 selection 时不拦截 —— 用户在 compose，
      // 大概率想复制自己刚选的内容。
      const sel = window.getSelection?.()?.toString() ?? "";
      if (sel.length > 0) return;
      // 找最近 assistant 消息文本。空 → 也不拦截（避免 webview 内"安全网"
      // 行为被剥夺）。
      const lastAssistant = [...visibleItems]
        .reverse()
        .find((m) => m.role === "assistant");
      if (!lastAssistant) return;
      const text = extractText(lastAssistant.content);
      if (!text) return;
      e.preventDefault();
      navigator.clipboard
        .writeText(text)
        .then(() => {
          setCopyToast("done");
          window.setTimeout(() => setCopyToast("none"), 1500);
        })
        .catch((err) => {
          console.error("⌘+C copy failed:", err);
          setCopyToast("err");
          window.setTimeout(() => setCopyToast("none"), 1500);
        });
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [visible, visibleItems]);

  // Shift+G 跳到 mini chat 底（vim 风格"jump to end"）。input / textarea /
  // contenteditable 聚焦时不拦截 —— 用户在打字，不应该被快捷键抢走。仅 visible
  // 期间挂监听，与 ⌘+C 同生命周期。
  useEffect(() => {
    if (!visible) return;
    const handler = (e: KeyboardEvent) => {
      // 仅响应 Shift+G（实际产出 key === "G"，无 ctrl/meta/alt）—— 严格匹配
      // vim 习惯避免与 `g` / `Ctrl+G` 等其它快捷冲突。
      if (e.key !== "G") return;
      if (e.ctrlKey || e.metaKey || e.altKey) return;
      const ae = document.activeElement as HTMLElement | null;
      if (
        ae &&
        (ae.tagName === "INPUT" ||
          ae.tagName === "TEXTAREA" ||
          ae.isContentEditable)
      )
        return;
      const el = scrollRef.current;
      if (!el) return;
      e.preventDefault();
      el.scrollTop = el.scrollHeight;
      followTailRef.current = true;
      setNotAtBottom(false);
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [visible]);

  // streaming 中按 Esc → 取消生成（soft cancel）。仅 visible + isLoading +
  // onCancel 注入时挂监听；input / textarea 聚焦时不拦截（用户在 typing，Esc
  // 通常是清空输入而非取消生成）。无 modifier 键单 Esc。
  useEffect(() => {
    if (!visible) return;
    if (!isLoading) return;
    if (!onCancel) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      if (e.ctrlKey || e.metaKey || e.altKey || e.shiftKey) return;
      const ae = document.activeElement as HTMLElement | null;
      if (
        ae &&
        (ae.tagName === "INPUT" ||
          ae.tagName === "TEXTAREA" ||
          ae.isContentEditable)
      )
        return;
      e.preventDefault();
      onCancel();
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [visible, isLoading, onCancel]);

  if (!visible) return null;

  // 反馈按钮（👍）挂在「最新那一条 assistant」上。streaming 中或 caller 不
  // 传 onLike 时不挂（避免误点未读完的内容写反馈）。
  const lastIdx = visibleItems.length - 1;
  const lastMsg = lastIdx >= 0 ? visibleItems[lastIdx] : null;
  const showFeedbackOnLast =
    !!lastMsg && lastMsg.role === "assistant" && !isLoading && !!onLike;

  const showStreamingBubble = isLoading && currentResponse.trim().length > 0;

  // 跳到底浮标的点击：滚到底 + 重置 followTail。
  const handleJumpToBottom = (e: React.MouseEvent) => {
    e.stopPropagation();
    const el = scrollRef.current;
    if (!el) return;
    el.scrollTop = el.scrollHeight;
    followTailRef.current = true;
    setNotAtBottom(false);
  };

  // 滚动监听：判断是否贴底，同步 followTailRef + notAtBottom。程序设
  // scrollTop=scrollHeight 也会触发本回调，distFromBottom=0 → 贴底，与
  // handleJumpToBottom 设的状态一致。
  const handleScroll = () => {
    const el = scrollRef.current;
    if (!el) return;
    const distFromBottom = el.scrollHeight - el.scrollTop - el.clientHeight;
    const atBottom = distFromBottom <= FOLLOW_BOTTOM_THRESHOLD_PX;
    followTailRef.current = atBottom;
    setNotAtBottom((prev) => (prev === !atBottom ? prev : !atBottom));
  };

  return (
    <>
      <style>{MINI_CHAT_STYLES}</style>
      {/* 容器是相对定位 wrapper，让 ⛶ / ↓ 浮标按钮可以基于它绝对定位
          而不会跑出 chat 列表区。flex: 1 让它占 Live2D 与输入框之间的全部
          剩余空间，与三段堆叠布局对齐。 */}
      <div style={{ flex: 1, position: "relative", padding: "8px 12px 0", minHeight: 0 }}>
        {/* ⌘F inline 搜索条：浮在 chat 列表顶部，不挤压列表本身（list 的
            paddingTop 用 visibility 切换的方式吸收 38px，避免空间被搜索条
            盖住）。Enter / Shift+Enter / Esc 在 input keydown 里处理。 */}
        {searchOpen && (
          <div
            onMouseDown={(e) => e.stopPropagation()}
            style={{
              position: "absolute",
              top: 12,
              left: 16,
              right: 16,
              display: "flex",
              alignItems: "center",
              gap: 4,
              background: "var(--pet-color-card)",
              border: "1px solid var(--pet-color-border)",
              borderRadius: 8,
              padding: "4px 6px",
              boxShadow: "var(--pet-shadow-sm)",
              zIndex: 14,
            }}
          >
            <input
              ref={searchInputRef}
              type="text"
              value={searchQuery}
              onChange={(e) => {
                setSearchQuery(e.target.value);
                setSearchActiveHitIdx(0);
              }}
              onKeyDown={handleSearchInputKeyDown}
              placeholder="搜历史消息（Enter 下一条 / Shift+Enter 上一条 / Esc 关）"
              style={{
                flex: 1,
                fontSize: 11,
                padding: "2px 4px",
                border: "none",
                outline: "none",
                background: "transparent",
                color: "var(--pet-color-fg)",
                minWidth: 0,
              }}
            />
            <span
              style={{
                fontSize: 10,
                color:
                  searchHits.length === 0 && searchQuery.trim()
                    ? "var(--pet-tint-red-fg)"
                    : "var(--pet-color-muted)",
                whiteSpace: "nowrap",
                fontVariantNumeric: "tabular-nums",
              }}
              title={
                searchHits.length === 0 && searchQuery.trim()
                  ? "无命中"
                  : "当前 active hit / 总命中数"
              }
            >
              {searchQuery.trim()
                ? searchHits.length === 0
                  ? "0"
                  : `${searchActiveHitIdx + 1}/${searchHits.length}`
                : ""}
            </span>
            <button
              type="button"
              onClick={() => {
                setSearchOpen(false);
                setSearchQuery("");
                setSearchActiveHitIdx(0);
              }}
              title="关闭搜索（Esc 等价）"
              aria-label="close search"
              style={{
                border: "none",
                background: "transparent",
                color: "var(--pet-color-muted)",
                cursor: "pointer",
                fontSize: 12,
                padding: "0 4px",
                lineHeight: 1,
              }}
            >
              ✕
            </button>
          </div>
        )}
        {/* ⚡ NOW 任务角标：仅在有 mark 时显，hover 弹列表显 title +
            倒计时。位置在顶部左侧，与右上角的 ⛶ / 📋 / 🔍 错开。 */}
        {nowTasks && nowTasks.size > 0 && (
          <div
            onMouseEnter={() => setNowOverlayHover(true)}
            onMouseLeave={() => setNowOverlayHover(false)}
            style={{
              position: "absolute",
              top: "14px",
              left: "20px",
              zIndex: 13,
              fontSize: 11,
              fontWeight: 600,
              padding: "2px 8px",
              borderRadius: 10,
              background: "var(--pet-tint-orange-bg)",
              color: "var(--pet-tint-orange-fg)",
              border: "1px solid var(--pet-tint-orange-fg)",
              cursor: "default",
              userSelect: "none",
              whiteSpace: "nowrap",
            }}
            title={`当前 NOW 标记的任务 ${nowTasks.size} 条 · hover 看详情`}
          >
            ⚡ NOW · {nowTasks.size}
            {nowOverlayHover && (
              <div
                style={{
                  position: "absolute",
                  top: "calc(100% + 4px)",
                  left: 0,
                  minWidth: 220,
                  maxWidth: 300,
                  background: "var(--pet-color-card)",
                  border: "1px solid var(--pet-color-border)",
                  borderRadius: 8,
                  boxShadow: "var(--pet-shadow-md)",
                  padding: 6,
                  zIndex: 14,
                  cursor: "default",
                }}
              >
                <div
                  style={{
                    fontSize: 10,
                    color: "var(--pet-color-muted)",
                    padding: "2px 6px",
                    marginBottom: 4,
                    fontWeight: 400,
                  }}
                >
                  当前专注（60s 内）
                </div>
                {/* 不动 nowTick 直接用，react useState set 触发的 re-render
                    + Date.now() 拿到最新时戳。 */}
                {(() => {
                  void nowTick; // 让 React 知道这个值参与渲染（避免 lint 误报）
                  const nowMs = Date.now();
                  const entries = Array.from(nowTasks.entries()).sort(
                    (a, b) => a[1] - b[1],
                  );
                  return entries.map(([title, expiresAt]) => {
                    const secLeft = Math.max(0, Math.ceil((expiresAt - nowMs) / 1000));
                    return (
                      <div
                        key={title}
                        style={{
                          display: "flex",
                          alignItems: "center",
                          gap: 6,
                          padding: "3px 6px",
                          fontSize: 11,
                          color: "var(--pet-color-fg)",
                        }}
                      >
                        <span
                          style={{
                            flex: 1,
                            overflow: "hidden",
                            textOverflow: "ellipsis",
                            whiteSpace: "nowrap",
                          }}
                          title={title}
                        >
                          {title}
                        </span>
                        <span
                          style={{
                            fontSize: 10,
                            color: secLeft <= 15
                              ? "var(--pet-tint-red-fg)"
                              : "var(--pet-color-muted)",
                            fontVariantNumeric: "tabular-nums",
                            fontFamily: "'SF Mono', 'Menlo', monospace",
                            fontWeight: secLeft <= 15 ? 600 : 400,
                          }}
                        >
                          {secLeft}s
                        </span>
                      </div>
                    );
                  });
                })()}
              </div>
            )}
          </div>
        )}
        {/* 「复制最近 N 条」按钮：⛶ 旁边。click 弹小菜单 3/5/10；选中后取
            最近 N 条 user/assistant 拼带角色前缀文本写剪贴板。 */}
        <div
          style={{
            position: "absolute",
            top: "14px",
            right: onOpenPanel ? "48px" : "20px",
            zIndex: 13,
          }}
        >
          <button
            type="button"
            className="pet-mini-maxbtn"
            onMouseDown={(e) => e.stopPropagation()}
            onClick={(e) => {
              e.stopPropagation();
              setCopyMenuOpen((v) => !v);
            }}
            title="复制最近 N 条对话到剪贴板（弹菜单选 N）"
            aria-label="copy recent messages"
            style={{
              width: "20px",
              height: "20px",
              borderRadius: "50%",
              border: "1px solid var(--pet-color-border)",
              background: "var(--pet-color-card)",
              color: "var(--pet-color-muted)",
              fontSize: "10px",
              lineHeight: 1,
              cursor: "pointer",
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              padding: 0,
              boxShadow: "var(--pet-shadow-sm)",
            }}
          >
            📋
          </button>
          {copyMenuOpen && (
            <div
              // mousedown stopPropagation：让 popover 内部的鼠标按下不触发外层
              // useEffect 里的"outside-click 关闭"监听。button onClick 后还能
              // 正常 fire 选中处理逻辑。
              onMouseDown={(e) => e.stopPropagation()}
              onClick={(e) => e.stopPropagation()}
              style={{
                position: "absolute",
                top: "26px",
                right: 0,
                background: "var(--pet-color-card)",
                border: "1px solid var(--pet-color-border)",
                borderRadius: 6,
                boxShadow: "var(--pet-shadow-md)",
                padding: 4,
                display: "flex",
                flexDirection: "column",
                gap: 2,
                minWidth: 80,
                animation: "pet-mini-chat-fade-in 140ms ease-out",
              }}
            >
              <div
                style={{
                  fontSize: 9,
                  color: "var(--pet-color-muted)",
                  padding: "2px 6px",
                  textTransform: "uppercase",
                  letterSpacing: 0.5,
                }}
              >
                复制最近
              </div>
              {[3, 5, 10].map((n) => (
                <button
                  key={n}
                  type="button"
                  onClick={() => copyRecentN(n)}
                  style={{
                    padding: "4px 8px",
                    fontSize: 11,
                    background: "transparent",
                    border: "none",
                    color: "var(--pet-color-fg)",
                    cursor: "pointer",
                    textAlign: "left",
                    borderRadius: 4,
                  }}
                  onMouseOver={(e) => {
                    (e.currentTarget as HTMLButtonElement).style.background =
                      "var(--pet-color-bg)";
                  }}
                  onMouseOut={(e) => {
                    (e.currentTarget as HTMLButtonElement).style.background =
                      "transparent";
                  }}
                >
                  {n} 条
                </button>
              ))}
              {/* 带时间戳开关：勾上下次点 3/5/10 时每条带 [HH:MM] 前缀。老
                  session 没 ts 的退回 [?] —— 提醒用户那是历史，不是 now 的输出。 */}
              <label
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 4,
                  fontSize: 10,
                  color: "var(--pet-color-muted)",
                  padding: "4px 6px",
                  borderTop: "1px solid var(--pet-color-border)",
                  marginTop: 2,
                  cursor: "pointer",
                }}
                title="勾选后每条前面带 [HH:MM] 时间戳。session 加载回来的旧消息没时间，显 [?] 不会乱估"
              >
                <input
                  type="checkbox"
                  checked={copyIncludeTime}
                  onChange={(e) => setCopyIncludeTime(e.target.checked)}
                  style={{ margin: 0 }}
                />
                带时间戳
              </label>
            </div>
          )}
        </div>
        {/* 「最大化」按钮：固定在 mini chat 容器右上角内侧。点击调用
            onOpenPanel —— 替代过去 ChatPanel 底栏的 💬 按钮。 */}
        {onOpenPanel && (
          <button
            type="button"
            className="pet-mini-maxbtn"
            onClick={(e) => {
              e.stopPropagation();
              onOpenPanel();
            }}
            title="在面板中打开聊天（看完整历史 / 多会话切换）"
            aria-label="open panel chat"
            style={{
              position: "absolute",
              top: "14px",
              right: "20px",
              width: "20px",
              height: "20px",
              borderRadius: "50%",
              border: "1px solid var(--pet-color-border)",
              background: "var(--pet-color-card)",
              color: "var(--pet-color-muted)",
              fontSize: "11px",
              lineHeight: 1,
              cursor: "pointer",
              zIndex: 12,
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              padding: 0,
              boxShadow: "var(--pet-shadow-sm)",
            }}
          >
            ⛶
          </button>
        )}
        <div
          className="pet-mini-chat"
          ref={scrollRef}
          onScroll={handleScroll}
          style={{
            height: "100%",
            overflowY: "auto",
            padding: "8px 10px",
            background: "var(--pet-color-card)",
            borderRadius: "12px",
            border: "1px solid var(--pet-color-border)",
            fontSize: "12px",
            lineHeight: "1.5",
            color: "var(--pet-color-fg)",
            boxShadow: "var(--pet-shadow-md)",
            animation: "pet-mini-chat-fade-in 220ms ease-out",
            boxSizing: "border-box",
          }}
        >
        {visibleItems.map((m, idx) => {
          const isLast = idx === lastIdx;
          const isAssistant = m.role === "assistant";
          const text = extractText(m.content);
          const imgs = extractImages(m.content);
          const hasImg = imgs.length > 0;
          const isSearchHit = searchHits.includes(idx);
          const isActiveSearchHit =
            isSearchHit && searchHits[searchActiveHitIdx] === idx;
          const isCopied = bubbleCopyIdx === idx;
          // 时间戳角标：仅在 ts 解析有效（formatBubbleTimestamp 非 `[?]`）时
          // 渲染。位置 absolute 浮在 bubble 上方靠 bubble 对齐方向那一侧，
          // 与既有 absolute 顶部 👍 反馈块（top:-4 right:0 仅最新行）错位
          // —— 这里用 top:-12 让两者不重叠。
          const timeLabel = formatBubbleTimestamp(m.ts);
          const hasValidTime = timeLabel !== "[?]";
          // 单条复制按钮：user 行靠 bubble 左侧；assistant 行靠 bubble 右侧
          // （与 bubble 对齐方向相反 = 不挤屏幕边）。仅 text 非空显示——纯
          // 图片的 bubble 不需要 text copy（用户可以 lightbox 内复制）。
          const copyBtn = text ? (
            <button
              type="button"
              className="pet-mini-row-copy"
              onClick={(e) => {
                e.stopPropagation();
                handleBubbleCopy(idx, text);
              }}
              title={isCopied ? "已复制到剪贴板" : "复制这条消息"}
              aria-label="copy this message"
              style={{
                alignSelf: "flex-end",
                opacity: isCopied ? 1 : undefined,
                color: isCopied ? "var(--pet-tint-green-fg)" : undefined,
                whiteSpace: "nowrap",
                flexShrink: 0,
              }}
            >
              {isCopied ? "✓" : "📋"}
            </button>
          ) : null;
          // 针对这条 assistant 消息再问：dispatch CustomEvent，让 ChatPanel
          // 监听后把 `关于「<excerpt 30 字>」` 拼到 textarea 现有内容前
          // （prefix 而非 append，让锚点在最前面更显眼）。仅 assistant + 非
          // 空 text 渲染；纯图 bubble 没法当文字 anchor。
          const respondBtn =
            isAssistant && text ? (
              <button
                type="button"
                className="pet-mini-row-copy"
                onClick={(e) => {
                  e.stopPropagation();
                  const excerpt = text.length > 30 ? text.slice(0, 30) + "…" : text;
                  window.dispatchEvent(
                    new CustomEvent("pet-mini-respond-to", { detail: excerpt }),
                  );
                }}
                title="针对这条 assistant 消息再问（在输入框前缀 关于「...」）"
                aria-label="respond to this message"
                style={{
                  alignSelf: "flex-end",
                  whiteSpace: "nowrap",
                  flexShrink: 0,
                }}
              >
                💭
              </button>
            ) : null;
          return (
            <div
              key={`${m.role}-${idx}-${text.length}-${imgs.length}`}
              className="pet-mini-row"
              data-mini-idx={idx}
              style={{
                display: "flex",
                justifyContent: m.role === "user" ? "flex-end" : "flex-start",
                alignItems: "flex-end",
                gap: 3,
                marginBottom: 6,
                position: "relative",
              }}
            >
              {/* hover-only 时间戳角标。bubble 上方浮 absolute；user 行靠右
                  / assistant 行靠左 —— 与 bubble 对齐方向同侧（time 是 bubble
                  的"附加信息"，靠 bubble 自身一边更直观）。 */}
              {hasValidTime && (
                <span
                  className="pet-mini-row-time"
                  style={{
                    position: "absolute",
                    top: -12,
                    [m.role === "user" ? "right" : "left"]: 8,
                    fontSize: 9,
                    color: "var(--pet-color-muted)",
                    fontFamily: "'SF Mono', 'Menlo', monospace",
                    whiteSpace: "nowrap",
                    pointerEvents: "none",
                    background: "var(--pet-color-card)",
                    padding: "0 4px",
                    borderRadius: 3,
                    lineHeight: "12px",
                  }}
                >
                  {timeLabel}
                </span>
              )}
              {/* user 右对齐 → 复制按钮在 bubble 左侧 */}
              {m.role === "user" && copyBtn}
              <div
                onDoubleClick={() => onOpenPanel?.()}
                title={
                  // hover tooltip 把时间戳 + 双击 hint 拼在一起。两条信息合
                  // 并到 title attr —— 原生 tooltip 只能挂一条，分多个不好。
                  `${formatBubbleTimestamp(m.ts)}${
                    onOpenPanel ? " · 双击进入面板聊天（看完整历史 / 多会话切换）" : ""
                  }`
                }
                style={{
                  ...bubbleStyle(m.role as "user" | "assistant"),
                  maxWidth: "85%",
                  padding: "6px 10px",
                  fontSize: "12px",
                  lineHeight: 1.45,
                  // 搜索命中：黄色外框；active hit 强化到 2px 实线 + box-shadow
                  // 让用户一眼看出"当前 Enter 跳到的是这条"。non-active hit
                  // 1px 虚线 + 浅黄，与 active 明显分级。
                  ...(isActiveSearchHit
                    ? {
                        outline: "2px solid var(--pet-tint-orange-fg)",
                        outlineOffset: -2,
                        boxShadow:
                          "0 0 0 3px color-mix(in srgb, var(--pet-tint-orange-fg) 28%, transparent)",
                      }
                    : isSearchHit
                      ? {
                          outline:
                            "1px dashed color-mix(in srgb, var(--pet-tint-orange-fg) 60%, transparent)",
                          outlineOffset: -2,
                        }
                      : {}),
                }}
              >
                {hasImg && (
                  <div
                    style={{
                      display: "flex",
                      flexWrap: "wrap",
                      gap: 4,
                      marginBottom: text ? 4 : 0,
                    }}
                  >
                    {imgs.map((src, j) => (
                      <ImageThumb
                        key={j}
                        src={src}
                        onOpen={() => setLightboxSrc(src)}
                        maxSize={96}
                      />
                    ))}
                  </div>
                )}
                {text && parseMarkdown(text)}
              </div>
              {/* assistant 左对齐 → 复制 + 再回应按钮在 bubble 右侧 */}
              {m.role === "assistant" && respondBtn}
              {m.role === "assistant" && copyBtn}
              {isLast && isAssistant && showFeedbackOnLast && (
                <div
                  onClick={(e) => e.stopPropagation()}
                  style={{
                    position: "absolute",
                    top: "-4px",
                    right: "0",
                    display: "flex",
                    alignItems: "center",
                    gap: "4px",
                    userSelect: "none",
                    background: "var(--pet-color-card)",
                    borderRadius: "10px",
                    padding: "1px 4px",
                  }}
                >
                  {onLike && (
                    <button
                      type="button"
                      className="pet-mini-bubble-like-btn"
                      aria-label="like this bubble"
                      title="给宠物点个赞（写 Liked 进 feedback_history，正向信号）"
                      onClick={(e) => {
                        e.stopPropagation();
                        onLike();
                      }}
                    >
                      👍
                    </button>
                  )}
                </div>
              )}
            </div>
          );
        })}
        {/* 思考脉冲：isLoading 但还没 chunk 到达（也没 toolStatus）时唯一可
            视提示——"我已收到，正在想"。chunk 一来 currentResponse 非空，
            showStreamingBubble true → 这条隐，streaming bubble 接班。toolStatus
            非空时下面已有"✅ X done"行，不再叠这条避免冗余。 */}
        {isLoading && !showStreamingBubble &&
          !(toolStatus && toolStatus.trim().length > 0) && (
          <div
            style={{
              display: "flex",
              justifyContent: "flex-start",
              marginBottom: 6,
              alignItems: "center",
              gap: 6,
              paddingLeft: 4,
            }}
            aria-live="polite"
            aria-label="宠物正在思考"
            title="宠物正在思考中…"
          >
            <span className="pet-mini-thinking-glyph">
              {effectiveAssistantGlyph}
            </span>
            <span
              className="pet-mini-thinking-dots"
              style={{
                fontSize: 11,
                color: "var(--pet-color-muted)",
                fontStyle: "italic",
              }}
            >
              思考中
            </span>
          </div>
        )}
        {showStreamingBubble && (
          <div
            style={{
              display: "flex",
              justifyContent: "flex-start",
              marginBottom: 6,
              alignItems: "flex-end",
              gap: 4,
            }}
          >
            <div
              style={{
                ...bubbleStyle("assistant"),
                maxWidth: "85%",
                padding: "6px 10px",
                fontSize: "12px",
                lineHeight: 1.45,
                opacity: 0.85,
                fontStyle: "italic",
              }}
            >
              {parseMarkdown(currentResponse)}
            </div>
            {/* Esc 取消 hint：streaming + onCancel 注入时显，让用户知道有这
                个快捷键。pointerEvents none 不挡 click；与 streaming bubble
                同 baseline，跟着 stream 自然滚动。 */}
            {onCancel && (
              <span
                style={{
                  fontSize: 10,
                  color: "var(--pet-color-muted)",
                  background: "var(--pet-color-card)",
                  border: "1px solid var(--pet-color-border)",
                  borderRadius: 4,
                  padding: "1px 6px",
                  whiteSpace: "nowrap",
                  fontFamily: "'SF Mono', 'Menlo', monospace",
                  pointerEvents: "none",
                  flexShrink: 0,
                }}
                title="按 Esc 取消生成（已写出的内容保留）"
              >
                Esc 取消
              </span>
            )}
          </div>
        )}
        {/* tool 状态行：宠物在调工具时 streaming chunk 流暂停，这条小字告诉
            用户"还在执行 X 不是卡死"。仅 isLoading + toolStatus 非空时显。 */}
        {isLoading && toolStatus && toolStatus.trim().length > 0 && (
          <div
            style={{
              fontSize: 10,
              color: "var(--pet-color-muted)",
              fontStyle: "italic",
              padding: "2px 6px",
              marginBottom: 4,
              userSelect: "none",
            }}
            title={`正在执行工具：${toolStatus}`}
          >
            {toolStatus}
          </div>
        )}
        </div>
        {/* 跳到底浮标：用户向上滚翻历史时显。绝对定位在 wrapper 内的右
            下角，点击滚到底 + 重启 follow-tail。 */}
        {notAtBottom && (
          <button
            type="button"
            onClick={handleJumpToBottom}
            title="跳到最新（点后新消息会自动跟随）"
            aria-label="jump to bottom"
            style={{
              position: "absolute",
              right: "20px",
              bottom: "12px",
              width: "28px",
              height: "28px",
              borderRadius: "50%",
              border: "1px solid var(--pet-color-accent)",
              background: "var(--pet-color-card)",
              color: "var(--pet-color-accent)",
              fontSize: "14px",
              lineHeight: 1,
              cursor: "pointer",
              zIndex: 11,
              boxShadow: "var(--pet-shadow-md)",
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              padding: 0,
              animation: "pet-mini-chat-fade-in 180ms ease-out",
            }}
          >
            ↓
          </button>
        )}
      </div>
      <ImageLightbox src={lightboxSrc} onClose={() => setLightboxSrc(null)} />
      {/* ⌘+C 快捷复制反馈：屏幕中心稍上的小气泡，pointerEvents none 让它不
          挡用户操作；1.5s 自清。fade-in 复用 mini-chat 同款 keyframes。 */}
      {copyToast !== "none" && (
        <div
          style={{
            position: "fixed",
            top: 24,
            left: "50%",
            transform: "translateX(-50%)",
            padding: "6px 14px",
            borderRadius: 999,
            fontSize: 12,
            color: "#fff",
            background:
              copyToast === "done"
                ? "color-mix(in srgb, var(--pet-tint-green-fg) 92%, transparent)"
                : "color-mix(in srgb, var(--pet-tint-red-fg) 92%, transparent)",
            boxShadow: "var(--pet-shadow-md)",
            zIndex: 80,
            pointerEvents: "none",
            animation: "pet-mini-chat-fade-in 140ms ease-out",
          }}
        >
          {copyToast === "done" ? "✓ 已复制最近回复" : "✗ 复制失败"}
        </div>
      )}
    </>
  );
}
