import { useState, useEffect, useMemo, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { openPath } from "@tauri-apps/plugin-opener";
import { renderContentWithTaskRefs } from "./panelChatBits";
import { EmptyState } from "./EmptyState";
import { LoadingState } from "./LoadingState";
import { Modal } from "./Modal";

interface MemoryItem {
  title: string;
  description: string;
  detail_path: string;
  created_at: string;
  updated_at: string;
}

interface CategoryData {
  label: string;
  items: MemoryItem[];
}

interface MemoryIndex {
  version: number;
  categories: Record<string, CategoryData>;
}

/// 类目展示顺序：活跃类目（butler_tasks / todo / ai_insights）压在最上首屏可见，
/// 长尾 / 归档（task_archive / general / user_profile）压后。task_archive 是只
/// 读归档，user_profile 是慢变档案，general 是兜底分类，三者放下面让用户先看
/// 到有动态的内容。CATEGORY_ORDER 之外的 category 由 PanelMemory 自身的
/// fallback 逻辑接到末尾。
const CATEGORY_ORDER = [
  "butler_tasks",
  "todo",
  "ai_insights",
  "task_archive",
  "general",
  "user_profile",
];

// Per-category description placeholder shown in the new/edit modal so the user knows
// what shape of entry each category expects. butler_tasks gets the most concrete
// example because it's the newest user-author category and the convention isn't yet
// learned. ai_insights warns the user it's pet-author territory — manual edits are
// allowed but unusual.
/// R118: butler_tasks schedule 语法模板。emoji 与 R80 schedule chip 配色
/// 习惯一致：每日 = 🔁 / 一次 = 📅 / 截止 = ⏳。text 末尾保留空格让用户
/// 直接写正文不需先打空格。
const SCHEDULE_TEMPLATES: Array<{ label: string; text: string }> = [
  { label: "🔁 every", text: "[every: 09:00] " },
  { label: "📅 once", text: "[once: 2026-05-10 14:00] " },
  { label: "⏳ deadline", text: "[deadline: 2026-05-10 14:00] " },
];

const CATEGORY_PLACEHOLDERS: Record<string, string> = {
  butler_tasks:
    "比如：[every: 09:00] 把今日日历汇总写到 ~/today.md\n或：[once: 2026-05-10 14:00] 周末整理 ~/Downloads（pet 在该时间点自动执行）\n或：[deadline: 2026-05-10 14:00] 把文档发出去（user 必须在那之前自己完成，pet 临近时提醒）\n或：直接写「整理 ~/Downloads，把 30 天旧文件挪到 ~/Archive」（不带前缀就由宠物自己判断时机）。\n（描述里说清楚做什么、多久做一次、写到哪里。）",
  todo: "用户提醒自己的事项。建议加前缀：\n[remind: 17:00] 喝水\n[remind: 2026-05-10 09:00] 看医生",
  user_profile: "关于用户习惯 / 偏好的稳定事实。\n比如：起床时间 通常 8:30 起床\n或：偏好 dark theme 编辑器",
  ai_insights: "宠物自己的反思 / 心情 / 长期画像，通常由 LLM 自己写。手动编辑可以，但注意 current_mood / persona_summary 是受保护的。",
  general: "其他不属于以上类别的记忆。",
};

interface PanelMemoryProps {
  /// 双击 butler_tasks item 描述里的 `「task title」` ref token → 请求父组
  /// 件切到「任务」tab 并把焦点落到该 title 的卡片上。可选 —— 不传则 ref
  /// 仍可 hover 显 status，但双击 noop。
  onRequestFocusTask?: (title: string) => void;
}

export function PanelMemory({ onRequestFocusTask }: PanelMemoryProps = {}) {
  const [index, setIndex] = useState<MemoryIndex | null>(null);
  const [loading, setLoading] = useState(true);
  const [searchKeyword, setSearchKeyword] = useState("");
  const [searchResults, setSearchResults] = useState<
    { category: string; title: string; description: string; detail_path: string }[] | null
  >(null);
  const [editingItem, setEditingItem] = useState<{
    category: string;
    title: string;
    description: string;
    isNew: boolean;
  } | null>(null);
  const [message, setMessage] = useState("");
  // 双击 inline 改 memory title。同时只允许一条 item 处于改名（多 input
  // 分散注意力）；key 用 `${catKey}::${oldTitle}` 跨 category 唯一。复用
  // 后端 memory_rename 命令（与 PanelTasks 改名同源）。
  const [renamingMemoryKey, setRenamingMemoryKey] = useState<string | null>(null);
  const [renameMemoryDraft, setRenameMemoryDraft] = useState("");
  const [renameMemoryBusy, setRenameMemoryBusy] = useState(false);
  const commitRenameMemory = async () => {
    const key = renamingMemoryKey;
    if (!key) return;
    const sep = key.indexOf("::");
    if (sep < 0) {
      setRenamingMemoryKey(null);
      setRenameMemoryDraft("");
      return;
    }
    const catKey = key.slice(0, sep);
    const oldTitle = key.slice(sep + 2);
    const newTitle = renameMemoryDraft.trim();
    if (!newTitle || newTitle === oldTitle) {
      setRenamingMemoryKey(null);
      setRenameMemoryDraft("");
      return;
    }
    setRenameMemoryBusy(true);
    try {
      await invoke("memory_rename", {
        category: catKey,
        oldTitle,
        newTitle,
      });
      // pinnedKeys 跟着改名迁移，否则旧 key 残留对新条目不生效
      setPinnedKeys((prev) => {
        if (!prev.has(key)) return prev;
        const next = new Set(prev);
        next.delete(key);
        next.add(`${catKey}::${newTitle}`);
        try {
          window.localStorage.setItem(
            "pet-memory-pinned",
            JSON.stringify([...next]),
          );
        } catch {
          // 配额满 / 私密浏览：UI state 仍生效
        }
        return next;
      });
      await loadIndex();
      setRenamingMemoryKey(null);
      setRenameMemoryDraft("");
    } catch (e) {
      setMessage(`改名失败：${e}`);
      setTimeout(() => setMessage(""), 4000);
    } finally {
      setRenameMemoryBusy(false);
    }
  };
  const cancelRenameMemory = () => {
    setRenamingMemoryKey(null);
    setRenameMemoryDraft("");
  };
  /// hover 500ms 后浮 detail.md preview 的状态。key 用 detail_path（跨 cat
  /// 唯一）。preview 缓存让用户重复 hover 不重发 IPC。
  const [previewHoverKey, setPreviewHoverKey] = useState<string | null>(null);
  const [previewCache, setPreviewCache] = useState<Record<string, string>>({});
  const previewHoverTimerRef = useRef<number | null>(null);
  const startPreviewHover = (detailPath: string) => {
    if (previewHoverTimerRef.current !== null) {
      window.clearTimeout(previewHoverTimerRef.current);
    }
    previewHoverTimerRef.current = window.setTimeout(() => {
      setPreviewHoverKey(detailPath);
      previewHoverTimerRef.current = null;
      // 缓存命中就别重发；首次 hover 才 invoke
      if (previewCache[detailPath] === undefined) {
        invoke<string>("memory_read_detail", { detailPath })
          .then((content) => {
            setPreviewCache((prev) => ({ ...prev, [detailPath]: content }));
          })
          .catch((e) => {
            setPreviewCache((prev) => ({
              ...prev,
              [detailPath]: `（读取失败：${e}）`,
            }));
          });
      }
    }, 500);
  };
  const endPreviewHover = () => {
    if (previewHoverTimerRef.current !== null) {
      window.clearTimeout(previewHoverTimerRef.current);
      previewHoverTimerRef.current = null;
    }
    setPreviewHoverKey(null);
  };
  useEffect(() => {
    return () => {
      if (previewHoverTimerRef.current !== null) {
        window.clearTimeout(previewHoverTimerRef.current);
      }
    };
  }, []);
  const [consolidating, setConsolidating] = useState(false);
  const [butlerHistory, setButlerHistory] = useState<string[]>([]);
  const [butlerDaily, setButlerDaily] = useState<string[]>([]);
  const [firingProactive, setFiringProactive] = useState(false);
  // R137: "立即处理" 二次确认 armed 态（与 R125 PanelDebug 立即开口同模式）。
  // 首点 armed 3s 自动 revert；再点真触发。firingProactive 是请求 in-flight
  // flag，与 armed 各管一半（armed 在 click 前 / firing 在 invoke 期间）。
  const [fireArmed, setFireArmed] = useState(false);
  // 单条 item "▶️ 现在跑一次"：与 fireArmed 同款二次确认，但 key 由 item
  // title 区分（多任务并存时 armed 状态各自独立）。fireOneArmed 在 3s 内
  // 不可重置成 null 让 3s 自动收回；正在 invoke 时 firingProactive 全局阻
  // 塞所有 fire 按钮，避免连点炸 LLM。
  const [fireOneArmedTitle, setFireOneArmedTitle] = useState<string | null>(null);
  const fireOneArmedTimer = useRef<number | null>(null);
  const handleFireOneTask = async (title: string) => {
    if (firingProactive) return;
    if (fireOneArmedTitle !== title) {
      setFireOneArmedTitle(title);
      if (fireOneArmedTimer.current !== null) {
        window.clearTimeout(fireOneArmedTimer.current);
      }
      fireOneArmedTimer.current = window.setTimeout(() => {
        setFireOneArmedTitle(null);
        fireOneArmedTimer.current = null;
      }, 3000);
      return;
    }
    setFireOneArmedTitle(null);
    if (fireOneArmedTimer.current !== null) {
      window.clearTimeout(fireOneArmedTimer.current);
      fireOneArmedTimer.current = null;
    }
    setFiringProactive(true);
    setMessage(`正在让宠物处理「${title}」…`);
    try {
      const status = await invoke<string>("trigger_proactive_turn_for_task", {
        title,
      });
      setMessage(status);
      await loadButlerHistory();
      await loadIndex();
    } catch (e: any) {
      setMessage(`触发失败：${e}`);
    } finally {
      setFiringProactive(false);
    }
  };
  // R95: butler 最近执行折叠状态。> 5 条时默认折叠到前 5（最新），用户点
  // "展开全部 N 条"切到 unbounded。session 内有效，关面板复位（与 R91
  // 长描述折叠同语义）。
  const [butlerHistoryExpanded, setButlerHistoryExpanded] = useState(false);
  // R143: butler 每日小结折叠状态（与 butlerHistoryExpanded 同模式独立）。
  // 长跑用户多日累积，不折叠时挤压下方任务列表。
  const [butlerDailyExpanded, setButlerDailyExpanded] = useState(false);
  // R102: 哪些 category 已被用户展开。默认 empty —— 所有 cat 走"自动折叠
  // 规则"（> 10 条时折叠到前 5）。手动 toggle 进入 set 即始终展开。
  //
  /// 类目自定义显示名：localStorage `pet-memory-cat-labels` → Record<catKey,
  /// customLabel>。仅前端展示层覆盖，不动后端 catKey；空字符串 / undefined
  /// 走 cat.label（后端默认）。双击 section 标题 → 输入框 → Enter / blur
  /// 保存。reset 行为 = 改回空字符串。
  const [categoryLabels, setCategoryLabels] = useState<Record<string, string>>(() => {
    try {
      const raw = window.localStorage.getItem("pet-memory-cat-labels");
      if (!raw) return {};
      const obj = JSON.parse(raw);
      if (obj && typeof obj === "object") return obj as Record<string, string>;
    } catch {
      // 解析失败 / localStorage 不可用 → 空 map
    }
    return {};
  });
  const setCategoryLabel = (catKey: string, label: string) => {
    setCategoryLabels((prev) => {
      const next = { ...prev };
      const trimmed = label.trim();
      if (!trimmed) delete next[catKey];
      else next[catKey] = trimmed;
      try {
        window.localStorage.setItem("pet-memory-cat-labels", JSON.stringify(next));
      } catch {
        // 私密浏览 / 配额满 — session 内仍生效
      }
      return next;
    });
  };
  /// 类目标题改名 inline 编辑状态：当前正在编辑的 catKey + 草稿值。同时只
  /// 一个 section 可编辑。
  const [renamingCatKey, setRenamingCatKey] = useState<string | null>(null);
  const [renameCatDraft, setRenameCatDraft] = useState("");

  /// 类目展示顺序自定义：localStorage `pet-memory-cat-order` → string[] of
  /// catKey。空 / 不存在 → 走 CATEGORY_ORDER 默认。拖动 section header 改
  /// 顺序时持久化用户的完整覆盖列表。不动后端 catKey / cat.label —— 是纯
  /// 前端展示偏好。dangling key（用户曾排过但 backend 删了该类目）渲染时
  /// 自然 skip。新出现的类目（用户没排过）按 CATEGORY_ORDER 默认位置 +
  /// fallback 接入。
  const [savedCatOrder, setSavedCatOrder] = useState<string[]>(() => {
    try {
      const raw = window.localStorage.getItem("pet-memory-cat-order");
      if (!raw) return [];
      const arr = JSON.parse(raw);
      if (Array.isArray(arr)) {
        return arr.filter((v): v is string => typeof v === "string");
      }
    } catch {
      // 解析失败 / localStorage 不可用 → 退默认
    }
    return [];
  });
  const persistCatOrder = (order: string[]) => {
    setSavedCatOrder(order);
    try {
      window.localStorage.setItem("pet-memory-cat-order", JSON.stringify(order));
    } catch {
      // 容量满 / 私密浏览 —— session 内仍生效
    }
  };
  /// 拖拽 reorder section：拖 source catKey + hover over target catKey。
  /// 仅 sectionTitle 左侧 "⋮⋮" handle draggable，外层 section div listen
  /// onDragOver/Drop。
  const [dragSrcCat, setDragSrcCat] = useState<string | null>(null);
  const [dragOverCat, setDragOverCat] = useState<string | null>(null);

  // 持久化：与 pinnedKeys 同 localStorage 模式。用户对"哪几个 category 我
  // 总想展开看全部"是稳定偏好（如 todo / butler_tasks 看高频，其它折叠看
  // 标题足够），跨重启保留减少每次都要再点开的摩擦。
  const [expandedCategories, setExpandedCategories] = useState<Set<string>>(() => {
    try {
      const raw = window.localStorage.getItem("pet-memory-expanded-cats");
      if (!raw) return new Set();
      const arr = JSON.parse(raw);
      if (Array.isArray(arr)) {
        return new Set(arr.filter((v): v is string => typeof v === "string"));
      }
    } catch {
      // 解析失败 / localStorage 不可用：退到 empty Set（与原默认一致）
    }
    return new Set();
  });
  /// butler_tasks 段 schedule kind 过滤：Set 内值为 "every" / "once" /
  /// "deadline" / "none"（合成 sentinel，含义 "无 schedule 前缀"）。空 Set
  /// = 不过滤；非空 = OR 命中（item.kind 或 "none" 命中则通过）。session 内
  /// 状态，不持久化 —— 过滤是即时阅读偏好，下次打开 panel 自然回到全显。
  const [butlerScheduleFilter, setButlerScheduleFilter] = useState<Set<string>>(
    new Set(),
  );
  /// "✏️ 改 schedule" 内联 modal 状态。装载完用户当前的 parsed schedule +
  /// topic（topic 用于拼新 description）。保存时 build new prefix + topic
  /// 走 memory_edit update。null = 不打开。
  type EditScheduleDraft = {
    title: string;
    /// 完整 description（含原 prefix + topic），保存时用 parsed 重建
    description: string;
    /// 新值 string 形式（"HH:MM" / "YYYY-MM-DD" / "HH:MM"）方便直接绑
    /// input value；保存时统一 parse。
    kind: "every" | "once" | "deadline";
    date: string; // YYYY-MM-DD（仅 once / deadline 用）
    time: string; // HH:MM
  };
  const [editScheduleDraft, setEditScheduleDraft] =
    useState<EditScheduleDraft | null>(null);
  const [editScheduleBusy, setEditScheduleBusy] = useState(false);
  /// modal 内 date / time input refs：kind 切换后自动 focus 对应输入框，
  /// 让用户少敲一次 tab。useEffect 监听 draft.kind 变化。
  const editScheduleDateRef = useRef<HTMLInputElement>(null);
  const editScheduleTimeRef = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (!editScheduleDraft) return;
    // setTimeout 0 等 React commit（date input 在 kind="every" 时被 conditional
    // 渲染撤掉 / 添回，立即 focus 会拿到 null）
    window.setTimeout(() => {
      if (editScheduleDraft.kind === "every") {
        editScheduleTimeRef.current?.focus();
      } else {
        editScheduleDateRef.current?.focus();
      }
    }, 0);
  }, [editScheduleDraft?.kind, editScheduleDraft?.title]);
  const toggleButlerSchedule = (kind: string) => {
    setButlerScheduleFilter((prev) => {
      const next = new Set(prev);
      if (next.has(kind)) next.delete(kind);
      else next.add(kind);
      return next;
    });
  };
  /// pin 集合：`${catKey}::${title}`。pin 的项在 category 内排首位（同 pin
  /// 之间保留原 backend 顺序）。仅前端 localStorage 持久化，不动 memory
  /// 文件 frontmatter —— pin 是用户的 UI 偏好，不应改变 LLM 看到的内容。
  /// 同名跨类目不冲突（key 含 catKey），重命名 / 删除 memory 后 key 会变
  /// 成 dangling 但无副作用（sort 时找不到照原序）。
  const [pinnedKeys, setPinnedKeys] = useState<Set<string>>(() => {
    try {
      const raw = window.localStorage.getItem("pet-memory-pinned");
      if (!raw) return new Set();
      const arr = JSON.parse(raw);
      return new Set(Array.isArray(arr) ? arr : []);
    } catch {
      return new Set();
    }
  });
  const togglePin = (catKey: string, title: string) => {
    const key = `${catKey}::${title}`;
    setPinnedKeys((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      try {
        window.localStorage.setItem(
          "pet-memory-pinned",
          JSON.stringify([...next]),
        );
      } catch {
        // 私密浏览 / 容量满 等 —— UI state 仍生效，下次 reload 才丢
      }
      return next;
    });
  };
  // R118: butler_tasks schedule 模板插入用 ref 拿 textarea 光标位置。仅
  // butler_tasks category 模板按钮可见时使用。
  const descTextareaRef = useRef<HTMLTextAreaElement>(null);
  // 顶部 memory_search input 的 DOM ref —— ⌘F / Ctrl+F 全局快捷键聚焦
  // 用。与 PanelTasks 同款 UX（与 mac Finder / 浏览器 / Notion 直觉一致）。
  const searchInputRef = useRef<HTMLInputElement>(null);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      // ⌘F / Ctrl+F 在 panel 内任何位置（含 INPUT / TEXTAREA / BUTTON）
      // 都拦下并聚焦 search input；浏览器原生 ⌘F 在 webview 里几乎无用
      // （这是单页应用不是文档），抢走更直观。Shift / Alt 修饰位避开
      // 防止误触组合键。
      if (
        (e.metaKey || e.ctrlKey) &&
        !e.shiftKey &&
        !e.altKey &&
        e.key.toLowerCase() === "f"
      ) {
        e.preventDefault();
        const el = searchInputRef.current;
        if (el) {
          el.focus();
          el.select();
        }
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
  // R140: 全局记忆总数。搜索结果 badge 显 N/M，让用户感知搜词命中率。
  // 复用 R98 导出 helper 同款 reduce sum 模式；依赖 index，index 切换时
  // 自动重算。
  const totalMemoryCount = useMemo(() => {
    if (!index) return 0;
    return Object.values(index.categories).reduce(
      (sum, c) => sum + c.items.length,
      0,
    );
  }, [index]);


  const loadIndex = async () => {
    try {
      const data = await invoke<MemoryIndex>("memory_list", {});
      setIndex(data);
    } catch (e: any) {
      console.error("Failed to load memories:", e);
    } finally {
      setLoading(false);
    }
  };

  // memory 目录磁盘占用：与 totalItems 一起在头部显，让用户感知何时该
  // consolidate。null = 还没拉过或失败。挂载时拉一次；用户做 edit / consolidate
  // 后不强刷（不是高频精确数据）。
  const [diskUsage, setDiskUsage] = useState<{
    total_bytes: number;
    file_count: number;
  } | null>(null);
  useEffect(() => {
    invoke<{ total_bytes: number; file_count: number }>("memory_disk_usage")
      .then(setDiskUsage)
      .catch((e) => console.error("memory_disk_usage failed:", e));
  }, []);

  /// item 行的 "detail X 字" 小灰字指示 —— 一次性拉所有 detail.md 的
  /// char count，挂载 + index 变化后刷新（用户做 edit / consolidate / fire
  /// 之后立刻反映新字数）。失败容忍：单文件读不到的 path 不进 map → 该
  /// item 不显字数。空 map / 0 字 / 路径缺失三种都按"不渲染"处理。
  const [detailSizes, setDetailSizes] = useState<Record<string, number>>({});
  const refreshDetailSizes = useCallback(async () => {
    try {
      const sizes = await invoke<Record<string, number>>("memory_detail_sizes");
      setDetailSizes(sizes);
    } catch {
      // 后端命令缺失（兼容旧版）/ memories_dir 异常 → 保留旧 map，不退化
      // 全部 indicator（仅未刷新到，不报错给用户）
    }
  }, []);
  useEffect(() => {
    void refreshDetailSizes();
  }, [refreshDetailSizes, index]);

  const loadButlerHistory = async () => {
    try {
      const lines = await invoke<string[]>("get_butler_history", { n: 5 });
      setButlerHistory(lines);
    } catch (e: any) {
      console.error("Failed to load butler history:", e);
    }
  };

  const loadButlerDaily = async () => {
    try {
      const lines = await invoke<string[]>("get_butler_daily_summaries", { n: 7 });
      setButlerDaily(lines);
    } catch (e: any) {
      console.error("Failed to load butler daily summaries:", e);
    }
  };

  // R110: 编辑 modal 打开时全局 Esc 关闭。挂 window 而非 modal 内 —— 让无
  // 论 focus 在 textarea / input / select / modal 空白处都能捕获。!editingItem
  // 短路返回让 modal 关时不挂 listener，cleanup 自动清。
  useEffect(() => {
    if (!editingItem) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        setEditingItem(null);
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [editingItem]);

  useEffect(() => {
    loadIndex();
    loadButlerHistory();
    loadButlerDaily();
    // Refresh history every 15s while panel is open. butler events come from LLM
    // tool calls in proactive turns, which fire at minute scale — 15s polling is
    // cheap and gives "I just saw the pet act on my task" feedback within seconds.
    // Daily summaries change at most once per consolidate run (hours apart) but the
    // poll is cheap so we just piggyback on the same interval.
    const t = setInterval(() => {
      loadButlerHistory();
      loadButlerDaily();
    }, 15_000);
    return () => clearInterval(t);
  }, []);

  // ---- Iter Cθ + R80: schedule-aware rendering for butler_tasks items ---------
  // Pure TS mirror of proactive.rs::parse_butler_schedule_prefix +
  // parse_butler_deadline_prefix + is_butler_due. Lets the panel render
  // `[every: HH:MM]` / `[once: ...]` / `[deadline: ...]` (R80) as chips
  // and flag due / urgent tasks in real time, instead of users needing
  // to do the math themselves.
  type ButlerSchedule =
    | { kind: "every"; hour: number; minute: number }
    | { kind: "once"; year: number; month: number; day: number; hour: number; minute: number }
    | { kind: "deadline"; year: number; month: number; day: number; hour: number; minute: number };

  const parseButlerSchedule = (desc: string): { schedule: ButlerSchedule; topic: string } | null => {
    const trimmed = desc.replace(/^\s+/, "");
    const m = trimmed.match(/^\[(every|once|deadline):\s*([^\]]+)\]\s*(.*)$/);
    if (!m) return null;
    const [, kind, body, topic] = m;
    if (!topic.trim()) return null;
    if (kind === "every") {
      const hm = body.trim().match(/^(\d{1,2}):(\d{1,2})$/);
      if (!hm) return null;
      const hour = Number(hm[1]);
      const minute = Number(hm[2]);
      if (hour > 23 || minute > 59) return null;
      return { schedule: { kind: "every", hour, minute }, topic: topic.trim() };
    }
    // once / deadline share the same YYYY-MM-DD HH:MM body shape.
    const dt = body.trim().match(/^(\d{4})-(\d{2})-(\d{2})\s+(\d{1,2}):(\d{1,2})$/);
    if (!dt) return null;
    return {
      schedule: {
        kind: kind as "once" | "deadline",
        year: Number(dt[1]),
        month: Number(dt[2]),
        day: Number(dt[3]),
        hour: Number(dt[4]),
        minute: Number(dt[5]),
      },
      topic: topic.trim(),
    };
  };

  // Iter R80: TS mirror of compute_deadline_urgency. Returns urgency tier
  // for [deadline:] tasks so panel can color-code by tier (matches R77/R78
  // semantics: > 6h = distant, 1-6h = approaching, < 1h = imminent, past = overdue).
  type DeadlineUrgency = "distant" | "approaching" | "imminent" | "overdue";
  const computeDeadlineUrgency = (
    schedule: Extract<ButlerSchedule, { kind: "deadline" }>,
    now: Date,
  ): DeadlineUrgency => {
    const target = new Date(
      schedule.year,
      schedule.month - 1,
      schedule.day,
      schedule.hour,
      schedule.minute,
    );
    if (now >= target) return "overdue";
    const diffHours = (target.getTime() - now.getTime()) / 3_600_000;
    if (diffHours >= 6) return "distant";
    if (diffHours >= 1) return "approaching";
    return "imminent";
  };

  const mostRecentFire = (schedule: ButlerSchedule, now: Date): Date | null => {
    if (schedule.kind === "once" || schedule.kind === "deadline") {
      // deadline shares the same "fire at this absolute moment" date shape
      // as once for scheduling purposes; due-ness/urgency come from urgency
      // computer for deadline (we don't gate it via mostRecentFire / isButlerDue).
      const target = new Date(
        schedule.year,
        schedule.month - 1,
        schedule.day,
        schedule.hour,
        schedule.minute,
      );
      return now >= target ? target : null;
    }
    const targetToday = new Date(
      now.getFullYear(),
      now.getMonth(),
      now.getDate(),
      schedule.hour,
      schedule.minute,
    );
    return now >= targetToday ? targetToday : new Date(targetToday.getTime() - 24 * 3600 * 1000);
  };

  const isButlerDue = (schedule: ButlerSchedule, lastUpdated: string, now: Date): boolean => {
    const fire = mostRecentFire(schedule, now);
    if (!fire) return false;
    const last = lastUpdated ? new Date(lastUpdated) : null;
    const lastValid = last && !isNaN(last.getTime()) ? last : null;
    return !lastValid || lastValid < fire;
  };

  // Iter Cκ: how long the task has been overdue, in minutes since most_recent_fire.
  // Returns null when not due / no fire yet. Only meaningful for due tasks; UI gates
  // on the indicator threshold to avoid spamming "等了 1m" on tasks that just hit.
  const overdueMinutes = (schedule: ButlerSchedule, now: Date): number | null => {
    const fire = mostRecentFire(schedule, now);
    if (!fire) return null;
    return Math.floor((now.getTime() - fire.getTime()) / 60_000);
  };

  // Iter Cπ: TS mirror of Rust's `has_butler_error`. Marker is "[error" anywhere
  // in description — LLM prepends `[error: brief reason]` after a tool failure
  // during execution. Substring check matches case-sensitively to keep parity
  // with Rust side. Returns `(hasError, reason)` where reason is the body of
  // `[error: <body>]`, or empty string when format is just `[error]`.
  /// `[done]` / `[done ...]` 标记的存在性判定 + 配套 `[result: ...]` 抽取。
  /// 后端 `has_done_marker` 与 TaskView.status="Done" 同语义；前端独立 parse
  /// 是因为 MemoryItem 只有 raw description，没经过 TaskView 转换。result
  /// 段是 LLM 在标 done 时常附的"产物摘要"，截 30 字进 chip 显眼但不撑长。
  const parseButlerDone = (desc: string): { isDone: boolean; result: string } => {
    // 匹配 `[done` 后接 `]` 或 ` ...]`（容忍 `[done at=...]` 之类未来扩展），
    // 但拒绝未闭合 `[done...`。与后端 has_done_marker 行为对齐。
    const doneMatch = desc.match(/\[done(?:\]|\s[^\]]*\])/);
    if (!doneMatch) return { isDone: false, result: "" };
    const rm = desc.match(/\[result\s*[:：]?\s*([^\]]*)\]/);
    const result = rm ? rm[1].trim() : "";
    return { isDone: true, result };
  };
  const parseButlerError = (desc: string): { hasError: boolean; reason: string } => {
    const idx = desc.indexOf("[error");
    if (idx < 0) return { hasError: false, reason: "" };
    // Look for the closing bracket of the [error...] block; if missing, still
    // treat as errored (we trust the LLM wrote a marker even if malformed).
    const end = desc.indexOf("]", idx);
    if (end < 0) return { hasError: true, reason: "" };
    const inner = desc.slice(idx + "[error".length, end);
    // Strip leading colon + whitespace to get the human reason.
    const reason = inner.replace(/^[:\s]+/, "").trim();
    return { hasError: true, reason };
  };

  /// butler_tasks 描述里 `「title」` ref token → 渲 hover preview + 双击导
  /// 航用的 taskRefMap。本面板已经持有 butler_tasks index，无需额外 IO；
  /// status 用既有 parseButlerError / parseButlerDone 推断（与 chip 视觉
  /// 同语义）。其它 category 描述里偶有 ref 也用同一份 map（按全局 task
  /// 全集解析），共用 1 个 hover lookup 空间。
  // eslint-disable-next-line react-hooks/exhaustive-deps
  const refTaskMap = useMemo(() => {
    const out: Record<string, { status: string; updated_at: string }> = {};
    const items = index?.categories.butler_tasks?.items ?? [];
    for (const it of items) {
      let status = "pending";
      if (parseButlerError(it.description).hasError) status = "error";
      else if (parseButlerDone(it.description).isDone) status = "done";
      out[it.title] = { status, updated_at: it.updated_at };
    }
    return out;
  }, [index]);

  const formatOverdue = (mins: number): string => {
    if (mins < 60) return `等了 ${mins}m`;
    const h = Math.floor(mins / 60);
    const m = mins % 60;
    return m === 0 ? `等了 ${h}h` : `等了 ${h}h${m}m`;
  };
  // Threshold above which a due task gets a visible "等了..." chip. 60 min = 1 hour
  // — short enough to surface a forgotten task before the user notices, long enough
  // that the chip doesn't fight with the ⏰ 到期 badge that just appeared.
  const OVERDUE_THRESHOLD_MIN = 60;

  // Pure helper: parse a butler-history line into structured fields.
  // Format: "<ts> <action> <title> :: <desc>". Falls back gracefully on malformed lines.
  const parseButlerLine = (line: string) => {
    const firstSpace = line.indexOf(" ");
    if (firstSpace < 0) return { ts: "", action: "", title: "", desc: line };
    const ts = line.slice(0, firstSpace);
    const rest = line.slice(firstSpace + 1);
    const sepIdx = rest.indexOf(" :: ");
    if (sepIdx < 0) return { ts, action: "", title: rest, desc: "" };
    const head = rest.slice(0, sepIdx);
    const desc = rest.slice(sepIdx + 4);
    const headSpace = head.indexOf(" ");
    if (headSpace < 0) return { ts, action: head, title: "", desc };
    return {
      ts,
      action: head.slice(0, headSpace),
      title: head.slice(headSpace + 1),
      desc,
    };
  };

  /// 最近 5 个搜索 keyword history：localStorage `pet-memory-search-history`
  /// → string[] (newest first)。每次成功 handleSearch 入栈；去重 + cap 5。
  /// native <datalist> 元素挂在 input 的 `list=` 属性，浏览器自动浮自动完
  /// 成下拉 —— 不用自己实现 popover / outside-click 关闭。
  const [searchHistory, setSearchHistory] = useState<string[]>(() => {
    try {
      const raw = window.localStorage.getItem("pet-memory-search-history");
      if (!raw) return [];
      const arr = JSON.parse(raw);
      if (Array.isArray(arr)) {
        return arr.filter((v): v is string => typeof v === "string").slice(0, 5);
      }
    } catch {
      // 解析失败 → 空 history（不退化用户体验）
    }
    return [];
  });
  const pushSearchHistory = (kw: string) => {
    const trimmed = kw.trim();
    if (!trimmed) return;
    setSearchHistory((prev) => {
      const next = [trimmed, ...prev.filter((x) => x !== trimmed)].slice(0, 5);
      try {
        window.localStorage.setItem(
          "pet-memory-search-history",
          JSON.stringify(next),
        );
      } catch {
        // 私密浏览 / quota 满 → session 内仍生效
      }
      return next;
    });
  };

  const handleSearch = async () => {
    if (!searchKeyword.trim()) {
      setSearchResults(null);
      return;
    }
    try {
      const results = await invoke<
        { category: string; title: string; description: string; detail_path: string }[]
      >("memory_search", { keyword: searchKeyword });
      setSearchResults(results);
      pushSearchHistory(searchKeyword);
    } catch (e: any) {
      setMessage(`搜索失败: ${e}`);
    }
  };

  // Iter Cχ: strip the [error: ...] block from a butler_tasks description.
  // Single-click clearance for the "ack failure, drop the marker" path —
  // alternative to navigating 编辑 → manually delete bracket → 保存.
  // Goes through commands::memory directly (panel path) so butler_history
  // is not touched — error clears by the user are config changes, not
  // executions that should appear in the timeline.
  const handleClearError = async (title: string, fullDesc: string) => {
    const stripped = fullDesc.replace(/\[error[^\]]*\]\s*/i, "").trim();
    try {
      await invoke("memory_edit", {
        action: "update",
        category: "butler_tasks",
        title,
        description: stripped,
      });
      setMessage(`已清除「${title}」的失败标记`);
      await loadIndex();
    } catch (e: any) {
      setMessage(`清除失败: ${e}`);
    }
  };

  const handleFireProactive = async () => {
    setFiringProactive(true);
    setMessage("正在让宠物处理…");
    try {
      const status = await invoke<string>("trigger_proactive_turn");
      setMessage(status);
      // Likely just touched a butler_task — refresh both views.
      await loadButlerHistory();
      await loadIndex();
    } catch (e: any) {
      setMessage(`触发失败: ${e}`);
    } finally {
      setFiringProactive(false);
    }
  };

  // consolidate 进度：后端 emit "consolidate-progress" 事件 → 这里更新进度
  // 条。phase 是当前阶段短语，progress / total 给百分比。空时不显进度条。
  const [consolidateProgress, setConsolidateProgress] = useState<
    { phase: string; progress: number; total: number } | null
  >(null);
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    (async () => {
      unlisten = await listen<{ phase: string; progress: number; total: number }>(
        "consolidate-progress",
        (event) => {
          setConsolidateProgress(event.payload);
        },
      );
    })();
    return () => {
      if (unlisten) unlisten();
    };
  }, []);

  const handleConsolidate = async () => {
    setConsolidating(true);
    setConsolidateProgress({ phase: "starting", progress: 0, total: 8 });
    setMessage("正在整理记忆，请稍候…");
    try {
      const status = await invoke<string>("trigger_consolidate");
      setMessage(status);
      await loadIndex();
    } catch (e: any) {
      const msg = String(e);
      if (msg.includes("用户取消")) {
        setMessage("已取消整理（已完成的步骤保留）");
      } else {
        setMessage(`整理失败: ${msg}`);
      }
    } finally {
      setConsolidating(false);
      setConsolidateProgress(null);
    }
  };
  const handleCancelConsolidate = async () => {
    try {
      await invoke("cancel_consolidate");
      setMessage("已发出取消信号 · 等待当前阶段结束…");
    } catch (e) {
      setMessage(`取消失败: ${e}`);
    }
  };

  // R98: 把整个 index 序列化成 markdown 复制到剪贴板。结构按 CATEGORY_ORDER
  // 分 H2 段落，每个 item 一个 H3 + blockquote ts + 描述。空 category 跳过。
  // 后端将来新增 category（不在 ORDER 里）会追加到末尾，不丢数据。
  // R118: 在 description textarea 当前光标位置插入 schedule 模板字符串。
  // 选中段被替换；setTimeout 0 等 React commit 完后把光标移到插入末尾 +
  // focus，让用户继续填具体值。
  const insertTemplate = (template: string) => {
    if (!editingItem) return;
    const ta = descTextareaRef.current;
    const cur = editingItem.description;
    let next: string;
    let newCursor: number;
    if (ta) {
      const start = ta.selectionStart ?? cur.length;
      const end = ta.selectionEnd ?? cur.length;
      next = cur.slice(0, start) + template + cur.slice(end);
      newCursor = start + template.length;
    } else {
      next = cur + template;
      newCursor = next.length;
    }
    setEditingItem({ ...editingItem, description: next });
    setTimeout(() => {
      const t = descTextareaRef.current;
      if (t) {
        t.focus();
        t.setSelectionRange(newCursor, newCursor);
      }
    }, 0);
  };

  const handleExportAll = async () => {
    if (!index) return;
    const md = exportMemoriesAsMarkdown(index);
    const totalItems = Object.values(index.categories).reduce(
      (sum, c) => sum + c.items.length,
      0,
    );
    try {
      await navigator.clipboard.writeText(md);
      setMessage(`已复制 ${totalItems} 条记忆 (${md.length} 字符) 到剪贴板`);
      setTimeout(() => setMessage(""), 4000);
    } catch (e: any) {
      setMessage(`导出失败: ${e}`);
    }
  };

  /// 一键导出 .md 文件：Blob + URL.createObjectURL + a.download 触发系统
  /// 下载对话框 / 直接落到 ~/Downloads（看用户浏览器设置，Tauri WKWebView
  /// 走 OS Save 面板）。比 clipboard 路径少"打开 vim → 粘 → 存"三步，
  /// 适合定期备份场景。文件名带 YYYY-MM-DD 避免重复导出互盖。
  const handleExportAllToFile = () => {
    if (!index) return;
    const md = exportMemoriesAsMarkdown(index);
    const totalItems = Object.values(index.categories).reduce(
      (sum, c) => sum + c.items.length,
      0,
    );
    try {
      const blob = new Blob([md], { type: "text/markdown;charset=utf-8" });
      const url = URL.createObjectURL(blob);
      const now = new Date();
      const y = now.getFullYear();
      const m = String(now.getMonth() + 1).padStart(2, "0");
      const d = String(now.getDate()).padStart(2, "0");
      const filename = `pet-memories-${y}-${m}-${d}.md`;
      const a = document.createElement("a");
      a.href = url;
      a.download = filename;
      document.body.appendChild(a);
      a.click();
      a.remove();
      // 延迟 revoke 让浏览器 / WKWebView 有时间发起下载（同步 revoke 偶发
      // 抢在写盘前生效，导致空文件）
      window.setTimeout(() => URL.revokeObjectURL(url), 1500);
      setMessage(`已保存 ${totalItems} 条记忆到 ${filename}`);
      setTimeout(() => setMessage(""), 4000);
    } catch (e: any) {
      setMessage(`保存失败: ${e}`);
      setTimeout(() => setMessage(""), 4000);
    }
  };

  // 删除按钮的"二次确认"状态：避免误删，且不依赖 window.confirm（Tauri 2
  // webview 在某些版本里把 confirm() 默认变成异步 / 直接禁掉，旧实现
  // `if (!confirm(...)) return` 会因为 confirm 返回 undefined 直接 early
  // return → 删除按钮永远不生效。改成"按一下变红 + 文案，3s 内再按确认"
  // 的 armed 模式，与 PanelDebug "立即开口" 按钮同模式。
  const [armedDeleteKey, setArmedDeleteKey] = useState<string | null>(null);
  const armDeleteTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const handleDelete = async (category: string, title: string) => {
    const key = `${category}::${title}`;
    if (armedDeleteKey !== key) {
      setArmedDeleteKey(key);
      if (armDeleteTimer.current) clearTimeout(armDeleteTimer.current);
      armDeleteTimer.current = setTimeout(() => setArmedDeleteKey(null), 3000);
      return;
    }
    if (armDeleteTimer.current) clearTimeout(armDeleteTimer.current);
    setArmedDeleteKey(null);
    try {
      await invoke("memory_edit", { action: "delete", category, title });
      setMessage("已删除");
      await loadIndex();
      if (category === "butler_tasks") await loadButlerHistory();
      setSearchResults(null);
    } catch (e: any) {
      setMessage(`删除失败: ${e}`);
    }
  };

  const handleSaveEdit = async () => {
    if (!editingItem) return;
    // R112: trim title 防止首尾不可见空白引发的"看着相同实则不同" entry。
    // 空白唯一 → 视为空标题前端 reject（后端虽也校验，前端早 reject 体验更好）。
    // update 路径下 title input 是 disabled，trim 与源值一致几乎等价；保守
    // 起见两路径都 trim 一致。
    const title = editingItem.title.trim();
    if (!title) {
      setMessage("标题不能为空");
      return;
    }
    try {
      if (editingItem.isNew) {
        await invoke("memory_edit", {
          action: "create",
          category: editingItem.category,
          title,
          description: editingItem.description,
        });
        setMessage("已创建");
      } else {
        await invoke("memory_edit", {
          action: "update",
          category: editingItem.category,
          title,
          description: editingItem.description,
        });
        setMessage("已更新");
      }
      const wasButler = editingItem.category === "butler_tasks";
      setEditingItem(null);
      await loadIndex();
      if (wasButler) await loadButlerHistory();
    } catch (e: any) {
      setMessage(`保存失败: ${e}`);
    }
  };

  if (loading) {
    return <LoadingState />;
  }

  const s = {
    container: { padding: 16, overflowY: "auto" as const, height: "100%", background: "var(--pet-color-bg)" },
    section: { marginBottom: 20 },
    sectionTitle: { fontSize: 13.5, fontWeight: 600, color: "var(--pet-color-fg)", marginBottom: 10, paddingBottom: 6, borderBottom: "1px solid var(--pet-color-border)", display: "flex", alignItems: "center", gap: 8, letterSpacing: 0.2 },
    badge: { fontSize: 11, background: "var(--pet-color-border)", color: "var(--pet-color-muted)", borderRadius: 10, padding: "1px 8px" },
    item: { padding: "10px 12px", background: "var(--pet-color-card)", border: "1px solid var(--pet-color-border)", borderRadius: 8, marginBottom: 6, fontSize: 13 },
    itemTitle: { fontWeight: 600, color: "var(--pet-color-fg)", marginBottom: 2 },
    itemDesc: { color: "var(--pet-color-muted)", fontSize: 12, lineHeight: 1.4 },
    itemMeta: { color: "var(--pet-color-muted)", fontSize: 11, marginTop: 4 },
    btn: { padding: "5px 11px", border: "1px solid var(--pet-color-border)", borderRadius: 6, background: "var(--pet-color-card)", color: "var(--pet-color-muted)", cursor: "pointer", fontSize: 12 },
    btnDanger: { padding: "5px 11px", border: "1px solid color-mix(in srgb, var(--pet-tint-red-fg) 40%, transparent)", borderRadius: 6, background: "var(--pet-color-card)", color: "var(--pet-tint-red-fg)", cursor: "pointer", fontSize: 12 },
    btnPrimary: { padding: "7px 18px", border: "none", borderRadius: 6, background: "var(--pet-color-accent)", color: "#fff", cursor: "pointer", fontSize: 13, fontWeight: 500 },
    input: { width: "100%", padding: "7px 11px", border: "1px solid var(--pet-color-border)", borderRadius: 6, fontSize: 13, boxSizing: "border-box" as const, background: "var(--pet-color-card)", color: "var(--pet-color-fg)" },
    textarea: { width: "100%", padding: "7px 11px", border: "1px solid var(--pet-color-border)", borderRadius: 6, fontSize: 13, resize: "vertical" as const, minHeight: 60, boxSizing: "border-box" as const, background: "var(--pet-color-card)", color: "var(--pet-color-fg)" },
    searchRow: { display: "flex", gap: 8, marginBottom: 16 },
    msg: { padding: "8px 12px", background: "var(--pet-tint-green-bg)", color: "var(--pet-tint-green-fg)", borderRadius: 6, fontSize: 12, marginBottom: 12, border: "1px solid color-mix(in srgb, var(--pet-tint-green-fg) 35%, transparent)" },
  };

  return (
    <div style={s.container}>
      {/* R122: items 列表 hover 高亮。inline style 不支持 :hover 伪类，
          走 className + 全局 <style> block + !important 反压 inline 优先级。
          配色用 var(--pet-color-bg) 与 card 反差一档，跨主题自动切。
          迭代 4：与 PanelTasks 同步加 box-shadow lift + accent border。 */}
      <style>
        {`
          .pet-memory-item {
            transition: background-color 0.14s ease, box-shadow 0.18s ease,
              border-color 0.18s ease;
          }
          .pet-memory-item:hover {
            background: var(--pet-color-bg) !important;
            border-color: color-mix(in srgb, var(--pet-color-accent) 35%, var(--pet-color-border)) !important;
            box-shadow: var(--pet-shadow-sm);
          }
        `}
      </style>
      {message && (
        <div style={s.msg} onClick={() => setMessage("")}>
          {message}
        </div>
      )}

      {/* memory 存储概览：总条目 + 磁盘占用。让用户感知何时该 consolidate。
          磁盘占用 lazy 显（diskUsage null 期间不渲染本段防 layout 抖动）。 */}
      {diskUsage && (
        <div
          style={{
            fontSize: 11,
            color: "var(--pet-color-muted)",
            padding: "4px 0 8px",
            display: "flex",
            gap: 10,
            flexWrap: "wrap",
          }}
          title={`memories 目录递归扫得：${diskUsage.file_count} 个文件，共 ${diskUsage.total_bytes.toLocaleString()} 字节`}
        >
          <span>
            📚 {totalMemoryCount} 条记忆
          </span>
          <span>
            💾 {formatBytes(diskUsage.total_bytes)} ({diskUsage.file_count} 个文件)
          </span>
        </div>
      )}

      {/* Search */}
      <div style={s.searchRow}>
        <input
          ref={searchInputRef}
          style={{ ...s.input, flex: 1 }}
          placeholder="搜索记忆…（⌘F / Ctrl+F 聚焦）"
          value={searchKeyword}
          onChange={(e) => setSearchKeyword(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && handleSearch()}
          list="pet-memory-search-history"
        />
        {/* 最近 5 个搜索 keyword 历史 —— 浏览器 native datalist 自动浮下拉，
            点击 / 上下键 + Enter 选完直接进 input value。history 空时 datalist
            不渲染 option 即 noop。pushSearchHistory 在 handleSearch 成功后入
            栈，保证只记录"真的搜过"的 keyword（误敲清空不污染历史）。 */}
        {searchHistory.length > 0 && (
          <datalist id="pet-memory-search-history">
            {searchHistory.map((kw) => (
              <option key={kw} value={kw} />
            ))}
          </datalist>
        )}
        <button style={s.btn} onClick={handleSearch}>
          搜索
        </button>
        {searchResults !== null && (
          <button
            style={s.btn}
            onClick={() => {
              setSearchResults(null);
              setSearchKeyword("");
            }}
          >
            清除
          </button>
        )}
        <button
          style={{
            ...s.btn,
            background: "var(--pet-color-accent)",
            color: "#fff",
            fontWeight: 600,
          }}
          onClick={() =>
            setEditingItem({ category: "butler_tasks", title: "", description: "", isNew: true })
          }
          title="委托一项管家任务给宠物——在 proactive 时段宠物会主动尝试执行（如读文件 / 写日报 / 整理目录）。"
        >
          + 委托任务
        </button>
        <button
          style={{
            ...s.btn,
            background: consolidating ? "var(--pet-color-muted)" : "var(--pet-tint-purple-fg)",
            color: "#fff",
          }}
          onClick={handleConsolidate}
          disabled={consolidating}
          title="立即让 LLM 检查并整理记忆（合并重复 / 删过期 todo / 清 stale reminder），不必等定时触发。"
        >
          {consolidating ? "整理中…" : "立即整理"}
        </button>
        {consolidating && (
          <button
            style={{
              ...s.btn,
              background: "var(--pet-tint-red-bg)",
              color: "var(--pet-tint-red-fg)",
              border: "1px solid var(--pet-tint-red-fg)",
            }}
            onClick={() => void handleCancelConsolidate()}
            title="设取消信号；下一个 checkpoint 时 pipeline 退出。已完成的 sweep 不回滚（LLM 调用中无法 fine-grained 中断；checkpoint 在 LLM 启动前的 sweep 阶段最有效）。"
          >
            ✕ 取消
          </button>
        )}
        {/* R98: 全部记忆导出为 markdown，复制到剪贴板。辅助操作，配色与
            + 委托任务 / 立即整理 等 primary action 区分（走默认 btn 样式）。 */}
        <button
          style={s.btn}
          onClick={handleExportAll}
          disabled={!index}
          title="把全部记忆（按 category 分组）拼成单 markdown 文本复制到剪贴板。可贴到 issue / 备份 / 跨设备移植。"
        >
          📋 导出
        </button>
        {/* 单 category 导出下拉：用户常只想导 butler_tasks 段而非全集。
            选 cat → 导出该段（H2 段 + 各条 H3）到剪贴板。value="" reset
            placeholder 同 schedule template pattern。空 cat / 全 0 条
            cat 不进 options 列表（用户选不到也不显示）。 */}
        <select
          value=""
          disabled={!index}
          onChange={async (e) => {
            const raw = e.target.value;
            if (!raw || !index) return;
            // value 编码："cat:<key>" 全段；"pin:<key>" 仅段内 pinned 子集
            // （与 R94 pin 系统配套，PanelMemory 顶部 pin 按钮的反向出口）
            const [mode, ...keyParts] = raw.split(":");
            const catKey = keyParts.join(":");
            const cat = index.categories[catKey];
            if (!cat) return;
            const pinnedOnly = mode === "pin";
            const items = pinnedOnly
              ? cat.items.filter((it) =>
                  pinnedKeys.has(`${catKey}::${it.title}`),
                )
              : cat.items;
            if (pinnedOnly && items.length === 0) {
              setMessage(`「${cat.label}」段内还没 pin 任何条目`);
              setTimeout(() => setMessage(""), 3000);
              e.currentTarget.value = "";
              return;
            }
            // 拼单 category markdown：H1 段名 + H2 各条 item
            const lines: string[] = [];
            const ts = new Date().toLocaleString();
            const labelSuffix = pinnedOnly ? " · 📌 pinned" : "";
            lines.push(
              `# ${cat.label}${labelSuffix} (${items.length} 条 · ${ts})`,
              "",
            );
            for (const item of items) {
              lines.push(`## ${item.title}`);
              if (item.updated_at) {
                lines.push(
                  `> 更新于 ${item.updated_at.slice(0, 16).replace("T", " ")}`,
                );
              }
              lines.push("", item.description, "");
            }
            try {
              await navigator.clipboard.writeText(lines.join("\n"));
              setMessage(
                pinnedOnly
                  ? `已复制「${cat.label}」📌 pinned（${items.length} 条）`
                  : `已复制「${cat.label}」段（${items.length} 条）`,
              );
            } catch (err: any) {
              setMessage(`复制失败：${err}`);
            }
            setTimeout(() => setMessage(""), 3000);
            // reset select 让重选同 cat 仍能 trigger
            e.currentTarget.value = "";
          }}
          title="单 category 导出 markdown：仅复制选中段（如只导 butler_tasks 给同事看任务清单）；下方 📌 子组只导该段内 pinned items"
          style={{
            padding: "6px 10px",
            fontSize: 12,
            border: "1px solid var(--pet-color-border)",
            borderRadius: 6,
            background: "var(--pet-color-card)",
            color: "var(--pet-color-fg)",
            cursor: index ? "pointer" : "default",
            fontFamily: "inherit",
            maxWidth: 180,
          }}
        >
          <option value="">📋 单段…</option>
          {index &&
            (() => {
              const nonEmpty = Object.entries(index.categories).filter(
                ([, cat]) => cat.items.length > 0,
              );
              // 段内 pinned 计数 —— 仅对至少有 1 pinned 的段生成 📌 子项，
              // 避免下拉里大量 "(0)" 无效项干扰用户视线。
              const pinnedCounts = new Map<string, number>(
                nonEmpty.map(([key, cat]) => [
                  key,
                  cat.items.filter((it) => pinnedKeys.has(`${key}::${it.title}`))
                    .length,
                ]),
              );
              const pinnedSegs = nonEmpty.filter(
                ([key]) => (pinnedCounts.get(key) ?? 0) > 0,
              );
              return (
                <>
                  <optgroup label="全段">
                    {nonEmpty.map(([key, cat]) => (
                      <option key={`cat-${key}`} value={`cat:${key}`}>
                        {cat.label} ({cat.items.length})
                      </option>
                    ))}
                  </optgroup>
                  {pinnedSegs.length > 0 && (
                    <optgroup label="📌 仅 pinned">
                      {pinnedSegs.map(([key, cat]) => (
                        <option key={`pin-${key}`} value={`pin:${key}`}>
                          📌 {cat.label} ({pinnedCounts.get(key)})
                        </option>
                      ))}
                    </optgroup>
                  )}
                </>
              );
            })()}
        </select>
        {/* 💾 .md：与 📋 导出同内容，但直接走系统下载对话框写本地。比"粘到
            编辑器再存"少一步，适合定期备份。文件名 pet-memories-YYYY-MM-DD.md。 */}
        <button
          style={s.btn}
          onClick={handleExportAllToFile}
          disabled={!index}
          title="把全部记忆保存为本地 .md 文件（文件名带日期 YYYY-MM-DD，重复导出不互盖）。"
        >
          💾 .md
        </button>
        {/* category collapse-all / expand-all：让用户在"概要扫读"和"全展开
            细读"两种阅读姿态之间一键切换，不必逐 section 点按钮。状态写
            进 localStorage（与逐 section toggle 同 key）。 */}
        <button
          style={s.btn}
          onClick={() => {
            if (!index) return;
            const allCats = Object.keys(index.categories);
            const next = new Set(allCats);
            setExpandedCategories(next);
            try {
              window.localStorage.setItem(
                "pet-memory-expanded-cats",
                JSON.stringify([...next]),
              );
            } catch {
              // 私密 / 配额满 → session 内仍生效
            }
          }}
          disabled={!index}
          title="把所有 category section 展开全部 items（覆盖每段 > 10 条时默认折前 5 的折叠行为）。"
        >
          ⊞ 全展开
        </button>
        <button
          style={s.btn}
          onClick={() => {
            setExpandedCategories(new Set());
            try {
              window.localStorage.setItem(
                "pet-memory-expanded-cats",
                JSON.stringify([]),
              );
            } catch {
              // 同上
            }
          }}
          disabled={!index}
          title="收起所有 category section 到默认折叠态（≤ 10 条全显 / > 10 条只显前 5）。"
        >
          ⊟ 全折叠
        </button>
      </div>

      {/* consolidate 进度条：仅 consolidating + 有进度数据时显。phase 文案
          + percent bar，让用户感知"做到哪一步了"。 */}
      {consolidating && consolidateProgress && (
        <div
          style={{
            marginBottom: 12,
            padding: "8px 12px",
            background: "var(--pet-color-card)",
            border: "1px solid var(--pet-color-border)",
            borderRadius: 6,
            fontSize: 12,
          }}
        >
          <div
            style={{
              display: "flex",
              justifyContent: "space-between",
              alignItems: "center",
              marginBottom: 6,
            }}
          >
            <span style={{ color: "var(--pet-color-fg)", fontWeight: 500 }}>
              📦 整理中 · {consolidateProgress.phase}
            </span>
            <span
              style={{
                fontSize: 11,
                color: "var(--pet-color-muted)",
                fontVariantNumeric: "tabular-nums",
                fontFamily: "'SF Mono', 'Menlo', monospace",
              }}
            >
              {consolidateProgress.progress} / {consolidateProgress.total}
            </span>
          </div>
          <div
            style={{
              height: 6,
              borderRadius: 3,
              background: "var(--pet-color-bg)",
              overflow: "hidden",
            }}
          >
            <div
              style={{
                width: `${Math.min(100, (consolidateProgress.progress / Math.max(1, consolidateProgress.total)) * 100)}%`,
                height: "100%",
                background: "var(--pet-color-accent)",
                transition: "width 240ms ease-out",
              }}
            />
          </div>
        </div>
      )}

      {/* Search results */}
      {searchResults !== null && (
        <div style={s.section}>
          <div style={s.sectionTitle}>
            搜索结果 <span style={s.badge}>
              {searchResults.length} / {totalMemoryCount}
            </span>
          </div>
          {searchResults.length === 0 && (
            <div style={{ color: "var(--pet-color-muted)", fontSize: 13 }}>未找到匹配项</div>
          )}
          {searchResults.map((r, i) => (
            <div key={i} className="pet-memory-item" style={s.item}>
              <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
                <div style={s.itemTitle}>
                  <HighlightedText text={r.title} query={searchKeyword} />
                </div>
                <span style={s.badge}>{r.category}</span>
              </div>
              <div style={s.itemDesc}>
                <HighlightedText text={r.description} query={searchKeyword} />
              </div>
            </div>
          ))}
        </div>
      )}

      {/* ✏️ 改 schedule modal：小窗只编辑 time / date+time。保存调
          memory_edit update 把新 prefix + 原 topic 写回 description。
          Esc / backdrop click 取消。 */}
      <Modal
        open={editScheduleDraft !== null}
        onClose={() => {
          if (!editScheduleBusy) setEditScheduleDraft(null);
        }}
        maxWidth={380}
        zIndex={110}
      >
        {editScheduleDraft && (
          <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
            <div
              style={{
                fontSize: 13,
                fontWeight: 600,
                color: "var(--pet-color-fg)",
              }}
            >
              改 schedule —「{editScheduleDraft.title}」
            </div>
            <div>
              <label style={{ fontSize: 11, color: "var(--pet-color-muted)" }}>
                kind（类型）
              </label>
              <select
                value={editScheduleDraft.kind}
                onChange={(e) => {
                  const nextKind = e.target.value as "every" | "once" | "deadline";
                  setEditScheduleDraft({
                    ...editScheduleDraft,
                    kind: nextKind,
                    // 切到 every 不需要 date；切到 once / deadline 若 date
                    // 空（从 every 切来）→ 用今天作默认，让用户少敲一段。
                    date:
                      nextKind === "every"
                        ? ""
                        : editScheduleDraft.date ||
                          (() => {
                            const d = new Date();
                            return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
                          })(),
                  });
                }}
                style={{
                  width: "100%",
                  padding: "6px 8px",
                  fontSize: 12,
                  border: "1px solid var(--pet-color-border)",
                  borderRadius: 4,
                  background: "var(--pet-color-bg)",
                  color: "var(--pet-color-fg)",
                  fontFamily: "inherit",
                  cursor: "pointer",
                }}
              >
                <option value="every">🔁 every（每天定时）</option>
                <option value="once">📅 once（单次定时）</option>
                <option value="deadline">⏳ deadline（截止前提醒）</option>
              </select>
            </div>
            {editScheduleDraft.kind !== "every" && (
              <div>
                <label style={{ fontSize: 11, color: "var(--pet-color-muted)" }}>
                  日期 (YYYY-MM-DD)
                </label>
                <input
                  ref={editScheduleDateRef}
                  type="date"
                  value={editScheduleDraft.date}
                  onChange={(e) =>
                    setEditScheduleDraft({
                      ...editScheduleDraft,
                      date: e.target.value,
                    })
                  }
                  style={{
                    width: "100%",
                    padding: "6px 8px",
                    fontSize: 12,
                    border: "1px solid var(--pet-color-border)",
                    borderRadius: 4,
                    background: "var(--pet-color-bg)",
                    color: "var(--pet-color-fg)",
                  }}
                />
              </div>
            )}
            <div>
              <label style={{ fontSize: 11, color: "var(--pet-color-muted)" }}>
                时间 (HH:MM)
              </label>
              <input
                ref={editScheduleTimeRef}
                type="time"
                value={editScheduleDraft.time}
                onChange={(e) =>
                  setEditScheduleDraft({
                    ...editScheduleDraft,
                    time: e.target.value,
                  })
                }
                style={{
                  width: "100%",
                  padding: "6px 8px",
                  fontSize: 12,
                  border: "1px solid var(--pet-color-border)",
                  borderRadius: 4,
                  background: "var(--pet-color-bg)",
                  color: "var(--pet-color-fg)",
                }}
              />
            </div>
            <div style={{ display: "flex", gap: 8, justifyContent: "flex-end", marginTop: 4 }}>
              <button
                type="button"
                onClick={() => setEditScheduleDraft(null)}
                disabled={editScheduleBusy}
                style={{
                  padding: "6px 12px",
                  fontSize: 12,
                  border: "1px solid var(--pet-color-border)",
                  borderRadius: 6,
                  background: "var(--pet-color-card)",
                  color: "var(--pet-color-fg)",
                  cursor: editScheduleBusy ? "default" : "pointer",
                }}
              >
                取消
              </button>
              <button
                type="button"
                onClick={async () => {
                  const d = editScheduleDraft;
                  // 校验 time
                  if (!/^\d{2}:\d{2}$/.test(d.time)) {
                    setMessage("时间格式应为 HH:MM");
                    setTimeout(() => setMessage(""), 3000);
                    return;
                  }
                  if (d.kind !== "every" && !/^\d{4}-\d{2}-\d{2}$/.test(d.date)) {
                    setMessage("日期格式应为 YYYY-MM-DD");
                    setTimeout(() => setMessage(""), 3000);
                    return;
                  }
                  // 拿 parsed topic（剩余非 prefix 部分）
                  const parsedNow = parseButlerSchedule(d.description);
                  if (!parsedNow) {
                    setMessage("无法识别原 schedule，请走编辑全编辑器");
                    setTimeout(() => setMessage(""), 4000);
                    return;
                  }
                  const newPrefix =
                    d.kind === "every"
                      ? `[every: ${d.time}]`
                      : `[${d.kind}: ${d.date} ${d.time}]`;
                  const newDesc = `${newPrefix} ${parsedNow.topic}`;
                  setEditScheduleBusy(true);
                  try {
                    await invoke("memory_edit", {
                      action: "update",
                      category: "butler_tasks",
                      title: d.title,
                      description: newDesc,
                    });
                    setMessage(`已更新 ${d.title} 的 schedule`);
                    setEditScheduleDraft(null);
                    await loadIndex();
                  } catch (e: any) {
                    setMessage(`保存失败：${e}`);
                  } finally {
                    setEditScheduleBusy(false);
                    setTimeout(() => setMessage(""), 3000);
                  }
                }}
                disabled={editScheduleBusy}
                style={{
                  padding: "6px 12px",
                  fontSize: 12,
                  border: "none",
                  borderRadius: 6,
                  background: "var(--pet-color-accent)",
                  color: "#fff",
                  fontWeight: 600,
                  cursor: editScheduleBusy ? "default" : "pointer",
                  opacity: editScheduleBusy ? 0.6 : 1,
                }}
              >
                {editScheduleBusy ? "保存中…" : "保存"}
              </button>
            </div>
          </div>
        )}
      </Modal>
      {/* Edit modal */}
      <Modal
        open={editingItem !== null}
        onClose={() => setEditingItem(null)}
        maxWidth={400}
      >
        {editingItem && (
          <>
            <div style={{ fontSize: 15, fontWeight: 600, marginBottom: 12 }}>
              {editingItem.isNew ? "新建记忆" : "编辑记忆"}
            </div>
            <div style={{ marginBottom: 8 }}>
              <label style={{ fontSize: 12, color: "var(--pet-color-muted)" }}>分类</label>
              <select
                style={s.input}
                value={editingItem.category}
                onChange={(e) => setEditingItem({ ...editingItem, category: e.target.value })}
                disabled={!editingItem.isNew}
              >
                {CATEGORY_ORDER.map((k) => (
                  <option key={k} value={k}>
                    {index?.categories[k]?.label || k}
                  </option>
                ))}
              </select>
            </div>
            <div style={{ marginBottom: 8 }}>
              <label style={{ fontSize: 12, color: "var(--pet-color-muted)" }}>标题</label>
              <input
                style={s.input}
                maxLength={20}
                value={editingItem.title}
                onChange={(e) => setEditingItem({ ...editingItem, title: e.target.value })}
                disabled={!editingItem.isNew}
              />
              {/* R119: 标题字数 counter。仅 isNew 模式显（edit 模式 input
                  disabled，counter 误导用户"还能改"）。三档颜色与 R113 描述
                  counter 同款（< 90% muted / 90-99% amber / 100% red）。 */}
              {editingItem.isNew && (() => {
                const len = editingItem.title.length;
                const MAX = 20;
                const WARN = 18;
                const color =
                  len >= MAX
                    ? "var(--pet-tint-red-fg)"
                    : len >= WARN
                      ? "var(--pet-tint-yellow-fg)"
                      : "var(--pet-color-muted)";
                const tip =
                  len >= MAX
                    ? "已达 maxLength=20；继续输入会被浏览器拒绝"
                    : len >= WARN
                      ? "接近 20 字上限"
                      : "标题长度限制 20 字";
                return (
                  <div
                    style={{ fontSize: 10, textAlign: "right", color, marginTop: 2 }}
                    title={tip}
                  >
                    {len} / {MAX}
                  </div>
                );
              })()}
            </div>
            <div style={{ marginBottom: 12 }}>
              <label style={{ fontSize: 12, color: "var(--pet-color-muted)" }}>描述</label>
              {/* R118: butler_tasks schedule 模板按钮。仅 butler_tasks
                  category 显；点击在光标位置插入 [every: ...] / [once: ...] /
                  [deadline: ...] 模板，新用户写 schedule 不再要记忆语法。 */}
              {editingItem.category === "butler_tasks" && (
                <div
                  style={{ display: "flex", gap: 4, marginTop: 4, marginBottom: 4, alignItems: "center", flexWrap: "wrap" }}
                >
                  {SCHEDULE_TEMPLATES.map(({ label, text }) => (
                    <button
                      key={text}
                      type="button"
                      onClick={() => insertTemplate(text)}
                      title={`在光标位置插入 \`${text.trim()}\` 模板（butler_tasks schedule 语法）`}
                      style={{
                        padding: "2px 8px",
                        fontSize: 11,
                        border: "1px solid var(--pet-color-border)",
                        borderRadius: 4,
                        background: "var(--pet-color-card)",
                        color: "var(--pet-color-fg)",
                        cursor: "pointer",
                        fontFamily: "inherit",
                      }}
                    >
                      {label}
                    </button>
                  ))}
                  {/* 从现有 butler_tasks 复制 schedule 前缀：只列出含
                      [every:/once:/deadline:] 的 item；用户挑一条 →
                      把其前缀（含尾空格）插入到光标位。比手敲分钟更
                      省事，新用户也能直接抄一份"我已经验证过的形状"。
                      none 时（如全新用户）不渲染下拉，避免空选择器干扰。 */}
                  {(() => {
                    const butler = index?.categories.butler_tasks?.items ?? [];
                    const candidates = butler
                      .map((it) => {
                        const m = it.description
                          .replace(/^\s+/, "")
                          .match(/^(\[(?:every|once|deadline):[^\]]+\])/);
                        if (!m) return null;
                        const parsed = parseButlerSchedule(it.description);
                        if (!parsed) return null;
                        return { title: it.title, prefix: m[1] + " " };
                      })
                      .filter((v): v is { title: string; prefix: string } => v !== null);
                    if (candidates.length === 0) return null;
                    return (
                      <select
                        value=""
                        onChange={(e) => {
                          const v = e.target.value;
                          if (!v) return;
                          insertTemplate(v);
                          // reset 回 placeholder，下次切换才能再 trigger
                          e.currentTarget.value = "";
                        }}
                        title="挑一条已有任务，把它的 schedule 前缀（[every:/once:/deadline:]）插到光标位，比手敲省事。"
                        style={{
                          padding: "2px 6px",
                          fontSize: 11,
                          border: "1px solid var(--pet-color-border)",
                          borderRadius: 4,
                          background: "var(--pet-color-card)",
                          color: "var(--pet-color-fg)",
                          cursor: "pointer",
                          fontFamily: "inherit",
                          maxWidth: 220,
                        }}
                      >
                        <option value="">📥 复制现有 schedule…</option>
                        {candidates.map(({ title, prefix }) => (
                          <option key={`${title}::${prefix}`} value={prefix}>
                            {title} — {prefix.trim()}
                          </option>
                        ))}
                      </select>
                    );
                  })()}
                </div>
              )}
              <textarea
                ref={descTextareaRef}
                style={{ ...s.textarea, minHeight: editingItem.category === "butler_tasks" ? 100 : 60 }}
                maxLength={300}
                placeholder={CATEGORY_PLACEHOLDERS[editingItem.category] || ""}
                value={editingItem.description}
                onChange={(e) => setEditingItem({ ...editingItem, description: e.target.value })}
                onKeyDown={(e) => {
                  // R105: ⌘S/Ctrl+S 触发保存。preventDefault 吃掉 webview
                  // "另存为页面"默认行为；handleSaveEdit 内部已有 try/catch
                  // 防 race。仿 PanelTasks 详情 detail.md 编辑同款 pattern。
                  if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "s") {
                    e.preventDefault();
                    void handleSaveEdit();
                  }
                }}
              />
              {/* R113: description 字数计数器。三档颜色：< 90% muted / 90-99%
                  amber / 100% red，让用户提前感知 maxLength=300 上限。 */}
              {(() => {
                const len = editingItem.description.length;
                const MAX = 300;
                const WARN = 270;
                const color =
                  len >= MAX
                    ? "var(--pet-tint-red-fg)"
                    : len >= WARN
                      ? "var(--pet-tint-yellow-fg)"
                      : "var(--pet-color-muted)";
                const tip =
                  len >= MAX
                    ? "已达 maxLength；继续输入会被浏览器拒绝"
                    : len >= WARN
                      ? "接近 300 字上限，建议提前收笔"
                      : "描述长度限制 300 字";
                return (
                  <div
                    style={{ fontSize: 10, textAlign: "right", color, marginTop: 2 }}
                    title={tip}
                  >
                    {len} / {MAX}
                  </div>
                );
              })()}
            </div>
            <div style={{ display: "flex", gap: 8, justifyContent: "flex-end" }}>
              <button style={s.btn} onClick={() => setEditingItem(null)}>
                取消
              </button>
              <button
                style={s.btnPrimary}
                onClick={handleSaveEdit}
                title="保存到 memory index（⌘S/Ctrl+S 等价）"
              >
                保存
              </button>
            </div>
          </>
        )}
      </Modal>

      {/* Categories */}
      {searchResults === null &&
        index &&
        (() => {
          // 计算 effective 类目顺序：
          // 1. savedCatOrder（用户拖过的项）按其顺序排首
          // 2. CATEGORY_ORDER 默认未在 saved 里的接其后
          // 3. backend index 里 unknown 的（用户 / 后端新增类目）接末尾
          // 三段 dedup 后 filter 出 index 实际存在的 cat。
          const seen = new Set<string>();
          const ordered: string[] = [];
          for (const k of savedCatOrder) {
            if (!seen.has(k)) {
              seen.add(k);
              ordered.push(k);
            }
          }
          for (const k of CATEGORY_ORDER) {
            if (!seen.has(k)) {
              seen.add(k);
              ordered.push(k);
            }
          }
          for (const k of Object.keys(index.categories)) {
            if (!seen.has(k)) {
              seen.add(k);
              ordered.push(k);
            }
          }
          return ordered;
        })().map((catKey) => {
          const cat = index.categories[catKey];
          if (!cat) return null;
          // Iter Cκ: compute how many butler tasks are overdue past the threshold
          // so the section header can offer a manual fire button when at least one
          // is stale. Cheap — items are ≤6 in practice.
          const now = new Date();
          const overdueCount =
            catKey === "butler_tasks"
              ? cat.items.filter((it) => {
                  const p = parseButlerSchedule(it.description);
                  if (!p) return false;
                  if (!isButlerDue(p.schedule, it.updated_at, now)) return false;
                  const mins = overdueMinutes(p.schedule, now);
                  return mins !== null && mins >= OVERDUE_THRESHOLD_MIN;
                }).length
              : 0;
          // R92: 最新更新相对时间。inline 计算（cat.items ≤ 10 廉价；useMemo
          // 在 .map 里不能用 —— hooks 规则要求每帧同序调用）。空 cat 时
          // latestTs===null → header 不渲染该 span。
          let latestTs: number | null = null;
          for (const item of cat.items) {
            const ts = Date.parse(item.updated_at);
            if (Number.isNaN(ts)) continue;
            if (latestTs === null || ts > latestTs) latestTs = ts;
          }
          // section header 上的 hover preview：列最近 3 条 item title（按 items
          // 倒序粗略取，假设 backend 按 updated_at 升序排）。让用户在 hover badge
          // 时就能瞄一眼该 category 里有什么，省一次"展开 + 滚动"。
          const previewLines = cat.items
            .slice(-3)
            .reverse()
            .map((it) => `- ${it.title}`)
            .join("\n");
          const previewTip =
            cat.items.length === 0
              ? "（空）"
              : `最近 ${Math.min(3, cat.items.length)} 条：\n${previewLines}`;
          const isDragSource = dragSrcCat === catKey;
          const isDragOverTarget =
            dragOverCat === catKey && dragSrcCat && dragSrcCat !== catKey;
          return (
            <div
              key={catKey}
              style={{
                ...s.section,
                ...(isDragSource ? { opacity: 0.4 } : {}),
                ...(isDragOverTarget
                  ? {
                      outline: "2px dashed var(--pet-color-accent)",
                      outlineOffset: 2,
                      borderRadius: 6,
                    }
                  : {}),
              }}
              onDragOver={(e) => {
                if (!dragSrcCat || dragSrcCat === catKey) return;
                e.preventDefault();
                e.dataTransfer.dropEffect = "move";
                if (dragOverCat !== catKey) setDragOverCat(catKey);
              }}
              onDragLeave={() => {
                setDragOverCat((cur) => (cur === catKey ? null : cur));
              }}
              onDrop={(e) => {
                e.preventDefault();
                const src = dragSrcCat;
                setDragSrcCat(null);
                setDragOverCat(null);
                if (!src || src === catKey) return;
                // 在当前 effective 顺序里把 src 移到 catKey 之前。最终持久
                // 化"完整 effective 顺序"，下次任何位置的 catKey 都按用户
                // 排过的样子展示。
                const cur = (() => {
                  const seen = new Set<string>();
                  const ordered: string[] = [];
                  for (const k of savedCatOrder) {
                    if (!seen.has(k)) {
                      seen.add(k);
                      ordered.push(k);
                    }
                  }
                  for (const k of CATEGORY_ORDER) {
                    if (!seen.has(k)) {
                      seen.add(k);
                      ordered.push(k);
                    }
                  }
                  for (const k of Object.keys(index.categories)) {
                    if (!seen.has(k)) {
                      seen.add(k);
                      ordered.push(k);
                    }
                  }
                  return ordered;
                })();
                const without = cur.filter((k) => k !== src);
                const dstIdx = without.indexOf(catKey);
                if (dstIdx < 0) return;
                without.splice(dstIdx, 0, src);
                persistCatOrder(without);
              }}
            >
              <div style={s.sectionTitle}>
                <span
                  draggable
                  onDragStart={(e) => {
                    setDragSrcCat(catKey);
                    e.dataTransfer.effectAllowed = "move";
                    try {
                      e.dataTransfer.setData("text/plain", catKey);
                    } catch {
                      // WKWebView 个别版本 setData 抛；忽略，state 仍生效
                    }
                  }}
                  onDragEnd={() => {
                    setDragSrcCat(null);
                    setDragOverCat(null);
                  }}
                  title="拖拽改类目顺序（顺序仅本机生效；localStorage 持久化）"
                  style={{
                    cursor: "grab",
                    color: "var(--pet-color-muted)",
                    userSelect: "none",
                    fontSize: 12,
                    letterSpacing: -2,
                    padding: "0 2px",
                  }}
                >
                  ⋮⋮
                </span>
                {renamingCatKey === catKey ? (
                  <input
                    autoFocus
                    type="text"
                    value={renameCatDraft}
                    onChange={(e) => setRenameCatDraft(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") {
                        e.preventDefault();
                        setCategoryLabel(catKey, renameCatDraft);
                        setRenamingCatKey(null);
                        setRenameCatDraft("");
                      } else if (e.key === "Escape") {
                        e.preventDefault();
                        setRenamingCatKey(null);
                        setRenameCatDraft("");
                      }
                    }}
                    onBlur={() => {
                      setCategoryLabel(catKey, renameCatDraft);
                      setRenamingCatKey(null);
                      setRenameCatDraft("");
                    }}
                    placeholder={cat.label}
                    style={{
                      fontSize: 14,
                      fontWeight: 600,
                      padding: "2px 6px",
                      border: "1px solid var(--pet-color-accent)",
                      borderRadius: 4,
                      background: "var(--pet-color-card)",
                      color: "var(--pet-color-fg)",
                      minWidth: 140,
                      fontFamily: "inherit",
                    }}
                  />
                ) : (
                  <span
                    onDoubleClick={() => {
                      setRenamingCatKey(catKey);
                      setRenameCatDraft(categoryLabels[catKey] ?? "");
                    }}
                    title={`双击改显示名（仅本机生效；空 = 用后端默认 "${cat.label}"）`}
                    style={{ cursor: "text" }}
                  >
                    {categoryLabels[catKey] || cat.label}
                  </span>
                )}
                <span style={s.badge} title={previewTip}>
                  {cat.items.length}
                </span>
                {latestTs !== null && (
                  <span
                    style={{ fontSize: 11, color: "var(--pet-color-muted)", fontWeight: 400 }}
                    title={`最新一条 item 的 updated_at = ${new Date(latestTs).toLocaleString()}`}
                  >
                    最近 {formatLastUpdated(latestTs, now.getTime())}
                  </span>
                )}
                {catKey === "butler_tasks" && overdueCount > 0 && (
                  <button
                    style={{
                      ...s.btn,
                      background: firingProactive
                        ? "var(--pet-color-muted)"
                        : fireArmed
                          ? "var(--pet-tint-red-bg)"
                          : "var(--pet-tint-red-fg)",
                      color: firingProactive
                        ? "#fff"
                        : fireArmed
                          ? "var(--pet-tint-red-fg)"
                          : "#fff",
                      borderColor: "transparent",
                      fontWeight: fireArmed ? 600 : undefined,
                      marginLeft: 8,
                    }}
                    onClick={() => {
                      if (firingProactive) return;
                      if (!fireArmed) {
                        setFireArmed(true);
                        window.setTimeout(() => setFireArmed(false), 3000);
                        return;
                      }
                      setFireArmed(false);
                      void handleFireProactive();
                    }}
                    disabled={firingProactive}
                    title={
                      fireArmed
                        ? "再次点击立即触发主动开口（3s 内有效）"
                        : `${overdueCount} 个任务已过期超过 ${OVERDUE_THRESHOLD_MIN} 分钟。点击立即触发一次主动开口（绕过 cooldown / quiet hours），让宠物现在去看任务列表并选一项处理。点击后 3s 内需再点确认，防误触。`
                    }
                  >
                    {firingProactive
                      ? "处理中…"
                      : fireArmed
                        ? "再点确认 (3s)"
                        : `立即处理 (${overdueCount})`}
                  </button>
                )}
                {/* "📋 今日 todo"：butler_tasks 段顶按钮，把今日要执行任
                    务（every / 今日 once / 今日 deadline）拼成 markdown 复制。
                    每日 work prep / 9am stand-up 复制走人。仅 butler_tasks
                    + 至少有 today 命中时浮。 */}
                {catKey === "butler_tasks" &&
                  (() => {
                    const now = new Date();
                    const todayY = now.getFullYear();
                    const todayM = now.getMonth() + 1;
                    const todayD = now.getDate();
                    const todayItems = cat.items.filter((it) => {
                      const p = parseButlerSchedule(it.description);
                      if (!p) return false;
                      const s = p.schedule;
                      if (s.kind === "every") return true;
                      return (
                        s.year === todayY &&
                        s.month === todayM &&
                        s.day === todayD
                      );
                    });
                    if (todayItems.length === 0) return null;
                    return (
                      <button
                        style={{ ...s.btn, marginLeft: "auto" }}
                        onClick={async () => {
                          const todayStr = `${todayY}-${String(todayM).padStart(2, "0")}-${String(todayD).padStart(2, "0")}`;
                          const lines: string[] = [
                            `# 📌 今日 todo（${todayStr} · ${todayItems.length} 条）`,
                            "",
                          ];
                          for (const it of todayItems) {
                            const p = parseButlerSchedule(it.description);
                            const tag = p
                              ? p.schedule.kind === "every"
                                ? `🔁 ${String(p.schedule.hour).padStart(2, "0")}:${String(p.schedule.minute).padStart(2, "0")}`
                                : p.schedule.kind === "once"
                                  ? `📅 ${String(p.schedule.hour).padStart(2, "0")}:${String(p.schedule.minute).padStart(2, "0")}`
                                  : `⏳ ${String(p.schedule.hour).padStart(2, "0")}:${String(p.schedule.minute).padStart(2, "0")}`
                              : "—";
                            lines.push(`- [ ] ${tag} ${it.title}`);
                          }
                          try {
                            await navigator.clipboard.writeText(lines.join("\n"));
                            setMessage(
                              `已复制今日 todo（${todayItems.length} 条）`,
                            );
                          } catch (e: any) {
                            setMessage(`复制失败：${e}`);
                          }
                          setTimeout(() => setMessage(""), 3000);
                        }}
                        title={`把今日要执行的 ${todayItems.length} 条 butler_task 拼成 markdown checkbox 列表复制到剪贴板（标题前带 🔁/📅/⏳ icon + 时间）。早 stand-up / work prep 走人用。`}
                      >
                        📋 今日 todo ({todayItems.length})
                      </button>
                    );
                  })()}
                <button
                  style={{
                    ...s.btn,
                    marginLeft:
                      catKey === "butler_tasks" && overdueCount > 0 ? 4 : "auto",
                  }}
                  onClick={() =>
                    setEditingItem({ category: catKey, title: "", description: "", isNew: true })
                  }
                >
                  + 新建
                </button>
              </div>
              {/* Iter Cη: per-day "今日小结" rolled up by consolidate. Each line is
                  "<date> <summary>". Newest day rendered at the top in a slightly
                  bolder treatment than the per-event timeline below. */}
              {catKey === "butler_tasks" && butlerDaily.length > 0 && (
                <div
                  style={{
                    background: "var(--pet-tint-yellow-bg)",
                    border: "1px solid #fde68a",
                    borderRadius: 6,
                    padding: "8px 10px",
                    marginBottom: 8,
                  }}
                >
                  <div style={{ fontSize: 11, color: "var(--pet-tint-yellow-fg)", marginBottom: 4, fontWeight: 600 }}>
                    每日小结 ({butlerDaily.length})
                  </div>
                  {/* R143: > 5 条时默认折叠到最新 5 条，加 "展开全部" 按钮。
                      reversed 在外面切片让"前 5"对应最新 5 天小结。 */}
                  {(() => {
                    const HISTORY_FOLD_THRESHOLD = 5;
                    const reversed = butlerDaily.slice().reverse();
                    const isLong = butlerDaily.length > HISTORY_FOLD_THRESHOLD;
                    const shown =
                      isLong && !butlerDailyExpanded
                        ? reversed.slice(0, HISTORY_FOLD_THRESHOLD)
                        : reversed;
                    return (
                      <>
                        {shown.map((line, i) => {
                          const firstSpace = line.indexOf(" ");
                          const date = firstSpace > 0 ? line.slice(0, firstSpace) : "";
                          const text = firstSpace > 0 ? line.slice(firstSpace + 1) : line;
                          return (
                            <div
                              key={i}
                              style={{
                                fontSize: 12,
                                color: "var(--pet-color-fg)",
                                marginTop: 2,
                                display: "flex",
                                gap: 6,
                                alignItems: "baseline",
                              }}
                            >
                              <span style={{ color: "var(--pet-tint-yellow-fg)", fontFamily: "'SF Mono', monospace", fontSize: 11 }}>
                                {date}
                              </span>
                              <span style={{ flex: 1 }}>{text}</span>
                            </div>
                          );
                        })}
                        {isLong && (
                          <button
                            type="button"
                            onClick={() =>
                              setButlerDailyExpanded((v) => !v)
                            }
                            title={
                              butlerDailyExpanded
                                ? "折叠回最新 5 条"
                                : `展开后显示全部 ${butlerDaily.length} 条历史小结`
                            }
                            style={{
                              marginTop: 4,
                              fontSize: 11,
                              padding: 0,
                              border: "none",
                              background: "transparent",
                              color: "var(--pet-tint-yellow-fg)",
                              cursor: "pointer",
                              fontFamily: "inherit",
                            }}
                          >
                            {butlerDailyExpanded
                              ? `收起 (${butlerDaily.length})`
                              : `… 展开全部 ${butlerDaily.length} 条`}
                          </button>
                        )}
                      </>
                    );
                  })()}
                </div>
              )}
              {/* Iter Cε: butler_tasks gets a "最近执行" mini-timeline showing the
                  last few times the LLM updated/deleted a task — closes the
                  feedback loop between assignment and execution. */}
              {catKey === "butler_tasks" && butlerHistory.length > 0 && (
                <div
                  style={{
                    background: "var(--pet-tint-blue-bg)",
                    border: "1px solid #bae6fd",
                    borderRadius: 6,
                    padding: "8px 10px",
                    marginBottom: 8,
                  }}
                >
                  <div style={{ fontSize: 11, color: "var(--pet-tint-blue-fg)", marginBottom: 4, fontWeight: 600 }}>
                    最近执行 ({butlerHistory.length})
                  </div>
                  {/* R95: > 5 条时默认折叠到最新 5 条，加 "展开全部" 按钮。
                      reversed 在外面切片让"前 5"对应最新 5 次执行。 */}
                  {(() => {
                    const HISTORY_FOLD_THRESHOLD = 5;
                    const reversed = butlerHistory.slice().reverse();
                    const isLong = butlerHistory.length > HISTORY_FOLD_THRESHOLD;
                    const shown =
                      isLong && !butlerHistoryExpanded
                        ? reversed.slice(0, HISTORY_FOLD_THRESHOLD)
                        : reversed;
                    return (
                      <>
                        {shown.map((line, i) => {
                      const p = parseButlerLine(line);
                      const when = p.ts.slice(5, 16).replace("T", " ");
                      const actionColor = p.action === "delete" ? "var(--pet-tint-red-fg)" : "var(--pet-tint-green-fg)";
                      return (
                        <div
                          key={i}
                          style={{
                            fontSize: 11,
                            color: "var(--pet-color-fg)",
                            marginTop: 2,
                            display: "flex",
                            gap: 6,
                            alignItems: "baseline",
                          }}
                        >
                          <span style={{ color: "var(--pet-color-muted)", fontFamily: "'SF Mono', monospace" }}>
                            {when}
                          </span>
                          <span style={{ color: actionColor, fontWeight: 600 }}>{p.action}</span>
                          <span style={{ fontWeight: 500 }}>{p.title}</span>
                          {p.desc && (
                            <span
                              style={{
                                color: "var(--pet-color-muted)",
                                whiteSpace: "nowrap",
                                overflow: "hidden",
                                textOverflow: "ellipsis",
                                flex: 1,
                              }}
                              title={p.desc}
                            >
                              :: {p.desc}
                            </span>
                          )}
                        </div>
                      );
                        })}
                        {isLong && (
                          <button
                            type="button"
                            onClick={() =>
                              setButlerHistoryExpanded((v) => !v)
                            }
                            title={
                              butlerHistoryExpanded
                                ? "折叠回最新 5 条"
                                : `展开后显示全部 ${butlerHistory.length} 条历史执行`
                            }
                            style={{
                              marginTop: 4,
                              fontSize: 11,
                              padding: 0,
                              border: "none",
                              background: "transparent",
                              color: "var(--pet-tint-blue-fg)",
                              cursor: "pointer",
                              fontFamily: "inherit",
                            }}
                          >
                            {butlerHistoryExpanded
                              ? `收起 (${butlerHistory.length})`
                              : `… 展开全部 ${butlerHistory.length} 条`}
                          </button>
                        )}
                      </>
                    );
                  })()}
                </div>
              )}
              {cat.items.length === 0 && (
                <EmptyState icon="📭" title="本段还没有条目" compact />
              )}
              {/* butler_tasks schedule kind 过滤 chip 行：仅 butler_tasks 段
                  显，且 cat.items 非空时浮。统计各 kind 计数；点击 chip
                  toggle 加入 / 移除过滤集。空集合 = 不过滤。 */}
              {catKey === "butler_tasks" && cat.items.length > 0 && (() => {
                // "今日要执行"判定：every 每天都触发 → 永远算今日；once /
                // deadline 看 schedule date 是否等于今天。其它（none / 不匹
                // 配）不算。with 今天的 y/m/d 三段单次构造，loop 内只比较。
                const now = new Date();
                const todayY = now.getFullYear();
                const todayM = now.getMonth() + 1;
                const todayD = now.getDate();
                const isTodayExecution = (
                  parsed: ReturnType<typeof parseButlerSchedule>,
                ): boolean => {
                  if (!parsed) return false;
                  const s = parsed.schedule;
                  if (s.kind === "every") return true;
                  return (
                    s.year === todayY && s.month === todayM && s.day === todayD
                  );
                };
                let everyCnt = 0,
                  onceCnt = 0,
                  deadlineCnt = 0,
                  noneCnt = 0,
                  todayCnt = 0;
                for (const it of cat.items) {
                  const p = parseButlerSchedule(it.description);
                  if (!p) noneCnt += 1;
                  else if (p.schedule.kind === "every") everyCnt += 1;
                  else if (p.schedule.kind === "once") onceCnt += 1;
                  else if (p.schedule.kind === "deadline") deadlineCnt += 1;
                  if (isTodayExecution(p)) todayCnt += 1;
                }
                const chips: Array<{
                  kind: string;
                  label: string;
                  count: number;
                  icon: string;
                  bg: string;
                  fg: string;
                }> = [
                  { kind: "today", label: "今日要执行", count: todayCnt, icon: "📌", bg: "var(--pet-tint-green-bg)", fg: "var(--pet-tint-green-fg)" },
                  { kind: "every", label: "每天", count: everyCnt, icon: "🔁", bg: "var(--pet-tint-blue-bg)", fg: "var(--pet-tint-blue-fg)" },
                  { kind: "once", label: "一次", count: onceCnt, icon: "📅", bg: "var(--pet-tint-yellow-bg)", fg: "var(--pet-tint-yellow-fg)" },
                  { kind: "deadline", label: "截止", count: deadlineCnt, icon: "⏳", bg: "var(--pet-tint-red-bg)", fg: "var(--pet-tint-red-fg)" },
                  { kind: "none", label: "无 schedule", count: noneCnt, icon: "🔢", bg: "var(--pet-color-border)", fg: "var(--pet-color-muted)" },
                ];
                return (
                  <div
                    style={{
                      display: "flex",
                      flexWrap: "wrap",
                      gap: 4,
                      marginBottom: 8,
                      paddingLeft: 4,
                      alignItems: "center",
                    }}
                  >
                    <span style={{ fontSize: 11, color: "var(--pet-color-muted)" }}>
                      schedule：
                    </span>
                    {chips.map((c) => {
                      if (c.count === 0) return null;
                      const active = butlerScheduleFilter.has(c.kind);
                      return (
                        <span
                          key={c.kind}
                          onClick={() => toggleButlerSchedule(c.kind)}
                          title={
                            active
                              ? `点击取消「${c.label}」过滤（${c.count} 条）`
                              : `点击只看「${c.label}」类（${c.count} 条）`
                          }
                          style={{
                            fontSize: 11,
                            padding: "2px 8px",
                            borderRadius: 10,
                            background: active ? c.bg : "var(--pet-color-card)",
                            color: active ? c.fg : "var(--pet-color-muted)",
                            border: `1px solid ${active ? c.fg : "var(--pet-color-border)"}`,
                            cursor: "pointer",
                            userSelect: "none",
                          }}
                        >
                          {active ? "✓ " : ""}
                          {c.icon} {c.label}
                          <span style={{ fontSize: 10, opacity: 0.7, marginLeft: 2 }}>
                            ({c.count})
                          </span>
                        </span>
                      );
                    })}
                    {butlerScheduleFilter.size > 0 && (
                      <button
                        type="button"
                        onClick={() => setButlerScheduleFilter(new Set())}
                        style={{
                          fontSize: 11,
                          padding: "2px 8px",
                          borderRadius: 10,
                          background: "var(--pet-color-card)",
                          color: "var(--pet-color-muted)",
                          border: "1px solid var(--pet-color-border)",
                          cursor: "pointer",
                          fontFamily: "inherit",
                        }}
                        title="清掉本段所有 schedule 过滤，恢复显示全部 butler_tasks"
                      >
                        ✕ 清除
                      </button>
                    )}
                  </div>
                );
              })()}
              {/* R102: > 10 条时默认折叠到前 5；用户点"展开全部"切到 unbounded。
                  ≤ 10 条不折叠（避免引入无用交互）。本段用 IIFE 包裹，让计数 /
                  按钮共享同一份 shownItems / isLong 状态。 */}
              {(() => {
                const CATEGORY_FOLD_THRESHOLD = 10;
                const CATEGORY_FOLD_PREVIEW = 5;
                // butler_tasks 段先按 schedule kind 过滤再走 pin / 折叠 /
                // 渲染路径。其它 category 不做 schedule 过滤（无意义）。
                // "today" 是合成 sentinel：every 永远命中 / once/deadline 当
                // 日命中，与 kind axis OR 关系（不是 AND）—— 多选 "today" +
                // "every" 等于"今日要执行 OR 每天类"。
                const scheduleFilteredItems =
                  catKey === "butler_tasks" && butlerScheduleFilter.size > 0
                    ? cat.items.filter((it) => {
                        const p = parseButlerSchedule(it.description);
                        const k = p ? p.schedule.kind : "none";
                        if (butlerScheduleFilter.has(k)) return true;
                        if (butlerScheduleFilter.has("today") && p) {
                          if (p.schedule.kind === "every") return true;
                          const now = new Date();
                          return (
                            p.schedule.year === now.getFullYear() &&
                            p.schedule.month === now.getMonth() + 1 &&
                            p.schedule.day === now.getDate()
                          );
                        }
                        return false;
                      })
                    : cat.items;
                // pin 排序：先把 pinSet 命中的 item 抓出来挂头，剩余照原序。
                // stable sort 在大多数 V8 实现已保证（ECMA 2019+），这里二
                // 分而非 .sort 以显式表达"两段拼接"语义并避开 comparator
                // 的 stability 顾虑。
                const pinned: MemoryItem[] = [];
                const rest: MemoryItem[] = [];
                for (const it of scheduleFilteredItems) {
                  if (pinnedKeys.has(`${catKey}::${it.title}`)) pinned.push(it);
                  else rest.push(it);
                }
                const sortedItems = pinned.length > 0 ? [...pinned, ...rest] : scheduleFilteredItems;
                const isLong = sortedItems.length > CATEGORY_FOLD_THRESHOLD;
                const expanded = expandedCategories.has(catKey);
                const shownItems =
                  isLong && !expanded
                    ? sortedItems.slice(0, CATEGORY_FOLD_PREVIEW)
                    : sortedItems;
                return (
                  <>
                    {shownItems.map((item, i) => {
                // Iter Cθ: only butler_tasks pays the parse cost; other categories
                // skip the work entirely. parsed === null when no schedule prefix.
                const parsed =
                  catKey === "butler_tasks" ? parseButlerSchedule(item.description) : null;
                // Iter R80: deadline tasks aren't "due" the way every/once are
                // (pet doesn't auto-execute deadlines). Skip due check for those.
                const due =
                  parsed && parsed.schedule.kind !== "deadline" && item.updated_at
                    ? isButlerDue(parsed.schedule, item.updated_at, new Date())
                    : false;
                // Iter R80: urgency tier (only meaningful for deadline kind).
                const deadlineUrgency: DeadlineUrgency | null =
                  parsed && parsed.schedule.kind === "deadline"
                    ? computeDeadlineUrgency(parsed.schedule, new Date())
                    : null;
                const errInfo =
                  catKey === "butler_tasks"
                    ? parseButlerError(item.description)
                    : { hasError: false, reason: "" };
                // ✅ 已完成 chip：与 error chip 互斥（同时存在意味着重试中
                // 状态未清；UI 以 error 为优先 —— 失败信号更重要，让用户先
                // 处理）。result 段截 30 字让 chip 不撑爆行。
                const doneInfo =
                  catKey === "butler_tasks" && !errInfo.hasError
                    ? parseButlerDone(item.description)
                    : { isDone: false, result: "" };
                const scheduleLabel = parsed
                  ? parsed.schedule.kind === "every"
                    ? `每天 ${String(parsed.schedule.hour).padStart(2, "0")}:${String(
                        parsed.schedule.minute,
                      ).padStart(2, "0")}`
                    : `${parsed.schedule.year}-${String(parsed.schedule.month).padStart(
                        2,
                        "0",
                      )}-${String(parsed.schedule.day).padStart(2, "0")} ${String(
                        parsed.schedule.hour,
                      ).padStart(2, "0")}:${String(parsed.schedule.minute).padStart(2, "0")}`
                  : null;
                // Strip schedule prefix + [error: ...] / [done] / [result: ...]
                // blocks from displayed description — chips already surface
                // these markers, no need to repeat raw bracket notation in body.
                const stripErrorBlock = (s: string): string =>
                  s.replace(/\[error[^\]]*\]\s*/i, "");
                const stripDoneBlocks = (s: string): string =>
                  s
                    .replace(/\[done(?:\]|\s[^\]]*\])\s*/gi, "")
                    .replace(/\[result\s*[:：]?\s*[^\]]*\]\s*/gi, "")
                    .trim();
                const displayDesc = (() => {
                  let base = parsed ? parsed.topic : item.description;
                  if (errInfo.hasError) base = stripErrorBlock(base).trim();
                  if (doneInfo.isDone) base = stripDoneBlocks(base);
                  return base;
                })();
                const previewActive = previewHoverKey === item.detail_path;
                const previewText = previewCache[item.detail_path];
                return (
                  <div
                    key={i}
                    className="pet-memory-item"
                    style={{ ...s.item, position: "relative" }}
                    onMouseEnter={() => startPreviewHover(item.detail_path)}
                    onMouseLeave={endPreviewHover}
                  >
                    {/* hover 500ms 浮的 detail.md 预览 tooltip。读取首字 ≤
                        600 字符；空内容时不渲染（无内容可显的"无信号"反馈
                        都没必要打扰）。 */}
                    {previewActive && previewText && previewText.length > 0 && (
                      <div
                        style={{
                          position: "absolute",
                          top: "100%",
                          left: 0,
                          right: 0,
                          marginTop: 4,
                          maxHeight: 220,
                          overflowY: "auto",
                          background: "var(--pet-color-card)",
                          border: "1px solid var(--pet-color-border)",
                          borderRadius: 6,
                          boxShadow: "0 4px 12px rgba(0,0,0,0.18)",
                          padding: "8px 10px",
                          fontSize: 11,
                          color: "var(--pet-color-fg)",
                          lineHeight: 1.5,
                          whiteSpace: "pre-wrap",
                          wordBreak: "break-word",
                          zIndex: 20,
                          pointerEvents: "none",
                          fontFamily:
                            "'SF Mono', 'Menlo', monospace",
                        }}
                      >
                        <div
                          style={{
                            fontSize: 10,
                            color: "var(--pet-color-muted)",
                            marginBottom: 4,
                          }}
                        >
                          📄 {item.detail_path}
                        </div>
                        {previewText}
                      </div>
                    )}
                    <div
                      style={{
                        display: "flex",
                        justifyContent: "space-between",
                        alignItems: "center",
                      }}
                    >
                      <div style={{ display: "flex", alignItems: "center", gap: 6, flexWrap: "wrap" }}>
                        {(() => {
                          const renameKey = `${catKey}::${item.title}`;
                          if (renamingMemoryKey === renameKey) {
                            return (
                              <input
                                autoFocus
                                type="text"
                                value={renameMemoryDraft}
                                disabled={renameMemoryBusy}
                                onChange={(e) => setRenameMemoryDraft(e.target.value)}
                                onKeyDown={(e) => {
                                  if (e.key === "Enter") {
                                    e.preventDefault();
                                    void commitRenameMemory();
                                  } else if (e.key === "Escape") {
                                    e.preventDefault();
                                    cancelRenameMemory();
                                  }
                                }}
                                onBlur={() => {
                                  void commitRenameMemory();
                                }}
                                style={{
                                  fontSize: 13,
                                  fontWeight: 600,
                                  padding: "2px 6px",
                                  border: "1px solid var(--pet-color-accent)",
                                  borderRadius: 4,
                                  background: "var(--pet-color-card)",
                                  color: "var(--pet-color-fg)",
                                  minWidth: 200,
                                  fontFamily: "inherit",
                                }}
                              />
                            );
                          }
                          return (
                            <div
                              style={{ ...s.itemTitle, cursor: "text" }}
                              onDoubleClick={() => {
                                setRenamingMemoryKey(renameKey);
                                setRenameMemoryDraft(item.title);
                              }}
                              title="双击改名"
                            >
                              {item.title}
                            </div>
                          );
                        })()}
                        {scheduleLabel && (() => {
                          // Iter R80: 4-way chip styling. every (循环) blue;
                          // once (一次性执行) amber; deadline (截止前提醒) by
                          // urgency tier — distant gray, approaching amber,
                          // imminent / overdue red so users see the urgency
                          // at a glance without parsing the date.
                          const kind = parsed!.schedule.kind;
                          let bg: string, color: string, icon: string, hint: string;
                          if (kind === "every") {
                            bg = "var(--pet-tint-blue-bg)";
                            color = "var(--pet-tint-blue-fg)";
                            icon = "🔁";
                            hint = "每日定时触发，到期后下一轮 proactive 主动开口时执行";
                          } else if (kind === "once") {
                            bg = "var(--pet-tint-yellow-bg)";
                            color = "var(--pet-tint-yellow-fg)";
                            icon = "📅";
                            hint = "单次定时触发：pet 在那个时间点自动执行";
                          } else {
                            // deadline — color by urgency
                            switch (deadlineUrgency) {
                              case "overdue":
                                bg = "var(--pet-tint-red-bg)";
                                color = "var(--pet-tint-red-fg)";
                                hint = "deadline 已过 — user 需自己完成（pet 不自动执行此类）";
                                break;
                              case "imminent":
                                bg = "var(--pet-tint-red-bg)";
                                color = "var(--pet-tint-red-fg)";
                                hint = "deadline 不到 1 小时 — pet proactive 会 override 静默原则提醒";
                                break;
                              case "approaching":
                                bg = "var(--pet-tint-yellow-bg)";
                                color = "var(--pet-tint-yellow-fg)";
                                hint = "deadline 1-6 小时 — pet 适时会提醒";
                                break;
                              default:
                                // distant / null
                                bg = "var(--pet-color-border)";
                                color = "var(--pet-color-muted)";
                                hint = "deadline 远在 6 小时之后 — 暂不打扰";
                            }
                            icon = "⏳";
                          }
                          return (
                            <span
                              style={{
                                fontSize: 10,
                                padding: "1px 6px",
                                borderRadius: 4,
                                background: bg,
                                color,
                                fontFamily: "'SF Mono', monospace",
                              }}
                              title={hint}
                            >
                              {icon} {scheduleLabel}
                            </span>
                          );
                        })()}
                        {/* ✏️ 改 schedule 快速按钮：仅 butler_tasks 有 parsed
                            schedule 时浮。点击 → 弹小 modal 编辑时间（仅
                            time / date+time 两字段，不改 kind / topic）。
                            修改 kind 走"编辑整条 description"重路径。 */}
                        {catKey === "butler_tasks" && parsed && (
                          <button
                            type="button"
                            onClick={() => {
                              const s = parsed.schedule;
                              setEditScheduleDraft({
                                title: item.title,
                                description: item.description,
                                kind: s.kind,
                                date:
                                  s.kind === "every"
                                    ? ""
                                    : `${s.year}-${String(s.month).padStart(2, "0")}-${String(s.day).padStart(2, "0")}`,
                                time: `${String(s.hour).padStart(2, "0")}:${String(s.minute).padStart(2, "0")}`,
                              });
                            }}
                            title="改这条任务的 schedule 时间（不变 kind / topic）"
                            aria-label="edit schedule"
                            style={{
                              fontSize: 10,
                              lineHeight: 1,
                              padding: "1px 5px",
                              borderRadius: 3,
                              border: "1px solid var(--pet-color-border)",
                              background: "var(--pet-color-card)",
                              color: "var(--pet-color-muted)",
                              cursor: "pointer",
                              fontFamily: "inherit",
                            }}
                          >
                            ✏️
                          </button>
                        )}
                        {/* ⏰ 下次触发：X 后 / 已过 X：让用户扫读一眼到
                            "这条还多久会跑"。仅有 parsed schedule 的 butler
                            任务才显。every 下次 = 今 / 明 HH:MM，once /
                            deadline 取绝对时间。inline 小灰字与 chip 同行
                            不抢眼。 */}
                        {catKey === "butler_tasks" && parsed && (() => {
                          const s = parsed.schedule;
                          const now = new Date();
                          let target: Date;
                          if (s.kind === "every") {
                            target = new Date(
                              now.getFullYear(),
                              now.getMonth(),
                              now.getDate(),
                              s.hour,
                              s.minute,
                            );
                            if (target.getTime() <= now.getTime()) {
                              target.setDate(target.getDate() + 1);
                            }
                          } else {
                            target = new Date(
                              s.year,
                              s.month - 1,
                              s.day,
                              s.hour,
                              s.minute,
                            );
                          }
                          const diff = target.getTime() - now.getTime();
                          let rel: string;
                          let isPast = false;
                          if (diff < 0) {
                            isPast = true;
                            const ago = -diff;
                            if (ago < 60_000) rel = "刚过";
                            else if (ago < 3_600_000)
                              rel = `已过 ${Math.floor(ago / 60_000)} 分`;
                            else if (ago < 86_400_000)
                              rel = `已过 ${Math.floor(ago / 3_600_000)} 时`;
                            else rel = `已过 ${Math.floor(ago / 86_400_000)} 天`;
                          } else if (diff < 60_000) {
                            rel = "1 分内";
                          } else if (diff < 3_600_000) {
                            rel = `${Math.floor(diff / 60_000)} 分后`;
                          } else if (diff < 86_400_000) {
                            rel = `${Math.floor(diff / 3_600_000)} 时后`;
                          } else {
                            rel = `${Math.floor(diff / 86_400_000)} 天后`;
                          }
                          return (
                            <span
                              style={{
                                fontSize: 10,
                                color: isPast
                                  ? "var(--pet-tint-orange-fg)"
                                  : "var(--pet-color-muted)",
                                fontFamily: "'SF Mono', monospace",
                              }}
                              title={
                                isPast
                                  ? `目标时刻已过：${target.toLocaleString()}（仍显是因为没被宠物处理 / 标 done）`
                                  : `下次触发：${target.toLocaleString()}`
                              }
                            >
                              ⏰ {rel}
                            </span>
                          );
                        })()}
                        {doneInfo.isDone && (
                          <span
                            style={{
                              fontSize: 10,
                              padding: "1px 6px",
                              borderRadius: 4,
                              background: "var(--pet-tint-green-bg)",
                              color: "var(--pet-tint-green-fg)",
                              fontWeight: 600,
                              border: "1px solid var(--pet-tint-green-fg)",
                              maxWidth: 260,
                              overflow: "hidden",
                              textOverflow: "ellipsis",
                              whiteSpace: "nowrap",
                            }}
                            title={
                              doneInfo.result
                                ? `LLM 已标 done。\n产物：${doneInfo.result}`
                                : "LLM 已标 done（未填具体产物 / result 段）。"
                            }
                          >
                            ✅ 已完成
                            {doneInfo.result
                              ? `：${doneInfo.result.length > 30 ? doneInfo.result.slice(0, 30) + "…" : doneInfo.result}`
                              : ""}
                          </span>
                        )}
                        {errInfo.hasError && (
                          <span style={{ display: "inline-flex", gap: 2, alignItems: "center" }}>
                            <span
                              style={{
                                fontSize: 10,
                                padding: "1px 6px",
                                borderRadius: 4,
                                background: "var(--pet-tint-red-bg)",
                                color: "var(--pet-tint-red-fg)",
                                fontWeight: 600,
                                border: "1px solid #fecaca",
                              }}
                              title={
                                errInfo.reason
                                  ? `上次执行失败：${errInfo.reason}`
                                  : "上次执行失败（LLM 没填具体原因）。检查 description 决定要不要重试。"
                              }
                            >
                              ❌ 失败{errInfo.reason ? `：${errInfo.reason.slice(0, 30)}` : ""}
                            </span>
                            <button
                              onClick={() => handleClearError(item.title, item.description)}
                              style={{
                                fontSize: 10,
                                lineHeight: 1,
                                padding: "1px 5px",
                                borderRadius: 3,
                                border: "1px solid #fecaca",
                                background: "var(--pet-color-card)",
                                color: "var(--pet-tint-red-fg)",
                                cursor: "pointer",
                              }}
                              title="清除失败标记（保留任务的 schedule 和正文，只去掉 [error: ...] 前缀）。如果你已经手动修复了原因或决定让宠物下次重试，点这个清掉红色 chip。"
                            >
                              ✕
                            </button>
                          </span>
                        )}
                        {due && (
                          <span
                            style={{
                              fontSize: 10,
                              padding: "1px 6px",
                              borderRadius: 4,
                              background: "var(--pet-tint-red-bg)",
                              color: "var(--pet-tint-red-fg)",
                              fontWeight: 600,
                            }}
                            title="计划时间已到、自上次到期后还没被宠物 update——下一次 proactive 会优先处理。"
                          >
                            ⏰ 到期
                          </span>
                        )}
                        {due &&
                          parsed &&
                          (() => {
                            const mins = overdueMinutes(parsed.schedule, now);
                            if (mins === null || mins < OVERDUE_THRESHOLD_MIN) return null;
                            return (
                              <span
                                style={{
                                  fontSize: 10,
                                  padding: "1px 6px",
                                  borderRadius: 4,
                                  background: "var(--pet-tint-yellow-bg)",
                                  color: "var(--pet-tint-yellow-fg)",
                                }}
                                title={`已过计划时刻 ${mins} 分钟 — 宠物还没动手。可能是在 quiet hours / focus / cooldown 窗口里；点上面"立即处理"可绕过 gate。`}
                              >
                                {formatOverdue(mins)}
                              </span>
                            );
                          })()}
                      </div>
                      <div style={{ display: "flex", gap: 4 }}>
                        {(() => {
                          const pinned = pinnedKeys.has(`${catKey}::${item.title}`);
                          return (
                            <button
                              style={{
                                ...s.btn,
                                ...(pinned && {
                                  background: "var(--pet-tint-yellow-bg)",
                                  color: "var(--pet-tint-yellow-fg)",
                                  border: "1px solid var(--pet-tint-yellow-fg)",
                                  fontWeight: 600,
                                }),
                              }}
                              onClick={() => togglePin(catKey, item.title)}
                              title={
                                pinned
                                  ? "取消 pin（恢复原顺序）"
                                  : "pin 到此 category 顶部（仅前端偏好，不改 memory 文件）"
                              }
                              aria-label={pinned ? "unpin memory" : "pin memory"}
                            >
                              {pinned ? "📌" : "📍"}
                            </button>
                          );
                        })()}
                        {/* ▶️ 现在跑一次：仅 butler_tasks 显。绕过 schedule /
                            cooldown / quiet hours 立即让宠物针对这一条 item 跑
                            一轮 proactive。复用全局 firingProactive in-flight
                            flag 避免连点炸 LLM；armed 二次确认防误触。 */}
                        {catKey === "butler_tasks" && (() => {
                          const armed = fireOneArmedTitle === item.title;
                          const busy = firingProactive;
                          return (
                            <button
                              type="button"
                              style={{
                                ...s.btn,
                                ...(armed
                                  ? {
                                      background: "var(--pet-tint-red-bg)",
                                      color: "var(--pet-tint-red-fg)",
                                      borderColor: "color-mix(in srgb, var(--pet-tint-red-fg) 40%, transparent)",
                                      fontWeight: 600,
                                    }
                                  : {}),
                                ...(busy && !armed
                                  ? { opacity: 0.5, cursor: "default" }
                                  : {}),
                              }}
                              disabled={busy && !armed}
                              onClick={() => void handleFireOneTask(item.title)}
                              title={
                                armed
                                  ? "再次点击确认（3s 内有效）"
                                  : "绕过 schedule / cooldown / quiet hours，让宠物现在针对这一条任务跑一轮 proactive；点击后 3s 内需再点确认。"
                              }
                              aria-label="fire this task"
                            >
                              {busy ? "处理中…" : armed ? "再点确认 (3s)" : "▶️ 现在跑"}
                            </button>
                          );
                        })()}
                        <button
                          style={s.btn}
                          onClick={() =>
                            setEditingItem({
                              category: catKey,
                              title: item.title,
                              description: item.description,
                              isNew: false,
                            })
                          }
                        >
                          编辑
                        </button>
                        {/* 🚀 在外部 markdown editor 打开 detail.md。走系统
                            默认 .md 关联（VSCode / Typora / iA Writer 等用户
                            自己选过的）。失败 → setMessage 短反馈，常见原因
                            是路径不存在（极旧 memory item 还没写 detail.md）。 */}
                        <button
                          style={s.btn}
                          onClick={async () => {
                            try {
                              await openPath(item.detail_path);
                              setMessage(`已请求系统打开 ${item.detail_path.split("/").pop()}`);
                              setTimeout(() => setMessage(""), 2500);
                            } catch (e) {
                              setMessage(`打开失败：${e}`);
                              setTimeout(() => setMessage(""), 4000);
                            }
                          }}
                          title={`用系统默认 .md 编辑器打开 ${item.detail_path}（适合大段写、想用 VSCode / Typora）`}
                          aria-label="open detail.md externally"
                        >
                          🚀
                        </button>
                        {/* 🔗 复制为 ref token：仅 butler_tasks 段显（其它
                            category 没 task ref 语义）。复制后粘到 chat 自动
                            被识别为 hover-able underline + 双击跳转源。与
                            PanelTasks 右键 ctx 菜单同款（iter #189），但 list
                            层直达 —— 用户不必先打开任务卡。 */}
                        {catKey === "butler_tasks" && (
                          <button
                            style={s.btn}
                            onClick={async () => {
                              const refToken = `「${item.title}」`;
                              try {
                                await navigator.clipboard.writeText(refToken);
                                setMessage(`已复制 ref：${refToken}`);
                              } catch (e) {
                                setMessage(`复制失败：${e}`);
                              }
                              setTimeout(() => setMessage(""), 2500);
                            }}
                            title={`复制 ${`\`「${item.title}」\``} 到剪贴板（粘到 chat 自动识别为 ref token，hover 显状态 + 双击跳源任务）`}
                            aria-label="copy task ref"
                          >
                            🔗
                          </button>
                        )}
                        {/* "📐 复制 schedule" 按钮：仅 butler_tasks 且 parsed
                            schedule 时浮。复制完整 `[kind: ...] topic` 形态，
                            适合迁移 / 备份 / 改造（粘到 PanelTasks 派单做单
                            次执行版本，或粘到外部 .md 备份）。 */}
                        {catKey === "butler_tasks" && parsed && (
                          <button
                            style={s.btn}
                            onClick={async () => {
                              const s = parsed.schedule;
                              const prefix =
                                s.kind === "every"
                                  ? `[every: ${String(s.hour).padStart(2, "0")}:${String(s.minute).padStart(2, "0")}]`
                                  : `[${s.kind}: ${s.year}-${String(s.month).padStart(2, "0")}-${String(s.day).padStart(2, "0")} ${String(s.hour).padStart(2, "0")}:${String(s.minute).padStart(2, "0")}]`;
                              const full = `${prefix} ${parsed.topic}`;
                              try {
                                await navigator.clipboard.writeText(full);
                                setMessage(`已复制完整 schedule：${full.slice(0, 40)}…`);
                              } catch (e) {
                                setMessage(`复制失败：${e}`);
                              }
                              setTimeout(() => setMessage(""), 2500);
                            }}
                            title="复制完整 [kind: ...] prefix + topic 文本（不含 [error] / [done] / [result] 等附加 marker）。迁移 / 备份 / 粘到 PanelTasks 派一次性变体用。"
                            aria-label="copy full schedule prefix + topic"
                          >
                            📐
                          </button>
                        )}
                        {/* 📋 复制 detail.md 全文：仅在 detailSizes 已知且 > 0
                            字时浮。与 hover preview 600 字截断 / 🚀 外部编辑
                            互补 —— 这条走系统剪贴板，让用户即时粘到外部 markdown
                            笔记 / chat / issue。读不到（IO 失败 / 0 字）就 toast
                            提示，不阻塞。 */}
                        {(detailSizes[item.detail_path] ?? 0) > 0 && (
                          <button
                            style={s.btn}
                            onClick={async () => {
                              try {
                                const content = await invoke<string>(
                                  "memory_read_detail_full",
                                  { detailPath: item.detail_path },
                                );
                                if (!content) {
                                  setMessage("detail.md 内容为空 / 读不到");
                                } else {
                                  await navigator.clipboard.writeText(content);
                                  const len = Array.from(content).length;
                                  setMessage(`已复制 detail.md 全文（${len} 字）`);
                                }
                              } catch (e) {
                                setMessage(`复制失败：${e}`);
                              }
                              setTimeout(() => setMessage(""), 3000);
                            }}
                            title={`复制 ${item.detail_path} 全文到剪贴板（不截断；hover preview 是 600 字截断版）`}
                            aria-label="copy detail.md full content"
                          >
                            📋
                          </button>
                        )}
                        {/* 📝 复制本条整段 markdown：把 item 的 title /
                            description / detail_path / 字数 / 更新时间打包
                            成 H2 段 + meta + body 的 markdown。与 📋 detail
                            全文（仅 detail.md body）和 🔗 ref（只 title）互
                            补；这条覆盖"完整 share / 提 issue"场景。
                            detail.md 内容也可选包入（已知 detail size > 0
                            时一并 fetch）。 */}
                        <button
                          style={s.btn}
                          onClick={async () => {
                            // 先组装 sync 部分（title / desc / path / 时间）
                            const lines: string[] = [
                              `## ${item.title}`,
                              `- 分类：${cat.label} (\`${catKey}\`)`,
                              `- 更新时间：${item.updated_at?.slice(0, 16).replace("T", " ") || "—"}`,
                              `- detail_path：\`${item.detail_path || "—"}\``,
                            ];
                            const size = detailSizes[item.detail_path] ?? 0;
                            if (size > 0) {
                              lines.push(`- detail.md 字数：${size}`);
                            }
                            lines.push("", "### Description", "");
                            lines.push(item.description || "（空）");
                            // 若 detail 非空，async 拉全文并 append 一个段
                            if (size > 0) {
                              try {
                                const content = await invoke<string>(
                                  "memory_read_detail_full",
                                  { detailPath: item.detail_path },
                                );
                                if (content) {
                                  lines.push("", "### detail.md", "");
                                  lines.push(content);
                                }
                              } catch {
                                // 单点 fetch 失败容忍 —— sync 部分仍可用
                                lines.push("", "### detail.md", "（读取失败）");
                              }
                            }
                            const md = lines.join("\n");
                            try {
                              await navigator.clipboard.writeText(md);
                              setMessage(`已复制本条 markdown（${md.length} 字符）`);
                            } catch (e) {
                              setMessage(`复制失败：${e}`);
                            }
                            setTimeout(() => setMessage(""), 3000);
                          }}
                          title={`把 "${item.title}" 整段（标题 / 元数据 / description / detail.md 全文）拼成 markdown 复制到剪贴板。share / 提 issue / 提 review 时一次粘贴够。`}
                          aria-label="copy full memory item as markdown"
                        >
                          📝
                        </button>
                        {(() => {
                          const armed =
                            armedDeleteKey === `${catKey}::${item.title}`;
                          return (
                            <button
                              style={
                                armed
                                  ? {
                                      ...s.btnDanger,
                                      background: "var(--pet-tint-red-fg)",
                                      color: "#fff",
                                      borderColor: "var(--pet-tint-red-fg)",
                                      fontWeight: 600,
                                    }
                                  : s.btnDanger
                              }
                              onClick={() =>
                                handleDelete(catKey, item.title)
                              }
                              title={
                                armed
                                  ? "再次点击确认删除（3s 后撤销）"
                                  : "点击删除（再点一次确认）"
                              }
                            >
                              {armed ? "确认删除" : "删除"}
                            </button>
                          );
                        })()}
                      </div>
                    </div>
                    <div style={s.itemDesc}>
                      {/* `「task title」` ref token 渲 hover preview / 双击导航
                          （与 PanelChat 同款）。helper 在没 ref 命中时 fast-path
                          返 parseUrls(content) —— 顺便给 memory 描述里偶发的
                          URL 也加蓝下划线，比原 plain text 强。 */}
                      {renderContentWithTaskRefs(
                        displayDesc,
                        refTaskMap,
                        onRequestFocusTask,
                      )}
                    </div>
                    <div style={s.itemMeta}>
                      {item.detail_path} | 更新于 {item.updated_at?.slice(0, 16).replace("T", " ")}
                      {/* "detail X 字"：仅 size > 0 且后端拉到时显。配色
                          走与编辑态 counter 同档（< 2000 muted / 2000-5000
                          amber / > 5000 red 加粗），让用户在 list 一眼看
                          到哪条 detail 已经偏长。0 / 缺失 → 不显（避免
                          "0 字"占视觉位）。 */}
                      {(() => {
                        const size = detailSizes[item.detail_path];
                        if (!size || size === 0) return null;
                        const longish = size > 2000;
                        const danger = size > 5000;
                        return (
                          <span
                            style={{
                              marginLeft: 6,
                              color: danger
                                ? "var(--pet-tint-red-fg)"
                                : longish
                                  ? "var(--pet-tint-yellow-fg)"
                                  : "var(--pet-color-muted)",
                              fontWeight: danger ? 600 : undefined,
                            }}
                            title={
                              danger
                                ? `detail.md 已 ${size} 字，建议精简（编辑面板 > 5000 字会浮 banner 提示原因）`
                                : longish
                                  ? `detail.md ${size} 字（> 2000 字提醒留意长度）`
                                  : `detail.md 共 ${size} 字`
                            }
                          >
                            · 📄 {size} 字
                          </span>
                        );
                      })()}
                    </div>
                  </div>
                );
                    })}
                    {isLong && (
                      <button
                        type="button"
                        onClick={() =>
                          setExpandedCategories((prev) => {
                            const next = new Set(prev);
                            if (next.has(catKey)) next.delete(catKey);
                            else next.add(catKey);
                            try {
                              window.localStorage.setItem(
                                "pet-memory-expanded-cats",
                                JSON.stringify([...next]),
                              );
                            } catch {
                              // 私密浏览 / 配额满 — 本次仍生效，下次启动丢
                            }
                            return next;
                          })
                        }
                        title={
                          expanded
                            ? `折叠回前 ${CATEGORY_FOLD_PREVIEW} 条`
                            : // 未展开态：把隐藏的 N-PREVIEW 条 title 列在 tooltip
                              // 里让用户展开前先瞄一眼。控制总长 max 20 行避免
                              // tooltip 撑爆屏幕。
                              `展开后显示全部 ${cat.items.length} 条\n\n` +
                              `隐藏的 ${cat.items.length - CATEGORY_FOLD_PREVIEW} 条：\n` +
                              cat.items
                                .slice(CATEGORY_FOLD_PREVIEW)
                                .slice(0, 20)
                                .map((it) => `- ${it.title}`)
                                .join("\n") +
                              (cat.items.length > CATEGORY_FOLD_PREVIEW + 20
                                ? `\n... 还有 ${cat.items.length - CATEGORY_FOLD_PREVIEW - 20} 条`
                                : "")
                        }
                        style={{
                          marginTop: 4,
                          fontSize: 11,
                          padding: "2px 8px",
                          border: "none",
                          background: "transparent",
                          color: "var(--pet-color-accent)",
                          cursor: "pointer",
                          fontFamily: "inherit",
                        }}
                      >
                        {expanded
                          ? `收起 (${cat.items.length})`
                          : `… 展开全部 ${cat.items.length} 条`}
                      </button>
                    )}
                  </>
                );
              })()}
            </div>
          );
        })}
    </div>
  );
}

