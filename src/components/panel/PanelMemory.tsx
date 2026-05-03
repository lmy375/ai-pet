import { useState, useEffect } from "react";
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
const CATEGORY_PLACEHOLDERS: Record<string, string> = {
  butler_tasks:
    "比如：[every: 09:00] 把今日日历汇总写到 ~/today.md\n或：[once: 2026-05-10 14:00] 周末整理 ~/Downloads\n或：直接写「整理 ~/Downloads，把 30 天旧文件挪到 ~/Archive」（不带前缀就由宠物自己判断时机）。\n（描述里说清楚做什么、多久做一次、写到哪里。）",
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

  // ---- Iter Cθ: schedule-aware rendering for butler_tasks items ---------------
  // Pure TS mirror of proactive.rs::parse_butler_schedule_prefix + is_butler_due.
  // Lets the panel render `[every: HH:MM]` / `[once: ...]` as a chip and flag due
  // tasks in real time, instead of users needing to do the math themselves.
  type ButlerSchedule =
    | { kind: "every"; hour: number; minute: number }
    | { kind: "once"; year: number; month: number; day: number; hour: number; minute: number };

  const parseButlerSchedule = (desc: string): { schedule: ButlerSchedule; topic: string } | null => {
    const trimmed = desc.replace(/^\s+/, "");
    const m = trimmed.match(/^\[(every|once):\s*([^\]]+)\]\s*(.*)$/);
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
    // once
    const dt = body.trim().match(/^(\d{4})-(\d{2})-(\d{2})\s+(\d{1,2}):(\d{1,2})$/);
    if (!dt) return null;
    return {
      schedule: {
        kind: "once",
        year: Number(dt[1]),
        month: Number(dt[2]),
        day: Number(dt[3]),
        hour: Number(dt[4]),
        minute: Number(dt[5]),
      },
      topic: topic.trim(),
    };
  };

  const mostRecentFire = (schedule: ButlerSchedule, now: Date): Date | null => {
    if (schedule.kind === "once") {
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
    try {
      if (editingItem.isNew) {
        await invoke("memory_edit", {
          action: "create",
          category: editingItem.category,
          title: editingItem.title,
          description: editingItem.description,
        });
        setMessage("已创建");
      } else {
        await invoke("memory_edit", {
          action: "update",
          category: editingItem.category,
          title: editingItem.title,
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
    return <div style={{ padding: 20, color: "#64748b" }}>加载中...</div>;
  }

  const s = {
    container: { padding: 16, overflowY: "auto" as const, height: "100%", fontFamily: "system-ui, sans-serif" },
    section: { marginBottom: 20 },
    sectionTitle: { fontSize: 14, fontWeight: 600, color: "#334155", marginBottom: 8, display: "flex", alignItems: "center", gap: 8 },
    badge: { fontSize: 11, background: "#e2e8f0", color: "#64748b", borderRadius: 10, padding: "1px 8px" },
    item: { padding: "8px 12px", background: "#fff", border: "1px solid #e2e8f0", borderRadius: 6, marginBottom: 6, fontSize: 13 },
    itemTitle: { fontWeight: 600, color: "#1e293b", marginBottom: 2 },
    itemDesc: { color: "#64748b", fontSize: 12, lineHeight: 1.4 },
    itemMeta: { color: "#94a3b8", fontSize: 11, marginTop: 4 },
    btn: { padding: "4px 10px", border: "1px solid #e2e8f0", borderRadius: 4, background: "#fff", color: "#64748b", cursor: "pointer", fontSize: 12 },
    btnDanger: { padding: "4px 10px", border: "1px solid #fecaca", borderRadius: 4, background: "#fff", color: "#ef4444", cursor: "pointer", fontSize: 12 },
    btnPrimary: { padding: "6px 16px", border: "none", borderRadius: 4, background: "#0ea5e9", color: "#fff", cursor: "pointer", fontSize: 13 },
    input: { width: "100%", padding: "6px 10px", border: "1px solid #e2e8f0", borderRadius: 4, fontSize: 13, boxSizing: "border-box" as const },
    textarea: { width: "100%", padding: "6px 10px", border: "1px solid #e2e8f0", borderRadius: 4, fontSize: 13, resize: "vertical" as const, minHeight: 60, boxSizing: "border-box" as const },
    searchRow: { display: "flex", gap: 8, marginBottom: 16 },
    msg: { padding: "6px 12px", background: "#f0fdf4", color: "#166534", borderRadius: 4, fontSize: 12, marginBottom: 12 },
  };

  return (
    <div style={s.container}>
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
            background: "#0ea5e9",
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
      </div>

      {/* Search results */}
      {searchResults !== null && (
        <div style={s.section}>
          <div style={s.sectionTitle}>
            搜索结果 <span style={s.badge}>{searchResults.length}</span>
          </div>
          {searchResults.length === 0 && (
            <div style={{ color: "#94a3b8", fontSize: 13 }}>未找到匹配项</div>
          )}
          {searchResults.map((r, i) => (
            <div key={i} style={s.item}>
              <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
                <div style={s.itemTitle}>{r.title}</div>
                <span style={s.badge}>{r.category}</span>
              </div>
              <div style={s.itemDesc}>{r.description}</div>
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
            style={{ background: "#fff", borderRadius: 8, padding: 20, width: 400, maxWidth: "90%" }}
            onClick={(e) => e.stopPropagation()}
          >
            <div style={{ fontSize: 15, fontWeight: 600, marginBottom: 12 }}>
              {editingItem.isNew ? "新建记忆" : "编辑记忆"}
            </div>
            <div style={{ marginBottom: 8 }}>
              <label style={{ fontSize: 12, color: "#64748b" }}>分类</label>
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
              <label style={{ fontSize: 12, color: "#64748b" }}>标题</label>
              <input
                style={s.input}
                maxLength={20}
                value={editingItem.title}
                onChange={(e) => setEditingItem({ ...editingItem, title: e.target.value })}
                disabled={!editingItem.isNew}
              />
            </div>
            <div style={{ marginBottom: 12 }}>
              <label style={{ fontSize: 12, color: "#64748b" }}>描述</label>
              <textarea
                style={{ ...s.textarea, minHeight: editingItem.category === "butler_tasks" ? 100 : 60 }}
                maxLength={300}
                placeholder={CATEGORY_PLACEHOLDERS[editingItem.category] || ""}
                value={editingItem.description}
                onChange={(e) => setEditingItem({ ...editingItem, description: e.target.value })}
              />
            </div>
            <div style={{ display: "flex", gap: 8, justifyContent: "flex-end" }}>
              <button style={s.btn} onClick={() => setEditingItem(null)}>
                取消
              </button>
              <button style={s.btnPrimary} onClick={handleSaveEdit}>
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
          return (
            <div key={catKey} style={s.section}>
              <div style={s.sectionTitle}>
                {cat.label}
                <span style={s.badge}>{cat.items.length}</span>
                {catKey === "butler_tasks" && overdueCount > 0 && (
                  <button
                    style={{
                      ...s.btn,
                      background: firingProactive ? "#94a3b8" : "#ef4444",
                      color: "#fff",
                      borderColor: "transparent",
                      marginLeft: 8,
                    }}
                    onClick={handleFireProactive}
                    disabled={firingProactive}
                    title={`${overdueCount} 个任务已过期超过 ${OVERDUE_THRESHOLD_MIN} 分钟。点击立即触发一次主动开口（绕过 cooldown / quiet hours），让宠物现在去看任务列表并选一项处理。`}
                  >
                    {firingProactive ? "处理中…" : `立即处理 (${overdueCount})`}
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
                    background: "#fefce8",
                    border: "1px solid #fde68a",
                    borderRadius: 6,
                    padding: "8px 10px",
                    marginBottom: 8,
                  }}
                >
                  <div style={{ fontSize: 11, color: "#a16207", marginBottom: 4, fontWeight: 600 }}>
                    每日小结 ({butlerDaily.length})
                  </div>
                  {butlerDaily
                    .slice()
                    .reverse()
                    .map((line, i) => {
                      const firstSpace = line.indexOf(" ");
                      const date = firstSpace > 0 ? line.slice(0, firstSpace) : "";
                      const text = firstSpace > 0 ? line.slice(firstSpace + 1) : line;
                      return (
                        <div
                          key={i}
                          style={{
                            fontSize: 12,
                            color: "#374151",
                            marginTop: 2,
                            display: "flex",
                            gap: 6,
                            alignItems: "baseline",
                          }}
                        >
                          <span style={{ color: "#a16207", fontFamily: "'SF Mono', monospace", fontSize: 11 }}>
                            {date}
                          </span>
                          <span style={{ flex: 1 }}>{text}</span>
                        </div>
                      );
                    })}
                </div>
              )}
              {/* Iter Cε: butler_tasks gets a "最近执行" mini-timeline showing the
                  last few times the LLM updated/deleted a task — closes the
                  feedback loop between assignment and execution. */}
              {catKey === "butler_tasks" && butlerHistory.length > 0 && (
                <div
                  style={{
                    background: "#f0f9ff",
                    border: "1px solid #bae6fd",
                    borderRadius: 6,
                    padding: "8px 10px",
                    marginBottom: 8,
                  }}
                >
                  <div style={{ fontSize: 11, color: "#0369a1", marginBottom: 4, fontWeight: 600 }}>
                    最近执行 ({butlerHistory.length})
                  </div>
                  {butlerHistory
                    .slice()
                    .reverse()
                    .map((line, i) => {
                      const p = parseButlerLine(line);
                      const when = p.ts.slice(5, 16).replace("T", " ");
                      const actionColor = p.action === "delete" ? "#dc2626" : "#0d9488";
                      return (
                        <div
                          key={i}
                          style={{
                            fontSize: 11,
                            color: "#475569",
                            marginTop: 2,
                            display: "flex",
                            gap: 6,
                            alignItems: "baseline",
                          }}
                        >
                          <span style={{ color: "#94a3b8", fontFamily: "'SF Mono', monospace" }}>
                            {when}
                          </span>
                          <span style={{ color: actionColor, fontWeight: 600 }}>{p.action}</span>
                          <span style={{ fontWeight: 500 }}>{p.title}</span>
                          {p.desc && (
                            <span
                              style={{
                                color: "#64748b",
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
                </div>
              )}
              {cat.items.length === 0 && (
                <div style={{ color: "#94a3b8", fontSize: 12, paddingLeft: 4 }}>暂无记忆</div>
              )}
              {cat.items.map((item, i) => {
                // Iter Cθ: only butler_tasks pays the parse cost; other categories
                // skip the work entirely. parsed === null when no schedule prefix.
                const parsed =
                  catKey === "butler_tasks" ? parseButlerSchedule(item.description) : null;
                const due =
                  parsed && item.updated_at
                    ? isButlerDue(parsed.schedule, item.updated_at, new Date())
                    : false;
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
                  <div key={i} style={s.item}>
                    <div
                      style={{
                        display: "flex",
                        justifyContent: "space-between",
                        alignItems: "center",
                      }}
                    >
                      <div style={{ display: "flex", alignItems: "center", gap: 6, flexWrap: "wrap" }}>
                        <div style={s.itemTitle}>{item.title}</div>
                        {scheduleLabel && (
                          <span
                            style={{
                              fontSize: 10,
                              padding: "1px 6px",
                              borderRadius: 4,
                              background: parsed!.schedule.kind === "every" ? "#dbeafe" : "#fef3c7",
                              color: parsed!.schedule.kind === "every" ? "#1e40af" : "#92400e",
                              fontFamily: "'SF Mono', monospace",
                            }}
                            title={
                              parsed!.schedule.kind === "every"
                                ? "每日定时触发，到期后下一轮 proactive 主动开口时执行"
                                : "单次定时触发"
                            }
                          >
                            {parsed!.schedule.kind === "every" ? "🔁" : "📅"} {scheduleLabel}
                          </span>
                        )}
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
            </div>
          );
        })}
    </div>
  );
}
