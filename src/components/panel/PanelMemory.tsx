import { useState, useEffect, useMemo, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";

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

const CATEGORY_ORDER = ["butler_tasks", "todo", "ai_insights", "user_profile", "general"];

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

export function PanelMemory() {
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
  const [consolidating, setConsolidating] = useState(false);
  const [butlerHistory, setButlerHistory] = useState<string[]>([]);
  const [butlerDaily, setButlerDaily] = useState<string[]>([]);
  const [firingProactive, setFiringProactive] = useState(false);
  // R137: "立即处理" 二次确认 armed 态（与 R125 PanelDebug 立即开口同模式）。
  // 首点 armed 3s 自动 revert；再点真触发。firingProactive 是请求 in-flight
  // flag，与 armed 各管一半（armed 在 click 前 / firing 在 invoke 期间）。
  const [fireArmed, setFireArmed] = useState(false);
  // R95: butler 最近执行折叠状态。> 5 条时默认折叠到前 5（最新），用户点
  // "展开全部 N 条"切到 unbounded。session 内有效，关面板复位（与 R91
  // 长描述折叠同语义）。
  const [butlerHistoryExpanded, setButlerHistoryExpanded] = useState(false);
  // R143: butler 每日小结折叠状态（与 butlerHistoryExpanded 同模式独立）。
  // 长跑用户多日累积，不折叠时挤压下方任务列表。
  const [butlerDailyExpanded, setButlerDailyExpanded] = useState(false);
  // R102: 哪些 category 已被用户展开。默认 empty —— 所有 cat 走"自动折叠
  // 规则"（> 10 条时折叠到前 5）。手动 toggle 进入 set 即始终展开。
  const [expandedCategories, setExpandedCategories] = useState<Set<string>>(
    new Set(),
  );
  // R118: butler_tasks schedule 模板插入用 ref 拿 textarea 光标位置。仅
  // butler_tasks category 模板按钮可见时使用。
  const descTextareaRef = useRef<HTMLTextAreaElement>(null);
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

  const handleConsolidate = async () => {
    setConsolidating(true);
    setMessage("正在整理记忆，请稍候…");
    try {
      const status = await invoke<string>("trigger_consolidate");
      setMessage(status);
      await loadIndex();
    } catch (e: any) {
      setMessage(`整理失败: ${e}`);
    } finally {
      setConsolidating(false);
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

  const handleDelete = async (category: string, title: string) => {
    if (!confirm(`确认删除 "${title}"？`)) return;
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
    return <div style={{ padding: 20, color: "var(--pet-color-muted)" }}>加载中...</div>;
  }

  const s = {
    container: { padding: 16, overflowY: "auto" as const, height: "100%", fontFamily: "system-ui, sans-serif" },
    section: { marginBottom: 20 },
    sectionTitle: { fontSize: 14, fontWeight: 600, color: "var(--pet-color-fg)", marginBottom: 8, display: "flex", alignItems: "center", gap: 8 },
    badge: { fontSize: 11, background: "var(--pet-color-border)", color: "var(--pet-color-muted)", borderRadius: 10, padding: "1px 8px" },
    item: { padding: "8px 12px", background: "var(--pet-color-card)", border: "1px solid var(--pet-color-border)", borderRadius: 6, marginBottom: 6, fontSize: 13 },
    itemTitle: { fontWeight: 600, color: "var(--pet-color-fg)", marginBottom: 2 },
    itemDesc: { color: "var(--pet-color-muted)", fontSize: 12, lineHeight: 1.4 },
    itemMeta: { color: "var(--pet-color-muted)", fontSize: 11, marginTop: 4 },
    btn: { padding: "4px 10px", border: "1px solid var(--pet-color-border)", borderRadius: 4, background: "var(--pet-color-card)", color: "var(--pet-color-muted)", cursor: "pointer", fontSize: 12 },
    btnDanger: { padding: "4px 10px", border: "1px solid #fecaca", borderRadius: 4, background: "var(--pet-color-card)", color: "#ef4444", cursor: "pointer", fontSize: 12 },
    btnPrimary: { padding: "6px 16px", border: "none", borderRadius: 4, background: "var(--pet-color-accent)", color: "#fff", cursor: "pointer", fontSize: 13 },
    input: { width: "100%", padding: "6px 10px", border: "1px solid var(--pet-color-border)", borderRadius: 4, fontSize: 13, boxSizing: "border-box" as const, background: "var(--pet-color-card)", color: "var(--pet-color-fg)" },
    textarea: { width: "100%", padding: "6px 10px", border: "1px solid var(--pet-color-border)", borderRadius: 4, fontSize: 13, resize: "vertical" as const, minHeight: 60, boxSizing: "border-box" as const, background: "var(--pet-color-card)", color: "var(--pet-color-fg)" },
    searchRow: { display: "flex", gap: 8, marginBottom: 16 },
    msg: { padding: "6px 12px", background: "#f0fdf4", color: "#166534", borderRadius: 4, fontSize: 12, marginBottom: 12 },
  };

  return (
    <div style={s.container}>
      {/* R122: items 列表 hover 高亮。inline style 不支持 :hover 伪类，
          走 className + 全局 <style> block + !important 反压 inline 优先级。
          配色用 var(--pet-color-bg) 与 card 反差一档，跨主题自动切。 */}
      <style>
        {`
          .pet-memory-item {
            transition: background-color 0.12s ease;
          }
          .pet-memory-item:hover {
            background: var(--pet-color-bg) !important;
          }
        `}
      </style>
      {message && (
        <div style={s.msg} onClick={() => setMessage("")}>
          {message}
        </div>
      )}

      {/* Search */}
      <div style={s.searchRow}>
        <input
          style={{ ...s.input, flex: 1 }}
          placeholder="搜索记忆..."
          value={searchKeyword}
          onChange={(e) => setSearchKeyword(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && handleSearch()}
        />
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
            background: consolidating ? "#94a3b8" : "#8b5cf6",
            color: "#fff",
          }}
          onClick={handleConsolidate}
          disabled={consolidating}
          title="立即让 LLM 检查并整理记忆（合并重复 / 删过期 todo / 清 stale reminder），不必等定时触发。"
        >
          {consolidating ? "整理中…" : "立即整理"}
        </button>
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
      </div>

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

      {/* Edit modal */}
      {editingItem && (
        <div
          style={{
            position: "fixed",
            inset: 0,
            background: "rgba(0,0,0,0.3)",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            zIndex: 100,
          }}
          onClick={() => setEditingItem(null)}
        >
          <div
            style={{ background: "var(--pet-color-card)", borderRadius: 8, padding: 20, width: 400, maxWidth: "90%" }}
            onClick={(e) => e.stopPropagation()}
          >
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
                    ? "#dc2626"
                    : len >= WARN
                      ? "#a16207"
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
                  style={{ display: "flex", gap: 4, marginTop: 4, marginBottom: 4 }}
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
                    ? "#dc2626"
                    : len >= WARN
                      ? "#a16207"
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
          </div>
        </div>
      )}

      {/* Categories */}
      {searchResults === null &&
        index &&
        CATEGORY_ORDER.map((catKey) => {
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
          return (
            <div key={catKey} style={s.section}>
              <div style={s.sectionTitle}>
                {cat.label}
                <span style={s.badge}>{cat.items.length}</span>
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
                        ? "#94a3b8"
                        : fireArmed
                          ? "#fef2f2"
                          : "#ef4444",
                      color: firingProactive
                        ? "#fff"
                        : fireArmed
                          ? "#b91c1c"
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
                <button
                  style={{ ...s.btn, marginLeft: catKey === "butler_tasks" && overdueCount > 0 ? 4 : "auto" }}
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
                      const actionColor = p.action === "delete" ? "#dc2626" : "#0d9488";
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
                <div style={{ color: "var(--pet-color-muted)", fontSize: 12, paddingLeft: 4 }}>暂无记忆</div>
              )}
              {/* R102: > 10 条时默认折叠到前 5；用户点"展开全部"切到 unbounded。
                  ≤ 10 条不折叠（避免引入无用交互）。本段用 IIFE 包裹，让计数 /
                  按钮共享同一份 shownItems / isLong 状态。 */}
              {(() => {
                const CATEGORY_FOLD_THRESHOLD = 10;
                const CATEGORY_FOLD_PREVIEW = 5;
                const isLong = cat.items.length > CATEGORY_FOLD_THRESHOLD;
                const expanded = expandedCategories.has(catKey);
                const shownItems =
                  isLong && !expanded
                    ? cat.items.slice(0, CATEGORY_FOLD_PREVIEW)
                    : cat.items;
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
                // Strip schedule prefix + [error: ...] block from displayed
                // description — chips already surface both, no need to repeat
                // the raw bracket notation in the body.
                const stripErrorBlock = (s: string): string =>
                  s.replace(/\[error[^\]]*\]\s*/i, "");
                const displayDesc = (() => {
                  const base = parsed ? parsed.topic : item.description;
                  return errInfo.hasError ? stripErrorBlock(base).trim() : base;
                })();
                return (
                  <div key={i} className="pet-memory-item" style={s.item}>
                    <div
                      style={{
                        display: "flex",
                        justifyContent: "space-between",
                        alignItems: "center",
                      }}
                    >
                      <div style={{ display: "flex", alignItems: "center", gap: 6, flexWrap: "wrap" }}>
                        <div style={s.itemTitle}>{item.title}</div>
                        {scheduleLabel && (() => {
                          // Iter R80: 4-way chip styling. every (循环) blue;
                          // once (一次性执行) amber; deadline (截止前提醒) by
                          // urgency tier — distant gray, approaching amber,
                          // imminent / overdue red so users see the urgency
                          // at a glance without parsing the date.
                          const kind = parsed!.schedule.kind;
                          let bg: string, color: string, icon: string, hint: string;
                          if (kind === "every") {
                            bg = "#dbeafe";
                            color = "#1e40af";
                            icon = "🔁";
                            hint = "每日定时触发，到期后下一轮 proactive 主动开口时执行";
                          } else if (kind === "once") {
                            bg = "#fef3c7";
                            color = "#92400e";
                            icon = "📅";
                            hint = "单次定时触发：pet 在那个时间点自动执行";
                          } else {
                            // deadline — color by urgency
                            switch (deadlineUrgency) {
                              case "overdue":
                                bg = "#fee2e2";
                                color = "#991b1b";
                                hint = "deadline 已过 — user 需自己完成（pet 不自动执行此类）";
                                break;
                              case "imminent":
                                bg = "#fee2e2";
                                color = "#b91c1c";
                                hint = "deadline 不到 1 小时 — pet proactive 会 override 静默原则提醒";
                                break;
                              case "approaching":
                                bg = "#fef3c7";
                                color = "#92400e";
                                hint = "deadline 1-6 小时 — pet 适时会提醒";
                                break;
                              default:
                                // distant / null
                                bg = "#e2e8f0";
                                color = "#475569";
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
                        {errInfo.hasError && (
                          <span style={{ display: "inline-flex", gap: 2, alignItems: "center" }}>
                            <span
                              style={{
                                fontSize: 10,
                                padding: "1px 6px",
                                borderRadius: 4,
                                background: "#fef2f2",
                                color: "#991b1b",
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
                                background: "#fff",
                                color: "#991b1b",
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
                              background: "#fee2e2",
                              color: "#b91c1c",
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
                                  background: "#fef3c7",
                                  color: "#92400e",
                                }}
                                title={`已过计划时刻 ${mins} 分钟 — 宠物还没动手。可能是在 quiet hours / focus / cooldown 窗口里；点上面"立即处理"可绕过 gate。`}
                              >
                                {formatOverdue(mins)}
                              </span>
                            );
                          })()}
                      </div>
                      <div style={{ display: "flex", gap: 4 }}>
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
                        <button
                          style={s.btnDanger}
                          onClick={() => handleDelete(catKey, item.title)}
                        >
                          删除
                        </button>
                      </div>
                    </div>
                    <div style={s.itemDesc}>{displayDesc}</div>
                    <div style={s.itemMeta}>
                      {item.detail_path} | 更新于 {item.updated_at?.slice(0, 16).replace("T", " ")}
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
                            return next;
                          })
                        }
                        title={
                          expanded
                            ? `折叠回前 ${CATEGORY_FOLD_PREVIEW} 条`
                            : `展开后显示全部 ${cat.items.length} 条`
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
  background: "#fef3c7",
  color: "#92400e",
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