/// R98: index → markdown 导出。H1 标题 + ts/总数 摘要；H2 = category（cat.label
/// 中文名）；H3 = item title + blockquote 更新时间 + 描述正文（保留 schedule
/// 前缀如 [every: 09:00]）。空 category 跳过避免占行。先按 CATEGORY_ORDER 列
/// 出，再追加任何 ORDER 外的 category（后端将来新增时不丢数据）。
function exportMemoriesAsMarkdown(idx: MemoryIndex): string {
  const lines: string[] = [];
  const now = new Date();
  const totalItems = Object.values(idx.categories).reduce(
    (sum, c) => sum + c.items.length,
    0,
  );
  lines.push("# 宠物记忆全部导出");
  lines.push(`> 导出时间: ${now.toLocaleString()} · 共 ${totalItems} 条`);
  lines.push("");
  const orderedKeys = [
    ...CATEGORY_ORDER,
    ...Object.keys(idx.categories).filter((k) => !CATEGORY_ORDER.includes(k)),
  ];
  for (const catKey of orderedKeys) {
    const cat = idx.categories[catKey];
    if (!cat || cat.items.length === 0) continue;
    lines.push(`## ${cat.label} (${cat.items.length} 条)`);
    lines.push("");
    for (const item of cat.items) {
      lines.push(`### ${item.title}`);
      if (item.updated_at) {
        lines.push(
          `> 更新于 ${item.updated_at.slice(0, 16).replace("T", " ")}`,
        );
      }
      lines.push("");
      lines.push(item.description);
      lines.push("");
    }
  }
  return lines.join("\n");
}

