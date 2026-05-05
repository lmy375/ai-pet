import { Fragment, useState, useEffect, useCallback, useMemo, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { parseMarkdown } from "../../utils/inlineMarkdown";

/** 与后端 `task_queue::TaskView` 一一对应。`status` 四态由后端判定，前端
 * 仅渲染。`due` 是无时区 ISO（`YYYY-MM-DDThh:mm`），与 datetime-local
 * input 的 value 直接对称，避免前端做 Date 转换。*/
type TaskStatus = "pending" | "done" | "error" | "cancelled";

interface TaskView {
  title: string;
  body: string;
  priority: number;
  due: string | null;
  status: TaskStatus;
  /** Error / Cancelled 状态时是括号内的原因（无时为 null）；Pending /
   * Done 时一律 null。字段名沿用 error_message 是历史兼容。 */
  error_message: string | null;
  /** 描述里抽出的 #tag 列表（不带 #）。空列表表示无 tag。 */
  tags: string[];
  /** [result: ...] 标记的内容。已结束（done/cancelled）任务的"产物"行
   * 单独展示。null 表示没写产物。 */
  result: string | null;
  created_at: string;
  updated_at: string;
}

interface TaskListResponse {
  tasks: TaskView[];
}

/// 与后端 `commands::task::TaskDetail` 一一对应。`raw_description` 故意保留
/// `[task pri=...]` 等所有 markers（与展示 body 不同），让用户回溯单条任务的
/// 全过程时看到的就是宠物在 yaml 里实际见到的字符串。
interface TaskHistoryEvent {
  timestamp: string;
  action: string;
  snippet: string;
}

interface TaskDetail {
  title: string;
  raw_description: string;
  detail_path: string;
  detail_md: string;
  created_at: string;
  updated_at: string;
  history: TaskHistoryEvent[];
  /// detail.md 真正读失败标志（NotFound / 起始空文件不算）。前端在标题旁
  /// 渲染红字让用户区分"真没数据"和"读失败"。
  detail_md_io_error: boolean;
  /// butler_history.log 真正读失败标志（同上语义）。
  history_io_error: boolean;
}

const PRIORITY_MAX = 9;

/** 状态徽章配色。cancelled 用灰色（结束态、不再有动作），与 done 的绿色
 * 区分开 — 用户一眼能区分"完成"与"取消"。 */
const STATUS_BADGE: Record<TaskStatus, { label: string; bg: string; fg: string }> = {
  pending: { label: "待办", bg: "#e0f2fe", fg: "#075985" },
  error: { label: "失败", bg: "var(--pet-tint-orange-bg)", fg: "var(--pet-tint-orange-fg)" },
  done: { label: "已完成", bg: "#dcfce7", fg: "#166534" },
  cancelled: { label: "已取消", bg: "#f1f5f9", fg: "#64748b" },
};

/** 哪些状态算"已结束"（结束段，被「显示已结束」开关控制）。pending / error
 * 在活动段，用户始终能看见。 */
function isFinished(status: TaskStatus): boolean {
  return status === "done" || status === "cancelled";
}

/** 把无时区 ISO 渲染为可读本地串。后端写入时已是本地时区，前端不再做
 * Date 解析（避免 datetime-local 缺时区被误判为 UTC）。*/
function formatDue(iso: string | null): string {
  if (!iso) return "";
  // 简单 split 即可：把 T 换成空格，分钟保留。
  return iso.replace("T", " ");
}

/** 任务行 due 紧迫度判定。返回值映射到 due 文字颜色 — 让扫长队列时一眼
 * 区分"现在就做 / 抓紧 / 还早"。终态任务（done / cancelled）一律 normal，
 * 与其它视觉提示（绿点 / 焦点蓝边）"终态保持中性"原则一致。
 *
 * `due` 是 `YYYY-MM-DDThh:mm` 无时区本地协议；拼上 `:00` 当本地时间 parse，
 * 解析失败（理论不会发生 — 来自后端的标准字符串）一律 normal。 */
type DueUrgency = "overdue" | "soon" | "normal";
const DUE_SOON_THRESHOLD_MS = 24 * 60 * 60 * 1000;
function dueUrgency(due: string, now: number, status: TaskStatus): DueUrgency {
  if (status === "done" || status === "cancelled") return "normal";
  const ts = Date.parse(`${due}:00`);
  if (Number.isNaN(ts)) return "normal";
  const delta = ts - now;
  if (delta <= 0) return "overdue";
  if (delta <= DUE_SOON_THRESHOLD_MS) return "soon";
  return "normal";
}
/** 判定 due 是否落在 `now` 所在本地日期。`due` 是 `YYYY-MM-DDThh:mm` 无时
 * 区本地协议，所以直接拿日期前缀（`YYYY-MM-DD` 共 10 字符）与 `now` 的本
 * 地年月日比对最稳 —— 既不会被 UTC 解析偏移影响，也不必走 Date 实例化。
 *
 * `due` 格式不合法（理论不会发生 — 来自后端的标准字符串）/ null → false。 */
function isDueToday(due: string | null, now: Date): boolean {
  if (!due || due.length < 10) return false;
  const y = now.getFullYear();
  const m = String(now.getMonth() + 1).padStart(2, "0");
  const d = String(now.getDate()).padStart(2, "0");
  const todayPrefix = `${y}-${m}-${d}`;
  return due.slice(0, 10) === todayPrefix;
}

/** 比较两个 RFC3339 / ISO8601 字符串：a 时刻晚于 b 返回 true。
 *
 * 用 `Date.parse` 转 ms 比较，跨时区表达式（`+08:00` vs `Z`）也能正确
 * 判断 —— 单纯字符串 lex 比较会因 '+' / '0'..'9' / 'Z' 的字符序错位
 * 出错（典型场景：localStorage 写 `Date.toISOString()` 是 UTC Z，后端
 * butler_history 写 `chrono::Local` 带本地时区）。
 *
 * `b === null` → true：让 caller 用 "null 当 -∞" 的便利语义处理"从未
 * 看过 / 首次打开"分支（如 history timeline "全部视为新"）。 */
function tsAfter(a: string, b: string | null): boolean {
  if (b === null) return true;
  const at = Date.parse(a);
  const bt = Date.parse(b);
  if (Number.isNaN(at) || Number.isNaN(bt)) return false;
  return at > bt;
}

/** 判定任务是否处于"逾期"状态。复用 `dueUrgency` 的语义：终态 (done /
 * cancelled) 永远不算逾期；空 due 不算逾期；过去 due 才算。 */
function isOverdue(
  due: string | null,
  now: number,
  status: TaskStatus,
): boolean {
  return due !== null && dueUrgency(due, now, status) === "overdue";
}

function dueColor(urgency: DueUrgency): string | undefined {
  switch (urgency) {
    case "overdue":
      return "var(--pet-tint-orange-fg)";
    case "soon":
      return "#ea580c";
    case "normal":
      return undefined; // 走父级 itemMeta 默认色
  }
}

/** 把单个任务渲染为 Markdown 段，供"Copy as MD"按钮一键塞进剪贴板。
 *
 * 设计取舍：
 * - 不导出 history（butler_history events） — 那是审计流，不是任务文档；
 *   塞进 markdown 会让长度爆炸 & 噪音盖过任务本体。
 * - 元信息以 `- **K**: V` 的列表形式呈现 —— GitHub / Notion / Obsidian 通吃；
 *   纯 `K: V` 行在某些渲染器（如 Notion）会被识别为段落文字而非列表。
 * - 空字段（body / due / tags / detail_md / result）直接整行/整段省略，
 *   避免出现 `标签：（空）` 这种垃圾行。
 *
 * 该函数是纯字符串拼装，幂等 — 同一对 (task, detail) 永远产出同一段。 */
export function formatTaskAsMarkdown(t: TaskView, detail?: TaskDetail): string {
  const lines: string[] = [];
  lines.push(`## ${t.title}`);
  lines.push("");
  lines.push(`- **状态**: ${STATUS_BADGE[t.status].label}`);
  lines.push(`- **优先级**: P${t.priority}`);
  if (t.due) lines.push(`- **截止**: ${formatDue(t.due)}`);
  if (t.tags.length > 0) {
    lines.push(`- **标签**: ${t.tags.map((x) => `#${x}`).join(" ")}`);
  }
  if (t.created_at) lines.push(`- **创建**: ${t.created_at}`);
  if (t.updated_at) lines.push(`- **更新**: ${t.updated_at}`);
  if (t.body.trim()) {
    lines.push("");
    lines.push(t.body.trim());
  }
  // detail 可选：批量复制场景下用户多半要"清单 view"而非每条详情；同时
  // 避免 N 次 task_get_detail invoke。单任务 caller 仍传 detail 拿到完整段。
  if (detail && detail.detail_md.trim()) {
    lines.push("");
    lines.push("### 进度笔记");
    lines.push("");
    lines.push(detail.detail_md.trim());
  }
  if (t.result && t.result.trim()) {
    lines.push("");
    lines.push("### 产物");
    lines.push("");
    lines.push(t.result.trim());
  }
  return lines.join("\n");
}

/** 任务面板搜索结果高亮：把 query 子串在 text 里第一次出现位置用 `<mark>`
 * 包起来。空 query / 未命中时原样输出。配色与 PanelChat SearchResultRow /
 * PanelSettings HighlightedText 一致（黄底深棕字），让"panel 内搜索高亮"风格
 * 统一。 */
const HIGHLIGHT_MARK_STYLE: React.CSSProperties = {
  background: "#fef3c7",
  color: "#92400e",
  padding: "0 1px",
  borderRadius: 2,
};
/** 任务面板「今日到期 / 逾期」chip。逾期红、今日橙；active 时填充 + 深色字。
 * 互斥由父级 dueFilter state 保证 —— 同一时刻只有一种被高亮。 */
function DueChip({
  kind,
  count,
  active,
  onToggle,
}: {
  kind: "today" | "overdue";
  count: number;
  active: boolean;
  onToggle: () => void;
}) {
  const isOver = kind === "overdue";
  const palette = isOver
    ? { bg: "#fef2f2", bgActive: "#fecaca", fg: "#991b1b", border: "#fca5a5", borderActive: "#ef4444" }
    : { bg: "#fff7ed", bgActive: "#fed7aa", fg: "#9a3412", border: "#fed7aa", borderActive: "#fb923c" };
  return (
    <span
      role="button"
      tabIndex={0}
      onClick={onToggle}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onToggle();
        }
      }}
      title={
        active
          ? `再次点击关闭「${isOver ? "逾期" : "今日到期"}」过滤，恢复显示其它任务`
          : isOver
            ? "只看 due 已过 & 未结束的任务"
            : "只看 due 在今天 & 未结束的任务"
      }
      style={{
        fontSize: 11,
        padding: "2px 8px",
        borderRadius: 10,
        background: active ? palette.bgActive : palette.bg,
        color: palette.fg,
        cursor: "pointer",
        whiteSpace: "nowrap",
        userSelect: "none",
        border: `1px solid ${active ? palette.borderActive : palette.border}`,
      }}
    >
      {active ? "✓ " : ""}
      {isOver ? "🔴 逾期" : "📅 今日到期"}
      <span style={{ fontSize: 10, opacity: 0.7, marginLeft: 2 }}> ({count})</span>
    </span>
  );
}

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

/** 任务详情时间线的 action 图标。create / update / delete 各配一个 emoji；
 * 与 `s.historyAction(action)` 的颜色互补，让色彩 + 图标 + 字面三通道并行。
 * 未知 action 落 `•` 占位。 */
function actionIcon(action: string): string {
  switch (action) {
    case "create":
      return "➕";
    case "update":
      return "📝";
    case "delete":
      return "🗑";
    default:
      return "•";
  }
}

/** "刚动过"判定：updated_at 距今 < 5 分钟（含未来时刻 = 不显示，防时钟漂移
 * 给"未来 updated_at"打绿点）。Date.parse 吃 RFC3339 / 带时区 ISO 输出 ms
 * 自 epoch 起的 UTC，跨时区一致。解析失败返回 false。 */
const RECENTLY_UPDATED_MS = 5 * 60 * 1000;
function isRecentlyUpdated(updatedAt: string, now: number): boolean {
  const ts = Date.parse(updatedAt);
  if (Number.isNaN(ts)) return false;
  const age = now - ts;
  return age >= 0 && age < RECENTLY_UPDATED_MS;
}

/** 给"刚动过"绿点的 hover tooltip。< 60s 用 "刚刚更新"，否则 "X 分钟前更新"。
 * 仅在 isRecentlyUpdated 通过时调用，age 必为非负且 < 5 分钟。 */
function formatRecentlyUpdatedHint(updatedAt: string, now: number): string {
  const age = now - Date.parse(updatedAt);
  if (age < 60_000) return "刚刚更新";
  return `${Math.floor(age / 60_000)} 分钟前更新`;
}

/** R87: itemMeta "创建于" 后的相对时间附文。覆盖更宽量级（minute / hour /
 * day）以分辨"新积压 vs 老欠债"。无效 timestamp 返空串让调用点降级到只显
 * 绝对时间，不挂"NaN 分钟前" 这种 UI bug。 */
function formatRelativeAge(createdAt: string, now: number): string {
  const ts = Date.parse(createdAt);
  if (Number.isNaN(ts)) return "";
  const age = now - ts;
  if (age < 60_000) return "刚创建";
  if (age < 3_600_000) return `${Math.floor(age / 60_000)} 分钟前`;
  if (age < 86_400_000) return `${Math.floor(age / 3_600_000)} 小时前`;
  return `${Math.floor(age / 86_400_000)} 天前`;
}

/** R136: due 距今相对时间。due hover tooltip 用，让用户快速判断紧迫度。
 * 三档：< 1 小时 → "1 小时内 / 刚过期"；< 1 天 → "X 小时后 / 已过 X 小时"；
 * ≥ 1 天 → "X 天后 / 已过 X 天"。无效 ISO 返空串。 */
function formatDueRelative(dueIso: string, now: number): string {
  const ts = Date.parse(dueIso);
  if (Number.isNaN(ts)) return "";
  const diffMs = ts - now;
  const absMs = Math.abs(diffMs);
  const future = diffMs >= 0;
  if (absMs < 3_600_000) {
    return future ? "1 小时内到期" : "刚过期";
  }
  const hours = Math.floor(absMs / 3_600_000);
  const days = Math.floor(absMs / 86_400_000);
  if (absMs < 86_400_000) {
    return future ? `${hours} 小时后到期` : `已过 ${hours} 小时`;
  }
  return future ? `${days} 天后到期` : `已过 ${days} 天`;
}

/** R91: 长描述折叠阈值。> 200 字才折叠，折叠时显前 120 字。中文 ~3 char/token，
 * 120 字 ≈ 40 tokens 足够 skim 一句完整中文。短描述（≤ 200）不动避免 noise。 */
const BODY_FOLD_THRESHOLD = 200;
const BODY_FOLD_PREVIEW = 120;

/** R94: showFinished=true 时，已完成任务按完成日期分桶渲染。4 桶覆盖典型
 * 复盘窗口（今天 / 昨天 / 本周 / 更早），过细分桶让短期交互认知成本高。 */
type FinishedBucket = "today" | "yesterday" | "week" | "earlier";
const BUCKET_LABELS: Record<FinishedBucket, string> = {
  today: "今天",
  yesterday: "昨天",
  week: "本周",
  earlier: "更早",
};

function bucketFor(
  ts: number,
  todayMs: number,
  yesterdayMs: number,
  weekStartMs: number,
): FinishedBucket {
  if (ts >= todayMs) return "today";
  if (ts >= yesterdayMs) return "yesterday";
  if (ts >= weekStartMs) return "week";
  return "earlier";
}

export function PanelTasks() {
  const [tasks, setTasks] = useState<TaskView[]>([]);
  const [loading, setLoading] = useState(true);
  const [showFinished, setShowFinished] = useState(false);
  // R91: 哪些任务的长描述已被用户展开。key = `${title}-${created_at}` 与
  // list <div key> 同款。session 内有效，关面板丢失（与 search / sort 等
  // 临时态同语义，不持久化）。
  const [expandedBodies, setExpandedBodies] = useState<Set<string>>(new Set());
  // R109: 任务详情 history timeline 折叠状态。> 8 条事件时默认显最新 5 条；
  // 用户点 "展开更早 N 条" 切到全部。Set per title 让多个 task 折叠状态
  // 独立（虽然 expandedTitle 单一互斥，保持模式与 R91 expandedBodies 一致）。
  const [expandedHistoryTitles, setExpandedHistoryTitles] = useState<
    Set<string>
  >(new Set());
  // 排序模式：默认 "queue"（沿用 backend compare_for_queue 综合序），切到
  // "due" 按 due 升序（无 due 排末尾），切到 "priority"（R107）按优先级降
  // 序（数值大 = 优先级高，与后端 compare_for_queue 方向一致）。重启即默认。
  const [sortMode, setSortMode] = useState<"queue" | "due" | "priority">(
    "queue",
  );
  const [search, setSearch] = useState("");
  // PanelTasks 处于活跃 tab 时 ⌘F / Ctrl+F 聚焦搜索框 —— 与 mac 浏览器 /
  // Finder / Notion 的"⌘F = 搜索"直觉对齐。ref 挂在 <input> 上，handler
  // 在 keydown useEffect 内拦截。
  const searchInputRef = useRef<HTMLInputElement>(null);
  // R116: 创建表单标题 input 的 ref，用于 "n" 快捷键 focus（与 ⌘F focus
  // 搜索框同模式）。表单折叠时 ref 是 null —— shortcut handler 用 setTimeout
  // 0 等 setCreateFormExpanded 触发的 React commit 完成后再 focus。
  const titleInputRef = useRef<HTMLInputElement>(null);
  const [selectedTags, setSelectedTags] = useState<Set<string>>(new Set());
  // 「due」轴快捷过滤：三态 enum 互斥，避免"今日 + 逾期"两 boolean 相互
  // 矛盾的死状态。与 sortMode 解耦（开 today / overdue 仍可选 queue 排序）。
  const [dueFilter, setDueFilter] = useState<"all" | "today" | "overdue">(
    "all",
  );
  // R104: priority 多选过滤。Set<number> 空 = "全部"；非空 = 任一命中即通过
  // （OR 语义）。与 R83 决策日志 / R39 工具风险等多选 chip 模式一致。P0 仍保
  // 留 "💡 idea 抽屉"语义在 chip glyph 上，老用户直觉不丢。
  const [priorityFilter, setPriorityFilter] = useState<Set<number>>(new Set());

  // 创建表单
  const [title, setTitle] = useState("");
  const [body, setBody] = useState("");
  const [priority, setPriority] = useState(3);
  const [due, setDue] = useState(""); // datetime-local 原始值，可空
  const [creating, setCreating] = useState(false);
  const [errMsg, setErrMsg] = useState("");
  // 新建表单展开态：跨 session 记忆，default 展开（兼容既有 UX）。用户
  // 折叠后偏好持久；下次打开 panel 仍折叠 → 节省垂直空间。
  const [createFormExpanded, setCreateFormExpanded] = useState<boolean>(() => {
    try {
      const raw = window.localStorage.getItem("pet-task-create-form-expanded");
      // 没存过 → default 展开；存的不是 "false" 都按展开（防御性）
      return raw !== "false";
    } catch {
      return true;
    }
  });
  useEffect(() => {
    try {
      window.localStorage.setItem(
        "pet-task-create-form-expanded",
        String(createFormExpanded),
      );
    } catch (e) {
      console.error("createFormExpanded localStorage save failed:", e);
    }
  }, [createFormExpanded]);

  // 行内动作状态：哪条任务正在被取消（展开 reason 输入）/ 重试中（按钮禁用）
  const [cancellingTitle, setCancellingTitle] = useState<string | null>(null);
  const [cancelReason, setCancelReason] = useState("");
  const [busyTitle, setBusyTitle] = useState<string | null>(null);
  const [actionErr, setActionErr] = useState("");

  // 单条任务的"展开详情"状态。同时只展开一条（accordion），避免长队列被详情挤
  // 到难以浏览。`detailMap` 是 lazy-fetched 缓存；reload 时清空（防止重试 / 取消
  // 后展示陈旧数据）。
  const [expandedTitle, setExpandedTitle] = useState<string | null>(null);
  const [detailMap, setDetailMap] = useState<Record<string, TaskDetail>>({});
  const [detailLoadingTitle, setDetailLoadingTitle] = useState<string | null>(null);
  const [detailErr, setDetailErr] = useState("");

  // 批量操作状态。selected 按 title 索引（与单条 retry/cancel 走同一套语义，
  // 重名走"首条匹配"）。bulkAction 控制二级输入面板（cancel reason / new
  // priority）是否展开。bulkResultMsg 给执行后短暂展示"重试 5 条 / 跳过 1
  // 条非 error"，~5s 后清掉。
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [bulkBusy, setBulkBusy] = useState(false);
  const [bulkAction, setBulkAction] = useState<"cancel" | "priority" | "due" | "tags" | null>(null);
  const [bulkReason, setBulkReason] = useState("");
  const [bulkPriority, setBulkPriority] = useState(3);
  // "改优先级" sub-panel 内附加 checkbox：true 时同次也把 due 清空。让用户
  // 把"P9 紧急"老任务重排时不必两步（先清 due 再改 pri）。
  const [bulkPriorityClearDue, setBulkPriorityClearDue] = useState(false);
  const [bulkDue, setBulkDue] = useState(""); // datetime-local 字符串；空 = 清 due
  const [bulkTagOps, setBulkTagOps] = useState(""); // 例如 "+a -b +工作"
  const [bulkResultMsg, setBulkResultMsg] = useState("");

  // 任务详情页 detail.md 编辑状态。同时只允许一条 detail 在编辑（与单 accordion
  // 展开风格一致）。切换 expanded 任务或保存成功后清空。
  const [editingDetailTitle, setEditingDetailTitle] = useState<string | null>(null);
  const [editingDetailContent, setEditingDetailContent] = useState("");
  // R117: detail.md 编辑器的 preview / edit 切换。同时只一个 task 处于
  // edit（editingDetailTitle 互斥保证），所以单 boolean 即可；切换不丢
  // 未保存内容（state 共享 editingDetailContent）。
  const [detailPreviewMode, setDetailPreviewMode] = useState(false);
  const [savingDetail, setSavingDetail] = useState(false);
  // 进度笔记浏览态的渲染模式：rendered 默认（更友好），source 偶尔查 raw
  // 时切。全局 toggle，不持久 — 与 PanelTasks 其它切换 state 同语义。
  // 不影响编辑模式（编辑永远是 raw）。
  const [detailMdRenderMode, setDetailMdRenderMode] = useState<"rendered" | "source">("rendered");
  // detail.md / 完整描述段的阅读行宽 cap，跨 session 记忆。range slider 限
  // [600, 1200] —— 600 紧凑、800 中文 60-80 字推荐、1200 已贴近超宽屏极限。
  // 解析失败 / 越界 / null → default 800。
  const [detailMaxWidth, setDetailMaxWidth] = useState<number>(() => {
    try {
      const raw = window.localStorage.getItem("pet-task-detail-max-width");
      if (raw === null) return 800;
      const n = parseInt(raw, 10);
      if (!Number.isFinite(n)) return 800;
      return Math.max(600, Math.min(1200, n));
    } catch {
      return 800;
    }
  });
  useEffect(() => {
    try {
      window.localStorage.setItem(
        "pet-task-detail-max-width",
        String(detailMaxWidth),
      );
    } catch (e) {
      // localStorage 不可用 → 不影响运行；仅丢"跨 session 记忆"
      console.error("detailMaxWidth localStorage save failed:", e);
    }
  }, [detailMaxWidth]);
  // window 宽度跟踪：让阅读宽度 slider 的 max 跟着窗口缩放联动 —— 否则
  // 用户在 700px 小窗口里仍能 set 1200 导致内容超出可见范围。绝对上限
  // 仍 1200（avoid 超宽屏 24" 上柱状条无意义铺满）；下限 600 保一手紧凑。
  const [windowWidth, setWindowWidth] = useState<number>(() =>
    typeof window !== "undefined" ? window.innerWidth : 1200,
  );
  useEffect(() => {
    const onResize = () => setWindowWidth(window.innerWidth);
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);
  const detailMaxWidthCap = Math.max(600, Math.min(1200, windowWidth - 80));
  // 若用户曾 set 大值，但现在窗口缩了 → render 时只显 cap 值；user 再次
  // 调 slider 会从 cap 起点。state 本身不动（窗口拉回宽时仍用旧偏好）。
  const detailMaxWidthEffective = Math.min(detailMaxWidth, detailMaxWidthCap);
  // 首次打开 detail 时旁边显一行小灰字 hint，3 秒后自动消失，localStorage
  // flag 防再次打扰。flag 不存在 / 为非 "true" 字符串都算"未见过"。
  const [showWidthHint, setShowWidthHint] = useState<boolean>(() => {
    try {
      return window.localStorage.getItem("pet-task-detail-width-hint-seen") !== "true";
    } catch {
      return false;
    }
  });
  // 一旦 detail 实际渲染过 hint，3 秒后自动 dismiss + 写 localStorage
  // 标记已见过，避免下次再打扰。`expandedTitle` 触发：仅当用户首次展开
  // 任务详情才计时。
  useEffect(() => {
    if (!showWidthHint) return;
    if (expandedTitle === null) return;
    const t = window.setTimeout(() => {
      setShowWidthHint(false);
      try {
        window.localStorage.setItem("pet-task-detail-width-hint-seen", "true");
      } catch (e) {
        console.error("width hint flag save failed:", e);
      }
    }, 3000);
    return () => window.clearTimeout(t);
  }, [showWidthHint, expandedTitle]);
  // 任务历史时间线「已读」标记的 lastview 缓存：title → 上次展开本任务
  // 详情时的 RFC3339 字符串。useRef 不是 state — 值在 handleToggleExpand
  // 一次性读 + 写 localStorage，render 只读，不需触 re-render。
  // localStorage 持久化保证跨 session 的"上次看过"语义稳定。
  const lastViewRef = useRef<Map<string, string | null>>(new Map());
  // 仅作为 reactivity 触发器：handleToggleExpand 写完 localStorage 后 bump
  // 一下，让列表行重算"未读红点"立刻消失（否则要等 nowMs 30s tick 才同步）。
  // 数值本身无含义，render 只用于 useMemo 依赖。
  const [lastviewBump, setLastviewBump] = useState(0);
  const [editDetailErr, setEditDetailErr] = useState("");

  // 键盘导航：focusedIdx 跟踪当前"键盘焦点"行（visibleTasks 索引）。null 表示
  // 用户尚未启动键盘导航（默认行为与鼠标用户一致：无任何视觉变化）。
  // ↑↓ 移动焦点；空格切换选中。
  const [focusedIdx, setFocusedIdx] = useState<number | null>(null);

  // 任务详情段「完整描述」/「进度笔记」的复制反馈：刚被复制的 section key
  // （形如 `${title}-rawDesc` / `${title}-detailMd`），1.5s 自动清掉。同时只
  // 跟踪一段，多任务并行展开极少（accordion 单展开），collide 风险无。
  const [copiedDetailKey, setCopiedDetailKey] = useState<string | null>(null);

  // "最近更新" 绿点：每 30s 刷新一次时钟快照，让 updated_at 距今 5 分钟阈值
  // 自然过期。Date.now() 当下 vs updated_at 解析比较；解析失败 / 未来 ts 不显示。
  const [nowMs, setNowMs] = useState<number>(() => Date.now());
  useEffect(() => {
    const id = window.setInterval(() => setNowMs(Date.now()), 30_000);
    return () => window.clearInterval(id);
  }, []);

  const reload = useCallback(async () => {
    try {
      const resp = await invoke<TaskListResponse>("task_list");
      setTasks(resp.tasks);
      // task_list 后清空详情缓存：retry / cancel / 新建都会让 description /
      // detail.md / history 翻新，命中旧缓存会让用户看到陈旧的回溯视图。
      setDetailMap({});
    } catch (e) {
      setErrMsg(`加载失败：${e}`);
    } finally {
      setLoading(false);
    }
  }, []);

  // 批量选择 helpers —— pure setState，统一在一处管理增/删/清。
  const toggleSelect = useCallback((taskTitle: string) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(taskTitle)) {
        next.delete(taskTitle);
      } else {
        next.add(taskTitle);
      }
      return next;
    });
  }, []);
  const clearSelection = useCallback(() => {
    setSelected(new Set());
    setBulkAction(null);
    setBulkReason("");
  }, []);

  const handleToggleExpand = useCallback(
    async (taskTitle: string) => {
      setDetailErr("");
      // 切换/折叠时重置 detail 编辑态——避免用户改了一半，切到别的任务再切回，
      // 还残留过时的 textarea 内容
      setEditingDetailTitle(null);
      setEditingDetailContent("");
      setEditDetailErr("");
      // 已展开同一条 → 折叠
      if (expandedTitle === taskTitle) {
        setExpandedTitle(null);
        return;
      }
      setExpandedTitle(taskTitle);
      // 「已读」语义：先读旧 lastview 进 ref（render 用），再把当前时间写
      // localStorage（下次展开时 ref 拿到的是这次展开的时刻）。首次展开
      // prev=null → render 把所有事件都视作"新"。折叠分支已 early-return，
      // 所以"刚展开看完就被自动标已读"不会发生。
      try {
        const key = `pet-task-history-lastview-${taskTitle}`;
        const prev = window.localStorage.getItem(key);
        lastViewRef.current.set(taskTitle, prev);
        const nowIso = new Date().toISOString();
        window.localStorage.setItem(key, nowIso);
        // 触发列表 re-render 让红点立刻消失，无需等 nowMs 30s tick
        setLastviewBump((n) => n + 1);
      } catch (e) {
        // localStorage 不可用（无痕 / 配额满）→ 不影响展开主路径
        console.error("task history lastview localStorage failed:", e);
      }
      // 缓存命中 → 不重复 fetch（reload 时已清空缓存，所以陈旧风险有界）
      if (detailMap[taskTitle]) return;
      setDetailLoadingTitle(taskTitle);
      try {
        const detail = await invoke<TaskDetail>("task_get_detail", { title: taskTitle });
        setDetailMap((prev) => ({ ...prev, [taskTitle]: detail }));
      } catch (e) {
        setDetailErr(`加载详情失败：${e}`);
      } finally {
        setDetailLoadingTitle(null);
      }
    },
    [expandedTitle, detailMap],
  );

  // 进入 detail.md 编辑：把当前 detail.detail_md 复制进 textarea，等用户改。
  // 同时记录 original 让 cancel 时检查是否 dirty —— 改了一半误点取消会丢，
  // 二次确认机制（同决策日志清空）阻止误触。
  const editingDetailOriginalRef = useRef<string>("");
  const [cancelEditArmed, setCancelEditArmed] = useState(false);
  const handleEnterEditDetail = useCallback((taskTitle: string, currentMd: string) => {
    setEditingDetailTitle(taskTitle);
    setEditingDetailContent(currentMd);
    editingDetailOriginalRef.current = currentMd;
    setEditDetailErr("");
    setCancelEditArmed(false);
  }, []);
  const handleCancelEditDetail = useCallback(() => {
    const dirty = editingDetailContent !== editingDetailOriginalRef.current;
    if (dirty && !cancelEditArmed) {
      // 改过且首次取消 → armed，3s 内再点才丢弃
      setCancelEditArmed(true);
      window.setTimeout(() => setCancelEditArmed(false), 3000);
      return;
    }
    setEditingDetailTitle(null);
    setEditingDetailContent("");
    setEditDetailErr("");
    setCancelEditArmed(false);
  }, [editingDetailContent, cancelEditArmed]);

  // 保存：写盘 + 同步本地 detailMap 缓存（不必 reload 整张 task_list —— detail.md
  // 改动不影响 description/due/priority/状态）。
  const handleSaveDetail = useCallback(
    async (taskTitle: string) => {
      setSavingDetail(true);
      setEditDetailErr("");
      try {
        await invoke("task_save_detail", { title: taskTitle, content: editingDetailContent });
        setDetailMap((prev) => {
          const cur = prev[taskTitle];
          if (!cur) return prev;
          return { ...prev, [taskTitle]: { ...cur, detail_md: editingDetailContent } };
        });
        setEditingDetailTitle(null);
        setEditingDetailContent("");
      } catch (e) {
        setEditDetailErr(`保存失败：${e}`);
      } finally {
        setSavingDetail(false);
      }
    },
    [editingDetailContent],
  );

  /// 详情段「完整描述」/「进度笔记」的复制：clipboard.writeText + 1.5s "已
  /// 复制"反馈（按 sectionKey 跟踪，让"两段并存时只点亮被复制那段"的语义
  /// 自然成立）。失败 → console.error 不弹 alert（剪贴板权限错误极少不值
  /// 打断用户）。
  const handleCopyDetail = useCallback(async (sectionKey: string, text: string) => {
    try {
      await navigator.clipboard.writeText(text);
      setCopiedDetailKey(sectionKey);
      window.setTimeout(() => {
        setCopiedDetailKey((prev) => (prev === sectionKey ? null : prev));
      }, 1500);
    } catch (e) {
      console.error("clipboard write failed:", e);
    }
  }, []);

  useEffect(() => {
    reload();
  }, [reload]);

  const handleCreate = async () => {
    setErrMsg("");
    if (!title.trim()) {
      setErrMsg("标题不能为空");
      return;
    }
    setCreating(true);
    try {
      await invoke<string>("task_create", {
        args: {
          title: title.trim(),
          body: body.trim(),
          priority,
          // datetime-local 输入若为空 string 视作 null，让后端按"无 due"对待
          due: due || null,
        },
      });
      setTitle("");
      setBody("");
      setPriority(3);
      setDue("");
      await reload();
    } catch (e) {
      setErrMsg(`创建失败：${e}`);
    } finally {
      setCreating(false);
    }
  };

  // R120: 创建表单内 ⌘Enter / Ctrl+Enter 提交。仅在 input/textarea focus
  // 时触发（scoped 到 4 个表单字段的 onKeyDown），不挂全局；creating 守卫
  // 防 race 重复创建；preventDefault 让 textarea 内按 ⌘Enter 不换行。
  const handleFormKeyDown = (
    e: React.KeyboardEvent<HTMLInputElement | HTMLTextAreaElement>,
  ) => {
    if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
      e.preventDefault();
      if (creating) return;
      void handleCreate();
    }
  };

  const handleRetry = async (taskTitle: string) => {
    setActionErr("");
    setBusyTitle(taskTitle);
    try {
      await invoke<void>("task_retry", { title: taskTitle });
      await reload();
    } catch (e) {
      setActionErr(`重试失败：${e}`);
    } finally {
      setBusyTitle(null);
    }
  };

  const handleCancelOpen = (taskTitle: string) => {
    setActionErr("");
    setCancelReason("");
    setCancellingTitle(taskTitle);
  };

  const handleCancelClose = () => {
    setCancellingTitle(null);
    setCancelReason("");
  };

  /// 批量执行 helper：对 selected 里每条 title 调 `op`；不满足前置（predicate
  /// 返回 false）的 skip。汇总成功 / 跳过 / 失败计数 + 末尾错误（多条同样错误时
  /// 不刷屏）。完成后 reload + 清选择 + 短暂展示结果文案。
  const runBulk = useCallback(
    async (
      label: string,
      predicate: (t: TaskView) => boolean,
      skipReason: string,
      op: (taskTitle: string) => Promise<void>,
    ) => {
      setBulkBusy(true);
      setBulkResultMsg("");
      setActionErr("");
      let success = 0;
      let skipped = 0;
      let failed = 0;
      let lastErr = "";
      const titles = Array.from(selected);
      const titleToTask = new Map(tasks.map((t) => [t.title, t]));
      for (const title of titles) {
        const task = titleToTask.get(title);
        if (!task) {
          // 选中后任务已消失（可能被重试 / 取消改了状态后又被刷新切走 —— 不太可能但保护一下）
          skipped += 1;
          continue;
        }
        if (!predicate(task)) {
          skipped += 1;
          continue;
        }
        try {
          await op(title);
          success += 1;
        } catch (e) {
          failed += 1;
          lastErr = `${e}`;
        }
      }
      const parts: string[] = [];
      if (success > 0) parts.push(`${label} ${success} 条 ✓`);
      if (skipped > 0) parts.push(`跳过 ${skipped} 条（${skipReason}）`);
      if (failed > 0) parts.push(`失败 ${failed} 条（${lastErr}）`);
      setBulkResultMsg(parts.join(" · "));
      window.setTimeout(() => setBulkResultMsg(""), 5000);
      setBulkBusy(false);
      clearSelection();
      await reload();
    },
    [selected, tasks, reload, clearSelection],
  );

  const handleBulkRetry = useCallback(async () => {
    await runBulk(
      "重试",
      (t) => t.status === "error",
      "非 error 状态",
      async (title) => {
        await invoke<void>("task_retry", { title });
      },
    );
  }, [runBulk]);

  // 批量复制为 markdown：选中任务依次拼成 `## title` 段，blank line 分隔。
  // 不带 detail.md 进度笔记 / history（清单 view 用例 + 避免 N 次 IO 延迟）。
  // 选中后被删的任务（罕见 race）跳过；一条都没拼到 → 提示。
  const handleBulkCopyAsMd = useCallback(async () => {
    const titleToTask = new Map(tasks.map((t) => [t.title, t]));
    const parts: string[] = [];
    for (const title of selected) {
      const t = titleToTask.get(title);
      if (!t) continue;
      parts.push(formatTaskAsMarkdown(t));
    }
    if (parts.length === 0) {
      setBulkResultMsg("无可复制任务（选中已被清掉）");
      window.setTimeout(() => setBulkResultMsg(""), 4000);
      return;
    }
    const text = parts.join("\n\n");
    try {
      await navigator.clipboard.writeText(text);
      setBulkResultMsg(`已复制 ${parts.length} 条为 markdown 到剪贴板`);
    } catch (e) {
      setBulkResultMsg(`复制失败：${e}`);
    }
    window.setTimeout(() => setBulkResultMsg(""), 4000);
  }, [selected, tasks]);

  // 批量复制纯标题：与"复制为 MD"互补 —— 这条只输出标题列表（一行一个），
  // 适合快速贴 todo dump 到聊天 / message。order 走 selected Set 在 tasks
  // 数组里的出现顺序（与 visibleTasks 的当前排序一致）。
  const handleBulkCopyTitles = useCallback(async () => {
    const titleSet = selected;
    const titles = tasks.filter((t) => titleSet.has(t.title)).map((t) => t.title);
    if (titles.length === 0) {
      setBulkResultMsg("无可复制任务（选中已被清掉）");
      window.setTimeout(() => setBulkResultMsg(""), 4000);
      return;
    }
    try {
      await navigator.clipboard.writeText(titles.join("\n"));
      setBulkResultMsg(`已复制 ${titles.length} 条标题到剪贴板`);
    } catch (e) {
      setBulkResultMsg(`复制失败：${e}`);
    }
    window.setTimeout(() => setBulkResultMsg(""), 4000);
  }, [selected, tasks]);

  const handleBulkCancelConfirm = useCallback(async () => {
    const reason = bulkReason.trim();
    await runBulk(
      "取消",
      (t) => t.status === "pending" || t.status === "error",
      "已结束",
      async (title) => {
        await invoke<void>("task_cancel", { title, reason });
      },
    );
  }, [runBulk, bulkReason]);

  const handleBulkSetPriorityConfirm = useCallback(async () => {
    const pri = Math.max(0, Math.min(PRIORITY_MAX, bulkPriority));
    const alsoClearDue = bulkPriorityClearDue;
    await runBulk(
      alsoClearDue ? "改优先级 + 清 due" : "改优先级",
      () => true, // 终态任务也允许改 — 无害（priority 只影响展示）
      "无可改条目",
      async (title) => {
        await invoke<void>("task_set_priority", { title, priority: pri });
        // 顺便清 due：分两步 invoke。失败被 runBulk 聚合到 failed 计数；
        // 局部 priority 改了 due 没清的情况是可接受的（priority 已生效，
        // due 旧值仍在；用户重试 / 手改即可）。
        if (alsoClearDue) {
          await invoke<void>("task_set_due", { title, due: null });
        }
      },
    );
  }, [runBulk, bulkPriority, bulkPriorityClearDue]);

  const handleBulkSetDueConfirm = useCallback(async () => {
    // 空字符串 → 传 null 给后端清掉 due；非空 → 传字符串走严格 datetime-local
    // 解析（无效格式后端会逐条 Err，runBulk 会聚合到 failed 计数 + 末尾错误）。
    const due = bulkDue.trim().length === 0 ? null : bulkDue;
    await runBulk(
      due === null ? "清空 due" : "改 due",
      () => true, // 终态也允许（仅影响展示）
      "无可改条目",
      async (title) => {
        await invoke<void>("task_set_due", { title, due });
      },
    );
  }, [runBulk, bulkDue]);

  const handleBulkSetTagsConfirm = useCallback(async () => {
    // ops_input 由后端 parse_tag_ops 校验：互斥 / 缺前缀 / 非法字符都会
    // Err。空输入也走那条防御 → runBulk 内每条都会拿到同样 Err，第一条
    // 失败时会被聚合 + 提示。
    const opsInput = bulkTagOps.trim();
    await runBulk(
      "改 tags",
      () => true, // 终态也允许（tag 是组织维度，与状态无关）
      "无可改条目",
      async (title) => {
        await invoke<void>("task_set_tags", { title, opsInput });
      },
    );
  }, [runBulk, bulkTagOps]);

  const handleCancelConfirm = async (taskTitle: string) => {
    setActionErr("");
    setBusyTitle(taskTitle);
    try {
      await invoke<void>("task_cancel", {
        title: taskTitle,
        reason: cancelReason.trim(),
      });
      setCancellingTitle(null);
      setCancelReason("");
      await reload();
    } catch (e) {
      setActionErr(`取消失败：${e}`);
    } finally {
      setBusyTitle(null);
    }
  };

  if (loading) {
    return <div style={{ padding: 20, color: "#64748b" }}>加载中...</div>;
  }

  // 四段过滤：status → dueToday → search → tag。每段都尽量早退零成本。
  // - search：case-insensitive 子串，命中 title 或 body 任一即通过
  // - tag：用户选中的 tag 集合非空时，任务的 tags 至少与其中一个相交（OR 语义）
  // - dueToday：开启时只看本地今日到期 & 未结束的任务
  const trimmedSearch = search.trim().toLowerCase();
  // 复用 nowMs 状态（每 30s 自动刷新）保证逾期 / 今日到期判定与"最近更新"
  // 绿点用同一时钟快照，避免出现"两块都看墙上时间却差几秒"的认知抖动。
  const nowDate = new Date(nowMs);
  const filteredTasks = tasks
    .filter((t) => showFinished || !isFinished(t.status))
    .filter((t) => {
      if (dueFilter === "all") return true;
      if (dueFilter === "today") {
        return !isFinished(t.status) && isDueToday(t.due, nowDate);
      }
      // dueFilter === "overdue"：dueUrgency 内已自动剔除终态 + 解析失败
      return isOverdue(t.due, nowMs, t.status);
    })
    .filter((t) =>
      priorityFilter.size === 0 || priorityFilter.has(t.priority),
    )
    .filter((t) => {
      if (!trimmedSearch) return true;
      return (
        t.title.toLowerCase().includes(trimmedSearch) ||
        t.body.toLowerCase().includes(trimmedSearch)
      );
    })
    .filter((t) => {
      if (selectedTags.size === 0) return true;
      return t.tags.some((tag) => selectedTags.has(tag));
    });
  // 排序：sortMode === "queue" 沿用 backend compare_for_queue 综合序；"due"
  // 时按 due 字符串升序（ISO `YYYY-MM-DDThh:mm` 字典序与时间序一致），无 due
  // 一律排到末尾。slice() 防止就地变异 backend 返回的 array。
  // R94: 拆 unfinished / finished 两段。unfinished 应用 sortMode；finished
  // 始终按 updated_at 降序（终态后即"完成时刻"），让分桶 subheader 与桶内
  // 时间序一致。两段 concat 让 unfinished 永远在 finished 之上，复盘视图
  // 自然分层。
  const sortedUnfinished = (() => {
    const unf = filteredTasks.filter((t) => !isFinished(t.status));
    if (sortMode === "due") {
      return unf.slice().sort((a, b) => {
        const ad = a.due ?? "";
        const bd = b.due ?? "";
        if (!ad && !bd) return 0;
        if (!ad) return 1;
        if (!bd) return -1;
        return ad < bd ? -1 : ad > bd ? 1 : 0;
      });
    }
    if (sortMode === "priority") {
      // R107: 数值大 = 优先级高（与后端 task_queue::compare_for_queue 一致）。
      // JS sort stable —— 同 priority 保持原 queue 综合序，让"P3 内部"仍是
      // backend 推荐处理顺序。
      return unf.slice().sort((a, b) => b.priority - a.priority);
    }
    return unf;
  })();
  const sortedFinished = filteredTasks
    .filter((t) => isFinished(t.status))
    .slice()
    .sort((a, b) => {
      const ta = Date.parse(a.updated_at) || 0;
      const tb = Date.parse(b.updated_at) || 0;
      return tb - ta;
    });
  const visibleTasks = [...sortedUnfinished, ...sortedFinished];

  // R94: 分桶预计算。本地午夜 / 昨天 00:00 / 本 ISO 周一 00:00 三个边界，
  // 桶内 size 单次预扫得到，render 时直接读避免每行算一次。
  const { bucketBoundaries, bucketCounts } = useMemo(() => {
    const today = new Date();
    today.setHours(0, 0, 0, 0);
    const todayMs = today.getTime();
    const yesterdayMs = todayMs - 86_400_000;
    const dow = today.getDay();
    const isoOffset = dow === 0 ? 6 : dow - 1;
    const weekStart = new Date(today);
    weekStart.setDate(weekStart.getDate() - isoOffset);
    weekStart.setHours(0, 0, 0, 0);
    const weekStartMs = weekStart.getTime();
    const counts: Record<FinishedBucket, number> = {
      today: 0,
      yesterday: 0,
      week: 0,
      earlier: 0,
    };
    for (const t of sortedFinished) {
      const ts = Date.parse(t.updated_at) || 0;
      counts[bucketFor(ts, todayMs, yesterdayMs, weekStartMs)] += 1;
    }
    return {
      bucketBoundaries: { todayMs, yesterdayMs, weekStartMs },
      bucketCounts: counts,
    };
    // sortedFinished 是 filteredTasks 派生数组，依赖在 filteredTasks +
    // nowMs 上（nowMs 30s tick 让跨午夜 boundary 自动滚动）。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sortedFinished, nowMs]);

  // 当前 tasks 集合里出现过的所有 tag，按出现频次降序、频次同则字典序升序。
  // 派生自 tasks 而非 visibleTasks，避免"筛掉一个 tag 后它的 chip 也消失"
  // 这种状态死循环 —— chip 列表稳定，用户能看到全集随时切换。
  const allTags = useMemo(() => {
    const counts = new Map<string, number>();
    for (const t of tasks) {
      for (const tag of t.tags) {
        counts.set(tag, (counts.get(tag) ?? 0) + 1);
      }
    }
    return [...counts.entries()].sort((a, b) =>
      b[1] - a[1] || a[0].localeCompare(b[0])
    );
  }, [tasks]);

  // 「今日到期 / 逾期」计数：派生自 tasks 全集，不被搜索/tag/sort 链上的
  // 过滤影响，让用户即使在 selectedTags 模式里也能看到"今天总共有 N 条到
  // 期 / M 条逾期"，决定要不要切。计数为 0 时单个 chip 不渲染（避免无事
  // 可做时占视觉位置），两者皆 0 时整行也不渲染。
  const { dueTodayCount, overdueCount } = useMemo(() => {
    const now = new Date(nowMs);
    let today = 0;
    let overdue = 0;
    for (const t of tasks) {
      if (!isFinished(t.status) && isDueToday(t.due, now)) today += 1;
      if (isOverdue(t.due, nowMs, t.status)) overdue += 1;
    }
    return { dueTodayCount: today, overdueCount: overdue };
  }, [tasks, nowMs]);

  // R104: 各 priority 的活动任务计数。派生 tasks 全集（不受 link 上 search /
  // tag / due / sort 过滤影响），让用户在任一 filter 下都能看到"还有哪几档
  // priority 有事"。只数活动态：finished 不在 chip row，由 showFinished
  // 单独展示。priority asc 序让 chip 行从 P0 → P9 自然。
  const priorityCounts = useMemo(() => {
    const m = new Map<number, number>();
    for (const t of tasks) {
      if (isFinished(t.status)) continue;
      m.set(t.priority, (m.get(t.priority) ?? 0) + 1);
    }
    return [...m.entries()].sort((a, b) => a[0] - b[0]);
  }, [tasks]);

  // R89: 完成率"流量计"。only `done`（cancelled 是放弃，不算产出）。
  // 今日 = 本地午夜起；近 7 天 = rolling 7×24h。配合 R87 created_at 相对值
  // 形成"积压 vs 产出"双维度感知。`tasks` 全集（含 finished），不受
  // showFinished 切换影响。
  const completionStats = useMemo(() => {
    const todayStart = new Date();
    todayStart.setHours(0, 0, 0, 0);
    const todayMs = todayStart.getTime();
    const weekAgoMs = nowMs - 7 * 86_400_000;
    let today = 0;
    let week = 0;
    for (const t of tasks) {
      if (t.status !== "done") continue;
      const ts = Date.parse(t.updated_at);
      if (Number.isNaN(ts)) continue;
      if (ts >= todayMs) today += 1;
      if (ts >= weekAgoMs) week += 1;
    }
    return { today, week };
  }, [tasks, nowMs]);

  const toggleTag = (tag: string) => {
    setSelectedTags((prev) => {
      const next = new Set(prev);
      if (next.has(tag)) {
        next.delete(tag);
      } else {
        next.add(tag);
      }
      return next;
    });
  };

  const filtersActive =
    trimmedSearch.length > 0 ||
    selectedTags.size > 0 ||
    dueFilter !== "all" ||
    priorityFilter.size > 0;

  // 键盘导航：window keydown 监听 ↑↓ 移动焦点、空格切换选中。用 ref 持最新
  // visibleTasks / toggleSelect，让监听器只挂一次（避免每次 visibleTasks 变化
  // 都 re-subscribe 的窗口竞态）。本块必须在 visibleTasks / toggleSelect 都
  // 已声明之后，否则 TS / runtime 会报"used before declaration"。
  const visibleTasksRef = useRef(visibleTasks);
  useEffect(() => {
    visibleTasksRef.current = visibleTasks;
  }, [visibleTasks]);
  const toggleSelectRef = useRef(toggleSelect);
  useEffect(() => {
    toggleSelectRef.current = toggleSelect;
  }, [toggleSelect]);
  const handleToggleExpandRef = useRef(handleToggleExpand);
  useEffect(() => {
    handleToggleExpandRef.current = handleToggleExpand;
  }, [handleToggleExpand]);
  const handleCancelOpenRef = useRef(handleCancelOpen);
  useEffect(() => {
    handleCancelOpenRef.current = handleCancelOpen;
  }, []);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      // ⌘F / Ctrl+F 永远聚焦搜索框，不论当前在哪个输入控件 —— 与 mac
      // 浏览器 / Finder / Notion 的"⌘F = 搜索"直觉一致。tagName 守卫**之
      // 后**就拦不到 input 内的 ⌘F 了，所以放最前。
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "f") {
        e.preventDefault();
        const el = searchInputRef.current;
        if (el) {
          el.focus();
          el.select();
        }
        return;
      }
      // 用户在 search / 创建表单 / 取消原因等输入里打字、或 button 聚焦时按
      // Enter 激活按钮，方向键 / 空格 / Enter 都不应被 keydown 监听截获 ——
      // tagName 守卫足够过滤所有交互控件（含 BUTTON 让 Enter 走原生 click）。
      const target = e.target as HTMLElement | null;
      const tag = target?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || tag === "BUTTON") return;
      // 单键 "/" 聚焦搜索框 —— 与 GitHub / Linear / VS Code 命令面板直觉
      // 一致。在 tagName 守卫之**后**避免拦截 input 内输入 "/"；只接 plain
      // 单击（无 modifier）让 ⌘/ 等系统快捷键仍能传递。
      if (
        e.key === "/" &&
        !e.metaKey &&
        !e.ctrlKey &&
        !e.altKey &&
        !e.shiftKey
      ) {
        e.preventDefault();
        const el = searchInputRef.current;
        if (el) {
          el.focus();
          el.select();
        }
        return;
      }
      // R116: "n" 快捷键 — 展开创建表单 + focus 标题输入。tagName 守卫已
      // 在上面挡掉 INPUT/TEXTAREA/SELECT/BUTTON，这里安全。setTimeout 0
      // 等 setCreateFormExpanded(true) 触发的 React commit 完，input 才挂上 ref。
      if (
        e.key === "n" &&
        !e.metaKey &&
        !e.ctrlKey &&
        !e.altKey &&
        !e.shiftKey
      ) {
        e.preventDefault();
        setCreateFormExpanded(true);
        setTimeout(() => {
          const el = titleInputRef.current;
          if (el) {
            el.focus();
            el.select();
          }
        }, 0);
        return;
      }
      const list = visibleTasksRef.current;
      if (e.key === "ArrowDown") {
        if (list.length === 0) return;
        e.preventDefault();
        setFocusedIdx((prev) => (prev === null ? 0 : Math.min(prev + 1, list.length - 1)));
      } else if (e.key === "ArrowUp") {
        if (list.length === 0) return;
        e.preventDefault();
        setFocusedIdx((prev) => (prev === null ? 0 : Math.max(0, prev - 1)));
      } else if (e.key === "Home") {
        // Home → 跳第一条；与 ↑↓ 不同，focusedIdx === null 时也直接启动焦点
        // （Home/End 语义明确，不像 Enter 容易误触）。
        if (list.length === 0) return;
        e.preventDefault();
        setFocusedIdx(0);
      } else if (e.key === "End") {
        if (list.length === 0) return;
        e.preventDefault();
        setFocusedIdx(list.length - 1);
      } else if (e.key === " " || e.code === "Space") {
        // 空格 toggle 当前焦点行的选中。focusedIdx === null 时空格不做事
        // （让用户先 ↑↓ 启动焦点模式）。
        setFocusedIdx((prev) => {
          if (prev === null) return null;
          const item = list[prev];
          if (!item) return prev;
          e.preventDefault();
          toggleSelectRef.current(item.title);
          return prev;
        });
      } else if (e.key === "Enter") {
        // Enter 切换当前焦点行的"展开详情"——与点击行 header 等价。同空格门
        // 槛：focusedIdx === null 时不响应（避免 Enter 在普通页面误触）。
        setFocusedIdx((prev) => {
          if (prev === null) return null;
          const item = list[prev];
          if (!item) return prev;
          e.preventDefault();
          // handleToggleExpand 是 async（涉 invoke），fire-and-forget；与
          // 鼠标 onClick 路径同语义。
          void handleToggleExpandRef.current(item.title);
          return prev;
        });
      } else if (e.key === "Delete" || e.key === "Backspace") {
        // Delete / Backspace 触发既有"取消 reason 输入"内联弹层（等价于点
        // 行内取消按钮）。仅 pending / error 行响应（终态行不响应，cancel
        // 已结束任务无意义）。autoFocus 让焦点立刻跳到 reason 输入框。
        setFocusedIdx((prev) => {
          if (prev === null) return null;
          const item = list[prev];
          if (!item) return prev;
          if (item.status !== "pending" && item.status !== "error") return prev;
          e.preventDefault();
          handleCancelOpenRef.current(item.title);
          return prev;
        });
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // visibleTasks 缩短（搜索 / 批量动作后任务消失）→ clamp focusedIdx 防越界
  useEffect(() => {
    setFocusedIdx((prev) => {
      if (prev === null) return null;
      if (visibleTasks.length === 0) return null;
      if (prev >= visibleTasks.length) return visibleTasks.length - 1;
      return prev;
    });
  }, [visibleTasks.length]);

  // 焦点变化 → 把对应行 scrollIntoView，让长队列里键盘翻页跟随视图。
  useEffect(() => {
    if (focusedIdx === null) return;
    const el = document.querySelector<HTMLElement>(`[data-task-idx="${focusedIdx}"]`);
    if (el) {
      el.scrollIntoView({ block: "nearest", behavior: "smooth" });
    }
  }, [focusedIdx]);

  const s = {
    // 主题迁移（迭代 2）：框架级 surface 走 CSS var；功能性配色（status
    // badge、action 按钮、chip、紧迫度等）保持原色不动 —— 它们携带 motion
    // 语义，跨主题需稳定可识别。
    container: { padding: 16, overflowY: "auto" as const, height: "100%", fontFamily: "system-ui, sans-serif", background: "var(--pet-color-bg)" },
    section: { marginBottom: 20 },
    sectionTitle: { fontSize: 14, fontWeight: 600, color: "var(--pet-color-fg)", marginBottom: 8 },
    formCard: { padding: 12, background: "var(--pet-color-card)", border: "1px solid var(--pet-color-border)", borderRadius: 8, marginBottom: 16 },
    label: { fontSize: 12, color: "var(--pet-color-muted)", display: "block", marginBottom: 4 },
    input: { width: "100%", padding: "6px 10px", border: "1px solid var(--pet-color-border)", background: "var(--pet-color-card)", color: "var(--pet-color-fg)", borderRadius: 4, fontSize: 13, boxSizing: "border-box" as const },
    textarea: { width: "100%", padding: "6px 10px", border: "1px solid var(--pet-color-border)", background: "var(--pet-color-card)", color: "var(--pet-color-fg)", borderRadius: 4, fontSize: 13, resize: "vertical" as const, minHeight: 60, boxSizing: "border-box" as const },
    twoCol: { display: "grid", gridTemplateColumns: "1fr 1fr", gap: 8, marginTop: 8 },
    item: { padding: "10px 12px", background: "var(--pet-color-card)", border: "1px solid var(--pet-color-border)", borderRadius: 6, marginBottom: 8 },
    itemHeader: { display: "flex", justifyContent: "space-between", alignItems: "flex-start", gap: 8, marginBottom: 4 },
    itemTitle: { fontWeight: 600, color: "var(--pet-color-fg)", fontSize: 13, lineHeight: 1.3 },
    itemBody: { color: "var(--pet-color-fg)", fontSize: 12, lineHeight: 1.5, marginTop: 4, whiteSpace: "pre-wrap" as const },
    bodyToggleBtn: {
      marginLeft: 4,
      fontSize: 11,
      padding: 0,
      border: "none",
      background: "transparent",
      color: "var(--pet-color-accent)",
      cursor: "pointer",
      fontFamily: "inherit",
      whiteSpace: "nowrap" as const,
    },
    bucketHeader: {
      fontSize: 12,
      fontWeight: 600,
      color: "var(--pet-color-fg)",
      marginTop: 12,
      marginBottom: 4,
      paddingBottom: 4,
      borderBottom: "1px dashed var(--pet-color-border)",
      display: "flex",
      alignItems: "baseline",
      gap: 8,
    },
    bucketCount: {
      color: "var(--pet-color-muted)",
      fontWeight: 400,
    },
    itemMeta: { color: "var(--pet-color-muted)", fontSize: 11, marginTop: 6, display: "flex", gap: 10, flexWrap: "wrap" as const },
    badge: (status: TaskStatus) => ({
      fontSize: 11,
      padding: "2px 8px",
      borderRadius: 10,
      background: STATUS_BADGE[status].bg,
      color: STATUS_BADGE[status].fg,
      whiteSpace: "nowrap" as const,
      flexShrink: 0,
    }),
    priBadge: { fontSize: 11, padding: "2px 8px", borderRadius: 10, background: "var(--pet-tint-yellow-bg)", color: "var(--pet-tint-yellow-fg)", whiteSpace: "nowrap" as const },
    btnPrimary: { padding: "6px 16px", border: "none", borderRadius: 4, background: "var(--pet-color-accent)", color: "var(--pet-color-card)", cursor: "pointer", fontSize: 13, marginTop: 8 },
    btnDisabled: { padding: "6px 16px", border: "none", borderRadius: 4, background: "#94a3b8", color: "#fff", cursor: "not-allowed", fontSize: 13, marginTop: 8 },
    err: { padding: "6px 12px", background: "var(--pet-tint-orange-bg)", color: "var(--pet-tint-orange-fg)", borderRadius: 4, fontSize: 12, marginTop: 8 },
    empty: { padding: 24, textAlign: "center" as const, color: "var(--pet-color-muted)", fontSize: 13 },
    toggleRow: { display: "flex", alignItems: "center", gap: 8, fontSize: 12, color: "var(--pet-color-muted)", marginBottom: 8 },
    searchRow: { display: "flex", gap: 6, marginBottom: 8 },
    searchInput: {
      flex: 1,
      padding: "6px 10px",
      border: "1px solid var(--pet-color-border)",
      background: "var(--pet-color-card)",
      color: "var(--pet-color-fg)",
      borderRadius: 4,
      fontSize: 13,
      boxSizing: "border-box" as const,
    },
    searchClearBtn: {
      padding: "0 10px",
      border: "1px solid var(--pet-color-border)",
      borderRadius: 4,
      background: "var(--pet-color-card)",
      color: "var(--pet-color-muted)",
      cursor: "pointer",
      fontSize: 12,
    },
    searchCount: {
      fontSize: 12,
      color: "var(--pet-color-muted)",
      alignSelf: "center" as const,
      flexShrink: 0,
      paddingLeft: 6,
      whiteSpace: "nowrap" as const,
    },
    tagFilterRow: {
      display: "flex",
      flexWrap: "wrap" as const,
      gap: 4,
      marginBottom: 8,
      alignItems: "center",
    },
    tagFilterLabel: { fontSize: 11, color: "var(--pet-color-muted)", marginRight: 4 },
    tagFilterChip: (selected: boolean) => ({
      fontSize: 11,
      padding: "2px 8px",
      borderRadius: 10,
      background: selected ? "#c7d2fe" : "#f1f5f9",
      color: selected ? "#3730a3" : "#475569",
      cursor: "pointer",
      whiteSpace: "nowrap" as const,
      userSelect: "none" as const,
      border: "1px solid transparent",
    }),
    tagFilterCount: {
      fontSize: 10,
      opacity: 0.7,
      marginLeft: 2,
    },
    errorMsg: { color: "var(--pet-tint-orange-fg)", fontSize: 11, marginTop: 4 },
    cancelledMsg: { color: "#64748b", fontSize: 11, marginTop: 4 },
    resultMsg: { color: "#166534", fontSize: 11, marginTop: 4 },
    tagRow: { display: "flex", flexWrap: "wrap" as const, gap: 4, marginTop: 4 },
    tagChip: {
      fontSize: 10,
      padding: "1px 6px",
      borderRadius: 8,
      background: "#f1f5f9",
      color: "#475569",
      whiteSpace: "nowrap" as const,
      cursor: "pointer" as const,
      userSelect: "none" as const,
    },
    actionRow: { display: "flex", gap: 6, marginTop: 8, flexWrap: "wrap" as const },
    actionBtn: {
      padding: "4px 10px",
      border: "1px solid #cbd5e1",
      borderRadius: 4,
      background: "#fff",
      color: "#334155",
      cursor: "pointer",
      fontSize: 12,
    },
    actionBtnRetry: {
      padding: "4px 10px",
      border: "1px solid #bae6fd",
      borderRadius: 4,
      background: "#f0f9ff",
      color: "#0369a1",
      cursor: "pointer",
      fontSize: 12,
    },
    actionBtnDanger: {
      padding: "4px 10px",
      border: "1px solid #fecaca",
      borderRadius: 4,
      background: "#fff",
      color: "var(--pet-tint-orange-fg)",
      cursor: "pointer",
      fontSize: 12,
    },
    actionBtnDisabled: {
      padding: "4px 10px",
      border: "1px solid #e2e8f0",
      borderRadius: 4,
      background: "#f1f5f9",
      color: "#94a3b8",
      cursor: "not-allowed",
      fontSize: 12,
    },
    cancelInputRow: { marginTop: 8, display: "flex", gap: 6, flexWrap: "wrap" as const },
    cancelInput: {
      flex: 1,
      minWidth: 180,
      padding: "4px 8px",
      border: "1px solid var(--pet-color-border)",
      background: "var(--pet-color-card)",
      color: "var(--pet-color-fg)",
      borderRadius: 4,
      fontSize: 12,
    },
    headerClickable: { cursor: "pointer" as const },
    chevron: {
      display: "inline-block" as const,
      width: 14,
      color: "var(--pet-color-muted)",
      fontSize: 11,
      marginRight: 4,
      userSelect: "none" as const,
    },
    detailPanel: {
      marginTop: 10,
      paddingTop: 10,
      borderTop: "1px dashed #e2e8f0",
      display: "flex",
      flexDirection: "column" as const,
      gap: 10,
    },
    detailSection: { display: "flex", flexDirection: "column" as const, gap: 4 },
    detailLabel: {
      fontSize: 11,
      color: "#64748b",
      fontWeight: 600,
      textTransform: "uppercase" as const,
      letterSpacing: "0.04em",
    },
    detailHint: { fontSize: 11, color: "#94a3b8", fontStyle: "italic" as const },
    rawDescBox: {
      fontSize: 12,
      color: "#1e293b",
      background: "#f8fafc",
      padding: "6px 8px",
      borderRadius: 4,
      whiteSpace: "pre-wrap" as const,
      wordBreak: "break-word" as const,
      fontFamily: "'SF Mono', 'Menlo', monospace",
      // 宽屏下锁住阅读行宽：~60-80 中文字符是舒适视幅；超 800px 时单行
      // 视线水平扫太费眼。窗口窄于 800px 时 maxWidth 不生效，仍 100%。
      maxWidth: 800,
    },
    detailMdBox: {
      fontSize: 12,
      color: "#334155",
      background: "#fff",
      padding: "6px 8px",
      border: "1px solid #f1f5f9",
      borderRadius: 4,
      whiteSpace: "pre-wrap" as const,
      lineHeight: 1.55,
      maxWidth: 800,
    },
    historyList: { display: "flex", flexDirection: "column" as const, gap: 4 },
    historyItem: {
      fontSize: 11,
      color: "#475569",
      display: "flex",
      gap: 8,
      alignItems: "flex-start",
      lineHeight: 1.5,
    },
    historyTs: {
      color: "#94a3b8",
      fontFamily: "'SF Mono', 'Menlo', monospace",
      flexShrink: 0,
    },
    bulkBar: {
      display: "flex",
      alignItems: "center",
      gap: 6,
      flexWrap: "wrap" as const,
      padding: "8px 10px",
      background: "#eff6ff",
      border: "1px solid #bfdbfe",
      borderRadius: 6,
      marginBottom: 8,
      fontSize: 12,
      color: "#1e3a8a",
    },
    bulkSelectionLabel: { fontWeight: 600, marginRight: 6 },
    bulkBtn: {
      padding: "4px 10px",
      border: "1px solid #bfdbfe",
      borderRadius: 4,
      background: "#fff",
      color: "#1e40af",
      cursor: "pointer",
      fontSize: 12,
    },
    bulkBtnActive: {
      padding: "4px 10px",
      border: "1px solid #1e40af",
      borderRadius: 4,
      background: "#1e40af",
      color: "#fff",
      cursor: "pointer",
      fontSize: 12,
    },
    bulkBtnDisabled: {
      padding: "4px 10px",
      border: "1px solid #e2e8f0",
      borderRadius: 4,
      background: "#f1f5f9",
      color: "#94a3b8",
      cursor: "not-allowed",
      fontSize: 12,
    },
    bulkSubPanel: {
      marginTop: 8,
      padding: "8px 10px",
      background: "#f8fafc",
      border: "1px dashed #cbd5e1",
      borderRadius: 4,
      display: "flex",
      gap: 6,
      flexWrap: "wrap" as const,
      alignItems: "center",
    },
    bulkSubInput: {
      flex: 1,
      minWidth: 140,
      padding: "4px 8px",
      border: "1px solid #cbd5e1",
      borderRadius: 4,
      fontSize: 12,
    },
    bulkResult: {
      padding: "4px 10px",
      background: "#dcfce7",
      color: "#166534",
      borderRadius: 4,
      fontSize: 12,
      marginBottom: 8,
    },
    rowCheckbox: {
      marginRight: 4,
      cursor: "pointer" as const,
      flexShrink: 0,
      width: 14,
      height: 14,
    },
    historyAction: (action: string): React.CSSProperties => {
      const map: Record<string, { bg: string; fg: string }> = {
        create: { bg: "#dbeafe", fg: "#1e40af" },
        update: { bg: "#f1f5f9", fg: "#475569" },
        delete: { bg: "#fee2e2", fg: "#991b1b" },
      };
      const c = map[action] ?? { bg: "#f1f5f9", fg: "#475569" };
      return {
        fontSize: 10,
        padding: "1px 6px",
        borderRadius: 8,
        background: c.bg,
        color: c.fg,
        whiteSpace: "nowrap" as const,
        flexShrink: 0,
      };
    },
  };

  return (
    <div style={s.container}>
      {/* CSS hover-only 显隐：与 PanelChat 既有 .pet-chat-row .pet-copy-btn 同
          模式（hover 整段渐显，再 hover 按钮自身强化）。已复制态用 inline
          style `opacity: 1` + 绿色覆盖默认 hover-only 显示，让 1.5s 反馈窗口
          内即便鼠标移开也可见。 */}
      <style>{`
        .pet-detail-section .pet-detail-copy-btn {
          opacity: 0;
          transition: opacity 120ms ease-out, color 120ms ease-out, border-color 120ms ease-out;
        }
        .pet-detail-section:hover .pet-detail-copy-btn {
          opacity: 0.85;
        }
        .pet-detail-section .pet-detail-copy-btn:hover {
          opacity: 1;
          color: #0ea5e9;
          border-color: #7dd3fc;
        }
        /* R123: 任务卡 hover 高亮。与 R122 PanelMemory 同款，bg 切到
           var(--pet-color-bg) 与 card 反差。!important 反压 inline s.item
           优先级。focus outline / 内部 detail 区块各自独立，互不干扰。 */
        .pet-task-card {
          transition: background-color 0.12s ease;
        }
        .pet-task-card:hover {
          background: var(--pet-color-bg) !important;
        }
      `}</style>
      <div style={s.section}>
        <div
          style={{ ...s.sectionTitle, display: "flex", alignItems: "center", gap: 6, cursor: "pointer", userSelect: "none" }}
          onClick={() => setCreateFormExpanded((v) => !v)}
          title={
            createFormExpanded
              ? "点击折叠新建任务表单（节省垂直空间，跨 session 记忆）"
              : "点击展开新建任务表单"
          }
        >
          <span style={{ width: 10, fontFamily: "monospace", color: "#475569" }}>
            {createFormExpanded ? "▾" : "▸"}
          </span>
          <span>新建任务</span>
        </div>
        {createFormExpanded && (
        <div style={s.formCard}>
          <label style={s.label}>标题</label>
          <input
            style={s.input}
            ref={titleInputRef}
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            onKeyDown={handleFormKeyDown}
            placeholder="比如：整理 Downloads"
          />
          <label style={{ ...s.label, marginTop: 8 }}>描述（可选）</label>
          <textarea
            style={s.textarea}
            value={body}
            onChange={(e) => setBody(e.target.value)}
            onKeyDown={handleFormKeyDown}
            placeholder="把要点说清楚，比如：把 30 天前的文件挪到 ~/Archive/"
          />
          <div style={s.twoCol}>
            <div>
              <label style={s.label}>优先级 (0-{PRIORITY_MAX})</label>
              <input
                type="number"
                min={0}
                max={PRIORITY_MAX}
                style={s.input}
                value={priority}
                onChange={(e) => {
                  const n = parseInt(e.target.value, 10);
                  if (Number.isNaN(n)) return;
                  setPriority(Math.max(0, Math.min(PRIORITY_MAX, n)));
                }}
                onKeyDown={handleFormKeyDown}
              />
            </div>
            <div>
              <label style={s.label}>截止时间（可选）</label>
              <input
                type="datetime-local"
                style={s.input}
                value={due}
                onChange={(e) => setDue(e.target.value)}
                onKeyDown={handleFormKeyDown}
              />
            </div>
          </div>
          <button
            style={creating || !title.trim() ? s.btnDisabled : s.btnPrimary}
            onClick={handleCreate}
            disabled={creating || !title.trim()}
            title="创建任务（⌘Enter / Ctrl+Enter 等价）"
          >
            {creating ? "创建中..." : "创建任务"}
          </button>
          {errMsg && <div style={s.err}>{errMsg}</div>}
        </div>
        )}
      </div>

      <div style={s.section}>
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 8 }}>
          <div>
            <div style={{ ...s.sectionTitle, marginBottom: 0 }}>
              队列
              {sortMode === "queue"
                ? "（按宠物处理顺序）"
                : sortMode === "due"
                  ? "（按 due 升序）"
                  : "（按优先级降序，高 → 低）"}
            </div>
            <div
              style={{ fontSize: 11, color: "var(--pet-color-muted)", fontWeight: 400, marginTop: 2 }}
              title="status=done 且 updated_at 在窗口内的任务数（cancelled 不计；近 7 天为 rolling 窗口）"
            >
              今日完成 {completionStats.today} · 近 7 天 {completionStats.week}
            </div>
          </div>
          <div style={{ display: "flex", gap: 4 }} title="切换排序模式：默认综合 / 按截止时间升序 / 按优先级降序">
            {(["queue", "due", "priority"] as const).map((mode) => {
              const active = sortMode === mode;
              return (
                <button
                  key={mode}
                  type="button"
                  onClick={() => setSortMode(mode)}
                  style={{
                    fontSize: 11,
                    padding: "2px 8px",
                    border: "1px solid",
                    borderColor: active ? "#0ea5e9" : "#e2e8f0",
                    borderRadius: 4,
                    background: active ? "#0ea5e9" : "#fff",
                    color: active ? "#fff" : "#475569",
                    cursor: active ? "default" : "pointer",
                    fontWeight: active ? 600 : 400,
                  }}
                >
                  {mode === "queue" ? "队列" : mode === "due" ? "due ↑" : "P ↓"}
                </button>
              );
            })}
          </div>
        </div>
        <div style={s.searchRow}>
          <input
            ref={searchInputRef}
            type="text"
            placeholder="按标题或内容搜索…（⌘F 或 / 聚焦）"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            style={s.searchInput}
          />
          {search && (
            <button
              type="button"
              onClick={() => setSearch("")}
              style={s.searchClearBtn}
              aria-label="清除搜索"
            >
              ✕
            </button>
          )}
          {filtersActive && (
            <>
              <span
                style={s.searchCount}
                title="visibleTasks / tasks 全集（不再叠加 showFinished 过滤）"
              >
                {visibleTasks.length} / {tasks.length} 条匹配
              </span>
              <button
                type="button"
                onClick={() => {
                  setSearch("");
                  setSelectedTags(new Set());
                  setDueFilter("all");
                  setPriorityFilter(new Set());
                }}
                style={s.searchClearBtn}
                title="一键清掉全部 active filter（search / tag / due / priority）"
                aria-label="清除全部过滤"
              >
                ✕ 全部
              </button>
            </>
          )}
        </div>
        {(dueTodayCount > 0 || overdueCount > 0 || priorityCounts.length > 0) && (
          <div style={{ ...s.tagFilterRow, marginBottom: 6 }}>
            {overdueCount > 0 && (
              <DueChip
                kind="overdue"
                count={overdueCount}
                active={dueFilter === "overdue"}
                onToggle={() =>
                  setDueFilter((prev) => (prev === "overdue" ? "all" : "overdue"))
                }
              />
            )}
            {dueTodayCount > 0 && (
              <DueChip
                kind="today"
                count={dueTodayCount}
                active={dueFilter === "today"}
                onToggle={() =>
                  setDueFilter((prev) => (prev === "today" ? "all" : "today"))
                }
              />
            )}
            {/* R104: priority 多选 chip 行。OR 命中（任一进集合即通过）；
                P0 保留 "💡 idea 抽屉" glyph 让老用户直觉不变，其它走 P{n}
                朴素文案。slate / gray 中性色与 dueFilter 同色族让 priority
                视为"另一个非-时态过滤维度"；与决策日志的鲜艳 accent 区分
                （priority 是结构化数字而非 kind enum）。 */}
            {priorityCounts.map(([p, count]) => {
              const active = priorityFilter.has(p);
              const togglePriority = () =>
                setPriorityFilter((prev) => {
                  const next = new Set(prev);
                  if (next.has(p)) next.delete(p);
                  else next.add(p);
                  return next;
                });
              return (
                <span
                  key={p}
                  role="button"
                  tabIndex={0}
                  onClick={togglePriority}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" || e.key === " ") {
                      e.preventDefault();
                      togglePriority();
                    }
                  }}
                  title={
                    active
                      ? `再次点击移出 P${p} 过滤集合（多选）`
                      : `加入到只看的 priority 集合（多选）：P${p}（${count} 条活动任务）`
                  }
                  style={{
                    fontSize: 11,
                    padding: "2px 8px",
                    borderRadius: 10,
                    background: active ? "#cbd5e1" : "#f1f5f9",
                    color: "#475569",
                    cursor: "pointer",
                    whiteSpace: "nowrap",
                    userSelect: "none",
                    border: `1px solid ${active ? "#94a3b8" : "#e2e8f0"}`,
                  }}
                >
                  {active ? "✓ " : ""}{p === 0 ? "💡 P0" : `P${p}`}
                  <span style={{ fontSize: 10, opacity: 0.7, marginLeft: 2 }}>
                    ({count})
                  </span>
                </span>
              );
            })}
          </div>
        )}
        {allTags.length > 0 && (
          <div style={s.tagFilterRow}>
            <span style={s.tagFilterLabel}>tag：</span>
            {allTags.map(([tag, count]) => {
              const selected = selectedTags.has(tag);
              return (
                <span
                  key={tag}
                  style={s.tagFilterChip(selected)}
                  onClick={() => toggleTag(tag)}
                  title={
                    selected
                      ? `点击取消「${tag}」过滤（当前共 ${count} 条带此 tag 的任务）`
                      : `点击只看带「${tag}」的任务（共 ${count} 条）`
                  }
                >
                  {selected ? "✓ " : ""}#{tag}
                  <span style={s.tagFilterCount}> ({count})</span>
                </span>
              );
            })}
            {selectedTags.size > 0 && (
              <button
                type="button"
                onClick={() => setSelectedTags(new Set())}
                style={s.searchClearBtn}
                aria-label="清除 tag 筛选"
              >
                清除
              </button>
            )}
          </div>
        )}
        <label style={s.toggleRow}>
          <input
            type="checkbox"
            checked={showFinished}
            onChange={(e) => setShowFinished(e.target.checked)}
          />
          显示已结束（含已完成 / 已取消）
        </label>
        {bulkResultMsg && <div style={s.bulkResult}>{bulkResultMsg}</div>}
        {selected.size > 0 && (
          <>
            <div style={s.bulkBar}>
              <span style={s.bulkSelectionLabel}>已选 {selected.size}</span>
              <button
                style={bulkBusy ? s.bulkBtnDisabled : s.bulkBtn}
                disabled={bulkBusy}
                onClick={handleBulkRetry}
                title="对所有选中任务尝试重试，跳过非 error 状态的"
              >
                重试
              </button>
              <button
                style={bulkBusy ? s.bulkBtnDisabled : (bulkAction === "cancel" ? s.bulkBtnActive : s.bulkBtn)}
                disabled={bulkBusy}
                onClick={() => {
                  setBulkAction((a) => (a === "cancel" ? null : "cancel"));
                  setBulkReason("");
                }}
                title="批量取消（共享同一个原因）"
              >
                取消
              </button>
              <button
                style={bulkBusy ? s.bulkBtnDisabled : (bulkAction === "priority" ? s.bulkBtnActive : s.bulkBtn)}
                disabled={bulkBusy}
                onClick={() => setBulkAction((a) => (a === "priority" ? null : "priority"))}
                title="把所有选中任务的优先级改成同一个值"
              >
                改优先级
              </button>
              <button
                style={bulkBusy ? s.bulkBtnDisabled : (bulkAction === "due" ? s.bulkBtnActive : s.bulkBtn)}
                disabled={bulkBusy}
                onClick={() => {
                  setBulkAction((a) => (a === "due" ? null : "due"));
                  setBulkDue("");
                }}
                title="把所有选中任务的截止时间改成同一个；留空确认 = 清掉 due"
              >
                改 due
              </button>
              <button
                style={bulkBusy ? s.bulkBtnDisabled : (bulkAction === "tags" ? s.bulkBtnActive : s.bulkBtn)}
                disabled={bulkBusy}
                onClick={() => {
                  setBulkAction((a) => (a === "tags" ? null : "tags"));
                  setBulkTagOps("");
                }}
                title="批量改 tag：输 +tag1 -tag2 单行，加 / 删可混用"
              >
                改 tags
              </button>
              <button
                style={bulkBusy ? s.bulkBtnDisabled : s.bulkBtn}
                disabled={bulkBusy}
                onClick={handleBulkCopyTitles}
                title="复制选中任务的标题清单（一行一个），适合快速贴 todo dump 到聊天"
              >
                复制标题
              </button>
              <button
                style={bulkBusy ? s.bulkBtnDisabled : s.bulkBtn}
                disabled={bulkBusy}
                onClick={handleBulkCopyAsMd}
                title="把所有选中任务拼成多段 ## title markdown 写到剪贴板（不含 detail.md 进度笔记）"
              >
                复制为 MD
              </button>
              <span style={{ flex: 1 }} />
              <button
                style={bulkBusy ? s.bulkBtnDisabled : s.bulkBtn}
                disabled={bulkBusy || visibleTasks.length === 0}
                onClick={() => {
                  setSelected(new Set(visibleTasks.map((t) => t.title)));
                }}
                title="把当前可见任务全部选中"
              >
                全选可见
              </button>
              <button
                style={bulkBusy ? s.bulkBtnDisabled : s.bulkBtn}
                disabled={bulkBusy}
                onClick={clearSelection}
              >
                取消选择
              </button>
            </div>
            {bulkAction === "cancel" && (
              <div style={s.bulkSubPanel}>
                <input
                  style={s.bulkSubInput}
                  placeholder="取消原因（共享，可留空）"
                  value={bulkReason}
                  onChange={(e) => setBulkReason(e.target.value)}
                  autoFocus
                />
                <button
                  style={bulkBusy ? s.bulkBtnDisabled : s.bulkBtnActive}
                  disabled={bulkBusy}
                  onClick={handleBulkCancelConfirm}
                >
                  {bulkBusy ? "处理中..." : "确认批量取消"}
                </button>
                <button
                  style={s.bulkBtn}
                  onClick={() => setBulkAction(null)}
                  disabled={bulkBusy}
                >
                  关闭
                </button>
              </div>
            )}
            {bulkAction === "priority" && (
              <div style={s.bulkSubPanel}>
                <span>新优先级 (0-{PRIORITY_MAX})：</span>
                <input
                  type="number"
                  min={0}
                  max={PRIORITY_MAX}
                  style={{ ...s.bulkSubInput, minWidth: 60, flex: "0 0 80px" }}
                  value={bulkPriority}
                  onChange={(e) => {
                    const n = parseInt(e.target.value, 10);
                    if (Number.isNaN(n)) return;
                    setBulkPriority(Math.max(0, Math.min(PRIORITY_MAX, n)));
                  }}
                  autoFocus
                />
                <label
                  style={{
                    fontSize: 11,
                    color: "#475569",
                    display: "flex",
                    alignItems: "center",
                    gap: 4,
                    whiteSpace: "nowrap",
                    cursor: "pointer",
                  }}
                  title="勾选后同次也清掉所有选中任务的 due，让重排紧急度后旧时间不残留"
                >
                  <input
                    type="checkbox"
                    checked={bulkPriorityClearDue}
                    onChange={(e) => setBulkPriorityClearDue(e.target.checked)}
                    disabled={bulkBusy}
                  />
                  同时清 due
                </label>
                <button
                  style={bulkBusy ? s.bulkBtnDisabled : s.bulkBtnActive}
                  disabled={bulkBusy}
                  onClick={handleBulkSetPriorityConfirm}
                >
                  {bulkBusy ? "处理中..." : "确认"}
                </button>
                <button
                  style={s.bulkBtn}
                  onClick={() => setBulkAction(null)}
                  disabled={bulkBusy}
                >
                  关闭
                </button>
              </div>
            )}
            {bulkAction === "due" && (
              <div style={s.bulkSubPanel}>
                <span>新截止时间：</span>
                <input
                  type="datetime-local"
                  style={{ ...s.bulkSubInput, flex: "0 0 200px" }}
                  value={bulkDue}
                  onChange={(e) => setBulkDue(e.target.value)}
                  autoFocus
                />
                <button
                  style={bulkBusy ? s.bulkBtnDisabled : s.bulkBtnActive}
                  disabled={bulkBusy}
                  onClick={handleBulkSetDueConfirm}
                  title={bulkDue.trim().length === 0 ? "留空确认 = 清掉所有选中任务的 due" : "覆盖到所有选中任务"}
                >
                  {bulkBusy
                    ? "处理中..."
                    : bulkDue.trim().length === 0
                      ? "确认（清空 due）"
                      : "确认"}
                </button>
                <button
                  style={s.bulkBtn}
                  onClick={() => setBulkAction(null)}
                  disabled={bulkBusy}
                >
                  关闭
                </button>
              </div>
            )}
            {bulkAction === "tags" && (
              <div style={s.bulkSubPanel}>
                <input
                  style={s.bulkSubInput}
                  placeholder="+tag1 -tag2 +工作"
                  value={bulkTagOps}
                  onChange={(e) => setBulkTagOps(e.target.value)}
                  autoFocus
                  title="+加 / -删，空白分隔；同次输入不能同时 +X -X。tag 名只许字母 / 数字 / 中文 / `_` / `-`。"
                />
                <button
                  style={bulkBusy || bulkTagOps.trim().length === 0 ? s.bulkBtnDisabled : s.bulkBtnActive}
                  disabled={bulkBusy || bulkTagOps.trim().length === 0}
                  onClick={handleBulkSetTagsConfirm}
                  title="覆盖到所有选中任务（每条独立 add/remove）"
                >
                  {bulkBusy ? "处理中..." : "确认"}
                </button>
                <button
                  style={s.bulkBtn}
                  onClick={() => setBulkAction(null)}
                  disabled={bulkBusy}
                >
                  关闭
                </button>
              </div>
            )}
          </>
        )}
        {actionErr && <div style={s.err}>{actionErr}</div>}
        {visibleTasks.length === 0 ? (
          <div style={s.empty}>
            {filtersActive
              ? "没有匹配筛选条件的任务"
              : showFinished
                ? "还没有任何任务"
                : "队列里没有进行中的任务"}
          </div>
        ) : (
          visibleTasks.map((t, idx) => {
            const busy = busyTitle === t.title;
            const cancelOpen = cancellingTitle === t.title;
            const canRetry = t.status === "error";
            const canCancel = t.status === "pending" || t.status === "error";
            const expanded = expandedTitle === t.title;
            const detail = detailMap[t.title];
            const detailLoading = detailLoadingTitle === t.title;
            const focused = focusedIdx === idx;
            // R94: 当前是 finished 任务且与前一条桶不同时插入 subheader。
            // 前一条非 finished（或 idx=0）时，prevBucket=null 触发首段
            // header；连续同桶时 prevBucket===curBucket 抑制 header。
            const isFin = isFinished(t.status);
            let curBucket: FinishedBucket | null = null;
            let showBucketHeader = false;
            if (isFin) {
              curBucket = bucketFor(
                Date.parse(t.updated_at) || 0,
                bucketBoundaries.todayMs,
                bucketBoundaries.yesterdayMs,
                bucketBoundaries.weekStartMs,
              );
              const prev = idx > 0 ? visibleTasks[idx - 1] : null;
              const prevBucket =
                prev && isFinished(prev.status)
                  ? bucketFor(
                      Date.parse(prev.updated_at) || 0,
                      bucketBoundaries.todayMs,
                      bucketBoundaries.yesterdayMs,
                      bucketBoundaries.weekStartMs,
                    )
                  : null;
              showBucketHeader = curBucket !== prevBucket;
            }
            const taskCard = (
              <div
                data-task-idx={idx}
                className="pet-task-card"
                style={{
                  ...s.item,
                  ...(focused
                    ? {
                        outline: "2px solid #93c5fd",
                        outlineOffset: "-2px",
                      }
                    : {}),
                }}
              >
                <div
                  style={{ ...s.itemHeader, ...s.headerClickable }}
                  onClick={() => handleToggleExpand(t.title)}
                  title={expanded ? "点击折叠详情" : "点击展开任务详情（描述 / 进度笔记 / 事件时间线）"}
                >
                  <div style={s.itemTitle}>
                    <input
                      type="checkbox"
                      style={s.rowCheckbox}
                      checked={selected.has(t.title)}
                      onChange={() => toggleSelect(t.title)}
                      // 阻止冒泡：勾选不触发 expand
                      onClick={(e) => e.stopPropagation()}
                      aria-label={`select ${t.title}`}
                    />
                    <span style={s.chevron}>{expanded ? "▾" : "▸"}</span>
                    <HighlightedText text={t.title} query={search} />
                    {isRecentlyUpdated(t.updated_at, nowMs) && (
                      <span
                        title={formatRecentlyUpdatedHint(t.updated_at, nowMs)}
                        style={{
                          color: "#22c55e",
                          fontSize: 8,
                          marginLeft: 6,
                          lineHeight: 1,
                          verticalAlign: "middle",
                          userSelect: "none",
                        }}
                        aria-label="recently updated"
                      >
                        ●
                      </span>
                    )}
                    {(() => {
                      // 「未读」红点：lastview 存在 & updated_at 比 lastview 晚。
                      // lastview 不存在（从未打开过）→ 不显（避免初次安装满屏红
                      // 点）；那种"全新任务"语义由绿点覆盖。lastviewBump 进入闭包
                      // 让 re-render 触发；值本身不参与判定。
                      void lastviewBump;
                      let lv: string | null = null;
                      try {
                        lv = window.localStorage.getItem(
                          `pet-task-history-lastview-${t.title}`,
                        );
                      } catch {
                        return null;
                      }
                      if (lv === null) return null;
                      if (!tsAfter(t.updated_at, lv)) return null;
                      return (
                        <span
                          title="距上次展开此任务后又有更新 — 点击展开看新事件"
                          style={{
                            color: "var(--pet-tint-orange-fg)",
                            fontSize: 8,
                            marginLeft: 4,
                            lineHeight: 1,
                            verticalAlign: "middle",
                            userSelect: "none",
                          }}
                          aria-label="unread updates"
                        >
                          ●
                        </span>
                      );
                    })()}
                  </div>
                  <div style={{ display: "flex", gap: 6 }}>
                    <span style={s.priBadge}>P{t.priority}</span>
                    <span style={s.badge(t.status)}>{STATUS_BADGE[t.status].label}</span>
                  </div>
                </div>
                {t.body && (() => {
                  // R91: 长描述折叠。> 200 字才折叠到前 120 字 + 展开按钮。
                  // 搜索 keyword 命中 body 时强制展开 —— 折叠态会让命中点
                  // 在 120 字外的高亮看不见，搜索 UX 会崩。
                  const isLong = t.body.length > BODY_FOLD_THRESHOLD;
                  const key = `${t.title}-${t.created_at}`;
                  const expanded = expandedBodies.has(key);
                  const q = search.trim().toLowerCase();
                  const matchInBody =
                    q !== "" && t.body.toLowerCase().includes(q);
                  const folded = isLong && !expanded && !matchInBody;
                  const shown = folded
                    ? t.body.slice(0, BODY_FOLD_PREVIEW) + "…"
                    : t.body;
                  return (
                    <div style={s.itemBody}>
                      <HighlightedText text={shown} query={search} />
                      {isLong && !matchInBody && (
                        <button
                          type="button"
                          onClick={() => {
                            setExpandedBodies((prev) => {
                              const next = new Set(prev);
                              if (next.has(key)) next.delete(key);
                              else next.add(key);
                              return next;
                            });
                          }}
                          style={s.bodyToggleBtn}
                          title={
                            folded
                              ? `展开全部 ${t.body.length} 字`
                              : "折叠到前 120 字"
                          }
                        >
                          {folded
                            ? `… 展开 (${t.body.length} 字)`
                            : `收起 (${t.body.length} 字)`}
                        </button>
                      )}
                    </div>
                  );
                })()}
                {t.tags.length > 0 && (
                  <div style={s.tagRow}>
                    {t.tags.map((tag) => (
                      <span
                        key={tag}
                        style={s.tagChip}
                        onClick={() => toggleTag(tag)}
                        title={selectedTags.has(tag) ? "点击取消该 tag 筛选" : "点击只看带此 tag 的任务"}
                      >
                        {selectedTags.has(tag) ? "✓ " : ""}#{tag}
                      </span>
                    ))}
                  </div>
                )}
                {/* 已结束（done / cancelled）的任务若有产物，独立一行显示 */}
                {t.result && (t.status === "done" || t.status === "cancelled") && (
                  <div style={s.resultMsg}>✓ 产物：{t.result}</div>
                )}
                {t.status === "error" && t.error_message && (
                  <div style={s.errorMsg}>失败原因：{t.error_message}</div>
                )}
                {t.status === "cancelled" && t.error_message && (
                  <div style={s.cancelledMsg}>取消原因：{t.error_message}</div>
                )}
                <div style={s.itemMeta}>
                  {t.due && (() => {
                    const urgency = dueUrgency(t.due, nowMs, t.status);
                    // R136: tooltip 在 enum-level urgency 之后附精确数字
                    // (X 小时 / 天 后/前)，让用户判断紧迫度更准。normal urgency
                    // 也显 relative（之前 undefined 不显，现在统一让 hover 都
                    // 有信息）。
                    const relative = formatDueRelative(t.due, nowMs);
                    const tooltip =
                      urgency === "overdue"
                        ? `已过期：${relative}`
                        : urgency === "soon"
                          ? `24 小时内到期：${relative}`
                          : relative;
                    return (
                      <span
                        style={{ color: dueColor(urgency), fontWeight: urgency === "normal" ? undefined : 600 }}
                        title={tooltip}
                      >
                        截止 {formatDue(t.due)}
                      </span>
                    );
                  })()}
                  <span>
                    创建于 {t.created_at.slice(0, 16).replace("T", " ")}
                    {(() => {
                      const rel = formatRelativeAge(t.created_at, nowMs);
                      return rel ? ` · ${rel}` : null;
                    })()}
                  </span>
                </div>
                {(canRetry || canCancel) && !cancelOpen && (
                  <div style={s.actionRow}>
                    {canRetry && (
                      <button
                        style={busy ? s.actionBtnDisabled : s.actionBtnRetry}
                        disabled={busy}
                        onClick={() => handleRetry(t.title)}
                      >
                        {busy ? "处理中..." : "重试"}
                      </button>
                    )}
                    {canCancel && (
                      <button
                        style={busy ? s.actionBtnDisabled : s.actionBtnDanger}
                        disabled={busy}
                        onClick={() => handleCancelOpen(t.title)}
                      >
                        取消
                      </button>
                    )}
                  </div>
                )}
                {cancelOpen && (
                  <div style={s.cancelInputRow}>
                    <input
                      style={s.cancelInput}
                      placeholder="取消原因（可留空）"
                      value={cancelReason}
                      onChange={(e) => setCancelReason(e.target.value)}
                      autoFocus
                    />
                    <button
                      style={busy ? s.actionBtnDisabled : s.actionBtnDanger}
                      disabled={busy}
                      onClick={() => handleCancelConfirm(t.title)}
                    >
                      {busy ? "处理中..." : "确认取消"}
                    </button>
                    <button style={s.actionBtn} onClick={handleCancelClose} disabled={busy}>
                      不取消
                    </button>
                  </div>
                )}
                {expanded && (
                  <div style={s.detailPanel}>
                    {detailLoading && !detail && (
                      <div style={s.detailHint}>加载中…</div>
                    )}
                    {detailErr && expandedTitle === t.title && (
                      <div style={s.err}>{detailErr}</div>
                    )}
                    {detail && (
                      <>
                        {/* 阅读宽度 slider —— 跨 session 记忆，限 [600, max(window-80, 1200)] */}
                        <div
                          style={{
                            display: "flex",
                            alignItems: "center",
                            gap: 6,
                            fontSize: 11,
                            color: "#94a3b8",
                            marginBottom: 4,
                          }}
                          title="拖动调整完整描述 / 进度笔记的阅读行宽（绝对上限 1200，但窗口窄于 1280 时跟随 window-80 缩短，避免超出可见范围）；自动写 localStorage 跨 session 记忆"
                        >
                          <span>阅读宽度</span>
                          <input
                            type="range"
                            min={600}
                            max={detailMaxWidthCap}
                            step={50}
                            value={detailMaxWidthEffective}
                            onChange={(e) => setDetailMaxWidth(parseInt(e.target.value, 10))}
                            style={{ flex: "0 0 160px" }}
                          />
                          <span style={{ fontFamily: "'SF Mono', 'Menlo', monospace" }}>
                            {detailMaxWidthEffective}px
                          </span>
                          {showWidthHint && (
                            <span
                              style={{
                                fontSize: 10,
                                color: "#cbd5e1",
                                fontStyle: "italic",
                                marginLeft: 4,
                              }}
                            >
                              💡 拖动调整阅读行宽 →
                            </span>
                          )}
                        </div>
                        {(() => {
                          const k = `${t.title}-asMd`;
                          const copied = copiedDetailKey === k;
                          return (
                            <div
                              style={{
                                display: "flex",
                                justifyContent: "flex-end",
                                marginBottom: 4,
                              }}
                            >
                              <button
                                type="button"
                                onClick={() =>
                                  handleCopyDetail(k, formatTaskAsMarkdown(t, detail))
                                }
                                title={
                                  copied
                                    ? "已复制 markdown"
                                    : "复制为 markdown：## 标题 + 状态/优先级/截止/标签 + 描述 + 进度笔记 + 产物，方便贴到 Notion / Obsidian / 周记"
                                }
                                style={{
                                  padding: "2px 8px",
                                  fontSize: 10,
                                  lineHeight: 1.2,
                                  border: "1px solid #cbd5e1",
                                  borderRadius: 4,
                                  background: "#fff",
                                  color: copied ? "#16a34a" : "#475569",
                                  cursor: "pointer",
                                  whiteSpace: "nowrap",
                                }}
                              >
                                {copied ? "已复制" : "Copy as MD"}
                              </button>
                            </div>
                          );
                        })()}
                        <div className="pet-detail-section" style={s.detailSection}>
                          <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
                            <span
                              style={s.detailLabel}
                              title="index.yaml 里的原始 description（含 [task pri=...] / [origin:...] 等所有 marker，未做剥离）。"
                            >
                              完整描述
                            </span>
                            {detail.raw_description && (() => {
                              const k = `${t.title}-rawDesc`;
                              const copied = copiedDetailKey === k;
                              return (
                                <button
                                  type="button"
                                  className="pet-detail-copy-btn"
                                  onClick={() => handleCopyDetail(k, detail.raw_description)}
                                  title={copied ? "已复制" : "复制完整描述"}
                                  style={{
                                    padding: "2px 6px",
                                    fontSize: 10,
                                    lineHeight: 1.2,
                                    border: "1px solid #cbd5e1",
                                    borderRadius: 4,
                                    background: "#fff",
                                    color: copied ? "#16a34a" : "#64748b",
                                    cursor: "pointer",
                                    whiteSpace: "nowrap",
                                    opacity: copied ? 1 : undefined,
                                  }}
                                >
                                  {copied ? "已复制" : "复制"}
                                </button>
                              );
                            })()}
                          </div>
                          <div style={{ ...s.rawDescBox, maxWidth: detailMaxWidthEffective }}>{detail.raw_description || "（空）"}</div>
                        </div>
                        <div className="pet-detail-section" style={s.detailSection}>
                          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                            <span
                              style={s.detailLabel}
                              title={`detail.md 路径：memories/${detail.detail_path} —— 由宠物 memory_edit 时写入，记录任务进度。`}
                            >
                              进度笔记 (detail.md)
                            </span>
                            {detail.detail_md_io_error && (
                              <span
                                style={{ fontSize: 10, color: "var(--pet-tint-orange-fg)", fontWeight: 600 }}
                                title={`读 memories/${detail.detail_path} 失败（权限 / corrupt 等）。文件不存在不会触发；这里说明真的有 IO 错误。`}
                              >
                                ⚠ 读失败
                              </span>
                            )}
                            {editingDetailTitle !== t.title && detail.detail_md.trim() && (
                              <button
                                type="button"
                                onClick={() =>
                                  setDetailMdRenderMode((m) =>
                                    m === "rendered" ? "source" : "rendered",
                                  )
                                }
                                title={
                                  detailMdRenderMode === "rendered"
                                    ? "切到源码：看 raw markdown 字面（含 ** _ - 等标记）"
                                    : "切到渲染：把 markdown 视觉化（粗体 / 列表 / inline code）"
                                }
                                style={{
                                  padding: "2px 6px",
                                  fontSize: 10,
                                  lineHeight: 1.2,
                                  border: "1px solid #cbd5e1",
                                  borderRadius: 4,
                                  background: "#fff",
                                  color: "#475569",
                                  cursor: "pointer",
                                  whiteSpace: "nowrap",
                                }}
                              >
                                {detailMdRenderMode === "rendered" ? "🅼 源码" : "🅼 渲染"}
                              </button>
                            )}
                            {editingDetailTitle !== t.title && detail.detail_md.trim() && (() => {
                              const k = `${t.title}-detailMd`;
                              const copied = copiedDetailKey === k;
                              return (
                                <button
                                  type="button"
                                  className="pet-detail-copy-btn"
                                  onClick={() => handleCopyDetail(k, detail.detail_md)}
                                  title={copied ? "已复制" : "复制进度笔记"}
                                  style={{
                                    padding: "2px 6px",
                                    fontSize: 10,
                                    lineHeight: 1.2,
                                    border: "1px solid #cbd5e1",
                                    borderRadius: 4,
                                    background: "#fff",
                                    color: copied ? "#16a34a" : "#64748b",
                                    cursor: "pointer",
                                    whiteSpace: "nowrap",
                                    opacity: copied ? 1 : undefined,
                                  }}
                                >
                                  {copied ? "已复制" : "复制"}
                                </button>
                              );
                            })()}
                            {editingDetailTitle !== t.title && (
                              <button
                                style={{
                                  fontSize: 10,
                                  padding: "2px 8px",
                                  border: "1px solid #cbd5e1",
                                  borderRadius: 4,
                                  background: "#fff",
                                  color: "#475569",
                                  cursor: "pointer",
                                }}
                                onClick={() => handleEnterEditDetail(t.title, detail.detail_md)}
                                title="编辑 detail.md（保存后覆盖文件；下次 LLM 也会读到你的版本）"
                              >
                                编辑
                              </button>
                            )}
                          </div>
                          {editingDetailTitle === t.title ? (
                            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                              {/* R117: edit / preview 切换 toggle。preview 用
                                  既有 parseMarkdown render；textarea state 共享，
                                  切换不丢未保存内容。
                                  R121: row 末附字数 counter（无上限阈值，纯信息）。 */}
                              <div style={{ display: "flex", gap: 4, alignItems: "center" }}>
                                {(["edit", "preview"] as const).map((mode) => {
                                  const active =
                                    mode === "edit"
                                      ? !detailPreviewMode
                                      : detailPreviewMode;
                                  return (
                                    <button
                                      key={mode}
                                      type="button"
                                      onClick={() =>
                                        setDetailPreviewMode(mode === "preview")
                                      }
                                      style={{
                                        fontSize: 11,
                                        padding: "2px 8px",
                                        border: "1px solid",
                                        borderColor: active ? "#0ea5e9" : "#e2e8f0",
                                        borderRadius: 4,
                                        background: active ? "#0ea5e9" : "#fff",
                                        color: active ? "#fff" : "#475569",
                                        cursor: active ? "default" : "pointer",
                                        fontWeight: active ? 600 : 400,
                                      }}
                                    >
                                      {mode === "edit" ? "✏️ 编辑" : "👁 预览"}
                                    </button>
                                  );
                                })}
                                {/* R141: dirty marker — content !== original 时
                                    显 "● 未保存"；marginLeft: auto 在字数 counter
                                    上，dirty marker 紧贴字数左侧（gap 4 分隔）。 */}
                                {editingDetailContent !==
                                  editingDetailOriginalRef.current && (
                                  <span
                                    style={{
                                      marginLeft: "auto",
                                      fontSize: 10,
                                      color: "var(--pet-color-muted)",
                                      fontFamily: "'SF Mono', 'Menlo', monospace",
                                    }}
                                    title="textarea 内容已改但未保存（⌘S 保存 / Esc 取消触发 dirty 二次确认）"
                                  >
                                    ● 未保存
                                  </span>
                                )}
                                <span
                                  style={{
                                    marginLeft:
                                      editingDetailContent !==
                                      editingDetailOriginalRef.current
                                        ? undefined
                                        : "auto",
                                    fontSize: 10,
                                    color: "var(--pet-color-muted)",
                                    fontFamily: "'SF Mono', 'Menlo', monospace",
                                  }}
                                  title="当前笔记字符数（Unicode code units 计；含换行 / 空白）"
                                >
                                  {editingDetailContent.length} 字
                                </span>
                              </div>
                              {detailPreviewMode ? (
                                <div
                                  style={{
                                    minHeight: 100,
                                    padding: "8px 10px",
                                    fontSize: 12,
                                    lineHeight: 1.55,
                                    border: "1px dashed #cbd5e1",
                                    borderRadius: 4,
                                    boxSizing: "border-box",
                                    color: "#1e293b",
                                    background: "var(--pet-color-bg)",
                                  }}
                                >
                                  {editingDetailContent.trim() === "" ? (
                                    <span
                                      style={{
                                        color: "var(--pet-color-muted)",
                                        fontStyle: "italic",
                                      }}
                                    >
                                      （空 — 切回 ✏️ 编辑写笔记）
                                    </span>
                                  ) : (
                                    parseMarkdown(editingDetailContent)
                                  )}
                                </div>
                              ) : (
                              <textarea
                                value={editingDetailContent}
                                onChange={(e) => setEditingDetailContent(e.target.value)}
                                onKeyDown={(e) => {
                                  // ⌘S/Ctrl+S 触发保存：与按钮等价。preventDefault
                                  // 吃掉 webview 默认"另存为页面"行为；savingDetail
                                  // 守卫防止保存进行中重复发请求。
                                  if (
                                    (e.metaKey || e.ctrlKey) &&
                                    e.key.toLowerCase() === "s"
                                  ) {
                                    e.preventDefault();
                                    if (savingDetail) return;
                                    handleSaveDetail(t.title);
                                    return;
                                  }
                                  // R138: Esc 触发取消编辑。dirty 时由
                                  // handleCancelEditDetail 内部走 armed 二次
                                  // 确认（再 Esc 才真丢改动）。
                                  if (e.key === "Escape") {
                                    e.preventDefault();
                                    handleCancelEditDetail();
                                  }
                                }}
                                placeholder="在这里追加 / 修改进度笔记…保存后覆盖 detail.md。（⌘S 保存 / Esc 取消）"
                                style={{
                                  width: "100%",
                                  minHeight: 100,
                                  padding: "8px 10px",
                                  fontSize: 12,
                                  fontFamily: "'SF Mono', 'Menlo', monospace",
                                  border: "1px solid #cbd5e1",
                                  borderRadius: 4,
                                  resize: "vertical",
                                  boxSizing: "border-box",
                                  lineHeight: 1.55,
                                  color: "#1e293b",
                                }}
                                autoFocus
                                disabled={savingDetail}
                              />
                              )}
                              <div style={{ display: "flex", gap: 6, alignItems: "center" }}>
                                <button
                                  style={savingDetail ? s.actionBtnDisabled : s.actionBtnRetry}
                                  disabled={savingDetail}
                                  onClick={() => handleSaveDetail(t.title)}
                                  title="保存进度笔记到 detail.md（⌘S 等价）"
                                >
                                  {savingDetail ? "保存中..." : "保存"}
                                </button>
                                <button
                                  style={
                                    cancelEditArmed
                                      ? {
                                          ...s.actionBtn,
                                          background: "var(--pet-tint-orange-bg)",
                                          borderColor: "var(--pet-tint-orange-fg)",
                                          color: "var(--pet-tint-orange-fg)",
                                          fontWeight: 600,
                                        }
                                      : s.actionBtn
                                  }
                                  disabled={savingDetail}
                                  onClick={handleCancelEditDetail}
                                  title={
                                    cancelEditArmed
                                      ? "再次点击立即丢弃改动（3s 内有效）"
                                      : "取消编辑；如内容已改，需 3s 内再点确认丢弃"
                                  }
                                >
                                  {cancelEditArmed ? "确认丢弃 (3s 内)" : "取消"}
                                </button>
                                {editDetailErr && (
                                  <span style={{ fontSize: 11, color: "var(--pet-tint-orange-fg)" }}>{editDetailErr}</span>
                                )}
                                {/* 字数计数：Array.from 算 code point 数，CJK / emoji /
                                    ASCII 都按"1 字"计数，符合中文用户对"字数"的直觉。
                                    `marginLeft: auto` 推到行末与按钮分隔。
                                    超过 2000 字 → 琥珀色 + 提示考虑拆子任务（不阻拦保存）。 */}
                                {(() => {
                                  const charCount = Array.from(editingDetailContent).length;
                                  const overLong = charCount > 2000;
                                  return (
                                    <span
                                      style={{
                                        marginLeft: "auto",
                                        fontSize: 11,
                                        color: overLong ? "#b45309" : "#94a3b8",
                                        whiteSpace: "nowrap",
                                        fontFamily: "'SF Mono', 'Menlo', monospace",
                                      }}
                                      title={
                                        overLong
                                          ? "笔记已超 2000 字，考虑拆出子任务避免单段过长 / 难浏览（不阻拦保存）"
                                          : "当前进度笔记字数（按 Unicode code point 计）"
                                      }
                                    >
                                      {charCount} 字
                                    </span>
                                  );
                                })()}
                              </div>
                            </div>
                          ) : detail.detail_md.trim() ? (
                            <div style={{ ...s.detailMdBox, maxWidth: detailMaxWidthEffective }}>
                              {detailMdRenderMode === "rendered"
                                ? parseMarkdown(detail.detail_md)
                                : detail.detail_md}
                            </div>
                          ) : (
                            <div style={s.detailHint}>宠物还没写进度笔记</div>
                          )}
                        </div>
                        <div style={s.detailSection}>
                          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                            <span
                              style={s.detailLabel}
                              title="butler_history.log 里精确匹配本任务标题的事件，按时间倒序（最新在前）。"
                            >
                              事件时间线（共 {detail.history.length} 条）
                            </span>
                            {detail.history_io_error && (
                              <span
                                style={{ fontSize: 10, color: "var(--pet-tint-orange-fg)", fontWeight: 600 }}
                                title="读 butler_history.log 失败（权限 / corrupt 等）。文件不存在不会触发；这里说明真的有 IO 错误。"
                              >
                                ⚠ 读失败
                              </span>
                            )}
                            {detail.history.length > 0 && (() => {
                              const k = `${t.title}-historyMd`;
                              const copied = copiedDetailKey === k;
                              return (
                                <button
                                  type="button"
                                  className="pet-detail-copy-btn"
                                  onClick={() => {
                                    // 拼成 `- HH:MM action: snippet` 列表。ts 截前 16
                                    // 字 + 把 T 换成空格便于阅读；snippet 缺时省略 ":"
                                    // 让行不残留尾巴。
                                    const lines = detail.history.map((ev) => {
                                      const ts = ev.timestamp.slice(0, 16).replace("T", " ");
                                      const snippet = ev.snippet.trim();
                                      return snippet
                                        ? `- ${ts} ${ev.action}: ${snippet}`
                                        : `- ${ts} ${ev.action}`;
                                    });
                                    const md = `### 事件时间线 — ${t.title}\n\n${lines.join("\n")}`;
                                    handleCopyDetail(k, md);
                                  }}
                                  title={
                                    copied
                                      ? "已复制 markdown"
                                      : "导出时间线为 markdown：### 事件时间线 + 每行 `- HH:MM action: snippet`"
                                  }
                                  style={{
                                    padding: "2px 6px",
                                    fontSize: 10,
                                    lineHeight: 1.2,
                                    border: "1px solid #cbd5e1",
                                    borderRadius: 4,
                                    background: "#fff",
                                    color: copied ? "#16a34a" : "#64748b",
                                    cursor: "pointer",
                                    whiteSpace: "nowrap",
                                    opacity: copied ? 1 : undefined,
                                  }}
                                >
                                  {copied ? "已复制" : "导出 MD"}
                                </button>
                              );
                            })()}
                          </div>
                          {detail.history.length > 0 ? (
                            <div style={s.historyList}>
                              {(() => {
                                // R109: history > 8 条时默认显最新 5 条
                                // （时间序：oldest first，最新在尾，slice(-5)
                                // 取 5 条最新）。≤ 8 不折叠。
                                const HISTORY_FOLD_THRESHOLD = 8;
                                const HISTORY_FOLD_PREVIEW = 5;
                                const isLongHistory =
                                  detail.history.length > HISTORY_FOLD_THRESHOLD;
                                const historyExpanded = expandedHistoryTitles.has(
                                  t.title,
                                );
                                const displayedHistory =
                                  isLongHistory && !historyExpanded
                                    ? detail.history.slice(-HISTORY_FOLD_PREVIEW)
                                    : detail.history;
                                // prev = 上次展开本任务时记录的"已读截止"。RFC3339
                                // lex 序 = 时间序（同 chrono::Local 来源）。首次展开
                                // prev=null → 全部视为新，符合"初次看见"语义。
                                const prev = lastViewRef.current.get(t.title) ?? null;
                                return (
                                  <>
                                    {isLongHistory && (
                                      <button
                                        type="button"
                                        onClick={() =>
                                          setExpandedHistoryTitles((p) => {
                                            const next = new Set(p);
                                            if (next.has(t.title)) next.delete(t.title);
                                            else next.add(t.title);
                                            return next;
                                          })
                                        }
                                        title={
                                          historyExpanded
                                            ? `折叠回最新 ${HISTORY_FOLD_PREVIEW} 条`
                                            : `展开后显示全部 ${detail.history.length} 条`
                                        }
                                        style={{
                                          marginBottom: 4,
                                          fontSize: 11,
                                          padding: 0,
                                          border: "none",
                                          background: "transparent",
                                          color: "var(--pet-color-accent)",
                                          cursor: "pointer",
                                          fontFamily: "inherit",
                                          alignSelf: "flex-start",
                                        }}
                                      >
                                        {historyExpanded
                                          ? `收起 (${detail.history.length})`
                                          : `… 展开更早 ${detail.history.length - HISTORY_FOLD_PREVIEW} 条`}
                                      </button>
                                    )}
                                    {displayedHistory.map((ev) => {
                                      const isNew = tsAfter(ev.timestamp, prev);
                                      return (
                                        <div
                                          key={`${ev.timestamp}-${ev.action}`}
                                          style={s.historyItem}
                                        >
                                          <span style={s.historyTs}>
                                            {ev.timestamp.slice(0, 16).replace("T", " ")}
                                          </span>
                                          {isNew && (
                                            <span
                                              title="距上次展开本任务详情后的新事件"
                                              style={{
                                                fontSize: 9,
                                                color: "var(--pet-tint-orange-fg)",
                                                fontWeight: 600,
                                                flexShrink: 0,
                                              }}
                                            >
                                              🆕
                                            </span>
                                          )}
                                          <span style={s.historyAction(ev.action)}>
                                            {actionIcon(ev.action)} {ev.action}
                                          </span>
                                          <span style={{ wordBreak: "break-word" }}>
                                            {ev.snippet || "（无描述）"}
                                          </span>
                                        </div>
                                      );
                                    })}
                                  </>
                                );
                              })()}
                            </div>
                          ) : (
                            <div style={s.detailHint}>
                              还没记录事件（butler_history 默认 cap 100 条，老任务可能已被轮转切掉）
                            </div>
                          )}
                        </div>
                      </>
                    )}
                  </div>
                )}
              </div>
            );
            return (
              <Fragment key={`${t.title}-${t.created_at}`}>
                {showBucketHeader && curBucket && (
                  <div style={s.bucketHeader}>
                    <span>{BUCKET_LABELS[curBucket]}</span>
                    <span style={s.bucketCount}>{bucketCounts[curBucket]}</span>
                  </div>
                )}
                {taskCard}
              </Fragment>
            );
          })
        )}
      </div>
    </div>
  );
}