/// R92: cat 最新更新相对时间文案。与 PanelTasks `formatRelativeAge` 同款
/// 分级（minute / hour / day），后缀 "更新" 贴 category 语义（vs Tasks
/// "前创建"）。调用前已保证 latestTs 非 null（空 cat 时 header 不渲染）。
/// 字节数格式化为人友好的 KB / MB / GB 字符串。基数 1024（与 macOS Finder /
/// Linux du -h 习惯一致）；小于 1KB 直接 `N B`。1 位小数足够，多 1 位精度
/// 在"该不该 consolidate"的判断上无价值。
function formatBytes(n: number): string {
  if (!Number.isFinite(n) || n < 0) return "0 B";
  if (n < 1024) return `${n} B`;
  const kb = n / 1024;
  if (kb < 1024) return `${kb.toFixed(1)} KB`;
  const mb = kb / 1024;
  if (mb < 1024) return `${mb.toFixed(1)} MB`;
  const gb = mb / 1024;
  return `${gb.toFixed(1)} GB`;
}

function formatLastUpdated(latestTs: number, now: number): string {
  const age = now - latestTs;
  if (age < 60_000) return "刚刚更新";
  if (age < 3_600_000) return `${Math.floor(age / 60_000)} 分钟前更新`;
  if (age < 86_400_000) return `${Math.floor(age / 3_600_000)} 小时前更新`;
  return `${Math.floor(age / 86_400_000)} 天前更新`;
}

/// R88: 搜索结果黄底高亮。与 PanelTasks / PanelSettings 同款（黄底深棕字），
/// 让"panel 内搜索高亮"风格统一。仅命中第一处子串；query 用当前 input 值
/// （结果 stale 时 idx<0 自然降级为原文）。
const HIGHLIGHT_MARK_STYLE: React.CSSProperties = {
  background: "var(--pet-tint-yellow-bg)",
  color: "var(--pet-tint-yellow-fg)",
  padding: "0 1px",
  borderRadius: 2,
};

function HighlightedText({ text, query }: { text: string; query: string }) {
  const q = query.trim();
  if (q.length === 0) return <>{text}</>;
  const idx = text.toLowerCase().indexOf(q.toLowerCase());
  if (idx < 0) return <>{text}</>;
  return (
    <>
      {text.slice(0, idx)}
      <mark style={HIGHLIGHT_MARK_STYLE}>{text.slice(idx, idx + q.length)}</mark>
      {text.slice(idx + q.length)}
    </>
  );
}
