import { Fragment, useState, useEffect, useCallback, useMemo, useRef } from "react";
import type { ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import { parseMarkdown } from "../../utils/inlineMarkdown";
import { ImageLightbox } from "../common/ImageLightbox";
import { ImageThumb } from "../common/ImageThumb";
import { useTaskKeyboardNav } from "./useTaskKeyboardNav";
import { EmptyState } from "./EmptyState";
import { LoadingState } from "./LoadingState";
import { Modal } from "./Modal";

/** 与后端 `task_queue::TaskView` 一一对应。`status` 四态由后端判定，前端
 * 仅渲染。`due` 是无时区 ISO（`YYYY-MM-DDThh:mm`），与 datetime-local
 * input 的 value 直接对称，避免前端做 Date 转换。*/
type TaskStatus = "pending" | "done" | "error" | "cancelled";

interface TaskView {
  title: string;
  body: string;
  /** 原始 description 完整体（含 [done] / [error: ...] / [origin:...] /
   * [result:...] / #tag 等所有 marker）。给前端 hover tooltip 用，让用户不
   * 展开详情就能看到 LLM 加的状态标记。 */
  raw_description: string;
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
  /** detail.md 相对路径（memories_dir 下）。给 hover preview 用，直接调
   * memory_read_detail 即可。 */
  detail_path?: string;
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

/// PanelTasks 创建表单的"📋 从模板"预填项。每条 = 一个 one-shot 任务范
/// 例，引导用户写出宠物易执行的形态（明确动作 + 明确产物 + 明确范围）。
/// label 是 dropdown 显示文案，title / body 是 prefill 值。priority 默认
/// 全 3（无信号偏置）；due 全空（用户决定）。新增 / 删模板就在这里改。
const TASK_TEMPLATES: Array<{ label: string; title: string; body: string }> = [
  {
    label: "📁 整理 Downloads",
    title: "整理 Downloads",
    body: "把 ~/Downloads 里 30 天前的文件挪到 ~/Archive/，按月份分子目录。\n做完在 detail 写一句「已挪 N 个文件」，列出最大的 3 个文件名。",
  },
  {
    label: "📝 总结一段文档",
    title: "总结：[文档名]",
    body: "把 [path/to/doc] 的核心要点提炼成 3-5 条 bullet。\n直接写到 detail.md 里，每条 ≤ 30 字。",
  },
  {
    label: "🔎 调研某主题",
    title: "调研：[主题]",
    body: "搜 [关键词]，整合 5 条最相关的资料链接 + 各一句话摘要。\n写到 detail.md，按相关度排序。",
  },
  {
    label: "🌐 翻译一段文字",
    title: "翻译",
    body: "把以下文字翻成中文 / 英文，保留 markdown 格式：\n\n[粘贴原文]",
  },
];

/** 状态徽章配色。cancelled 用灰色（结束态、不再有动作），与 done 的绿色
 * 区分开 — 用户一眼能区分"完成"与"取消"。全部走 theme tint var 让深色主题
 * 自动跟随（旧 hardcoded `#e0f2fe` / `#dcfce7` 在 dark 下太亮反差刺眼）。 */
const STATUS_BADGE: Record<TaskStatus, { label: string; bg: string; fg: string }> = {
  pending: { label: "待办", bg: "var(--pet-tint-blue-bg)", fg: "var(--pet-tint-blue-fg)" },
  error: { label: "失败", bg: "var(--pet-tint-orange-bg)", fg: "var(--pet-tint-orange-fg)" },
  done: { label: "已完成", bg: "var(--pet-tint-green-bg)", fg: "var(--pet-tint-green-fg)" },
  cancelled: { label: "已取消", bg: "var(--pet-color-bg)", fg: "var(--pet-color-muted)" },
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
      return "var(--pet-tint-red-fg)";
    case "soon":
      return "var(--pet-tint-yellow-fg)";
    case "normal":
      return undefined; // 走父级 itemMeta 默认色
  }
}

/** 紧迫度对应的背景色 token —— normal 不挂背景（保持平面），overdue / soon
 * 走 tint-bg 让一行 due 直接是个色块 chip，扫长队列时秒判断。 */
function dueBg(urgency: DueUrgency): string | undefined {
  switch (urgency) {
    case "overdue":
      return "var(--pet-tint-red-bg)";
    case "soon":
      return "var(--pet-tint-yellow-bg)";
    case "normal":
      return undefined;
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

/** 判定 markdown image 语法里的 url 是否真的是图片：data:image/... 或 http(s)
 * 后缀 png/jpg/jpeg/gif/webp/svg/bmp。非图链接 → 当普通 markdown 渲（避免误把
 * `![logo](https://x.com/some-page)` 这种文档链接渲成 broken img）。 */
function isImageUrl(url: string): boolean {
  if (url.startsWith("data:image/")) return true;
  return /^https?:\/\/.+\.(png|jpe?g|gif|webp|svg|bmp)(\?|#|$)/i.test(url);
}

/** 解析 detail.md：把 markdown image 语法 `![alt](url)` 切出来用 ImageThumb 渲，
 * 其它文本段交给现有 parseMarkdown。让任务详情里贴的截图直接可见 + 可点开 +
 * 可复制，不用切到 markdown 编辑器。
 *
 * 不识别带 title 的形式 `![alt](url "title")` —— 大模型 / 用户实际写的几乎全是
 * 朴素双段，复杂语法后续再扩。 */
function parseDetailMdWithImages(
  md: string,
  onOpenImage: (src: string) => void,
): ReactNode[] {
  const out: ReactNode[] = [];
  // url 部分用 [^)\s] 限制不含右括号 / 空白，避免吃过界
  const re = /!\[([^\]]*)\]\(([^)\s]+)\)/g;
  let lastIdx = 0;
  let imgKey = 0;
  let m: RegExpExecArray | null;
  while ((m = re.exec(md)) !== null) {
    if (m.index > lastIdx) {
      out.push(
        <Fragment key={`txt-${m.index}`}>
          {parseMarkdown(md.slice(lastIdx, m.index))}
        </Fragment>,
      );
    }
    const url = m[2];
    if (isImageUrl(url)) {
      out.push(
        <div key={`img-${imgKey++}`} style={{ margin: "6px 0" }}>
          <ImageThumb src={url} onOpen={() => onOpenImage(url)} />
        </div>,
      );
    } else {
      // 非图链接：原样放回，交给 parseMarkdown 当文本处理（保持 markdown
      // 字面，不乱拆）。
      out.push(
        <Fragment key={`raw-${m.index}`}>{parseMarkdown(m[0])}</Fragment>,
      );
    }
    lastIdx = m.index + m[0].length;
  }
  if (lastIdx < md.length) {
    out.push(
      <Fragment key={`txt-tail`}>{parseMarkdown(md.slice(lastIdx))}</Fragment>,
    );
  }
  return out;
}

/** 任务面板搜索结果高亮：把 query 子串在 text 里第一次出现位置用 `<mark>`
 * 包起来。空 query / 未命中时原样输出。配色与 PanelChat SearchResultRow /
 * PanelSettings HighlightedText 一致（黄底深棕字），让"panel 内搜索高亮"风格
 * 统一。 */
const HIGHLIGHT_MARK_STYLE: React.CSSProperties = {
  background: "var(--pet-tint-yellow-bg)",
  color: "var(--pet-tint-yellow-fg)",
  padding: "0 1px",
  borderRadius: 2,
};
/** 任务面板时态切片 chip。逾期红 / 今日到期橙 / 今日创建蓝；active 时填充 +
 * 深色字。互斥由父级 dueFilter state 保证 —— 同一时刻只有一种被高亮。 */
type DueChipKind = "today" | "overdue" | "createdToday";
function DueChip({
  kind,
  count,
  active,
  onToggle,
}: {
  kind: DueChipKind;
  count: number;
  active: boolean;
  onToggle: () => void;
}) {
  // 三档 chip palette：base / active / border 三层用 tint var 的不同 alpha
  // 配出层次。bgActive 30% / border 20% 是经验比例，与原 hardcoded 视觉接近。
  const palette =
    kind === "overdue"
      ? {
          bg: "var(--pet-tint-red-bg)",
          bgActive: "color-mix(in srgb, var(--pet-tint-red-fg) 30%, var(--pet-tint-red-bg))",
          fg: "var(--pet-tint-red-fg)",
          border: "color-mix(in srgb, var(--pet-tint-red-fg) 40%, transparent)",
          borderActive: "var(--pet-tint-red-fg)",
        }
      : kind === "today"
        ? {
            bg: "var(--pet-tint-orange-bg)",
            bgActive: "color-mix(in srgb, var(--pet-tint-orange-fg) 30%, var(--pet-tint-orange-bg))",
            fg: "var(--pet-tint-orange-fg)",
            border: "color-mix(in srgb, var(--pet-tint-orange-fg) 40%, transparent)",
            borderActive: "var(--pet-tint-orange-fg)",
          }
        : {
            bg: "var(--pet-tint-blue-bg)",
            bgActive: "color-mix(in srgb, var(--pet-tint-blue-fg) 30%, var(--pet-tint-blue-bg))",
            fg: "var(--pet-tint-blue-fg)",
            border: "color-mix(in srgb, var(--pet-tint-blue-fg) 40%, transparent)",
            borderActive: "var(--pet-tint-blue-fg)",
          };
  const labelText =
    kind === "overdue"
      ? "🔴 逾期"
      : kind === "today"
        ? "📅 今日到期"
        : "🆕 今日创建";
  const tooltip = active
    ? `再次点击关闭「${labelText.slice(2)}」过滤，恢复显示其它任务`
    : kind === "overdue"
      ? "只看 due 已过 & 未结束的任务"
      : kind === "today"
        ? "只看 due 在今天 & 未结束的任务"
        : "只看今天本地日期内创建的任务（不分状态）";
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
      title={tooltip}
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
      {labelText}
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

/** detail.md 编辑器 markdown 工具栏按钮样式。轻边框 + 紧凑 padding，不抢
 * textarea 的视觉重点；hover 由全局 button 规则提升 shadow（迭代 1）。 */
const mdToolbarBtnStyle: React.CSSProperties = {
  padding: "3px 8px",
  fontSize: 12,
  border: "1px solid var(--pet-color-border)",
  borderRadius: 4,
  background: "var(--pet-color-card)",
  color: "var(--pet-color-fg)",
  cursor: "pointer",
  lineHeight: 1.2,
  fontFamily: "inherit",
  minWidth: 28,
};

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

interface PanelTasksProps {
  /// PanelChat 双击 ref 后 PanelApp 把目标 title 推到这里。本组件挂载后
  /// useEffect 消费 → findIndex visibleTasks 命中即 setFocusedIdx +
  /// 既有 scrollIntoView effect 跟进。消费完调 onConsumeFocus 清空，避免
  /// 用户后续操作（filter / sort）重新触发滚动。命中失败（已归档 / 重命名）
  /// 也 consume，仅在 actionErr 提示一行让用户知情。
  pendingFocusTitle?: string | null;
  onConsumeFocus?: () => void;
}

export function PanelTasks({ pendingFocusTitle, onConsumeFocus }: PanelTasksProps = {}) {
  const [tasks, setTasks] = useState<TaskView[]>([]);
  const [loading, setLoading] = useState(true);
  const [showFinished, setShowFinished] = useState(false);
  // 归档查看：default 折叠；点开 + lazy fetch task_archive 类目下的条目。
  // 归档是只读视图（不展示 checkbox / action 按钮），用户回看老完成 / 取消
  // 任务用。fetch 一次后保留在内存，再次展开不重 fetch（避免来回开关闪烁）；
  // 用户主动「刷新」按钮强制重拉。
  const [archiveExpanded, setArchiveExpanded] = useState(false);
  /// 顶部"队列 / 归档"tab 切换。default queue 保留原 UX。session 内 toggle，
  /// 不持久化（用户多数情况在队列；归档是偶发回看）。切到 archive 时顺手
  /// reloadArchive 避免空数据。
  const [taskViewTab, setTaskViewTab] = useState<"queue" | "archive">("queue");
  const [archiveLoaded, setArchiveLoaded] = useState(false);
  const [archiveLoading, setArchiveLoading] = useState(false);
  const [archiveItems, setArchiveItems] = useState<{
    title: string;
    description: string;
    updated_at: string;
  }[]>([]);
  const [archiveError, setArchiveError] = useState("");
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
  // 时态轴快捷过滤：四态 enum 互斥，避免多 boolean 相互矛盾。
  // - today / overdue：按 due 时间筛
  // - createdToday：按创建时间筛（不分状态，看"今天派/接了什么单"）
  // 与 sortMode 解耦（开任一切片仍可选 queue 排序）。
  const [dueFilter, setDueFilter] = useState<
    "all" | "today" | "overdue" | "createdToday"
  >("all");
  // R104: priority 多选过滤。Set<number> 空 = "全部"；非空 = 任一命中即通过
  // （OR 语义）。与 R83 决策日志 / R39 工具风险等多选 chip 模式一致。P0 仍保
  // 留 "💡 idea 抽屉"语义在 chip glyph 上，老用户直觉不丢。
  const [priorityFilter, setPriorityFilter] = useState<Set<number>>(new Set());
  /// origin 入口过滤。两条线分明：TG（raw_description 含 `[origin:tg:`）vs
  /// 面板（不含）。Set 空 = 不过滤。后端 origin 模型若后续扩充（如 [origin:pet]
  /// / [origin:panel]）这里仍只匹配 TG marker；新 origin 进来时扩 chip 即可。
  const [originFilter, setOriginFilter] = useState<Set<"tg" | "panel">>(new Set());
  const taskHasTgOrigin = (t: TaskView): boolean =>
    t.raw_description.includes("[origin:tg:");

  // 创建表单
  const [title, setTitle] = useState("");
  const [body, setBody] = useState("");
  const [priority, setPriority] = useState(3);
  const [due, setDue] = useState(""); // datetime-local 原始值，可空
  const [creating, setCreating] = useState(false);
  const [errMsg, setErrMsg] = useState("");
  // 新建表单展开态：跨 session 记忆，default 展开（兼容既有 UX）。用户
  // 折叠后偏好持久；下次打开 panel 仍折叠 → 节省垂直空间。
  // ⌘N quick-add 全屏遮罩模态：与 inline 表单共享同一份 title / body / 等
  // state（用户敲到一半切换形态不丢）。仅 open 标志独立，handleCreate 成
  // 功后顺手设 false。
  const [quickAddOpen, setQuickAddOpen] = useState(false);
  /// 任务卡 hover 500ms 后浮 detail.md + 最近 3 条 history 预览。
  /// 缓存复用 `detailMap`（同 expand 视图同源）—— hover 后用户再点 expand
  /// 不重复 fetch；反之 expand 过的任务再 hover 即时还原。任务被改名 / 删
  /// 除后 dangling 不要紧（key 命中时显当时数据；用户重 hover / reload
  /// 触发 refetch）。
  const [taskPreviewHoverTitle, setTaskPreviewHoverTitle] = useState<string | null>(null);
  const taskPreviewTimerRef = useRef<number | null>(null);
  const startTaskPreviewHover = (title: string, detailPath: string | undefined) => {
    if (!detailPath) return;
    if (taskPreviewTimerRef.current !== null) {
      window.clearTimeout(taskPreviewTimerRef.current);
    }
    taskPreviewTimerRef.current = window.setTimeout(() => {
      setTaskPreviewHoverTitle(title);
      taskPreviewTimerRef.current = null;
      // 已缓存则跳过 fetch（与 expand 路径同源）
      if (detailMap[title]) return;
      invoke<TaskDetail>("task_get_detail", { title })
        .then((detail) => {
          setDetailMap((prev) => ({ ...prev, [title]: detail }));
        })
        .catch(() => {
          // hover 读失败 silently 忽略（tooltip 不渲染）—— 用户能继续
          // 点 expand 拿到详细错误。避免 hover 阶段闪 toast 打扰流。
        });
    }, 500);
  };
  const endTaskPreviewHover = () => {
    if (taskPreviewTimerRef.current !== null) {
      window.clearTimeout(taskPreviewTimerRef.current);
      taskPreviewTimerRef.current = null;
    }
    setTaskPreviewHoverTitle(null);
  };
  useEffect(() => {
    return () => {
      if (taskPreviewTimerRef.current !== null) {
        window.clearTimeout(taskPreviewTimerRef.current);
      }
    };
  }, []);
  /// "⚡ NOW" 标记：60s 内 task 浮顶 + 桌面气泡 nudge。session 内有效（mark
  /// 是即时反应不该跨重启）。timer ref 让多个 mark 各自独立 60s 不互相打乱。
  const [nowMarkedTitles, setNowMarkedTitles] = useState<Set<string>>(new Set());
  const nowTimersRef = useRef<Map<string, number>>(new Map());
  // 并行维护 markedAt 时间戳让 hover preview 能显倒计时秒数；用 ref 而非
  // state，避免每秒重渲整面板（tooltip 只在 hover 时读一次即可）。
  const nowMarkedAtRef = useRef<Map<string, number>>(new Map());
  const markTaskNow = useCallback((title: string) => {
    setNowMarkedTitles((prev) => {
      const next = new Set(prev);
      next.add(title);
      return next;
    });
    nowMarkedAtRef.current.set(title, Date.now());
    // 跨窗口通知 pet 桌面：发即时确认 + 60s 后再提醒一次（pet 端各自
    // schedule，避免 panel 关掉后丢消息）
    void emit("task-now-mark", { title }).catch(() => {
      // 事件总线失败不影响 panel 端 mark；仅 pet nudge 丢
    });
    // 清掉同 title 已有 timer（连续点同一条会让 timer 重置成新 60s）
    const existing = nowTimersRef.current.get(title);
    if (existing !== undefined) {
      window.clearTimeout(existing);
    }
    const id = window.setTimeout(() => {
      setNowMarkedTitles((prev) => {
        if (!prev.has(title)) return prev;
        const next = new Set(prev);
        next.delete(title);
        return next;
      });
      nowTimersRef.current.delete(title);
      nowMarkedAtRef.current.delete(title);
    }, 60_000);
    nowTimersRef.current.set(title, id);
  }, []);
  // 组件 unmount 时清掉所有 pending timer 防内存泄漏。
  useEffect(() => {
    return () => {
      for (const id of nowTimersRef.current.values()) {
        window.clearTimeout(id);
      }
      nowTimersRef.current.clear();
    };
  }, []);
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
  /// "取消原因"历史 datalist：用户取消任务时常用的几条原因（"已不需要" /
  /// "转给人工" / "时间过了" 等）会重复出现；记录最近 5 条，下次单条 /
  /// 批量取消时 native datalist 自动浮自动完成。与 iter #201 PanelMemory
  /// search history 同模式。仅在 handleCancelConfirm / handleBulkCancelConfirm
  /// 成功后写入 —— 误打开 cancel input 又关掉的原因不污染。
  const [cancelReasonHistory, setCancelReasonHistory] = useState<string[]>(() => {
    try {
      const raw = window.localStorage.getItem("pet-tasks-cancel-reason-history");
      if (!raw) return [];
      const arr = JSON.parse(raw);
      if (Array.isArray(arr)) {
        return arr.filter((v): v is string => typeof v === "string").slice(0, 5);
      }
    } catch {
      // 解析失败 → 空 history
    }
    return [];
  });
  const pushCancelReasonHistory = (reason: string) => {
    const trimmed = reason.trim();
    if (!trimmed) return;
    setCancelReasonHistory((prev) => {
      const next = [trimmed, ...prev.filter((x) => x !== trimmed)].slice(0, 5);
      try {
        window.localStorage.setItem(
          "pet-tasks-cancel-reason-history",
          JSON.stringify(next),
        );
      } catch {
        // 私密 / quota 满 —— session 内仍生效
      }
      return next;
    });
  };

  // 单条任务的"展开详情"状态。同时只展开一条（accordion），避免长队列被详情挤
  // 到难以浏览。`detailMap` 是 lazy-fetched 缓存；reload 时清空（防止重试 / 取消
  // 后展示陈旧数据）。
  const [expandedTitle, setExpandedTitle] = useState<string | null>(null);
  /// 详情区"完整描述"段的展开状态：raw_description > 300 字时默认折叠到
  /// 前 300 字 + 末尾省略 + 展开按钮。set<title> 记哪些任务用户已经展开。
  /// 折叠 / 展开是阅读偏好，未跨 session 持久化 —— 用户每次重开 panel 自
  /// 然以"先看 chip + 摘要"状态进入，长 description 不会一打开就轰炸视觉。
  const [expandedRawDescTitles, setExpandedRawDescTitles] = useState<Set<string>>(
    new Set(),
  );
  const toggleRawDescExpand = (title: string) => {
    setExpandedRawDescTitles((prev) => {
      const next = new Set(prev);
      if (next.has(title)) next.delete(title);
      else next.add(title);
      return next;
    });
  };
  const [detailMap, setDetailMap] = useState<Record<string, TaskDetail>>({});
  const [detailLoadingTitle, setDetailLoadingTitle] = useState<string | null>(null);
  const [detailErr, setDetailErr] = useState("");

  // 批量操作状态。selected 按 title 索引（与单条 retry/cancel 走同一套语义，
  // 重名走"首条匹配"）。bulkAction 控制二级输入面板（cancel reason / new
  // priority）是否展开。bulkResultMsg 给执行后短暂展示"重试 5 条 / 跳过 1
  // 条非 error"，~5s 后清掉。
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [bulkBusy, setBulkBusy] = useState(false);
  const [bulkAction, setBulkAction] = useState<"cancel" | "priority" | "due" | "tags" | "done" | null>(null);
  const [bulkReason, setBulkReason] = useState("");
  const [bulkPriority, setBulkPriority] = useState(3);
  // "改优先级" sub-panel 内附加 checkbox：true 时同次也把 due 清空。让用户
  // 把"P9 紧急"老任务重排时不必两步（先清 due 再改 pri）。
  const [bulkPriorityClearDue, setBulkPriorityClearDue] = useState(false);
  const [bulkDue, setBulkDue] = useState(""); // datetime-local 字符串；空 = 清 due
  const [bulkTagOps, setBulkTagOps] = useState(""); // 例如 "+a -b +工作"
  const [bulkResultMsg, setBulkResultMsg] = useState("");
  // 批量标 done 的共享 result 输入。空 / 仅空白 → 等价键盘 d 路径仅追加
  // [done]（与单条 markDoneDialog 同语义）；非空 → 对每个选中任务追加同一
  // 段 [result: <text>] marker。任务粒度本就独立，共享 result 的语义是
  // "我对这批任务的总体结果一句话"。
  const [bulkDoneResult, setBulkDoneResult] = useState("");

  // 任务详情页 detail.md 编辑状态。同时只允许一条 detail 在编辑（与单 accordion
  // 展开风格一致）。切换 expanded 任务或保存成功后清空。
  const [editingDetailTitle, setEditingDetailTitle] = useState<string | null>(null);
  const [editingDetailContent, setEditingDetailContent] = useState("");
  // R117: detail.md 编辑器的视图模式。tri-state：
  //   - "edit"：单 textarea（默认）
  //   - "preview"：单 markdown 渲染
  //   - "split"：左 textarea / 右 preview 并排，适合宽 panel 边写边看
  // 同时只一个 task 处于 edit（editingDetailTitle 互斥保证）；切换不丢
  // 未保存内容（state 共享 editingDetailContent）。
  type DetailViewMode = "edit" | "split" | "preview";
  const [detailViewMode, setDetailViewMode] = useState<DetailViewMode>("edit");
  const [savingDetail, setSavingDetail] = useState(false);
  // 进度笔记浏览态的渲染模式：rendered 默认（更友好），source 偶尔查 raw
  // 时切。全局 toggle，不持久 — 与 PanelTasks 其它切换 state 同语义。
  // 不影响编辑模式（编辑永远是 raw）。
  const [detailMdRenderMode, setDetailMdRenderMode] = useState<"rendered" | "source">("rendered");
  // detail.md 内 markdown 图片点开时的 lightbox 大图。整个 panel 单实例 state
  // 即可，因同时只能放大一张。
  const [detailLightboxSrc, setDetailLightboxSrc] = useState<string | null>(null);
  // priority badge 行内 picker 的目标 task title。null = 关闭。同时只允许一个
  // picker 浮起 —— 多 popover 同屏分散注意力。
  const [priorityPickerTitle, setPriorityPickerTitle] = useState<string | null>(null);
  // status badge 行内 picker。与 priority 同模式但只在 pending 行可点（done /
  // cancelled 暂无回退路径；error 走既有"重试"按钮）。
  const [statusPickerTitle, setStatusPickerTitle] = useState<string | null>(null);
  // task title 双击 inline 改名：renamingTitle 是旧 title key；draft 是
  // 当前 textarea 值。同时只允许一条 task 处于改名（多 input 同屏分散注
  // 意力）。commit / cancel handler 在 reload 声明之后（见下方"任务行右
  // 键菜单"段后），那样能让 useCallback 直接拿 reload 引用。
  const [renamingTaskTitle, setRenamingTaskTitle] = useState<string | null>(null);
  const [renameTaskDraft, setRenameTaskDraft] = useState("");
  const [renamingTaskBusy, setRenamingTaskBusy] = useState(false);

  // 任务行右键菜单。把分散在 priority badge / status badge / 行尾按钮里的动作
  // （标 done / 重试 / 取消 / 改 priority / 复制 title / 展开详情 / 复制为 MD）
  // 在一处聚拢，让用户不必"扫整条行"找入口。x/y 是 viewport 坐标（position:
  // fixed），prioritySubmenu 控制嵌套面板（hover 'priority' 项展开）。
  /// tag 颜色自定义：localStorage `pet-tag-colors` -> Record<tagName, colorKey>。
  /// 右键 tag chip 弹小调色板，选中即写入 + 立即生效。colorKey 落在白名单
  /// （default + 5 个 tint key）；落老条目时即使白名单变了也只读不写。仅前
  /// 端偏好，不污染 task 描述里的 #tag 字面量。
  const TAG_COLOR_OPTIONS: Array<{ key: string; label: string; tint: string | null }> = [
    { key: "default", label: "默认", tint: null },
    { key: "blue", label: "蓝", tint: "blue" },
    { key: "green", label: "绿", tint: "green" },
    { key: "purple", label: "紫", tint: "purple" },
    { key: "orange", label: "橙", tint: "orange" },
    { key: "yellow", label: "黄", tint: "yellow" },
    { key: "red", label: "红", tint: "red" },
  ];
  const [tagColors, setTagColors] = useState<Record<string, string>>(() => {
    try {
      const raw = window.localStorage.getItem("pet-tag-colors");
      if (raw) {
        const obj = JSON.parse(raw);
        if (obj && typeof obj === "object") return obj as Record<string, string>;
      }
    } catch {
      // localStorage 不可用 / JSON 损坏 → 空 map
    }
    return {};
  });
  const setTagColor = (tag: string, colorKey: string) => {
    setTagColors((prev) => {
      const next = { ...prev };
      if (colorKey === "default") delete next[tag];
      else next[tag] = colorKey;
      try {
        window.localStorage.setItem("pet-tag-colors", JSON.stringify(next));
      } catch {
        // 私密浏览 / 配额满 — UI state 仍生效，下次启动丢
      }
      return next;
    });
  };
  /// 给指定 tag 算 chip 的 background / color 样式。default / 未配色 → 返
  /// 空对象（让 base style 接管）。tint 命名直接接到 --pet-tint-{X}-bg / -fg
  /// CSS var，主题切换时自动跟随。
  const getTagTintStyle = (tag: string): React.CSSProperties => {
    const key = tagColors[tag];
    if (!key || key === "default") return {};
    const opt = TAG_COLOR_OPTIONS.find((o) => o.key === key);
    if (!opt || !opt.tint) return {};
    return {
      background: `var(--pet-tint-${opt.tint}-bg)`,
      color: `var(--pet-tint-${opt.tint}-fg)`,
    };
  };
  /// tag 调色板浮窗：右键 tag chip 弹出。x/y 是 viewport 坐标。
  const [tagColorPicker, setTagColorPicker] = useState<
    | { tag: string; x: number; y: number }
    | null
  >(null);

  /// 拖拽改 priority：仅在 sortMode === "priority" 时启用。drag source 是
  /// 被拖的 task title；drop target 是当前 dragOver 的 task title（用于边缘
  /// 高亮）。onDrop 把 target.priority 写给 source（"我要和这条一样重"）。
  /// 不做"插入位置 → 计算新 P 值"那种连续排序 —— 离散 P0..P9 没有 in-between
  /// 空隙，丢回某条头就用那条的 P 值最直白。
  const [dragSourceTitle, setDragSourceTitle] = useState<string | null>(null);
  const [dragOverTitle, setDragOverTitle] = useState<string | null>(null);
  const [taskCtxMenu, setTaskCtxMenu] = useState<
    | {
        title: string;
        status: TaskStatus;
        priority: number;
        x: number;
        y: number;
        prioritySubmenu: boolean;
      }
    | null
  >(null);
  // 外部 click 关 picker：与 ChatMini 顶部 📋 弹框同模式。统一关四类 picker
  // （priority / status badge 行内 picker + 右键菜单 + tag 调色板）。
  useEffect(() => {
    if (!priorityPickerTitle && !statusPickerTitle && !taskCtxMenu && !tagColorPicker) return;
    const close = () => {
      setPriorityPickerTitle(null);
      setStatusPickerTitle(null);
      setTaskCtxMenu(null);
      setTagColorPicker(null);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        setPriorityPickerTitle(null);
        setStatusPickerTitle(null);
        setTaskCtxMenu(null);
        setTagColorPicker(null);
      }
    };
    window.addEventListener("mousedown", close);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", close);
      window.removeEventListener("keydown", onKey);
    };
  }, [priorityPickerTitle, statusPickerTitle, taskCtxMenu, tagColorPicker]);

  // ⌘N / Ctrl+N 打开 quick-add 模态。Tauri WKWebView 没原生"新窗口"
  // 默认行为可吃，preventDefault 兜底。Esc 关闭。input / textarea 内
  // focus 时也响应 —— 用户在搜索框里也想 ⌘N 直接开建任务，符合 IDE 直觉。
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && (e.key === "n" || e.key === "N")) {
        if (e.altKey || e.shiftKey) return;
        e.preventDefault();
        setQuickAddOpen(true);
        // 下一帧 focus title input（modal 渲染完才挂上）
        window.setTimeout(() => {
          titleInputRef.current?.focus();
          try {
            titleInputRef.current?.setSelectionRange(
              title.length,
              title.length,
            );
          } catch {
            // ignore selectionRange not supported
          }
        }, 0);
        return;
      }
      if (e.key === "Escape" && quickAddOpen) {
        // 输入框聚焦时也允许 Esc 关 modal（与既有内嵌表单 cancel 语义对齐）
        setQuickAddOpen(false);
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [quickAddOpen, title]);
  // detail.md 编辑器 textarea 引用：粘贴图片时往光标位置插 markdown image。
  // 单 task 编辑互斥（editingDetailTitle 是单值），所以单 ref 够用。
  const detailEditorRef = useRef<HTMLTextAreaElement>(null);

  /// markdown toolbar 通用 helper：在 textarea 当前 selection 上做 wrap /
  /// replace。`wrap` 模式：选区前后插 prefix/suffix（粗体 / 链接的内容包裹）；
  /// `line-prefix` 模式：每选中行行首插 prefix（列表 / 引用）。空选区时 wrap
  /// 退化到"插入 prefix+suffix + 光标置中"，方便用户继续敲。
  const insertMarkdownAtCursor = useCallback(
    (mode: "wrap" | "line-prefix", prefix: string, suffix: string) => {
      const ta = detailEditorRef.current;
      if (!ta) return;
      const start = ta.selectionStart ?? 0;
      const end = ta.selectionEnd ?? start;
      const value = ta.value;
      const selected = value.slice(start, end);
      let next: string;
      let cursorPos: number;
      if (mode === "wrap") {
        const inserted = prefix + selected + suffix;
        next = value.slice(0, start) + inserted + value.slice(end);
        cursorPos =
          selected.length === 0
            ? start + prefix.length
            : start + inserted.length;
      } else {
        // line-prefix：把选中段每一行行首加 prefix；空选时只对当前行
        const lineStart = value.lastIndexOf("\n", start - 1) + 1;
        const lineEnd = end === start ? value.indexOf("\n", end) : end;
        const sliceEnd = lineEnd === -1 ? value.length : lineEnd;
        const block = value.slice(lineStart, sliceEnd);
        const lines = block.length === 0 ? [""] : block.split("\n");
        const transformed = lines.map((l) => prefix + l).join("\n");
        next = value.slice(0, lineStart) + transformed + value.slice(sliceEnd);
        cursorPos = lineStart + transformed.length;
      }
      setEditingDetailContent(next);
      requestAnimationFrame(() => {
        const cur = detailEditorRef.current;
        if (!cur) return;
        cur.focus();
        cur.selectionStart = cur.selectionEnd = cursorPos;
      });
    },
    [],
  );

  /// 把一组 image blob 异步读为 data URL，统一拼成 markdown `![](data:...)` 行
  /// 插到当前 textarea 光标位置。一次性 Promise.all 后单次 setState，避免多个
  /// reader.onload 并发改 selectionStart 漂移。
  const insertImageBlobsIntoDetail = useCallback(async (blobs: Blob[]) => {
    if (blobs.length === 0) return;
    const ta = detailEditorRef.current;
    if (!ta) return;
    const dataUrls = await Promise.all(
      blobs.map(
        (blob) =>
          new Promise<string>((resolve, reject) => {
            const reader = new FileReader();
            reader.onload = () => {
              const url = reader.result;
              if (typeof url === "string") resolve(url);
              else reject(new Error("FileReader result is not a string"));
            };
            reader.onerror = () => reject(reader.error);
            reader.readAsDataURL(blob);
          }),
      ),
    );
    const start = ta.selectionStart ?? 0;
    const end = ta.selectionEnd ?? start;
    // 前后各加换行让 markdown 段落分隔清晰；同次粘贴的多图也各占一行。
    const insert =
      "\n" + dataUrls.map((u) => `![](${u})`).join("\n") + "\n";
    setEditingDetailContent((prev) => prev.slice(0, start) + insert + prev.slice(end));
    // setState 后 React 重渲，textarea value 重置；用 rAF 等下一帧再写光标位置
    // 与 focus，否则 selectionStart 设上去会被 React 渲染覆盖。
    requestAnimationFrame(() => {
      const cur = detailEditorRef.current;
      if (!cur) return;
      cur.selectionStart = cur.selectionEnd = start + insert.length;
      cur.focus();
    });
  }, []);
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

  /// task title 改名 commit / cancel：依赖 reload，放在 reload 声明之后。
  /// Enter / onBlur 进入 commit；空 / 同名走 noop 分支。失败把 actionErr 写
  /// 上让用户看到原因（如 "Title already exists" 重名拒绝）。
  const commitRenameTask = useCallback(async () => {
    const oldTitle = renamingTaskTitle;
    if (!oldTitle) return;
    const newTitle = renameTaskDraft.trim();
    if (!newTitle || newTitle === oldTitle) {
      setRenamingTaskTitle(null);
      setRenameTaskDraft("");
      return;
    }
    setRenamingTaskBusy(true);
    try {
      await invoke("memory_rename", {
        category: "butler_tasks",
        oldTitle,
        newTitle,
      });
      await reload();
      setRenamingTaskTitle(null);
      setRenameTaskDraft("");
    } catch (e) {
      setActionErr(`改名失败：${e}`);
    } finally {
      setRenamingTaskBusy(false);
    }
  }, [renamingTaskTitle, renameTaskDraft, reload]);
  const cancelRenameTask = useCallback(() => {
    setRenamingTaskTitle(null);
    setRenameTaskDraft("");
  }, []);

  // 导出归档 markdown 的 toast 状态（reuse 既有 bulkResultMsg 通道；4s 自清）。
  const handleExportArchiveAsMd = useCallback(async () => {
    if (archiveItems.length === 0) {
      setBulkResultMsg("归档为空，无可导出条目");
      window.setTimeout(() => setBulkResultMsg(""), 4000);
      return;
    }
    // 按 title 前缀 YYYY-MM-DD 解析日期 → 按 YYYY-MM 分组。无法解析的 fallback
    // 到"未归档日期"段（理论不会出现 —— archive 都是 consolidate 写的）。
    type Group = { ym: string; items: typeof archiveItems };
    const groups: Group[] = [];
    const groupMap = new Map<string, Group>();
    for (const it of archiveItems) {
      const dateMatch = it.title.match(/^(\d{4})-(\d{2})-(\d{2})_/);
      const ym = dateMatch ? `${dateMatch[1]}-${dateMatch[2]}` : "未归档日期";
      let g = groupMap.get(ym);
      if (!g) {
        g = { ym, items: [] };
        groupMap.set(ym, g);
        groups.push(g);
      }
      g.items.push(it);
    }
    // 月份倒序（最新月份在前；同月内已经 sort 过 updated_at desc）
    groups.sort((a, b) => b.ym.localeCompare(a.ym));
    const lines: string[] = [
      `# 任务归档 (${archiveItems.length} 条 · ${new Date().toLocaleString()})`,
      "",
    ];
    for (const g of groups) {
      lines.push(`## ${g.ym} (${g.items.length} 条)`);
      lines.push("");
      for (const it of g.items) {
        // 去掉 title 的 YYYY-MM-DD_ 前缀让正文更干净；保留完整 description
        // 作 sub-detail 让用户看到 [done] / [result: ...] / #tag。
        const cleanTitle = it.title.replace(/^\d{4}-\d{2}-\d{2}_/, "");
        const dateOnly = it.title.match(/^(\d{4}-\d{2}-\d{2})_/)?.[1] ?? "";
        lines.push(`- **${dateOnly}** ${cleanTitle}`);
        if (it.description.trim()) {
          lines.push(`  - ${it.description.trim()}`);
        }
      }
      lines.push("");
    }
    try {
      await navigator.clipboard.writeText(lines.join("\n"));
      setBulkResultMsg(`已导出 ${archiveItems.length} 条归档到剪贴板（按月份分组）`);
    } catch (e) {
      setBulkResultMsg(`导出失败：${e}`);
    }
    window.setTimeout(() => setBulkResultMsg(""), 4000);
  }, [archiveItems]);

  // 拉取 task_archive 类目下的条目。`memory_list` 返回 categories.task_archive.items
  // (title / description / updated_at)。失败时把错误信息显在 banner，不挡视图。
  const reloadArchive = useCallback(async () => {
    setArchiveLoading(true);
    setArchiveError("");
    try {
      const idx = await invoke<{
        categories: Record<string, { items: { title: string; description: string; updated_at: string }[] }>;
      }>("memory_list", { category: "task_archive" });
      const items = idx.categories?.task_archive?.items ?? [];
      // updated_at 字典序倒排：相同格式 RFC3339 字符串与时序一致。
      items.sort((a, b) => b.updated_at.localeCompare(a.updated_at));
      setArchiveItems(items);
      setArchiveLoaded(true);
    } catch (e) {
      setArchiveError(`加载归档失败：${e}`);
    } finally {
      setArchiveLoading(false);
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
        // 先 patch detail_md 让阅读态 UI 无 flicker；保存路径不动 history /
        // updated_at —— 那两项 task_save_detail 在后端会推进，下面 refetch
        // 把它们对齐。同时 hover preview 也读 detailMap → 刷完后 hover
        // tooltip 自然显新的 history 行（避免 R181 引入的"detail 内容更新
        // 但 history 仍旧"陈旧感）。
        setDetailMap((prev) => {
          const cur = prev[taskTitle];
          if (!cur) return prev;
          return { ...prev, [taskTitle]: { ...cur, detail_md: editingDetailContent } };
        });
        setEditingDetailTitle(null);
        setEditingDetailContent("");
        // 后台 refetch：失败容忍，保留 patch 后的状态而不是清空（清空会让
        // 阅读态闪空白）。命中即覆盖完整 TaskDetail（含新 history + 新
        // updated_at），保持 hover preview / 阅读态都看到最新视图。
        try {
          const fresh = await invoke<TaskDetail>("task_get_detail", {
            title: taskTitle,
          });
          setDetailMap((prev) => ({ ...prev, [taskTitle]: fresh }));
        } catch {
          // refetch 失败（如 task 同步被删）→ 保留 patch；下次 reload /
          // 用户重新 hover 时再尝试
        }
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

  /// "📋 从模板" 下拉选中后调用：把所选模板的 title/body 填入表单 state，
  /// priority 重置默认 3、due 清空。inline create form / quickAdd modal /
  /// empty-state 三处共用一份 handler。
  const applyTaskTemplate = (idx: number) => {
    const tpl = TASK_TEMPLATES[idx];
    if (!tpl) return;
    setTitle(tpl.title);
    setBody(tpl.body);
    setPriority(3);
    setDue("");
  };

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
      // 成功后顺手关闭 quick-add modal（如果开着）— 让用户立即看到队列
      // 更新；保留 PanelSettings 模式 / 折叠表单不变（同 state 不冲突）。
      setQuickAddOpen(false);
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

  // 键盘 d 直接走快路径：不弹 dialog，零摩擦标 done（与原 R94 行为一致）。
  // 鼠标点击路径走 openMarkDoneDialog → confirmMarkDone，能选填 result 摘要。
  const handleMarkDone = async (taskTitle: string, result?: string) => {
    setActionErr("");
    setBusyTitle(taskTitle);
    try {
      await invoke<void>("task_mark_done", {
        title: taskTitle,
        result: result ?? null,
      });
      await reload();
    } catch (e) {
      setActionErr(`标 done 失败：${e}`);
    } finally {
      setBusyTitle(null);
    }
  };
  /// 手动标 done dialog：用户从鼠标按钮触发时弹此 dialog 选填 result 摘要
  /// ([result: ...] marker，与 LLM 自动标 done 时形态一致）。空 result 等
  /// 同键盘 d 路径（仅 [done]）。markDoneTitle 单值 state —— 同时只一条
  /// 任务在确认 dialog 里。
  const [markDoneTitle, setMarkDoneTitle] = useState<string | null>(null);
  const [markDoneResult, setMarkDoneResult] = useState("");
  const openMarkDoneDialog = (taskTitle: string) => {
    setMarkDoneTitle(taskTitle);
    setMarkDoneResult("");
  };
  const closeMarkDoneDialog = () => {
    setMarkDoneTitle(null);
    setMarkDoneResult("");
  };
  const confirmMarkDone = async () => {
    if (!markDoneTitle) return;
    const title = markDoneTitle;
    const result = markDoneResult.trim();
    closeMarkDoneDialog();
    await handleMarkDone(title, result || undefined);
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

  /// 一键重试所有 error：不必先选 row，直接扫 tasks 全集找 status==="error"
  /// 顺序 invoke。与 runBulk 流程并行实现（不走 selected 路径），让按钮入口
  /// 与 bulk 工具栏分离 — 用户从 chip 区直接一击触发。
  const handleRetryAllErrors = useCallback(async () => {
    const errorTitles = tasks
      .filter((t) => t.status === "error")
      .map((t) => t.title);
    if (errorTitles.length === 0) return;
    setBulkBusy(true);
    setBulkResultMsg("");
    setActionErr("");
    let success = 0;
    let failed = 0;
    let lastErr = "";
    for (const title of errorTitles) {
      try {
        await invoke<void>("task_retry", { title });
        success += 1;
      } catch (e) {
        failed += 1;
        lastErr = `${e}`;
      }
    }
    const parts: string[] = [`重试 ${success} 条 ✓`];
    if (failed > 0) parts.push(`失败 ${failed} 条（${lastErr}）`);
    setBulkResultMsg(parts.join(" · "));
    window.setTimeout(() => setBulkResultMsg(""), 5000);
    setBulkBusy(false);
    await reload();
  }, [tasks, reload]);

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

  // 批量拼 ref 列表：把所有选中任务拼成 `「A」「B」「C」` 单行字符串，
  // 适合粘到 chat 一句话引用多条任务（每个 token 都触发 hover preview /
  // 双击导航）。`「」` 自带分隔语义，无需额外 separator —— 紧凑形态阅
  // 读 / 解析最稳。
  const handleBulkCopyAsRefs = useCallback(async () => {
    const titleSet = selected;
    const titles = tasks.filter((t) => titleSet.has(t.title)).map((t) => t.title);
    if (titles.length === 0) {
      setBulkResultMsg("无可复制任务（选中已被清掉）");
      window.setTimeout(() => setBulkResultMsg(""), 4000);
      return;
    }
    const refs = titles.map((t) => `「${t}」`).join("");
    try {
      await navigator.clipboard.writeText(refs);
      setBulkResultMsg(`已复制 ${titles.length} 条 ref 到剪贴板`);
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
    pushCancelReasonHistory(reason);
  }, [runBulk, bulkReason]);

  /// 批量标 done：对所有 pending / error 的选中任务追加 [done] +（可选）
  /// 共享 [result: <text>]。终态任务跳过（后端会拒，与单条 mark done 同
  /// 策略），由 runBulk 统计成 skipped。runBulk 末尾会 clearSelection +
  /// reload；同时这里把 sub-panel 关掉、清空输入。
  const handleBulkMarkDoneConfirm = useCallback(async () => {
    const result = bulkDoneResult.trim();
    const payload: string | null = result.length === 0 ? null : result;
    await runBulk(
      "标 done",
      (t) => t.status === "pending" || t.status === "error",
      "已结束",
      async (title) => {
        await invoke<void>("task_mark_done", { title, result: payload });
      },
    );
    setBulkAction(null);
    setBulkDoneResult("");
  }, [runBulk, bulkDoneResult]);

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

  /// 相对调整 priority：每条选中任务的当前 priority + delta，clamp 到合法区间。
  /// 与"绝对 set"互补 —— 批量"全部往上提一档 / 往下降一档"是常见需求，不必
  /// 算出"每条具体多少"。tasks 数组用 ref 捕获当前快照查 priority；title 找不
  /// 到 → 跳过（罕见 race，runBulk 计数为 skipped）。
  /// 单条任务行内 priority 改：picker 选定后直接 invoke + reload。失败把
  /// actionErr 写一下让用户看到原因；reload 拉新值让 UI 立即对齐。
  const handleInlineSetPriority = useCallback(
    async (title: string, priority: number) => {
      setPriorityPickerTitle(null);
      try {
        await invoke<void>("task_set_priority", { title, priority });
        await reload();
      } catch (e) {
        setActionErr(`改 priority 失败：${e}`);
      }
    },
    [reload],
  );

  /// 拖拽到 target 后被 onDrop 调：把 target 的 priority 写给 source。
  /// source / target 同 priority 时静默退出（避免无意义的 invoke + reload）。
  /// 失败 actionErr 透传，成功后 reload 让排序立即反映。
  const handleDragDropPriority = useCallback(
    async (sourceTitle: string, targetTitle: string) => {
      if (sourceTitle === targetTitle) return;
      const target = tasks.find((t) => t.title === targetTitle);
      const source = tasks.find((t) => t.title === sourceTitle);
      if (!target || !source) return;
      if (target.priority === source.priority) return;
      try {
        await invoke<void>("task_set_priority", {
          title: sourceTitle,
          priority: target.priority,
        });
        await reload();
      } catch (e) {
        setActionErr(`拖拽改 priority 失败：${e}`);
      }
    },
    [tasks, reload],
  );

  const handleBulkAdjustPriority = useCallback(
    async (delta: number) => {
      const snapshot = new Map(tasks.map((t) => [t.title, t.priority]));
      await runBulk(
        delta > 0 ? `priority +${delta}` : `priority ${delta}`,
        () => true,
        "无可调条目",
        async (title) => {
          const cur = snapshot.get(title);
          if (cur === undefined) {
            throw new Error("找不到原 priority（race）");
          }
          const next = Math.max(0, Math.min(PRIORITY_MAX, cur + delta));
          if (next === cur) {
            // 已经在边界，不再发请求避免 noop；runBulk 把它当 success（语义"不
            // 需要改"），不弹 skip。
            return;
          }
          await invoke<void>("task_set_priority", { title, priority: next });
        },
      );
    },
    [runBulk, tasks],
  );

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
      pushCancelReasonHistory(cancelReason);
      setCancellingTitle(null);
      setCancelReason("");
      await reload();
    } catch (e) {
      setActionErr(`取消失败：${e}`);
    } finally {
      setBusyTitle(null);
    }
  };

  // loading early return 故意**不**放这里 —— 下面还有许多 useMemo /
  // useTaskKeyboardNav / useEffect。提前 return 会让首次 loading=true
  // 时这些 hook 不跑、loading=false 时它们出现 → React 抛
  // "Rendered more hooks than during the previous render"。把 guard
  // 推到主 return 的 JSX 里就避开了 hook 调用次数随状态变化的问题。

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
      if (dueFilter === "createdToday") {
        // 按本地今日日期前缀对比 created_at（chrono::Local::now().naive_local()
        // 输出 `YYYY-MM-DDTHH:MM:SS.fff` 串）。不分 status —— 用户复盘当日新派
        // 单时也想看到已 done / cancelled 的"今天处理过的"。
        if (t.created_at.length < 10) return false;
        const y = nowDate.getFullYear();
        const m = String(nowDate.getMonth() + 1).padStart(2, "0");
        const d = String(nowDate.getDate()).padStart(2, "0");
        return t.created_at.slice(0, 10) === `${y}-${m}-${d}`;
      }
      // dueFilter === "overdue"：dueUrgency 内已自动剔除终态 + 解析失败
      return isOverdue(t.due, nowMs, t.status);
    })
    .filter((t) =>
      priorityFilter.size === 0 || priorityFilter.has(t.priority),
    )
    .filter((t) => {
      if (originFilter.size === 0) return true;
      const isTg = taskHasTgOrigin(t);
      return originFilter.has(isTg ? "tg" : "panel");
    })
    .filter((t) => {
      if (!trimmedSearch) return true;
      return (
        t.title.toLowerCase().includes(trimmedSearch) ||
        t.body.toLowerCase().includes(trimmedSearch)
      );
    })
    .filter((t) => {
      if (selectedTags.size === 0) return true;
      // 空串 "" 是 "无 tag" 的合成 sentinel —— 真实 tag 经 parseTag 后不
      // 会是空串。命中时让 tags 为空的任务通过；其它任务走原 some(any-of)
      // 逻辑。两条 OR：用户可同时选 "无 tag" + 真实 tag → 既看未分类，
      // 也看带 X tag 的（并集），与多选 tag 的 OR 语义一致。
      if (selectedTags.has("") && t.tags.length === 0) return true;
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
    let sorted: TaskView[];
    if (sortMode === "due") {
      sorted = unf.slice().sort((a, b) => {
        const ad = a.due ?? "";
        const bd = b.due ?? "";
        if (!ad && !bd) return 0;
        if (!ad) return 1;
        if (!bd) return -1;
        return ad < bd ? -1 : ad > bd ? 1 : 0;
      });
    } else if (sortMode === "priority") {
      // R107: 数值大 = 优先级高（与后端 task_queue::compare_for_queue 一致）。
      // JS sort stable —— 同 priority 保持原 queue 综合序，让"P3 内部"仍是
      // backend 推荐处理顺序。
      sorted = unf.slice().sort((a, b) => b.priority - a.priority);
    } else {
      sorted = unf;
    }
    // "⚡ NOW" 标记的任务永远浮顶（不论 sortMode）。同 mark 之间保留 sort
    // 内的相对序。60s 后 timer 自动清除 mark，自然回到原排序。
    if (nowMarkedTitles.size === 0) return sorted;
    const marked: TaskView[] = [];
    const rest: TaskView[] = [];
    for (const t of sorted) {
      if (nowMarkedTitles.has(t.title)) marked.push(t);
      else rest.push(t);
    }
    return [...marked, ...rest];
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

  /// 把当前 visibleTasks（应用了搜索 / tag / due / priority 过滤之后的列表）
  /// 全部导出为 markdown，不依赖 selected 选中态。复盘 / 周回顾用例：用户调
  /// "导出 MD" 是否包含 detail.md 进度笔记。localStorage 持久化，跨重启
  /// 记忆偏好。toggle on 时 export 走 N 次 task_get_detail（合并 detailMap
  /// 缓存，仅 fetch missing），可能耗时；off 时仅导 title/desc/meta 三段，
  /// 现行行为。
  const [exportIncludeDetail, setExportIncludeDetail] = useState<boolean>(() => {
    try {
      const raw = window.localStorage.getItem("pet-tasks-export-include-detail");
      return raw === "1";
    } catch {
      return false;
    }
  });
  const setExportIncludeDetailPersist = (v: boolean) => {
    setExportIncludeDetail(v);
    try {
      window.localStorage.setItem(
        "pet-tasks-export-include-detail",
        v ? "1" : "0",
      );
    } catch {
      // session 内仍生效
    }
  };
  /// 好过滤就一键导出当前视图。
  const handleExportAllVisibleAsMd = useCallback(async () => {
    if (visibleTasks.length === 0) {
      setBulkResultMsg("当前过滤下没有任务可导出");
      window.setTimeout(() => setBulkResultMsg(""), 4000);
      return;
    }
    const header = `# 任务导出（${visibleTasks.length} 条 · ${new Date().toLocaleString()}）\n\n`;
    // include detail 模式：对每条任务取 detailMap 缓存；缓存 miss 则 fetch。
    // 失败容忍 —— 单条失败不阻塞其它，缺失 detail 走原 formatTaskAsMarkdown
    // 默认（没 detail 段）。Promise.all 并发提速。
    let body: string;
    if (exportIncludeDetail) {
      setBulkResultMsg("正在拉 detail.md…");
      const enriched = await Promise.all(
        visibleTasks.map(async (t) => {
          const cached = detailMap[t.title];
          if (cached) return formatTaskAsMarkdown(t, cached);
          try {
            const fresh = await invoke<TaskDetail>("task_get_detail", {
              title: t.title,
            });
            return formatTaskAsMarkdown(t, fresh);
          } catch {
            return formatTaskAsMarkdown(t);
          }
        }),
      );
      body = enriched.join("\n\n");
    } else {
      body = visibleTasks.map((t) => formatTaskAsMarkdown(t)).join("\n\n");
    }
    try {
      await navigator.clipboard.writeText(header + body);
      setBulkResultMsg(
        exportIncludeDetail
          ? `已导出 ${visibleTasks.length} 条（含 detail）到剪贴板`
          : `已导出 ${visibleTasks.length} 条到剪贴板`,
      );
    } catch (e) {
      setBulkResultMsg(`导出失败：${e}`);
    }
    window.setTimeout(() => setBulkResultMsg(""), 4000);
  }, [visibleTasks, exportIncludeDetail, detailMap]);

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

  // tags 为空的任务总数，给 tag 过滤行的 "🚫 无 tag" chip 用。仅当
  // > 0 时浮 chip —— 全部任务都打了 tag 时这个 chip 是噪声。
  const untaggedCount = useMemo(
    () => tasks.filter((t) => t.tags.length === 0).length,
    [tasks],
  );

  // 「今日到期 / 逾期」计数：派生自 tasks 全集，不被搜索/tag/sort 链上的
  // 过滤影响，让用户即使在 selectedTags 模式里也能看到"今天总共有 N 条到
  // 期 / M 条逾期"，决定要不要切。计数为 0 时单个 chip 不渲染（避免无事
  // 可做时占视觉位置），两者皆 0 时整行也不渲染。
  const { dueTodayCount, overdueCount, createdTodayCount } = useMemo(() => {
    const now = new Date(nowMs);
    const y = now.getFullYear();
    const m = String(now.getMonth() + 1).padStart(2, "0");
    const d = String(now.getDate()).padStart(2, "0");
    const todayPrefix = `${y}-${m}-${d}`;
    let today = 0;
    let overdue = 0;
    let createdToday = 0;
    for (const t of tasks) {
      if (!isFinished(t.status) && isDueToday(t.due, now)) today += 1;
      if (isOverdue(t.due, nowMs, t.status)) overdue += 1;
      if (t.created_at.length >= 10 && t.created_at.slice(0, 10) === todayPrefix) {
        createdToday += 1;
      }
    }
    return { dueTodayCount: today, overdueCount: overdue, createdTodayCount: createdToday };
  }, [tasks, nowMs]);

  // R104: 各 priority 的活动任务计数。派生 tasks 全集（不受 link 上 search /
  // tag / due / sort 过滤影响），让用户在任一 filter 下都能看到"还有哪几档
  // priority 有事"。只数活动态：finished 不在 chip row，由 showFinished
  // 单独展示。priority asc 序让 chip 行从 P0 → P9 自然。
  /// origin 入口的活动任务计数：tg / panel 两段。tg = raw_description 含
  /// `[origin:tg:`；其余都算 panel（无 marker / 未来扩 [origin:panel] 都
  /// 落 panel 段）。只数活动态 —— 与 priorityCounts 同语义，让 chip 列表
  /// 反映"还需处理的入口分布"。
  /// error 状态 task 数（不分时间窗口）。> 0 时 chip 行末尾出"批量重试"
  /// 红 chip 让用户一键重试所有；= 0 时不渲染避免占位噪音。
  const errorTaskCount = useMemo(
    () => tasks.filter((t) => t.status === "error").length,
    [tasks],
  );
  // 已结束（done / cancelled）任务总数 —— "清除全部已结束" chip 用。
  // 不计 pending / error（活跃任务，明显不能误删）。0 时 chip 不浮。
  const finishedTaskCount = useMemo(
    () =>
      tasks.filter((t) => t.status === "done" || t.status === "cancelled")
        .length,
    [tasks],
  );
  // "清除全部已结束"二次确认 armed 态 + busy flag。bulk 删除走 memory_edit
  // "delete" action 逐条 invoke —— 与 handleDelete 单条同源。
  const [clearFinishedArmed, setClearFinishedArmed] = useState(false);
  const [clearFinishedBusy, setClearFinishedBusy] = useState(false);
  const handleClearAllFinished = useCallback(async () => {
    if (clearFinishedBusy) return;
    if (!clearFinishedArmed) {
      setClearFinishedArmed(true);
      window.setTimeout(() => setClearFinishedArmed(false), 3000);
      return;
    }
    setClearFinishedArmed(false);
    setClearFinishedBusy(true);
    const targets = tasks.filter(
      (t) => t.status === "done" || t.status === "cancelled",
    );
    let okCnt = 0;
    let failCnt = 0;
    for (const t of targets) {
      try {
        await invoke<string>("memory_edit", {
          action: "delete",
          category: "butler_tasks",
          title: t.title,
        });
        okCnt += 1;
      } catch {
        failCnt += 1;
      }
    }
    setBulkResultMsg(
      failCnt === 0
        ? `已清除 ${okCnt} 条已结束任务`
        : `清除完成：成功 ${okCnt} · 失败 ${failCnt}`,
    );
    window.setTimeout(() => setBulkResultMsg(""), 4000);
    await reload();
    setClearFinishedBusy(false);
  }, [clearFinishedArmed, clearFinishedBusy, tasks, reload]);
  const originCounts = useMemo(() => {
    let tg = 0;
    let panel = 0;
    for (const t of tasks) {
      if (isFinished(t.status)) continue;
      if (taskHasTgOrigin(t)) tg += 1;
      else panel += 1;
    }
    return { tg, panel };
  }, [tasks]);
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
    // 顺便收集"今日 / 近 7 天 done"的 task title，给小卡展开列表用。
    // 内部按 updated_at 降序便于阅读（最新完成的在最上面）。
    const todayList: { title: string; ts: number }[] = [];
    const weekList: { title: string; ts: number }[] = [];
    for (const t of tasks) {
      if (t.status !== "done") continue;
      const ts = Date.parse(t.updated_at);
      if (Number.isNaN(ts)) continue;
      if (ts >= todayMs) {
        today += 1;
        todayList.push({ title: t.title, ts });
      }
      if (ts >= weekAgoMs) {
        week += 1;
        weekList.push({ title: t.title, ts });
      }
    }
    todayList.sort((a, b) => b.ts - a.ts);
    weekList.sort((a, b) => b.ts - a.ts);
    return { today, week, todayList, weekList };
  }, [tasks, nowMs]);
  /// 完成统计小卡展开态。点小卡 toggle；点 title 触发定位后自动关闭。
  const [completedListExpanded, setCompletedListExpanded] = useState(false);
  /// 跨 render 定位 by title：点 title 后 setShowFinished + 清 filter，下一帧
  /// visibleTasks 重算才包含该任务；effect 在 visibleTasks 变化后查 idx 并
  /// setFocusedIdx，触发既有 scrollIntoView。
  const [pendingTitleFocus, setPendingTitleFocus] = useState<string | null>(null);

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
    priorityFilter.size > 0 ||
    originFilter.size > 0;

  // 键盘导航整段抽到 useTaskKeyboardNav（ref-stable 监听 + visibleTasks
  // 长度 clamp）。hook 内部用 ref 持最新依赖，避免每次 visibleTasks 变化
  // 都 re-subscribe 的窗口竞态。
  useTaskKeyboardNav({
    visibleTasks,
    toggleSelect,
    handleToggleExpand,
    handleCancelOpen,
    handleMarkDone,
    handleRetry,
    searchInputRef,
    titleInputRef,
    setCreateFormExpanded,
    setFocusedIdx,
  });

  // 焦点变化 → 把对应行 scrollIntoView，让长队列里键盘翻页跟随视图。
  useEffect(() => {
    if (focusedIdx === null) return;
    const el = document.querySelector<HTMLElement>(`[data-task-idx="${focusedIdx}"]`);
    if (el) {
      el.scrollIntoView({ block: "nearest", behavior: "smooth" });
    }
  }, [focusedIdx]);

  // pendingTitleFocus 消费：完成小卡里点 title 后此 state 被写；下一帧
  // visibleTasks 重算时找 idx → setFocusedIdx 触发上面的 scroll effect。
  // 找不到（title 被改名 / 删 / 仍被某 filter 隐藏）就静默清掉。
  useEffect(() => {
    if (pendingTitleFocus === null) return;
    const idx = visibleTasks.findIndex((t) => t.title === pendingTitleFocus);
    if (idx >= 0) setFocusedIdx(idx);
    setPendingTitleFocus(null);
  }, [pendingTitleFocus, visibleTasks]);

  // PanelChat 双击 ref → PanelApp 把 title 透传到 prop。挂载后 useEffect
  // 把它桥接进既有 pendingTitleFocus state（共用 scroll + highlight 路径，
  // 不重复实现）。消费一次即调 onConsumeFocus 清空 prop，避免用户后续
  // 改 filter / sort 时被 stale value 重新滚回。
  useEffect(() => {
    if (!pendingFocusTitle) return;
    setPendingTitleFocus(pendingFocusTitle);
    onConsumeFocus?.();
  }, [pendingFocusTitle, onConsumeFocus]);

  // 完成小卡展开后外部 click / Esc 关闭。点 popover 内部的 title 也会冒泡到
  // window —— 那条路径自己 setCompletedListExpanded(false)，先发生的是 title
  // 的 onClick（setPendingTitleFocus + close），window mousedown 是叠加 close
  // 不影响结果。
  useEffect(() => {
    if (!completedListExpanded) return;
    const close = () => setCompletedListExpanded(false);
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setCompletedListExpanded(false);
    };
    // 延迟 1 frame 挂 mousedown，避免与触发 expand 的同次点击同帧 close
    const id = window.setTimeout(() => {
      window.addEventListener("mousedown", close);
    }, 0);
    window.addEventListener("keydown", onKey);
    return () => {
      window.clearTimeout(id);
      window.removeEventListener("mousedown", close);
      window.removeEventListener("keydown", onKey);
    };
  }, [completedListExpanded]);

  const s = {
    // 主题迁移（迭代 2）：框架级 surface 走 CSS var；功能性配色（status
    // badge、action 按钮、chip、紧迫度等）保持原色不动 —— 它们携带 motion
    // 语义，跨主题需稳定可识别。
    container: { padding: 16, overflowY: "auto" as const, height: "100%", background: "var(--pet-color-bg)" },
    section: { marginBottom: 20 },
    sectionTitle: { fontSize: 13.5, fontWeight: 600, color: "var(--pet-color-fg)", marginBottom: 10, paddingBottom: 6, borderBottom: "1px solid var(--pet-color-border)", letterSpacing: 0.2 },
    formCard: { padding: 14, background: "var(--pet-color-card)", border: "1px solid var(--pet-color-border)", borderRadius: 10, marginBottom: 14, boxShadow: "0 1px 2px rgba(15, 23, 42, 0.04)" },
    label: { fontSize: 12, color: "var(--pet-color-muted)", display: "block", marginBottom: 4, fontWeight: 500 },
    input: { width: "100%", padding: "7px 11px", border: "1px solid var(--pet-color-border)", background: "var(--pet-color-card)", color: "var(--pet-color-fg)", borderRadius: 6, fontSize: 13, boxSizing: "border-box" as const },
    textarea: { width: "100%", padding: "7px 11px", border: "1px solid var(--pet-color-border)", background: "var(--pet-color-card)", color: "var(--pet-color-fg)", borderRadius: 6, fontSize: 13, resize: "vertical" as const, minHeight: 60, boxSizing: "border-box" as const },
    twoCol: { display: "grid", gridTemplateColumns: "1fr 1fr", gap: 8, marginTop: 8 },
    item: { padding: "10px 12px", background: "var(--pet-color-card)", border: "1px solid var(--pet-color-border)", borderRadius: 8, marginBottom: 8 },
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
      fontSize: 10.5,
      padding: "2px 9px",
      borderRadius: 999,
      background: STATUS_BADGE[status].bg,
      color: STATUS_BADGE[status].fg,
      whiteSpace: "nowrap" as const,
      flexShrink: 0,
      fontWeight: 600,
      letterSpacing: 0.3,
      // 让 pill 在 light 主题下也有"轮廓感"；fg 色 18% alpha 边沿正好
      // 比纯 bg 更精致，不喧宾夺主。
      border: `1px solid color-mix(in srgb, ${STATUS_BADGE[status].fg} 18%, transparent)`,
    }),
    priBadge: (priority: number): React.CSSProperties => {
      // P0-9 五档色阶：P0 最紧急走红 / P1-2 橙 / P3-4 默认黄 / P5-6 淡灰 /
      // P7-9 muted（idea 抽屉色）。让队列扫读时一眼看到 priority 分布。
      let bg = "var(--pet-tint-yellow-bg)";
      let fg = "var(--pet-tint-yellow-fg)";
      if (priority === 0) {
        bg = "var(--pet-tint-red-bg)";
        fg = "var(--pet-tint-red-fg)";
      } else if (priority <= 2) {
        bg = "var(--pet-tint-orange-bg)";
        fg = "var(--pet-tint-orange-fg)";
      } else if (priority <= 4) {
        // 默认黄，base 已是
      } else if (priority <= 6) {
        bg = "var(--pet-color-bg)";
        fg = "var(--pet-color-fg)";
      } else {
        bg = "var(--pet-color-bg)";
        fg = "var(--pet-color-muted)";
      }
      return {
        fontSize: 10.5,
        padding: "2px 9px",
        borderRadius: 999,
        background: bg,
        color: fg,
        whiteSpace: "nowrap" as const,
        fontWeight: 600,
        letterSpacing: 0.3,
        border: `1px solid color-mix(in srgb, ${fg} 18%, transparent)`,
      };
    },
    btnPrimary: { padding: "6px 16px", border: "none", borderRadius: 4, background: "var(--pet-color-accent)", color: "var(--pet-color-card)", cursor: "pointer", fontSize: 13, marginTop: 8 },
    btnDisabled: { padding: "6px 16px", border: "none", borderRadius: 4, background: "var(--pet-color-muted)", color: "var(--pet-color-card)", cursor: "not-allowed", fontSize: 13, marginTop: 8 },
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
      background: selected ? "var(--pet-tint-blue-bg)" : "var(--pet-color-bg)",
      color: selected ? "var(--pet-tint-blue-fg)" : "var(--pet-color-muted)",
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
    cancelledMsg: { color: "var(--pet-color-muted)", fontSize: 11, marginTop: 4 },
    resultMsg: { color: "var(--pet-tint-green-fg)", fontSize: 11, marginTop: 4 },
    tagRow: { display: "flex", flexWrap: "wrap" as const, gap: 4, marginTop: 4 },
    tagChip: {
      fontSize: 10,
      padding: "1px 6px",
      borderRadius: 8,
      background: "var(--pet-color-bg)",
      color: "var(--pet-color-muted)",
      whiteSpace: "nowrap" as const,
      cursor: "pointer" as const,
      userSelect: "none" as const,
    },
    actionRow: { display: "flex", gap: 6, marginTop: 8, flexWrap: "wrap" as const },
    actionBtn: {
      padding: "4px 10px",
      border: "1px solid #cbd5e1",
      borderRadius: 4,
      background: "var(--pet-color-card)",
      color: "var(--pet-color-fg)",
      cursor: "pointer",
      fontSize: 12,
    },
    actionBtnRetry: {
      padding: "4px 10px",
      border: "1px solid #bae6fd",
      borderRadius: 4,
      background: "var(--pet-tint-blue-bg)",
      color: "var(--pet-tint-blue-fg)",
      cursor: "pointer",
      fontSize: 12,
    },
    actionBtnDanger: {
      padding: "4px 10px",
      border: "1px solid #fecaca",
      borderRadius: 4,
      background: "var(--pet-color-card)",
      color: "var(--pet-tint-orange-fg)",
      cursor: "pointer",
      fontSize: 12,
    },
    actionBtnDisabled: {
      padding: "4px 10px",
      border: "1px solid #e2e8f0",
      borderRadius: 4,
      background: "var(--pet-color-bg)",
      color: "var(--pet-color-muted)",
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
      borderTop: "1px dashed var(--pet-color-border)",
      display: "flex",
      flexDirection: "column" as const,
      gap: 10,
    },
    detailSection: { display: "flex", flexDirection: "column" as const, gap: 4 },
    detailLabel: {
      fontSize: 11,
      color: "var(--pet-color-muted)",
      fontWeight: 600,
      textTransform: "uppercase" as const,
      letterSpacing: "0.04em",
    },
    detailHint: { fontSize: 11, color: "var(--pet-color-muted)", fontStyle: "italic" as const },
    rawDescBox: {
      fontSize: 12,
      color: "var(--pet-color-fg)",
      background: "var(--pet-color-bg)",
      padding: "10px 14px",
      borderRadius: 8,
      border: "1px solid var(--pet-color-border)",
      whiteSpace: "pre-wrap" as const,
      wordBreak: "break-word" as const,
      fontFamily: "'SF Mono', 'Menlo', monospace",
      lineHeight: 1.6,
      // 宽屏下锁住阅读行宽：~60-80 中文字符是舒适视幅；超 800px 时单行
      // 视线水平扫太费眼。窗口窄于 800px 时 maxWidth 不生效，仍 100%。
      maxWidth: 800,
    },
    detailMdBox: {
      fontSize: 12,
      color: "var(--pet-color-fg)",
      background: "var(--pet-color-card)",
      padding: "12px 16px",
      border: "1px solid var(--pet-color-border)",
      borderRadius: 8,
      boxShadow: "var(--pet-shadow-sm)",
      whiteSpace: "pre-wrap" as const,
      lineHeight: 1.65,
      maxWidth: 800,
    },
    historyList: { display: "flex", flexDirection: "column" as const, gap: 4 },
    historyItem: {
      fontSize: 11,
      color: "var(--pet-color-muted)",
      display: "flex",
      gap: 8,
      alignItems: "flex-start",
      lineHeight: 1.5,
    },
    historyTs: {
      color: "var(--pet-color-muted)",
      fontFamily: "'SF Mono', 'Menlo', monospace",
      flexShrink: 0,
    },
    bulkBar: {
      display: "flex",
      alignItems: "center",
      gap: 6,
      flexWrap: "wrap" as const,
      padding: "8px 10px",
      background: "var(--pet-tint-blue-bg)",
      border: "1px solid #bfdbfe",
      borderRadius: 6,
      marginBottom: 8,
      fontSize: 12,
      color: "var(--pet-tint-blue-fg)",
    },
    bulkSelectionLabel: { fontWeight: 600, marginRight: 6 },
    bulkBtn: {
      padding: "4px 10px",
      border: "1px solid #bfdbfe",
      borderRadius: 4,
      background: "var(--pet-color-card)",
      color: "var(--pet-tint-blue-fg)",
      cursor: "pointer",
      fontSize: 12,
    },
    bulkBtnActive: {
      padding: "4px 10px",
      border: "1px solid #1e40af",
      borderRadius: 4,
      background: "var(--pet-tint-blue-fg)",
      color: "var(--pet-color-card)",
      cursor: "pointer",
      fontSize: 12,
    },
    bulkBtnDisabled: {
      padding: "4px 10px",
      border: "1px solid #e2e8f0",
      borderRadius: 4,
      background: "var(--pet-color-bg)",
      color: "var(--pet-color-muted)",
      cursor: "not-allowed",
      fontSize: 12,
    },
    bulkSubPanel: {
      marginTop: 8,
      padding: "8px 10px",
      background: "var(--pet-color-bg)",
      border: "1px dashed var(--pet-color-border)",
      borderRadius: 6,
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
      background: "var(--pet-tint-green-bg)",
      color: "var(--pet-tint-green-fg)",
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
        create: { bg: "var(--pet-tint-blue-bg)", fg: "var(--pet-tint-blue-fg)" },
        update: { bg: "var(--pet-color-bg)", fg: "var(--pet-color-muted)" },
        delete: { bg: "var(--pet-tint-red-bg)", fg: "var(--pet-tint-red-fg)" },
      };
      const c = map[action] ?? { bg: "var(--pet-color-bg)", fg: "var(--pet-color-muted)" };
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

  if (loading) {
    return <LoadingState />;
  }

  return (
    <div style={s.container}>
      {/* "取消原因"历史 datalist：单条 cancel input + 批量 cancel input 共
          用同一份 id；空 history 时不渲 option list，浏览器自动跳过自动完成。 */}
      {cancelReasonHistory.length > 0 && (
        <datalist id="pet-tasks-cancel-reason-history">
          {cancelReasonHistory.map((r) => (
            <option key={r} value={r} />
          ))}
        </datalist>
      )}
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
          color: var(--pet-color-accent);
          border-color: color-mix(in srgb, var(--pet-color-accent) 50%, transparent);
        }
        /* R123: 任务卡 hover 高亮。与 R122 PanelMemory 同款，bg 切到
           var(--pet-color-bg) 与 card 反差。!important 反压 inline s.item
           优先级。focus outline / 内部 detail 区块各自独立，互不干扰。
           迭代 4：加 box-shadow lift + accent 18% alpha border，让 hover 像
           "卡片轻轻浮起"，扫长队列时 hover 落点更明确。 */
        .pet-task-card {
          transition: background-color 0.14s ease, box-shadow 0.18s ease,
            border-color 0.18s ease, transform 0.14s ease;
        }
        .pet-task-card:hover {
          background: var(--pet-color-bg) !important;
          border-color: color-mix(in srgb, var(--pet-color-accent) 35%, var(--pet-color-border)) !important;
          box-shadow: var(--pet-shadow-sm);
        }
        /* ⚡ NOW 标记 chip 的脉冲动画：让"提醒"chip 在 60s 内持续抓眼，
           过期 React 卸 class → 动画自然停。reduced-motion 退化为常亮。 */
        @keyframes pet-task-now-pulse {
          0%, 100% { transform: scale(1); opacity: 1; }
          50%      { transform: scale(1.06); opacity: 0.85; }
        }
        @media (prefers-reduced-motion: reduce) {
          [style*="pet-task-now-pulse"] { animation: none !important; }
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
          <span style={{ width: 10, fontFamily: "monospace", color: "var(--pet-color-muted)" }}>
            {createFormExpanded ? "▾" : "▸"}
          </span>
          <span>新建任务</span>
        </div>
        {createFormExpanded && (
        <div style={s.formCard}>
          <div
            style={{
              display: "flex",
              alignItems: "center",
              justifyContent: "space-between",
              marginBottom: 4,
            }}
          >
            <label style={s.label}>标题</label>
            {/* 📋 从模板 下拉：选中后 prefill title/body/priority/due。
                value="" 是 disabled placeholder，选完立刻 reset 让下次能
                重选同一个模板。与 iter #176 PanelMemory "复制 schedule"
                下拉同模式。 */}
            <select
              value=""
              onChange={(e) => {
                const v = e.target.value;
                if (!v) return;
                applyTaskTemplate(parseInt(v, 10));
                e.currentTarget.value = "";
              }}
              title="选一个常见任务范例预填表单（你可以直接保存或改完再交付）"
              style={{
                padding: "2px 6px",
                fontSize: 11,
                border: "1px solid var(--pet-color-border)",
                borderRadius: 4,
                background: "var(--pet-color-card)",
                color: "var(--pet-color-fg)",
                cursor: "pointer",
                fontFamily: "inherit",
                maxWidth: 200,
              }}
            >
              <option value="">📋 从模板…</option>
              {TASK_TEMPLATES.map((tpl, i) => (
                <option key={tpl.label} value={i}>
                  {tpl.label}
                </option>
              ))}
            </select>
          </div>
          <input
            style={s.input}
            ref={titleInputRef}
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            onKeyDown={handleFormKeyDown}
            placeholder="比如：整理 Downloads"
          />
          {/* 检测 schedule 前缀：用户敲 `[every:` / `[once:` / `[deadline:`
              说明意图是定时 / 截止任务，应在 PanelMemory butler_tasks 段创建，
              而非 PanelTasks 队列（队列任务是一次性的）。inline 提示让用户
              不必先建错再删。检查 title 与 body 两处，任一命中即浮 hint。 */}
          {(() => {
            const SCHEDULE_RE = /\[(every|once|deadline)[:\s]/i;
            const hitsTitle = SCHEDULE_RE.test(title);
            const hitsBody = SCHEDULE_RE.test(body);
            if (!hitsTitle && !hitsBody) return null;
            return (
              <div
                style={{
                  marginTop: 4,
                  padding: "6px 10px",
                  fontSize: 11,
                  lineHeight: 1.5,
                  background: "var(--pet-tint-yellow-bg)",
                  border: "1px solid var(--pet-tint-yellow-fg)",
                  color: "var(--pet-tint-yellow-fg)",
                  borderRadius: 4,
                  display: "flex",
                  gap: 6,
                  alignItems: "flex-start",
                }}
                title="schedule 前缀（[every:/once:/deadline:]）是 butler_tasks memory 的语法，让宠物按时机自动执行；本面板的队列是一次性派单。两者数据 source 不同。"
              >
                <span style={{ flexShrink: 0 }}>💡</span>
                <span>
                  检测到 schedule 前缀 —— 想定时 / 周期执行？建议改在
                  「记忆」面板的 butler_tasks 段新建（pet 会按时间自己跑）。
                  这里建的任务是一次性"立即派单"。
                </span>
              </div>
            );
          })()}
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
              <div style={{ display: "flex", gap: 4 }}>
                <input
                  type="number"
                  min={0}
                  max={PRIORITY_MAX}
                  style={{ ...s.input, flex: 1 }}
                  value={priority}
                  onChange={(e) => {
                    const n = parseInt(e.target.value, 10);
                    if (Number.isNaN(n)) return;
                    setPriority(Math.max(0, Math.min(PRIORITY_MAX, n)));
                  }}
                  onKeyDown={handleFormKeyDown}
                />
                {/* ▲▼ 微调按钮：与 type="number" 原生 spinner 互补 ——
                    WKWebView 原生 spinner 偏小 + 视觉淡，显式按钮更易点。
                    clamp 到 [0, PRIORITY_MAX]。 */}
                <button
                  type="button"
                  onClick={() =>
                    setPriority((p) => Math.min(PRIORITY_MAX, p + 1))
                  }
                  disabled={priority >= PRIORITY_MAX}
                  title="优先级 +1（数字大 = 不紧急）"
                  style={{
                    padding: "0 8px",
                    fontSize: 10,
                    lineHeight: 1,
                    border: "1px solid var(--pet-color-border)",
                    borderRadius: 4,
                    background: "var(--pet-color-card)",
                    color: "var(--pet-color-muted)",
                    cursor: priority >= PRIORITY_MAX ? "default" : "pointer",
                    flexShrink: 0,
                  }}
                >
                  ▲
                </button>
                <button
                  type="button"
                  onClick={() => setPriority((p) => Math.max(0, p - 1))}
                  disabled={priority <= 0}
                  title="优先级 -1（数字小 = 紧急）"
                  style={{
                    padding: "0 8px",
                    fontSize: 10,
                    lineHeight: 1,
                    border: "1px solid var(--pet-color-border)",
                    borderRadius: 4,
                    background: "var(--pet-color-card)",
                    color: "var(--pet-color-muted)",
                    cursor: priority <= 0 ? "default" : "pointer",
                    flexShrink: 0,
                  }}
                >
                  ▼
                </button>
              </div>
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

      {/* 顶部 tab：队列 / 归档 二选一。default 队列；切归档时立即触发
          reloadArchive。归档没单独 filter / sort，与队列分离。 */}
      <div
        style={{
          display: "flex",
          gap: 4,
          marginBottom: 12,
          borderBottom: "1px solid var(--pet-color-border)",
        }}
      >
        {(
          [
            { key: "queue" as const, label: "队列", glyph: "📋" },
            { key: "archive" as const, label: "归档", glyph: "📦" },
          ]
        ).map(({ key, label, glyph }) => {
          const active = taskViewTab === key;
          const isArchive = key === "archive";
          const badge = isArchive
            ? archiveLoaded
              ? archiveItems.length
              : null
            : tasks.filter((t) => !isFinished(t.status)).length;
          return (
            <button
              key={key}
              type="button"
              onClick={() => {
                setTaskViewTab(key);
                if (isArchive && !archiveLoaded) {
                  setArchiveExpanded(true);
                  void reloadArchive();
                }
              }}
              style={{
                fontSize: 13,
                padding: "8px 14px 6px",
                border: "none",
                borderBottom: active
                  ? "2px solid var(--pet-color-accent)"
                  : "2px solid transparent",
                background: "transparent",
                color: active ? "var(--pet-color-accent)" : "var(--pet-color-muted)",
                fontWeight: active ? 600 : 500,
                cursor: active ? "default" : "pointer",
                fontFamily: "inherit",
                marginBottom: -1,
              }}
            >
              {glyph} {label}
              {badge !== null && (
                <span
                  style={{
                    marginLeft: 6,
                    fontSize: 11,
                    fontWeight: 400,
                    color: active ? "var(--pet-color-accent)" : "var(--pet-color-muted)",
                    fontVariantNumeric: "tabular-nums",
                  }}
                >
                  ({badge})
                </span>
              )}
            </button>
          );
        })}
      </div>

      {taskViewTab === "queue" && (
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
              style={{ position: "relative", marginTop: 2 }}
              onMouseDown={(e) => e.stopPropagation()}
            >
              <button
                type="button"
                onClick={() => setCompletedListExpanded((v) => !v)}
                title="点击展开 / 折叠近 7 天的完成任务列表（点 title 跳到该行；cancelled 不计）"
                style={{
                  fontSize: 11,
                  color: "var(--pet-color-muted)",
                  fontWeight: 400,
                  background: completedListExpanded
                    ? "var(--pet-tint-blue-bg)"
                    : "transparent",
                  border: completedListExpanded
                    ? "1px solid var(--pet-tint-blue-fg)"
                    : "1px dashed var(--pet-color-border)",
                  borderRadius: 4,
                  padding: "2px 8px",
                  cursor: "pointer",
                  fontFamily: "inherit",
                }}
              >
                ✅ 今日完成 {completionStats.today} · 近 7 天 {completionStats.week}
                {completionStats.week > 0 && (
                  <span style={{ marginLeft: 4 }}>{completedListExpanded ? "▾" : "▸"}</span>
                )}
              </button>
              {completedListExpanded && completionStats.week > 0 && (
                <div
                  style={{
                    position: "absolute",
                    top: "calc(100% + 4px)",
                    left: 0,
                    minWidth: 240,
                    maxWidth: 360,
                    maxHeight: 280,
                    overflowY: "auto",
                    background: "var(--pet-color-card)",
                    border: "1px solid var(--pet-color-border)",
                    borderRadius: 6,
                    boxShadow: "0 4px 12px rgba(0,0,0,0.15)",
                    padding: 6,
                    zIndex: 30,
                    fontSize: 12,
                  }}
                >
                  {(() => {
                    const todayMs = bucketBoundaries.todayMs;
                    const todayItems = completionStats.weekList.filter((x) => x.ts >= todayMs);
                    const earlierItems = completionStats.weekList.filter((x) => x.ts < todayMs);
                    const Section = ({ label, items }: { label: string; items: { title: string; ts: number }[] }) => {
                      if (items.length === 0) return null;
                      return (
                        <div style={{ marginBottom: 4 }}>
                          <div style={{ fontSize: 10, color: "var(--pet-color-muted)", padding: "2px 6px", fontWeight: 600 }}>
                            {label}（{items.length}）
                          </div>
                          {items.map((it) => (
                            <button
                              key={`${it.title}-${it.ts}`}
                              type="button"
                              onClick={() => {
                                // 清 filter + 显 finished，让目标行一定出现在 visibleTasks 里
                                setSearch("");
                                setSelectedTags(new Set());
                                setDueFilter("all");
                                setPriorityFilter(new Set());
                                setShowFinished(true);
                                setPendingTitleFocus(it.title);
                                setCompletedListExpanded(false);
                              }}
                              title={`updated_at: ${new Date(it.ts).toLocaleString()} · 点击跳到该行`}
                              style={{
                                display: "block",
                                width: "100%",
                                textAlign: "left",
                                background: "transparent",
                                border: "none",
                                padding: "3px 6px",
                                fontSize: 12,
                                color: "var(--pet-color-fg)",
                                cursor: "pointer",
                                borderRadius: 3,
                                fontFamily: "inherit",
                                whiteSpace: "nowrap",
                                overflow: "hidden",
                                textOverflow: "ellipsis",
                              }}
                              onMouseOver={(e) => {
                                (e.currentTarget as HTMLButtonElement).style.background = "var(--pet-color-bg)";
                              }}
                              onMouseOut={(e) => {
                                (e.currentTarget as HTMLButtonElement).style.background = "transparent";
                              }}
                            >
                              ✓ {it.title}
                            </button>
                          ))}
                        </div>
                      );
                    };
                    return (
                      <>
                        <Section label="今日" items={todayItems} />
                        <Section label="近 7 天（早些）" items={earlierItems} />
                      </>
                    );
                  })()}
                </div>
              )}
            </div>
          </div>
          <div style={{ display: "flex", gap: 4, alignItems: "center" }} title="切换排序模式：默认综合 / 按截止时间升序 / 按优先级降序（priority 模式下可拖卡片改 P）">
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
                    borderColor: active ? "var(--pet-color-accent)" : "var(--pet-color-border)",
                    borderRadius: 4,
                    background: active ? "var(--pet-color-accent)" : "var(--pet-color-card)",
                    color: active ? "#fff" : "var(--pet-color-muted)",
                    cursor: active ? "default" : "pointer",
                    fontWeight: active ? 600 : 400,
                  }}
                >
                  {mode === "queue" ? "队列" : mode === "due" ? "due ↑" : "P ↓"}
                </button>
              );
            })}
            {sortMode === "priority" && (
              <span
                style={{ fontSize: 10, color: "var(--pet-color-muted)", marginLeft: 4 }}
                title="拖卡片到另一条上 → 自己的 priority 改成对方的 P 值"
              >
                · 可拖
              </span>
            )}
          </div>
        </div>
        <div style={s.searchRow}>
          <input
            ref={searchInputRef}
            type="text"
            placeholder="按标题或内容搜索…（⌘F / ⌘K / `/` 聚焦）"
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
                  setOriginFilter(new Set());
                }}
                style={s.searchClearBtn}
                title="一键清掉全部 active filter（search / tag / due / priority / origin）"
                aria-label="清除全部过滤"
              >
                ✕ 全部
              </button>
            </>
          )}
          {/* "✓ 含 detail" toggle：把 detail.md 进度笔记一并塞 export；
              缓存命中即时，miss fetch（N 次 IO，长队列耗时几秒）。偏好
              localStorage 持久化跨重启。 */}
          <label
            style={{
              display: "inline-flex",
              alignItems: "center",
              gap: 4,
              fontSize: 11,
              color: "var(--pet-color-muted)",
              userSelect: "none",
              cursor: "pointer",
            }}
            title="勾上后 '导出 MD' 顺手把每条任务的 detail.md 进度笔记一并拼进 markdown（N 次 IO，长队列耗时几秒）。偏好跨重启记忆。"
          >
            <input
              type="checkbox"
              checked={exportIncludeDetail}
              onChange={(e) => setExportIncludeDetailPersist(e.target.checked)}
            />
            含 detail
          </label>
          {/* 全部导出 MD：直接拼当前 visibleTasks 写剪贴板，不用先点选。复盘 /
              周回顾场景常用。disabled 时（无任务）muted 灰态，仍渲染让位置稳定。 */}
          <button
            type="button"
            onClick={handleExportAllVisibleAsMd}
            disabled={visibleTasks.length === 0}
            style={s.searchClearBtn}
            title={
              filtersActive
                ? `把当前过滤下的 ${visibleTasks.length} 条任务一次性拼成 markdown 写到剪贴板${exportIncludeDetail ? "（含 detail.md 进度笔记）" : "（不含 detail.md / history）"}`
                : `把全部 ${visibleTasks.length} 条任务一次性拼成 markdown 写到剪贴板${exportIncludeDetail ? "（含 detail.md 进度笔记）" : "（不含 detail.md / history）"}`
            }
            aria-label="导出全部任务为 markdown"
          >
            📋 导出 MD ({visibleTasks.length})
          </button>
        </div>
        {(dueTodayCount > 0 || overdueCount > 0 || createdTodayCount > 0 || priorityCounts.length > 0 || originCounts.tg > 0 || errorTaskCount > 0 || finishedTaskCount > 0) && (
          <div style={{ ...s.tagFilterRow, marginBottom: 6 }}>
            {/* 一键重试所有 error 任务 chip。> 0 时显，红底突出。点击调
                handleRetryAllErrors 顺序 invoke task_retry；bulkBusy 期间
                disabled 防双触。与 due / overdue chip 同列位置便于一眼扫到。 */}
            {errorTaskCount > 0 && (
              <button
                type="button"
                onClick={() => void handleRetryAllErrors()}
                disabled={bulkBusy}
                style={{
                  fontSize: 11,
                  padding: "2px 8px",
                  borderRadius: 10,
                  border: "1px solid #fca5a5",
                  background: bulkBusy ? "var(--pet-tint-red-bg)" : "var(--pet-tint-red-bg)",
                  color: "var(--pet-tint-red-fg)",
                  cursor: bulkBusy ? "default" : "pointer",
                  fontWeight: 600,
                  whiteSpace: "nowrap",
                  opacity: bulkBusy ? 0.6 : 1,
                }}
                title={`一键 task_retry 所有 status=error 的 ${errorTaskCount} 个任务（按 tasks 顺序逐条调用;带宽 / quota 失败会汇总到 bulk result）。`}
                aria-label="batch retry all error tasks"
              >
                🔄 重试错误 ({errorTaskCount})
              </button>
            )}
            {/* "清除全部已结束" chip：删除所有 done / cancelled 任务（逐条
                memory_edit delete）。两次点击确认（armed 红字 → 再点真删）。
                busy 期间 disabled 防双触。 */}
            {finishedTaskCount > 0 && (
              <button
                type="button"
                onClick={() => void handleClearAllFinished()}
                disabled={clearFinishedBusy}
                style={{
                  fontSize: 11,
                  padding: "2px 8px",
                  borderRadius: 10,
                  border: clearFinishedArmed
                    ? "1px solid #dc2626"
                    : "1px solid var(--pet-color-border)",
                  background: clearFinishedBusy
                    ? "var(--pet-color-bg)"
                    : clearFinishedArmed
                      ? "var(--pet-tint-red-bg)"
                      : "var(--pet-color-card)",
                  color: clearFinishedBusy
                    ? "var(--pet-color-muted)"
                    : clearFinishedArmed
                      ? "var(--pet-tint-red-fg)"
                      : "var(--pet-color-muted)",
                  cursor: clearFinishedBusy ? "default" : "pointer",
                  fontWeight: clearFinishedArmed ? 600 : undefined,
                  whiteSpace: "nowrap",
                }}
                title={
                  clearFinishedBusy
                    ? "清除中…"
                    : clearFinishedArmed
                      ? "再次点击确认删除（3s 内有效；删完不可恢复）"
                      : `批量删除所有 done / cancelled 任务（${finishedTaskCount} 条）— 走 memory_edit delete 逐条调用。点击后 3s 内需再点确认。`
                }
                aria-label="clear all finished tasks"
              >
                {clearFinishedBusy
                  ? `清除中…`
                  : clearFinishedArmed
                    ? `再点确认 (3s)`
                    : `🗑️ 清结束 (${finishedTaskCount})`}
              </button>
            )}
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
            {createdTodayCount > 0 && (
              <DueChip
                kind="createdToday"
                count={createdTodayCount}
                active={dueFilter === "createdToday"}
                onToggle={() =>
                  setDueFilter((prev) =>
                    prev === "createdToday" ? "all" : "createdToday",
                  )
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
                    background: active ? "var(--pet-color-border)" : "var(--pet-color-bg)",
                    color: "var(--pet-color-muted)",
                    cursor: "pointer",
                    whiteSpace: "nowrap",
                    userSelect: "none",
                    border: `1px solid ${active ? "var(--pet-color-muted)" : "var(--pet-color-border)"}`,
                  }}
                >
                  {active ? "✓ " : ""}{p === 0 ? "💡 P0" : `P${p}`}
                  <span style={{ fontSize: 10, opacity: 0.7, marginLeft: 2 }}>
                    ({count})
                  </span>
                </span>
              );
            })}
            {/* origin chip：仅在有 TG 来源任务时才显（否则二元退化为单选无意义）。
                两 chip 互不互斥（与 priority 同 OR 语义集合）—— 同时选 tg+panel
                等同于"不过滤"，UI 仍允许避免用户多点取消。📨 = TG / 💻 = 面板。 */}
            {originCounts.tg > 0 && (
              <>
                {(
                  [
                    { key: "tg" as const, glyph: "📨", label: "TG", count: originCounts.tg },
                    { key: "panel" as const, glyph: "💻", label: "面板", count: originCounts.panel },
                  ]
                ).map(({ key, glyph, label, count }) => {
                  const active = originFilter.has(key);
                  const toggle = () =>
                    setOriginFilter((prev) => {
                      const next = new Set(prev);
                      if (next.has(key)) next.delete(key);
                      else next.add(key);
                      return next;
                    });
                  return (
                    <span
                      key={key}
                      role="button"
                      tabIndex={0}
                      onClick={toggle}
                      onKeyDown={(e) => {
                        if (e.key === "Enter" || e.key === " ") {
                          e.preventDefault();
                          toggle();
                        }
                      }}
                      title={
                        active
                          ? `再次点击移出"${label}入口"过滤`
                          : `只看${label}入口的任务（${count} 条活动）`
                      }
                      style={{
                        fontSize: 11,
                        padding: "2px 8px",
                        borderRadius: 10,
                        background: active ? "var(--pet-tint-blue-bg)" : "var(--pet-color-bg)",
                        color: active ? "var(--pet-tint-blue-fg)" : "var(--pet-color-muted)",
                        cursor: "pointer",
                        whiteSpace: "nowrap",
                        userSelect: "none",
                        border: `1px solid ${active ? "color-mix(in srgb, var(--pet-color-accent) 50%, var(--pet-color-border))" : "var(--pet-color-border)"}`,
                      }}
                    >
                      {active ? "✓ " : ""}{glyph} {label}
                      <span style={{ fontSize: 10, opacity: 0.7, marginLeft: 2 }}>
                        ({count})
                      </span>
                    </span>
                  );
                })}
              </>
            )}
          </div>
        )}
        {(allTags.length > 0 || untaggedCount > 0) && (
          <div style={s.tagFilterRow}>
            <span style={s.tagFilterLabel}>tag：</span>
            {allTags.map(([tag, count]) => {
              const selected = selectedTags.has(tag);
              const tintStyle = getTagTintStyle(tag);
              return (
                <span
                  key={tag}
                  // selected 时 base style 已是高亮色（accent 蓝），跨 selected
                  // 仍叠用户自选 tint 会冲突 —— selected 优先（用户在筛，颜色
                  // 反馈"已选"语义最重要）；只在 unselected 才用 tint。
                  style={selected ? s.tagFilterChip(selected) : { ...s.tagFilterChip(false), ...tintStyle }}
                  onClick={() => toggleTag(tag)}
                  onContextMenu={(e) => {
                    e.preventDefault();
                    e.stopPropagation();
                    setTagColorPicker({ tag, x: e.clientX, y: e.clientY });
                  }}
                  title={
                    selected
                      ? `点击取消「${tag}」过滤（当前共 ${count} 条带此 tag 的任务）`
                      : `点击只看带「${tag}」的任务（共 ${count} 条） · 右键改颜色`
                  }
                >
                  {selected ? "✓ " : ""}#{tag}
                  <span style={s.tagFilterCount}> ({count})</span>
                </span>
              );
            })}
            {/* "无 tag" 合成 chip：用 "" sentinel 加入 selectedTags 集合，
                filter 链里特例命中 t.tags.length === 0 的任务。视觉上走 dashed
                border 区分"这不是一个普通 tag 而是一个谓词类别"。 */}
            {untaggedCount > 0 && (() => {
              const selected = selectedTags.has("");
              return (
                <span
                  key="__notag__"
                  style={{
                    ...s.tagFilterChip(selected),
                    borderStyle: "dashed",
                  }}
                  onClick={() => toggleTag("")}
                  title={
                    selected
                      ? `点击取消「无 tag」过滤（当前共 ${untaggedCount} 条未打 tag 的任务）`
                      : `点击只看未打 tag 的任务（共 ${untaggedCount} 条）`
                  }
                >
                  {selected ? "✓ " : ""}🚫 无 tag
                  <span style={s.tagFilterCount}> ({untaggedCount})</span>
                </span>
              );
            })()}
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
                style={bulkBusy ? s.bulkBtnDisabled : (bulkAction === "done" ? s.bulkBtnActive : s.bulkBtn)}
                disabled={bulkBusy}
                onClick={() => {
                  setBulkAction((a) => (a === "done" ? null : "done"));
                  setBulkDoneResult("");
                }}
                title="批量标 done（共享一段 result 摘要，可留空只追加 [done]）"
              >
                ✓ 标 done
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
                onClick={handleBulkCopyAsRefs}
                title="把选中任务拼成 `「A」「B」「C」` 一段写到剪贴板。粘到 chat 每个 token 都是 hover-able ref（hover 显状态 / 双击跳源任务）。"
              >
                🔗 拼为 ref
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
            {bulkAction === "done" && (
              <div style={s.bulkSubPanel}>
                <input
                  style={s.bulkSubInput}
                  placeholder="result 摘要（共享，可留空 = 仅追加 [done]）"
                  value={bulkDoneResult}
                  onChange={(e) => setBulkDoneResult(e.target.value)}
                  autoFocus
                  onKeyDown={(e) => {
                    if (e.key === "Enter" && !bulkBusy) {
                      e.preventDefault();
                      void handleBulkMarkDoneConfirm();
                    }
                  }}
                />
                <button
                  style={bulkBusy ? s.bulkBtnDisabled : s.bulkBtnActive}
                  disabled={bulkBusy}
                  onClick={handleBulkMarkDoneConfirm}
                >
                  {bulkBusy ? "处理中..." : "确认批量标 done"}
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
            {bulkAction === "cancel" && (
              <div style={s.bulkSubPanel}>
                <input
                  style={s.bulkSubInput}
                  placeholder="取消原因（共享，可留空）"
                  value={bulkReason}
                  onChange={(e) => setBulkReason(e.target.value)}
                  autoFocus
                  list="pet-tasks-cancel-reason-history"
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
                    color: "var(--pet-color-muted)",
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
                {/* 相对 ± 调整：与绝对 set 互补的"全部往上提 / 往下降一档"快捷。
                    各条任务的当前 priority 各自 + delta 后 clamp，边界条不发请求。 */}
                <span
                  style={{
                    fontSize: 11,
                    color: "var(--pet-color-muted)",
                    marginLeft: 8,
                  }}
                  title="按各条任务的当前 priority 相对调整。注：priority 数字越小越重要（P0 最重）"
                >
                  或相对：
                </span>
                <button
                  style={bulkBusy ? s.bulkBtnDisabled : s.bulkBtn}
                  disabled={bulkBusy}
                  onClick={() => void handleBulkAdjustPriority(-1)}
                  title="每条 priority -1（更重要）；边界条不变"
                >
                  ↑ -1
                </button>
                <button
                  style={bulkBusy ? s.bulkBtnDisabled : s.bulkBtn}
                  disabled={bulkBusy}
                  onClick={() => void handleBulkAdjustPriority(+1)}
                  title="每条 priority +1（更次要）；边界条不变"
                >
                  ↓ +1
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
          <EmptyState
            icon={filtersActive ? "🔍" : showFinished ? "✅" : "🎉"}
            title={
              filtersActive
                ? "没有匹配筛选条件的任务"
                : showFinished
                  ? "还没有任何任务"
                  : "队列里没有进行中的任务"
            }
            hint={
              !filtersActive && showFinished
                ? "试试用范例新建一条，或在上方表单自由填写"
                : undefined
            }
          >
            {/* filter 命中 0 条时给一个就地"清除全部过滤"按钮 ——
                现有 "✕ 全部" 按钮在 search 行可能被滚出视野，用户
                看到 empty 文案后能立刻点回去。 */}
            {filtersActive && (
              <button
                type="button"
                style={{
                  fontSize: 12,
                  padding: "6px 14px",
                  border: "1px solid var(--pet-color-border)",
                  borderRadius: 6,
                  background: "var(--pet-color-card)",
                  color: "var(--pet-color-fg)",
                  cursor: "pointer",
                }}
                onClick={() => {
                  setSearch("");
                  setSelectedTags(new Set());
                  setDueFilter("all");
                  setPriorityFilter(new Set());
                  setOriginFilter(new Set());
                }}
                title="清掉全部 active filter（search / tag / due / priority / origin）"
              >
                ✕ 清除全部过滤
              </button>
            )}
            {!filtersActive && showFinished && (
              <button
                type="button"
                style={{
                  fontSize: 12,
                  padding: "6px 14px",
                  border: "1px solid var(--pet-color-accent)",
                  borderRadius: 6,
                  background: "var(--pet-color-card)",
                  color: "var(--pet-color-accent)",
                  cursor: "pointer",
                  fontWeight: 500,
                }}
                onClick={() => {
                  applyTaskTemplate(0);
                  setQuickAddOpen(true);
                }}
                title="点击打开新建表单，用一个具体任务范例预填"
              >
                📋 用范例预填一条
              </button>
            )}
          </EmptyState>
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
            // 拖拽 priority 改：仅 priority sort 模式启用。其他 sort 下拖
            // 卡片"位置 → priority"映射不直观（按 due 排时拖卡片改 priority
            // 会让卡片自己跳走）。终态任务也拖 —— 改 P 对它们仅是排序展示
            // 维度，对未来 retry 后排队也有意义。
            const dragEnabled = sortMode === "priority";
            const isDragSource = dragSourceTitle === t.title;
            const isDragOverTarget =
              dragOverTitle === t.title && dragSourceTitle && dragSourceTitle !== t.title;
            const taskCard = (
              <div
                data-task-idx={idx}
                className="pet-task-card"
                draggable={dragEnabled}
                onDragStart={(e) => {
                  if (!dragEnabled) return;
                  setDragSourceTitle(t.title);
                  e.dataTransfer.effectAllowed = "move";
                  // 写一份 plaintext 让 DnD API 满意（部分浏览器要求非空）；
                  // 接收端不读，靠 React state 拿 source。
                  try {
                    e.dataTransfer.setData("text/plain", t.title);
                  } catch {
                    // Tauri WKWebView 个别版本 setData 抛；可忽略 — state 仍生效
                  }
                }}
                onDragEnd={() => {
                  setDragSourceTitle(null);
                  setDragOverTitle(null);
                }}
                onDragOver={(e) => {
                  if (!dragEnabled || !dragSourceTitle || dragSourceTitle === t.title)
                    return;
                  e.preventDefault();
                  e.dataTransfer.dropEffect = "move";
                  if (dragOverTitle !== t.title) setDragOverTitle(t.title);
                }}
                onDragLeave={() => {
                  // 仅当真离开这条 target 才清；onDragOver 几乎每 frame 重写，
                  // 不清也不会 stale，但 leave 时显式收尾让 hover 不残留。
                  setDragOverTitle((cur) => (cur === t.title ? null : cur));
                }}
                onDrop={(e) => {
                  if (!dragEnabled) return;
                  e.preventDefault();
                  const src = dragSourceTitle;
                  setDragSourceTitle(null);
                  setDragOverTitle(null);
                  if (src) void handleDragDropPriority(src, t.title);
                }}
                onContextMenu={(e) => {
                  e.preventDefault();
                  e.stopPropagation();
                  setTaskCtxMenu({
                    title: t.title,
                    status: t.status,
                    priority: t.priority,
                    x: e.clientX,
                    y: e.clientY,
                    prioritySubmenu: false,
                  });
                }}
                onMouseEnter={() => startTaskPreviewHover(t.title, t.detail_path)}
                onMouseLeave={endTaskPreviewHover}
                style={{
                  ...s.item,
                  position: "relative",
                  ...(focused
                    ? {
                        outline: "2px solid #93c5fd",
                        outlineOffset: "-2px",
                      }
                    : {}),
                  ...(dragEnabled ? { cursor: "grab" } : {}),
                  ...(isDragSource ? { opacity: 0.4 } : {}),
                  ...(isDragOverTarget
                    ? {
                        outline: "2px dashed var(--pet-color-accent)",
                        outlineOffset: "-2px",
                        background: "var(--pet-tint-blue-bg)",
                      }
                    : {}),
                }}
              >
                {!expanded &&
                  taskPreviewHoverTitle === t.title &&
                  detailMap[t.title] &&
                  (() => {
                    const pd = detailMap[t.title];
                    const recentHistory = pd.history.slice(-3).reverse();
                    const detailSnippet =
                      pd.detail_md.length > 600
                        ? pd.detail_md.slice(0, 600) + "…"
                        : pd.detail_md;
                    const dueDisplay = t.due
                      ? t.due.slice(0, 16).replace("T", " ")
                      : null;
                    // ⚡ NOW 倒计时：marked 状态下显剩余秒（60s 内）。读取
                    // markedAt ref 一次（hover 时单次计算，不每秒更新），
                    // 数字粗略足够；用户回头 hover 时会取到新值。
                    const isNowMarked = nowMarkedTitles.has(t.title);
                    const nowRemainingSec = (() => {
                      if (!isNowMarked) return null;
                      const markedAt = nowMarkedAtRef.current.get(t.title);
                      if (markedAt === undefined) return null;
                      const elapsed = (Date.now() - markedAt) / 1000;
                      const remain = Math.max(0, Math.ceil(60 - elapsed));
                      return remain;
                    })();
                    const hasChips =
                      isNowMarked ||
                      t.priority !== 3 ||
                      dueDisplay !== null ||
                      t.tags.length > 0;
                    // 全空（无 chips / 无 history / 无 detail）就不浮 tooltip。
                    // priority === 3（默认值）单独不算"信息" —— 与新建表单
                    // default 一致，无信号价值。chips 行只有非默认 priority /
                    // 有 due / 有 tags 任一时才触发。
                    if (
                      !hasChips &&
                      recentHistory.length === 0 &&
                      detailSnippet.length === 0
                    ) {
                      return null;
                    }
                    return (
                      <div
                        style={{
                          position: "absolute",
                          top: "100%",
                          left: 0,
                          right: 0,
                          marginTop: 4,
                          maxHeight: 280,
                          overflowY: "auto",
                          background: "var(--pet-color-card)",
                          border: "1px solid var(--pet-color-border)",
                          borderRadius: 6,
                          boxShadow: "0 4px 12px rgba(0,0,0,0.18)",
                          padding: "8px 10px",
                          fontSize: 11,
                          color: "var(--pet-color-fg)",
                          lineHeight: 1.5,
                          wordBreak: "break-word",
                          zIndex: 20,
                          pointerEvents: "none",
                          fontFamily: "'SF Mono', 'Menlo', monospace",
                        }}
                      >
                        {/* 元数据 chips 行：仅显非默认值。priority=3 (default)
                            不渲染；有 due / 任意 tags 才渲对应 chip。让 hover
                            一眼看到"优先级 / 截止 / 标签"三大维度，不用展开。 */}
                        {hasChips && (
                          <div
                            style={{
                              display: "flex",
                              flexWrap: "wrap",
                              gap: 4,
                              marginBottom: 6,
                              paddingBottom: 6,
                              borderBottom:
                                recentHistory.length > 0 || detailSnippet.length > 0
                                  ? "1px dashed var(--pet-color-border)"
                                  : "none",
                            }}
                          >
                            {nowRemainingSec !== null && (
                              <span
                                style={{
                                  fontSize: 10,
                                  padding: "1px 6px",
                                  borderRadius: 3,
                                  background: "var(--pet-tint-orange-bg)",
                                  color: "var(--pet-tint-orange-fg)",
                                  fontWeight: 600,
                                  fontFamily: "inherit",
                                }}
                                title={`⚡ NOW 标记还有 ${nowRemainingSec}s 自动消失（标记时 pet 已收到桌面 nudge）`}
                              >
                                ⚡ NOW {nowRemainingSec}s
                              </span>
                            )}
                            {t.priority !== 3 && (
                              <span
                                style={{
                                  fontSize: 10,
                                  padding: "1px 6px",
                                  borderRadius: 3,
                                  background: "var(--pet-color-bg)",
                                  color: "var(--pet-color-fg)",
                                  fontWeight: 600,
                                  fontFamily: "inherit",
                                }}
                              >
                                🎯 P{t.priority}
                              </span>
                            )}
                            {dueDisplay && (
                              <span
                                style={{
                                  fontSize: 10,
                                  padding: "1px 6px",
                                  borderRadius: 3,
                                  background: "var(--pet-color-bg)",
                                  color: "var(--pet-color-fg)",
                                  fontFamily: "inherit",
                                }}
                              >
                                📅 {dueDisplay}
                              </span>
                            )}
                            {t.tags.map((tg) => (
                              <span
                                key={tg}
                                style={{
                                  fontSize: 10,
                                  padding: "1px 6px",
                                  borderRadius: 3,
                                  background: "var(--pet-color-bg)",
                                  color: "var(--pet-color-fg)",
                                  fontFamily: "inherit",
                                }}
                              >
                                #{tg}
                              </span>
                            ))}
                          </div>
                        )}
                        {recentHistory.length > 0 && (
                          <>
                            <div
                              style={{
                                fontSize: 10,
                                color: "var(--pet-color-muted)",
                                marginBottom: 4,
                              }}
                            >
                              🕒 最近 {recentHistory.length} 条事件
                            </div>
                            {recentHistory.map((ev) => (
                              <div
                                key={`${ev.timestamp}-${ev.action}`}
                                style={{
                                  display: "flex",
                                  gap: 6,
                                  alignItems: "baseline",
                                  marginBottom: 2,
                                }}
                              >
                                <span
                                  style={{
                                    color: "var(--pet-color-muted)",
                                    fontSize: 10,
                                    flexShrink: 0,
                                  }}
                                >
                                  {ev.timestamp.slice(0, 16).replace("T", " ")}
                                </span>
                                <span
                                  style={{
                                    fontSize: 10,
                                    color: "var(--pet-color-accent)",
                                    flexShrink: 0,
                                  }}
                                >
                                  {ev.action}
                                </span>
                                <span
                                  style={{
                                    flex: 1,
                                    overflow: "hidden",
                                    textOverflow: "ellipsis",
                                    whiteSpace: "nowrap",
                                  }}
                                >
                                  {ev.snippet || "（无）"}
                                </span>
                              </div>
                            ))}
                          </>
                        )}
                        {detailSnippet.length > 0 && (
                          <>
                            <div
                              style={{
                                fontSize: 10,
                                color: "var(--pet-color-muted)",
                                marginTop: recentHistory.length > 0 ? 8 : 0,
                                marginBottom: 4,
                                paddingTop: recentHistory.length > 0 ? 6 : 0,
                                borderTop:
                                  recentHistory.length > 0
                                    ? "1px dashed var(--pet-color-border)"
                                    : "none",
                              }}
                            >
                              📄 {t.detail_path}
                            </div>
                            <div style={{ whiteSpace: "pre-wrap" }}>
                              {detailSnippet}
                            </div>
                          </>
                        )}
                      </div>
                    );
                  })()}
                <div
                  style={{ ...s.itemHeader, ...s.headerClickable }}
                  onClick={() => handleToggleExpand(t.title)}
                  title={
                    // 把 raw_description 拼在 tooltip 后面（点击 hint + 原始
                    // 描述），让用户 hover 行就能看 [done] / [error: ...] /
                    // [origin:...] / [result:...] / #tag 等 marker，不必展开
                    // detail tab。raw 可能长，控制 max 400 字符避免 tooltip
                    // 撑爆屏幕（OS 自身也会截断）。
                    `${expanded ? "点击折叠详情" : "点击展开任务详情（描述 / 进度笔记 / 事件时间线）"}\n\n原始 description：\n${
                      t.raw_description.length > 400
                        ? t.raw_description.slice(0, 400) + "…"
                        : t.raw_description
                    }`
                  }
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
                    {renamingTaskTitle === t.title ? (
                      <input
                        autoFocus
                        type="text"
                        value={renameTaskDraft}
                        disabled={renamingTaskBusy}
                        onChange={(e) => setRenameTaskDraft(e.target.value)}
                        onClick={(e) => e.stopPropagation()}
                        onMouseDown={(e) => e.stopPropagation()}
                        onKeyDown={(e) => {
                          if (e.key === "Enter") {
                            e.preventDefault();
                            void commitRenameTask();
                          } else if (e.key === "Escape") {
                            e.preventDefault();
                            cancelRenameTask();
                          }
                        }}
                        onBlur={() => {
                          // 失焦 = 提交（与 PanelChat session rename 同模式）；
                          // 空 / 同名走 commit 内部的 noop 分支
                          void commitRenameTask();
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
                          flex: 1,
                          fontFamily: "inherit",
                        }}
                      />
                    ) : (
                      <span
                        onDoubleClick={(e) => {
                          e.stopPropagation();
                          setRenamingTaskTitle(t.title);
                          setRenameTaskDraft(t.title);
                        }}
                        title="双击改名"
                        style={{ cursor: "text" }}
                      >
                        {/* title 内 inline #tag 高亮：split by 正则 `#word`，
                            非 tag 段照常走 HighlightedText 保 search 高亮；
                            tag 段叠 getTagTintStyle 配色（与 body chip 行的
                            同 tag 一致）+ 轻量 chip 样式。 */}
                        {(() => {
                          const re = /#([\p{L}\p{N}_-]+)/gu;
                          const parts: React.ReactNode[] = [];
                          let lastIdx = 0;
                          let m: RegExpExecArray | null;
                          let key = 0;
                          while ((m = re.exec(t.title)) !== null) {
                            if (m.index > lastIdx) {
                              parts.push(
                                <HighlightedText
                                  key={key++}
                                  text={t.title.slice(lastIdx, m.index)}
                                  query={search}
                                />,
                              );
                            }
                            const tagName = m[1];
                            const tintStyle = getTagTintStyle(tagName);
                            const hasColor =
                              tintStyle.background !== undefined;
                            parts.push(
                              <span
                                key={key++}
                                style={{
                                  ...(hasColor
                                    ? tintStyle
                                    : {
                                        background:
                                          "var(--pet-tint-blue-bg)",
                                        color:
                                          "var(--pet-tint-blue-fg)",
                                      }),
                                  padding: "0 5px",
                                  borderRadius: 3,
                                  fontWeight: 600,
                                  fontSize: "0.92em",
                                }}
                                title={`#${tagName}（右键 tag chip 行可改色）`}
                              >
                                #{tagName}
                              </span>,
                            );
                            lastIdx = re.lastIndex;
                          }
                          if (lastIdx < t.title.length) {
                            parts.push(
                              <HighlightedText
                                key={key++}
                                text={t.title.slice(lastIdx)}
                                query={search}
                              />,
                            );
                          }
                          // 无 # 命中时直接走原路径，确保 0-tag title 行为
                          // 与之前完全一致（无副作用）
                          if (parts.length === 0) {
                            return <HighlightedText text={t.title} query={search} />;
                          }
                          return parts;
                        })()}
                      </span>
                    )}
                    {isRecentlyUpdated(t.updated_at, nowMs) && (
                      <span
                        title={formatRecentlyUpdatedHint(t.updated_at, nowMs)}
                        style={{
                          color: "var(--pet-tint-green-fg)",
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
                    {/* ⚡ NOW 标记：浮顶 + 桌面 nudge 60s 内有效，过期自动消失。 */}
                    {nowMarkedTitles.has(t.title) && (
                      <span
                        title="标记为 NOW：60s 内浮顶 + 桌面气泡 nudge；过期自动消失"
                        style={{
                          fontSize: 10,
                          fontWeight: 700,
                          marginLeft: 6,
                          padding: "1px 6px",
                          borderRadius: 4,
                          background: "var(--pet-tint-orange-bg)",
                          color: "var(--pet-tint-orange-fg)",
                          border: "1px solid var(--pet-tint-orange-fg)",
                          lineHeight: 1.2,
                          verticalAlign: "middle",
                          letterSpacing: 0.5,
                          animation: "pet-task-now-pulse 1.6s ease-in-out infinite",
                        }}
                        aria-label="marked as now"
                      >
                        ⚡ NOW
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
                  <div style={{ display: "flex", gap: 6, position: "relative" }}>
                    <button
                      type="button"
                      onMouseDown={(e) => e.stopPropagation()}
                      onClick={(e) => {
                        e.stopPropagation();
                        setPriorityPickerTitle((cur) =>
                          cur === t.title ? null : t.title,
                        );
                      }}
                      onContextMenu={(e) => {
                        // 右键开 priority picker（与左键同效果），抢在任务
                        // 卡 onContextMenu 之前阻止默认 + 冒泡。鼠标用户右
                        // 键改优先级更顺手。
                        e.preventDefault();
                        e.stopPropagation();
                        setPriorityPickerTitle((cur) =>
                          cur === t.title ? null : t.title,
                        );
                      }}
                      style={{
                        ...s.priBadge(t.priority),
                        border: "none",
                        cursor: "pointer",
                        fontFamily: "inherit",
                      }}
                      title={`点击 / 右键改 priority（P0..P9）\n\n数字含义：\n  P0 = 最重要 / 紧急（队列优先做）\n  P3 = 默认（无特别标注）\n  P9 = 最不重要 / 长期 idea 抽屉\n\n当前：P${t.priority}`}
                    >
                      P{t.priority}
                    </button>
                    {priorityPickerTitle === t.title && (
                      <div
                        onMouseDown={(e) => e.stopPropagation()}
                        onClick={(e) => e.stopPropagation()}
                        style={{
                          position: "absolute",
                          top: "calc(100% + 4px)",
                          right: 0,
                          background: "var(--pet-color-card)",
                          border: "1px solid var(--pet-color-border)",
                          borderRadius: 6,
                          boxShadow: "0 2px 8px rgba(0,0,0,0.15)",
                          padding: 4,
                          display: "grid",
                          gridTemplateColumns: "repeat(5, 1fr)",
                          gap: 2,
                          zIndex: 20,
                          minWidth: 140,
                        }}
                      >
                        {Array.from({ length: PRIORITY_MAX + 1 }, (_, p) => {
                          const active = p === t.priority;
                          return (
                            <button
                              key={p}
                              type="button"
                              onClick={() => void handleInlineSetPriority(t.title, p)}
                              style={{
                                padding: "3px 6px",
                                fontSize: 11,
                                border: "none",
                                borderRadius: 3,
                                background: active
                                  ? "var(--pet-tint-blue-bg)"
                                  : "transparent",
                                color: active
                                  ? "var(--pet-tint-blue-fg)"
                                  : "var(--pet-color-fg)",
                                cursor: active ? "default" : "pointer",
                                fontWeight: active ? 600 : 400,
                                fontFamily: "inherit",
                              }}
                              onMouseOver={(e) => {
                                if (!active) {
                                  (e.currentTarget as HTMLButtonElement).style.background =
                                    "var(--pet-color-bg)";
                                }
                              }}
                              onMouseOut={(e) => {
                                if (!active) {
                                  (e.currentTarget as HTMLButtonElement).style.background =
                                    "transparent";
                                }
                              }}
                            >
                              P{p}
                            </button>
                          );
                        })}
                      </div>
                    )}
                    {t.status === "pending" ? (
                      <div style={{ position: "relative" }}>
                        <button
                          type="button"
                          onMouseDown={(e) => e.stopPropagation()}
                          onClick={(e) => {
                            e.stopPropagation();
                            setStatusPickerTitle((cur) =>
                              cur === t.title ? null : t.title,
                            );
                          }}
                          style={{
                            ...s.badge(t.status),
                            border: "none",
                            cursor: "pointer",
                            fontFamily: "inherit",
                          }}
                          title="点击改状态（✓ 标 done / ✗ 取消）"
                        >
                          {STATUS_BADGE[t.status].label}
                        </button>
                        {statusPickerTitle === t.title && (
                          <div
                            onMouseDown={(e) => e.stopPropagation()}
                            onClick={(e) => e.stopPropagation()}
                            style={{
                              position: "absolute",
                              top: "calc(100% + 4px)",
                              right: 0,
                              background: "var(--pet-color-card)",
                              border: "1px solid var(--pet-color-border)",
                              borderRadius: 6,
                              boxShadow: "0 2px 8px rgba(0,0,0,0.15)",
                              padding: 4,
                              display: "flex",
                              flexDirection: "column",
                              gap: 2,
                              zIndex: 20,
                              minWidth: 120,
                            }}
                          >
                            <button
                              type="button"
                              onClick={() => {
                                setStatusPickerTitle(null);
                                openMarkDoneDialog(t.title);
                              }}
                              style={{
                                padding: "4px 8px",
                                fontSize: 11,
                                border: "none",
                                borderRadius: 4,
                                background: "transparent",
                                color: "var(--pet-tint-green-fg)",
                                cursor: "pointer",
                                textAlign: "left",
                                fontFamily: "inherit",
                              }}
                              onMouseOver={(e) => {
                                (e.currentTarget as HTMLButtonElement).style.background =
                                  "var(--pet-tint-green-bg)";
                              }}
                              onMouseOut={(e) => {
                                (e.currentTarget as HTMLButtonElement).style.background =
                                  "transparent";
                              }}
                            >
                              ✓ 标 done
                            </button>
                            <button
                              type="button"
                              onClick={() => {
                                setStatusPickerTitle(null);
                                // 打开 cancel reason 输入条；handleCancelOpen 已
                                // 维护 cancelOpen / cancelReason state，下方 actionRow
                                // 会渲染对应输入。
                                handleCancelOpen(t.title);
                              }}
                              style={{
                                padding: "4px 8px",
                                fontSize: 11,
                                border: "none",
                                borderRadius: 4,
                                background: "transparent",
                                color: "var(--pet-tint-red-fg)",
                                cursor: "pointer",
                                textAlign: "left",
                                fontFamily: "inherit",
                              }}
                              onMouseOver={(e) => {
                                (e.currentTarget as HTMLButtonElement).style.background =
                                  "var(--pet-tint-red-bg)";
                              }}
                              onMouseOut={(e) => {
                                (e.currentTarget as HTMLButtonElement).style.background =
                                  "transparent";
                              }}
                            >
                              ✗ 取消…
                            </button>
                          </div>
                        )}
                      </div>
                    ) : (
                      <span style={s.badge(t.status)}>{STATUS_BADGE[t.status].label}</span>
                    )}
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
                        style={{ ...s.tagChip, ...getTagTintStyle(tag) }}
                        onClick={(e) => {
                          // task card 本身有 onClick 展开详情；阻冒泡防止
                          // 点 tag 也展开详情。
                          e.stopPropagation();
                          toggleTag(tag);
                        }}
                        onContextMenu={(e) => {
                          e.preventDefault();
                          e.stopPropagation();
                          setTagColorPicker({ tag, x: e.clientX, y: e.clientY });
                        }}
                        title={`${selectedTags.has(tag) ? "点击取消该 tag 筛选" : "点击只看带此 tag 的任务"} · 右键改颜色`}
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
                    const isUrgent = urgency !== "normal";
                    return (
                      <span
                        style={{
                          color: dueColor(urgency),
                          fontWeight: isUrgent ? 600 : undefined,
                          background: dueBg(urgency),
                          padding: isUrgent ? "1px 6px" : undefined,
                          borderRadius: isUrgent ? 999 : undefined,
                        }}
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
                  {/* 更新于 X · Y 前 [· N 次更新]：与"创建于"对称展示活跃
                      度。updated_at 与 created_at 同 → 任务建后没动过，省
                      此 span 避免重复噪声。N 次更新依赖 detailMap[title] 已
                      经被 hover preview / expand 加载 —— 没加载就只显时间，
                      graceful degrade。 */}
                  {t.updated_at && t.updated_at !== t.created_at && (
                    <span>
                      更新于 {t.updated_at.slice(0, 16).replace("T", " ")}
                      {(() => {
                        const rel = formatRelativeAge(t.updated_at, nowMs);
                        return rel ? ` · ${rel}` : null;
                      })()}
                      {(() => {
                        const pd = detailMap[t.title];
                        if (!pd || pd.history.length === 0) return null;
                        return ` · ${pd.history.length} 次更新`;
                      })()}
                    </span>
                  )}
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
                      list="pet-tasks-cancel-reason-history"
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
                      <LoadingState inline />
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
                            color: "var(--pet-color-muted)",
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
                                color: "var(--pet-color-border)",
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
                                  background: "var(--pet-color-card)",
                                  color: copied ? "var(--pet-tint-green-fg)" : "var(--pet-color-muted)",
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
                                    background: "var(--pet-color-card)",
                                    color: copied ? "var(--pet-tint-green-fg)" : "var(--pet-color-muted)",
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
                          {/* > 300 字折叠：raw_description 含 [task pri=...] /
                              [origin:...] / 多条 [result: ...] / [done] 等 marker
                              + 用户写的描述，LLM 长跑后这段常超过千字。默认折
                              到 300 字让卡片视觉不被轰炸；展开按钮单独占一行
                              让 hit zone 明显。≤ 300 字时按钮不浮，与其他短任
                              务的视觉保持一致。 */}
                          {(() => {
                            const raw = detail.raw_description || "";
                            const LIMIT = 300;
                            const isLong = raw.length > LIMIT;
                            const expandedRaw = expandedRawDescTitles.has(t.title);
                            const shown =
                              !isLong || expandedRaw ? raw : raw.slice(0, LIMIT) + "…";
                            return (
                              <>
                                <div
                                  style={{
                                    ...s.rawDescBox,
                                    maxWidth: detailMaxWidthEffective,
                                  }}
                                >
                                  {shown || "（空）"}
                                </div>
                                {isLong && (
                                  <button
                                    type="button"
                                    onClick={() => toggleRawDescExpand(t.title)}
                                    style={{
                                      marginTop: 4,
                                      fontSize: 11,
                                      padding: 0,
                                      border: "none",
                                      background: "transparent",
                                      color: "var(--pet-color-accent)",
                                      cursor: "pointer",
                                      fontFamily: "inherit",
                                    }}
                                    title={
                                      expandedRaw
                                        ? "折叠回前 300 字"
                                        : `展开剩余 ${raw.length - LIMIT} 字`
                                    }
                                  >
                                    {expandedRaw
                                      ? `收起 (${raw.length})`
                                      : `… 展开剩余 ${raw.length - LIMIT} 字`}
                                  </button>
                                )}
                              </>
                            );
                          })()}
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
                                  background: "var(--pet-color-card)",
                                  color: "var(--pet-color-muted)",
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
                                    background: "var(--pet-color-card)",
                                    color: copied ? "var(--pet-tint-green-fg)" : "var(--pet-color-muted)",
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
                                  background: "var(--pet-color-card)",
                                  color: "var(--pet-color-muted)",
                                  cursor: "pointer",
                                }}
                                onClick={() => handleEnterEditDetail(t.title, detail.detail_md)}
                                title="编辑 detail.md（保存后覆盖文件；下次 LLM 也会读到你的版本）"
                              >
                                编辑
                              </button>
                            )}
                            {/* 阅读态字数 counter。与编辑态 counter（R121 实
                                现）同 Array.from(...).length 方法（Unicode code
                                point，对中文 / emoji 正确）；> 2000 字给红色提
                                醒"长文，复盘 / 分享前可能要精简"。 */}
                            {editingDetailTitle !== t.title && detail.detail_md.trim() && (() => {
                              const count = Array.from(detail.detail_md).length;
                              const longish = count > 2000;
                              return (
                                <span
                                  style={{
                                    fontSize: 10,
                                    color: longish
                                      ? "var(--pet-tint-red-fg)"
                                      : "var(--pet-color-muted)",
                                    fontFamily: "'SF Mono', 'Menlo', monospace",
                                    marginLeft: "auto",
                                  }}
                                  title={
                                    longish
                                      ? "进度笔记超过 2000 字，建议精简（按 Unicode code point 计；含空白）"
                                      : "进度笔记字数（按 Unicode code point 计；含空白）"
                                  }
                                >
                                  {count} 字
                                </span>
                              );
                            })()}
                          </div>
                          {editingDetailTitle === t.title ? (
                            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                              {/* R117: edit / split / preview 三态切换。preview
                                  用既有 parseMarkdown render；split 左 textarea
                                  / 右 preview 并排。textarea state 三态共享，
                                  切换不丢未保存内容。
                                  R121: row 末附字数 counter（无上限阈值，纯信息）。 */}
                              <div style={{ display: "flex", gap: 4, alignItems: "center" }}>
                                {(
                                  [
                                    { key: "edit" as const, label: "✏️ 编辑" },
                                    { key: "split" as const, label: "🔀 分屏" },
                                    { key: "preview" as const, label: "👁 预览" },
                                  ]
                                ).map(({ key, label }) => {
                                  const active = detailViewMode === key;
                                  return (
                                    <button
                                      key={key}
                                      type="button"
                                      onClick={() => setDetailViewMode(key)}
                                      style={{
                                        fontSize: 11,
                                        padding: "2px 8px",
                                        border: "1px solid",
                                        borderColor: active ? "var(--pet-color-accent)" : "var(--pet-color-border)",
                                        borderRadius: 4,
                                        background: active ? "var(--pet-color-accent)" : "var(--pet-color-card)",
                                        color: active ? "#fff" : "var(--pet-color-muted)",
                                        cursor: active ? "default" : "pointer",
                                        fontWeight: active ? 600 : 400,
                                      }}
                                      title={
                                        key === "edit"
                                          ? "纯编辑（只看 textarea）"
                                          : key === "split"
                                            ? "左编辑 + 右预览（适合 panel 宽 600+ 写大段时实时看效果）"
                                            : "纯预览（只看渲染结果）"
                                      }
                                    >
                                      {label}
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
                                {(() => {
                                  // 编辑态 counter 三档配色：与阅读态 counter
                                  // （Array.from length；> 2000 amber）共一套
                                  // 阈值语义但更激进 —— edit 是 user 主动写，
                                  // > 5000 字进 red banner（下一行）。
                                  const editCount = Array.from(editingDetailContent).length;
                                  const longish = editCount > 2000;
                                  const danger = editCount > 5000;
                                  return (
                                    <span
                                      style={{
                                        marginLeft:
                                          editingDetailContent !==
                                          editingDetailOriginalRef.current
                                            ? undefined
                                            : "auto",
                                        fontSize: 10,
                                        color: danger
                                          ? "var(--pet-tint-red-fg)"
                                          : longish
                                            ? "var(--pet-tint-yellow-fg)"
                                            : "var(--pet-color-muted)",
                                        fontWeight: danger ? 600 : undefined,
                                        fontFamily: "'SF Mono', 'Menlo', monospace",
                                      }}
                                      title="当前笔记字符数（Unicode code points 计；含换行 / 空白）"
                                    >
                                      {editCount} 字
                                    </span>
                                  );
                                })()}
                              </div>
                              {/* > 5000 字阈值显眼 banner：detail.md 喂 LLM 时
                                  整篇会进 context（不 chunk），长文写多了 token
                                  膨胀 + LLM 抓重点变难。给用户一句话提醒精简，
                                  banner 是建议不是阻塞 —— 用户能继续写继续保存。 */}
                              {Array.from(editingDetailContent).length > 5000 && (
                                <div
                                  style={{
                                    fontSize: 11,
                                    padding: "6px 10px",
                                    background: "var(--pet-tint-orange-bg)",
                                    border: "1px solid var(--pet-tint-orange-fg)",
                                    color: "var(--pet-tint-orange-fg)",
                                    borderRadius: 4,
                                    lineHeight: 1.5,
                                    display: "flex",
                                    gap: 6,
                                    alignItems: "flex-start",
                                  }}
                                  title="detail.md 整篇被宠物每次执行任务时读入 prompt 上下文，过长会让模型抓不住重点 + 浪费 token；建议把已完成段落归档到 history / 拆成子任务。"
                                >
                                  <span style={{ flexShrink: 0 }}>⚠</span>
                                  <span>
                                    笔记已超 5000 字 —— detail.md 整篇会被宠物读进 prompt
                                    上下文，建议把已完成段落归档或拆子任务，让 LLM 抓重点更稳。
                                  </span>
                                </div>
                              )}
                              {/* markdown 工具栏：3 个常用快捷（粗体 / 列表 /
                                  链接）。preview 模式无 textarea，不渲染。
                                  hover title 含等价的手敲语法，让用户能学到
                                  快捷方式 + 直接键入也行。 */}
                              {detailViewMode !== "preview" && (
                                <div
                                  style={{
                                    display: "flex",
                                    gap: 4,
                                    marginBottom: 4,
                                  }}
                                >
                                  <button
                                    type="button"
                                    onClick={() =>
                                      insertMarkdownAtCursor("wrap", "**", "**")
                                    }
                                    title="加粗（**...**）。选中后点击包裹；无选区时光标落在 ** | ** 中间。"
                                    style={mdToolbarBtnStyle}
                                  >
                                    <strong>B</strong>
                                  </button>
                                  <button
                                    type="button"
                                    onClick={() =>
                                      insertMarkdownAtCursor("line-prefix", "- ", "")
                                    }
                                    title="无序列表（- ...）。每选中行的行首加 -。无选区时给当前行加。"
                                    style={mdToolbarBtnStyle}
                                  >
                                    •
                                  </button>
                                  <button
                                    type="button"
                                    onClick={() =>
                                      insertMarkdownAtCursor("wrap", "[", "](url)")
                                    }
                                    title="链接（[...](url)）。选中作链接文本；无选区时光标落在 [|] 让你先写文本。url 占位符提示填地址。"
                                    style={mdToolbarBtnStyle}
                                  >
                                    🔗
                                  </button>
                                </div>
                              )}
                              {/* edit / split / preview 三态渲染。split 用
                                  flex 行让 textarea 与 preview 各占一半，
                                  preview pane 复用同一 JSX 不重写。 */}
                              {detailViewMode === "split" ? (
                                <div
                                  style={{
                                    display: "flex",
                                    gap: 8,
                                    alignItems: "stretch",
                                  }}
                                >
                                  <div style={{ flex: 1, display: "flex" }}>
                              <textarea
                                ref={detailEditorRef}
                                value={editingDetailContent}
                                onChange={(e) => setEditingDetailContent(e.target.value)}
                                onPaste={(e) => {
                                  // 粘贴板里抓 image/* 文件 → preventDefault
                                  // 阻止默认（不会把 file 路径文本错粘进 textarea）
                                  // → 拼成 markdown image 行插光标位置。
                                  const items = e.clipboardData?.items;
                                  if (!items) return;
                                  const blobs: Blob[] = [];
                                  for (let i = 0; i < items.length; i++) {
                                    const it = items[i];
                                    if (
                                      it.kind === "file" &&
                                      it.type.startsWith("image/")
                                    ) {
                                      const f = it.getAsFile();
                                      if (f) blobs.push(f);
                                    }
                                  }
                                  if (blobs.length === 0) return;
                                  e.preventDefault();
                                  void insertImageBlobsIntoDetail(blobs);
                                }}
                                onDrop={(e) => {
                                  // 拖入 image 文件：与 ChatPanel 路径同算法。
                                  const types = Array.from(
                                    e.dataTransfer?.types ?? [],
                                  );
                                  if (!types.includes("Files")) return;
                                  const files = e.dataTransfer.files;
                                  if (!files || files.length === 0) return;
                                  const blobs: Blob[] = [];
                                  for (let i = 0; i < files.length; i++) {
                                    const f = files[i];
                                    if (f.type.startsWith("image/")) blobs.push(f);
                                  }
                                  if (blobs.length === 0) return;
                                  e.preventDefault();
                                  void insertImageBlobsIntoDetail(blobs);
                                }}
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
                                  minHeight: 120,
                                  padding: "12px 14px",
                                  fontSize: 12,
                                  fontFamily: "'SF Mono', 'Menlo', monospace",
                                  border: "1px solid var(--pet-color-border)",
                                  borderRadius: 8,
                                  resize: "vertical",
                                  boxSizing: "border-box",
                                  lineHeight: 1.65,
                                  color: "var(--pet-color-fg)",
                                  background: "var(--pet-color-card)",
                                  boxShadow: "var(--pet-shadow-sm)",
                                }}
                                autoFocus
                                disabled={savingDetail}
                              />
                                  </div>
                                  <div
                                    style={{
                                      flex: 1,
                                      minHeight: 120,
                                      padding: "12px 14px",
                                      fontSize: 12,
                                      lineHeight: 1.65,
                                      border: "1px dashed color-mix(in srgb, var(--pet-color-border) 80%, var(--pet-color-accent))",
                                      borderRadius: 8,
                                      boxSizing: "border-box",
                                      color: "var(--pet-color-fg)",
                                      background: "var(--pet-color-bg)",
                                      overflowY: "auto",
                                    }}
                                  >
                                    {editingDetailContent.trim() === "" ? (
                                      <span
                                        style={{
                                          color: "var(--pet-color-muted)",
                                          fontStyle: "italic",
                                        }}
                                      >
                                        （空 — 在左侧编辑写笔记）
                                      </span>
                                    ) : (
                                      parseMarkdown(editingDetailContent)
                                    )}
                                  </div>
                                </div>
                              ) : detailViewMode === "preview" ? (
                                <div
                                  style={{
                                    minHeight: 120,
                                    padding: "12px 14px",
                                    fontSize: 12,
                                    lineHeight: 1.65,
                                    border: "1px dashed color-mix(in srgb, var(--pet-color-border) 80%, var(--pet-color-accent))",
                                    borderRadius: 8,
                                    boxSizing: "border-box",
                                    color: "var(--pet-color-fg)",
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
                                ref={detailEditorRef}
                                value={editingDetailContent}
                                onChange={(e) => setEditingDetailContent(e.target.value)}
                                onPaste={(e) => {
                                  const items = e.clipboardData?.items;
                                  if (!items) return;
                                  const blobs: Blob[] = [];
                                  for (let i = 0; i < items.length; i++) {
                                    const it = items[i];
                                    if (
                                      it.kind === "file" &&
                                      it.type.startsWith("image/")
                                    ) {
                                      const f = it.getAsFile();
                                      if (f) blobs.push(f);
                                    }
                                  }
                                  if (blobs.length === 0) return;
                                  e.preventDefault();
                                  void insertImageBlobsIntoDetail(blobs);
                                }}
                                onDrop={(e) => {
                                  const types = Array.from(
                                    e.dataTransfer?.types ?? [],
                                  );
                                  if (!types.includes("Files")) return;
                                  const files = e.dataTransfer.files;
                                  if (!files || files.length === 0) return;
                                  const blobs: Blob[] = [];
                                  for (let i = 0; i < files.length; i++) {
                                    const f = files[i];
                                    if (f.type.startsWith("image/")) blobs.push(f);
                                  }
                                  if (blobs.length === 0) return;
                                  e.preventDefault();
                                  void insertImageBlobsIntoDetail(blobs);
                                }}
                                onKeyDown={(e) => {
                                  if (
                                    (e.metaKey || e.ctrlKey) &&
                                    e.key.toLowerCase() === "s"
                                  ) {
                                    e.preventDefault();
                                    if (savingDetail) return;
                                    handleSaveDetail(t.title);
                                    return;
                                  }
                                  if (e.key === "Escape") {
                                    e.preventDefault();
                                    handleCancelEditDetail();
                                  }
                                }}
                                placeholder="在这里追加 / 修改进度笔记…保存后覆盖 detail.md。（⌘S 保存 / Esc 取消）"
                                style={{
                                  width: "100%",
                                  minHeight: 120,
                                  padding: "12px 14px",
                                  fontSize: 12,
                                  fontFamily: "'SF Mono', 'Menlo', monospace",
                                  border: "1px solid var(--pet-color-border)",
                                  borderRadius: 8,
                                  resize: "vertical",
                                  boxSizing: "border-box",
                                  lineHeight: 1.65,
                                  color: "var(--pet-color-fg)",
                                  background: "var(--pet-color-card)",
                                  boxShadow: "var(--pet-shadow-sm)",
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
                                        color: overLong ? "var(--pet-tint-yellow-fg)" : "var(--pet-color-muted)",
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
                                ? parseDetailMdWithImages(
                                    detail.detail_md,
                                    setDetailLightboxSrc,
                                  )
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
                                    background: "var(--pet-color-card)",
                                    color: copied ? "var(--pet-tint-green-fg)" : "var(--pet-color-muted)",
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
      )}
      {taskViewTab === "archive" && (
      <div style={s.section}>
        {/* 归档查看（只读）：tab 切到归档时强制展开；不参与队列的过滤 /
            排序 / 操作 —— 是独立的回看视图。 */}
        <div style={{ marginTop: 0 }}>
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: 8,
              cursor: "pointer",
              userSelect: "none",
            }}
            onClick={async () => {
              const next = !archiveExpanded;
              setArchiveExpanded(next);
              if (next && !archiveLoaded) {
                await reloadArchive();
              }
            }}
            title={archiveExpanded ? "点击折叠归档列表" : "点击展开 task_archive（consolidate 自动归档的老任务）"}
          >
            <span style={{ width: 10, fontFamily: "monospace", color: "var(--pet-color-muted)" }}>
              {archiveExpanded ? "▾" : "▸"}
            </span>
            <span style={{ fontSize: 13, fontWeight: 600, color: "var(--pet-color-fg)" }}>
              📦 归档
            </span>
            <span style={{ fontSize: 11, color: "var(--pet-color-muted)" }}>
              {archiveLoaded ? `（${archiveItems.length} 条）` : "（点击加载）"}
            </span>
            {archiveLoaded && (
              <>
                <button
                  type="button"
                  onClick={(e) => {
                    e.stopPropagation();
                    void handleExportArchiveAsMd();
                  }}
                  disabled={archiveItems.length === 0}
                  style={{
                    marginLeft: "auto",
                    fontSize: 11,
                    padding: "2px 8px",
                    border: "1px solid var(--pet-color-border)",
                    borderRadius: 4,
                    background: "var(--pet-color-card)",
                    color: "var(--pet-color-muted)",
                    cursor: archiveItems.length === 0 ? "default" : "pointer",
                  }}
                  title={`把 ${archiveItems.length} 条归档按 YYYY-MM 分组拼成 markdown 写到剪贴板（带 [done] / [result] / #tag 等 marker）`}
                >
                  📋 导出 MD ({archiveItems.length})
                </button>
                <button
                  type="button"
                  onClick={(e) => {
                    e.stopPropagation();
                    void reloadArchive();
                  }}
                  disabled={archiveLoading}
                  style={{
                    fontSize: 11,
                    padding: "2px 8px",
                    border: "1px solid var(--pet-color-border)",
                    borderRadius: 4,
                    background: "var(--pet-color-card)",
                    color: "var(--pet-color-muted)",
                    cursor: archiveLoading ? "default" : "pointer",
                  }}
                  title="重新拉取归档列表"
                >
                  {archiveLoading ? "刷新中…" : "刷新"}
                </button>
              </>
            )}
          </div>
          {archiveError && (
            <div style={{ ...s.err, marginTop: 8 }}>{archiveError}</div>
          )}
          {archiveExpanded && (
            <div style={{ marginTop: 8 }}>
              {archiveLoading && !archiveLoaded ? (
                <LoadingState message="正在加载归档…" compact />
              ) : archiveItems.length === 0 ? (
                <EmptyState
                  icon="🗃"
                  title="归档为空"
                  hint="consolidate 会把 30 天前已结束的 butler_tasks 自动挪过来"
                  compact
                />
              ) : (
                archiveItems.map((it) => {
                  // title 形如 "2026-04-01_整理 downloads"；display 把日期前缀
                  // 单独亮出来。description 形如 "[archived: 2026-04-01] [task ...] 整理 [done] [result: 完成]"。
                  const m = it.title.match(/^(\d{4}-\d{2}-\d{2})_(.*)$/);
                  const archiveDate = m ? m[1] : "—";
                  const displayTitle = m ? m[2] : it.title;
                  return (
                    <div
                      key={it.title}
                      style={{
                        ...s.item,
                        padding: "8px 10px",
                        marginBottom: 6,
                        background: "var(--pet-color-card)",
                        opacity: 0.92,
                      }}
                    >
                      <div style={{ display: "flex", alignItems: "baseline", gap: 8 }}>
                        <span
                          style={{
                            fontSize: 10,
                            fontFamily: "'SF Mono', 'Menlo', monospace",
                            color: "var(--pet-color-muted)",
                            background: "var(--pet-color-bg)",
                            padding: "1px 5px",
                            borderRadius: 3,
                            flexShrink: 0,
                          }}
                          title="归档日期（来自 archive 时刻的 updated_at）"
                        >
                          {archiveDate}
                        </span>
                        <span style={{ fontSize: 12, fontWeight: 600, color: "var(--pet-color-fg)", wordBreak: "break-word" }}>
                          {displayTitle}
                        </span>
                      </div>
                      <div
                        style={{
                          fontSize: 11,
                          color: "var(--pet-color-muted)",
                          marginTop: 4,
                          lineHeight: 1.5,
                          whiteSpace: "pre-wrap",
                          wordBreak: "break-word",
                        }}
                      >
                        {it.description}
                      </div>
                    </div>
                  );
                })
              )}
            </div>
          )}
        </div>
      </div>
      )}
      <ImageLightbox
        src={detailLightboxSrc}
        onClose={() => setDetailLightboxSrc(null)}
      />
      {/* 手动标 done 时的"可选 result 摘要"对话框。键盘 d 路径不走此处
          （保留快捷键的零摩擦）；鼠标点击状态 picker / 右键菜单的"标 done"
          路径进入。Enter 提交（即使空 result 也走 done），Esc / backdrop 取消。 */}
      <Modal open={markDoneTitle !== null} onClose={closeMarkDoneDialog} maxWidth={460}>
        {markDoneTitle && (
          <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
            <div
              style={{
                fontSize: 13,
                fontWeight: 600,
                color: "var(--pet-color-fg)",
              }}
            >
              标记「{markDoneTitle}」为已完成
            </div>
            {/* recurring 警告：若该 task 的 raw_description 含 `[every:`，
                它是 butler_task 的循环 schedule。标 done 会让下一次 schedule
                被宠物视作"已完成"跳过。提示用户更合适的路径（cancel / 留
                给 LLM 自标）。带 [once: ...] / [deadline: ...] 则不警告 —
                一次性 schedule 本就该 done 收尾。 */}
            {(() => {
              const t = tasks.find((x) => x.title === markDoneTitle);
              if (!t) return null;
              if (!/\[every[:\s]/i.test(t.raw_description)) return null;
              return (
                <div
                  style={{
                    padding: "8px 10px",
                    fontSize: 11,
                    lineHeight: 1.5,
                    background: "var(--pet-tint-yellow-bg)",
                    border: "1px solid var(--pet-tint-yellow-fg)",
                    color: "var(--pet-tint-yellow-fg)",
                    borderRadius: 4,
                    display: "flex",
                    gap: 6,
                    alignItems: "flex-start",
                  }}
                >
                  <span style={{ flexShrink: 0 }}>⚠</span>
                  <span>
                    这是循环 schedule（含 [every: ...]）。标 done 之后宠物会
                    把它当 "已完成" 跳过下一周期。如果你想"今天这条不要再做"，
                    用"取消"更准确；想 retire 整条循环，请到「记忆」→ butler_tasks
                    删除或改 description。
                  </span>
                </div>
              );
            })()}
            <label style={{ fontSize: 11, color: "var(--pet-color-muted)" }}>
              可选：填一句产物 / 结果（写到 description 的 [result: ...] marker，
              与 LLM 自动标 done 时形态一致；留空则只写 [done]）
            </label>
            <textarea
              autoFocus
              value={markDoneResult}
              onChange={(e) => setMarkDoneResult(e.target.value)}
              onKeyDown={(e) => {
                // ⌘/Ctrl+Enter 或 Enter 都提交，让单行小段输入快进；Shift+Enter
                // 给真要换行的用户。Esc 取消。
                if (e.key === "Enter" && !e.shiftKey) {
                  e.preventDefault();
                  void confirmMarkDone();
                } else if (e.key === "Escape") {
                  e.preventDefault();
                  closeMarkDoneDialog();
                }
              }}
              placeholder="比如：38 个文件已挪到 ~/Archive/2026-04 / 2026-05"
              style={{
                padding: "8px 10px",
                fontSize: 12,
                fontFamily: "inherit",
                border: "1px solid var(--pet-color-border)",
                borderRadius: 6,
                background: "var(--pet-color-bg)",
                color: "var(--pet-color-fg)",
                resize: "vertical",
                minHeight: 60,
                boxSizing: "border-box",
              }}
            />
            <div style={{ display: "flex", gap: 8, justifyContent: "flex-end" }}>
              <button
                type="button"
                onClick={closeMarkDoneDialog}
                style={{
                  padding: "6px 12px",
                  fontSize: 12,
                  border: "1px solid var(--pet-color-border)",
                  borderRadius: 6,
                  background: "var(--pet-color-card)",
                  color: "var(--pet-color-fg)",
                  cursor: "pointer",
                }}
              >
                取消
              </button>
              <button
                type="button"
                onClick={() => void confirmMarkDone()}
                style={{
                  padding: "6px 12px",
                  fontSize: 12,
                  border: "none",
                  borderRadius: 6,
                  background: "var(--pet-tint-green-fg)",
                  color: "var(--pet-color-card)",
                  fontWeight: 600,
                  cursor: "pointer",
                }}
                title={
                  markDoneResult.trim()
                    ? "确认标 done 并附 [result: ...]"
                    : "确认标 done（不附 result）"
                }
              >
                ✓ 确认
              </button>
            </div>
          </div>
        )}
      </Modal>
      {/* ⌘N quick-add 全屏遮罩模态。复用既有 title / body / priority / due
          state，不复制状态机；handleCreate 成功后顺手 setQuickAddOpen(false)
          关闭。backdrop click / Esc 关闭（由 Modal 统一处理）。 */}
      <Modal open={quickAddOpen} onClose={() => setQuickAddOpen(false)} maxWidth={520}>
        {quickAddOpen && (
          <>
            <div
              style={{
                display: "flex",
                alignItems: "center",
                justifyContent: "space-between",
                marginBottom: 12,
                paddingBottom: 8,
                borderBottom: "1px solid var(--pet-color-border)",
              }}
            >
              <span
                style={{
                  fontSize: 14,
                  fontWeight: 600,
                  color: "var(--pet-color-fg)",
                  letterSpacing: 0.2,
                }}
              >
                ⚡ 快速委托
              </span>
              <button
                type="button"
                onClick={() => setQuickAddOpen(false)}
                style={{
                  border: "none",
                  background: "transparent",
                  color: "var(--pet-color-muted)",
                  cursor: "pointer",
                  fontSize: 14,
                  padding: "2px 6px",
                }}
                title="Esc 关闭"
                aria-label="关闭"
              >
                ✕
              </button>
            </div>
            <div
              style={{
                display: "flex",
                alignItems: "center",
                justifyContent: "space-between",
                marginBottom: 4,
              }}
            >
              <label style={s.label}>标题</label>
              <select
                value=""
                onChange={(e) => {
                  const v = e.target.value;
                  if (!v) return;
                  applyTaskTemplate(parseInt(v, 10));
                  e.currentTarget.value = "";
                }}
                title="选一个常见任务范例预填表单"
                style={{
                  padding: "2px 6px",
                  fontSize: 11,
                  border: "1px solid var(--pet-color-border)",
                  borderRadius: 4,
                  background: "var(--pet-color-card)",
                  color: "var(--pet-color-fg)",
                  cursor: "pointer",
                  fontFamily: "inherit",
                  maxWidth: 200,
                }}
              >
                <option value="">📋 从模板…</option>
                {TASK_TEMPLATES.map((tpl, i) => (
                  <option key={tpl.label} value={i}>
                    {tpl.label}
                  </option>
                ))}
              </select>
            </div>
            <input
              style={s.input}
              ref={titleInputRef}
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              onKeyDown={handleFormKeyDown}
              placeholder="比如：整理 Downloads"
              autoFocus
            />
            {/* 与 inline 创建表单同款 schedule 前缀检测 hint —— 见上面注释。
                quickAdd modal 与表单是两个入口同一 state，两处都补提示
                让用户不会因为切到 ⌘N modal 就丢失这条引导。 */}
            {(() => {
              const SCHEDULE_RE = /\[(every|once|deadline)[:\s]/i;
              const hits = SCHEDULE_RE.test(title) || SCHEDULE_RE.test(body);
              if (!hits) return null;
              return (
                <div
                  style={{
                    marginTop: 4,
                    padding: "6px 10px",
                    fontSize: 11,
                    lineHeight: 1.5,
                    background: "var(--pet-tint-yellow-bg)",
                    border: "1px solid var(--pet-tint-yellow-fg)",
                    color: "var(--pet-tint-yellow-fg)",
                    borderRadius: 4,
                    display: "flex",
                    gap: 6,
                    alignItems: "flex-start",
                  }}
                >
                  <span style={{ flexShrink: 0 }}>💡</span>
                  <span>
                    检测到 schedule 前缀 —— 想定时 / 周期执行？建议改在
                    「记忆」面板的 butler_tasks 段新建。
                  </span>
                </div>
              );
            })()}
            <label style={{ ...s.label, marginTop: 10 }}>描述（可选）</label>
            <textarea
              style={{ ...s.textarea, minHeight: 80 }}
              value={body}
              onChange={(e) => setBody(e.target.value)}
              onKeyDown={handleFormKeyDown}
              placeholder="把要点说清楚，比如：把 30 天前的文件挪到 ~/Archive/"
            />
            <div style={{ ...s.twoCol, marginTop: 10 }}>
              <div>
                <label style={s.label}>优先级 (0-{PRIORITY_MAX})</label>
                <div style={{ display: "flex", gap: 4 }}>
                  <input
                    type="number"
                    min={0}
                    max={PRIORITY_MAX}
                    style={{ ...s.input, flex: 1 }}
                    value={priority}
                    onChange={(e) => {
                      const n = parseInt(e.target.value, 10);
                      if (Number.isNaN(n)) return;
                      setPriority(Math.max(0, Math.min(PRIORITY_MAX, n)));
                    }}
                    onKeyDown={handleFormKeyDown}
                  />
                  <button
                    type="button"
                    onClick={() =>
                      setPriority((p) => Math.min(PRIORITY_MAX, p + 1))
                    }
                    disabled={priority >= PRIORITY_MAX}
                    title="优先级 +1"
                    style={{
                      padding: "0 8px",
                      fontSize: 10,
                      lineHeight: 1,
                      border: "1px solid var(--pet-color-border)",
                      borderRadius: 4,
                      background: "var(--pet-color-card)",
                      color: "var(--pet-color-muted)",
                      cursor: priority >= PRIORITY_MAX ? "default" : "pointer",
                      flexShrink: 0,
                    }}
                  >
                    ▲
                  </button>
                  <button
                    type="button"
                    onClick={() => setPriority((p) => Math.max(0, p - 1))}
                    disabled={priority <= 0}
                    title="优先级 -1"
                    style={{
                      padding: "0 8px",
                      fontSize: 10,
                      lineHeight: 1,
                      border: "1px solid var(--pet-color-border)",
                      borderRadius: 4,
                      background: "var(--pet-color-card)",
                      color: "var(--pet-color-muted)",
                      cursor: priority <= 0 ? "default" : "pointer",
                      flexShrink: 0,
                    }}
                  >
                    ▼
                  </button>
                </div>
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
            <div style={{ display: "flex", gap: 8, marginTop: 14, alignItems: "center" }}>
              <button
                style={creating || !title.trim() ? s.btnDisabled : s.btnPrimary}
                onClick={handleCreate}
                disabled={creating || !title.trim()}
                title="创建任务（⌘Enter / Ctrl+Enter 等价）"
              >
                {creating ? "创建中..." : "创建任务"}
              </button>
              <button
                type="button"
                onClick={() => setQuickAddOpen(false)}
                style={{
                  padding: "7px 14px",
                  border: "1px solid var(--pet-color-border)",
                  background: "var(--pet-color-bg)",
                  color: "var(--pet-color-muted)",
                  borderRadius: 6,
                  cursor: "pointer",
                  fontSize: 13,
                  fontFamily: "inherit",
                }}
              >
                取消
              </button>
              <span style={{ flex: 1 }} />
              <span
                style={{
                  fontSize: 11,
                  color: "var(--pet-color-muted)",
                  fontFamily: "'SF Mono', 'Menlo', monospace",
                }}
              >
                ⌘Enter 创建 · Esc 关闭
              </span>
            </div>
            {errMsg && <div style={{ ...s.err, marginTop: 10 }}>{errMsg}</div>}
          </>
        )}
      </Modal>

      {tagColorPicker && (() => {
        // 小调色板：7 个色样圆按钮（默认 + 6 色）。位置 fixed 用 viewport 坐标；
        // 右 / 下越界 clamp。当前 tag 已绑定色 → 该色按钮加白边 + ✓。
        const p = tagColorPicker;
        const W = 200;
        const H = 90;
        const left = Math.max(8, Math.min(p.x, window.innerWidth - W - 8));
        const top = Math.max(8, Math.min(p.y, window.innerHeight - H - 8));
        const curKey = tagColors[p.tag] ?? "default";
        return (
          <div
            onMouseDown={(e) => e.stopPropagation()}
            onClick={(e) => e.stopPropagation()}
            onContextMenu={(e) => {
              e.preventDefault();
              e.stopPropagation();
            }}
            style={{
              position: "fixed",
              left,
              top,
              width: W,
              padding: 6,
              background: "var(--pet-color-card)",
              border: "1px solid var(--pet-color-border)",
              borderRadius: 6,
              boxShadow: "0 4px 12px rgba(0,0,0,0.18)",
              zIndex: 50,
              fontFamily: "inherit",
            }}
          >
            <div
              style={{
                fontSize: 11,
                color: "var(--pet-color-muted)",
                padding: "2px 4px 6px",
                borderBottom: "1px solid var(--pet-color-border)",
                marginBottom: 6,
                whiteSpace: "nowrap",
                overflow: "hidden",
                textOverflow: "ellipsis",
              }}
              title={p.tag}
            >
              #{p.tag} 颜色
            </div>
            <div style={{ display: "flex", gap: 6, flexWrap: "wrap" }}>
              {TAG_COLOR_OPTIONS.map((opt) => {
                const active = curKey === opt.key;
                const swatchStyle: React.CSSProperties = opt.tint
                  ? {
                      background: `var(--pet-tint-${opt.tint}-bg)`,
                      color: `var(--pet-tint-${opt.tint}-fg)`,
                      border: active
                        ? `2px solid var(--pet-tint-${opt.tint}-fg)`
                        : "1px solid var(--pet-color-border)",
                    }
                  : {
                      background: "var(--pet-color-bg)",
                      color: "var(--pet-color-muted)",
                      border: active
                        ? "2px solid var(--pet-color-fg)"
                        : "1px dashed var(--pet-color-border)",
                    };
                return (
                  <button
                    key={opt.key}
                    type="button"
                    onClick={() => {
                      setTagColor(p.tag, opt.key);
                      setTagColorPicker(null);
                    }}
                    title={`${opt.label}（${active ? "当前" : "点击应用"}）`}
                    style={{
                      width: 24,
                      height: 24,
                      borderRadius: 12,
                      cursor: active ? "default" : "pointer",
                      padding: 0,
                      fontSize: 10,
                      lineHeight: 1,
                      fontFamily: "inherit",
                      ...swatchStyle,
                    }}
                  >
                    {active ? "✓" : opt.key === "default" ? "○" : ""}
                  </button>
                );
              })}
            </div>
          </div>
        );
      })()}
      {taskCtxMenu && (() => {
        // viewport 右 / 下越界时把菜单往回挪；menu 实际宽度 / 高度由内容定，
        // 这里用经验值 180 / 320 做夹紧足够（带 priority 子面板时纵向 +60）。
        const m = taskCtxMenu;
        const W = 180;
        const H = m.prioritySubmenu ? 360 : 300;
        const left = Math.max(8, Math.min(m.x, window.innerWidth - W - 8));
        const top = Math.max(8, Math.min(m.y, window.innerHeight - H - 8));
        const t = tasks.find((x) => x.title === m.title) ?? null;
        const canRetry = m.status === "error";
        const canMarkDone = m.status === "pending" || m.status === "error";
        const canCancel = m.status === "pending" || m.status === "error";
        const itemBtn: React.CSSProperties = {
          display: "block",
          width: "100%",
          textAlign: "left",
          padding: "6px 10px",
          fontSize: 12,
          lineHeight: 1.3,
          border: "none",
          background: "transparent",
          color: "var(--pet-color-fg)",
          cursor: "pointer",
          fontFamily: "inherit",
          borderRadius: 4,
        };
        const itemBtnHoverIn = (e: React.MouseEvent<HTMLButtonElement>) => {
          (e.currentTarget as HTMLButtonElement).style.background =
            "var(--pet-color-bg)";
        };
        const itemBtnHoverOut = (e: React.MouseEvent<HTMLButtonElement>) => {
          (e.currentTarget as HTMLButtonElement).style.background = "transparent";
        };
        const sep = (
          <div
            style={{
              height: 1,
              background: "var(--pet-color-border)",
              margin: "4px 0",
            }}
          />
        );
        return (
          <div
            onMouseDown={(e) => e.stopPropagation()}
            onClick={(e) => e.stopPropagation()}
            onContextMenu={(e) => {
              e.preventDefault();
              e.stopPropagation();
            }}
            style={{
              position: "fixed",
              left,
              top,
              width: W,
              background: "var(--pet-color-card)",
              border: "1px solid var(--pet-color-border)",
              borderRadius: 6,
              boxShadow: "0 4px 12px rgba(0,0,0,0.18)",
              padding: 4,
              zIndex: 50,
              fontFamily: "inherit",
            }}
          >
            <div
              style={{
                padding: "4px 10px 6px",
                fontSize: 11,
                color: "var(--pet-color-muted)",
                borderBottom: "1px solid var(--pet-color-border)",
                marginBottom: 4,
                whiteSpace: "nowrap",
                overflow: "hidden",
                textOverflow: "ellipsis",
              }}
              title={m.title}
            >
              {m.title}
            </div>
            <button
              type="button"
              style={itemBtn}
              onMouseOver={itemBtnHoverIn}
              onMouseOut={itemBtnHoverOut}
              onClick={() => {
                setTaskCtxMenu(null);
                handleToggleExpand(m.title);
              }}
            >
              📂 展开详情
            </button>
            {canMarkDone && (
              <button
                type="button"
                style={{ ...itemBtn, color: "var(--pet-tint-green-fg)" }}
                onMouseOver={itemBtnHoverIn}
                onMouseOut={itemBtnHoverOut}
                onClick={() => {
                  setTaskCtxMenu(null);
                  openMarkDoneDialog(m.title);
                }}
              >
                ✓ 标 done
              </button>
            )}
            {/* "⚡ 标 NOW"：仅 pending 行可点（done / error / cancelled 没意
                义）。已 mark 时按钮变"⚡ 续 60s"重置计时。 */}
            {m.status === "pending" && (
              <button
                type="button"
                style={{ ...itemBtn, color: "var(--pet-tint-orange-fg)" }}
                onMouseOver={itemBtnHoverIn}
                onMouseOut={itemBtnHoverOut}
                onClick={() => {
                  setTaskCtxMenu(null);
                  markTaskNow(m.title);
                }}
              >
                {nowMarkedTitles.has(m.title) ? "⚡ 续 60s" : "⚡ 标 NOW (60s 浮顶 + 桌面 nudge)"}
              </button>
            )}
            {/* due preset：覆盖 90% 的常见 due 场景 —— "今天下班前" /
                "明早第一件事"。绕开 datetime picker 鼠标用户更快。
                helper 在 onClick 内闭包计算当前日期，避免 stale state。
                成功后 reload 让队列重 sort。 */}
            {canMarkDone && [
              { label: "⏰ due 今日 18:00", hour: 18, minute: 0, dayOffset: 0 },
              { label: "⏰ due 明日 09:00", hour: 9, minute: 0, dayOffset: 1 },
            ].map((preset) => (
              <button
                key={preset.label}
                type="button"
                style={itemBtn}
                onMouseOver={itemBtnHoverIn}
                onMouseOut={itemBtnHoverOut}
                onClick={async () => {
                  setTaskCtxMenu(null);
                  const d = new Date();
                  d.setDate(d.getDate() + preset.dayOffset);
                  d.setHours(preset.hour, preset.minute, 0, 0);
                  const y = d.getFullYear();
                  const mo = String(d.getMonth() + 1).padStart(2, "0");
                  const da = String(d.getDate()).padStart(2, "0");
                  const hh = String(d.getHours()).padStart(2, "0");
                  const mm = String(d.getMinutes()).padStart(2, "0");
                  const due = `${y}-${mo}-${da}T${hh}:${mm}`;
                  setActionErr("");
                  setBusyTitle(m.title);
                  try {
                    await invoke<void>("task_set_due", { title: m.title, due });
                    await reload();
                  } catch (e) {
                    setActionErr(`设 due 失败：${e}`);
                  } finally {
                    setBusyTitle(null);
                  }
                }}
              >
                {preset.label}
              </button>
            ))}
            {canRetry && (
              <button
                type="button"
                style={{ ...itemBtn, color: "var(--pet-color-accent)" }}
                onMouseOver={itemBtnHoverIn}
                onMouseOut={itemBtnHoverOut}
                onClick={() => {
                  setTaskCtxMenu(null);
                  void handleRetry(m.title);
                }}
              >
                🔄 重试
              </button>
            )}
            {canCancel && (
              <button
                type="button"
                style={{ ...itemBtn, color: "var(--pet-tint-red-fg)" }}
                onMouseOver={itemBtnHoverIn}
                onMouseOut={itemBtnHoverOut}
                onClick={() => {
                  setTaskCtxMenu(null);
                  handleCancelOpen(m.title);
                }}
              >
                ✗ 取消…
              </button>
            )}
            {sep}
            <button
              type="button"
              style={itemBtn}
              onMouseOver={itemBtnHoverIn}
              onMouseOut={itemBtnHoverOut}
              onClick={() =>
                setTaskCtxMenu((cur) =>
                  cur ? { ...cur, prioritySubmenu: !cur.prioritySubmenu } : cur,
                )
              }
            >
              {m.prioritySubmenu ? "▾" : "▸"} 改 priority（当前 P{m.priority}）
            </button>
            {m.prioritySubmenu && (
              <div
                style={{
                  display: "grid",
                  gridTemplateColumns: "repeat(5, 1fr)",
                  gap: 2,
                  padding: "2px 4px 4px",
                }}
              >
                {Array.from({ length: PRIORITY_MAX + 1 }, (_, p) => {
                  const active = p === m.priority;
                  return (
                    <button
                      key={p}
                      type="button"
                      onClick={() => {
                        setTaskCtxMenu(null);
                        void handleInlineSetPriority(m.title, p);
                      }}
                      style={{
                        padding: "3px 0",
                        fontSize: 11,
                        border: "none",
                        borderRadius: 3,
                        background: active
                          ? "var(--pet-tint-blue-bg)"
                          : "transparent",
                        color: active
                          ? "var(--pet-tint-blue-fg)"
                          : "var(--pet-color-fg)",
                        cursor: active ? "default" : "pointer",
                        fontWeight: active ? 600 : 400,
                        fontFamily: "inherit",
                      }}
                      onMouseOver={(e) => {
                        if (!active) {
                          (e.currentTarget as HTMLButtonElement).style.background =
                            "var(--pet-color-bg)";
                        }
                      }}
                      onMouseOut={(e) => {
                        if (!active) {
                          (e.currentTarget as HTMLButtonElement).style.background =
                            "transparent";
                        }
                      }}
                    >
                      P{p}
                    </button>
                  );
                })}
              </div>
            )}
            {sep}
            <button
              type="button"
              style={itemBtn}
              onMouseOver={itemBtnHoverIn}
              onMouseOut={itemBtnHoverOut}
              onClick={async () => {
                setTaskCtxMenu(null);
                try {
                  await navigator.clipboard.writeText(m.title);
                  setBulkResultMsg(`已复制标题：${m.title}`);
                } catch (e) {
                  setBulkResultMsg(`复制失败：${e}`);
                }
                window.setTimeout(() => setBulkResultMsg(""), 3000);
              }}
            >
              📋 复制标题
            </button>
            {/* 复制为 ref token：用 `「title」` 全角直角引号包裹，与 ⌘K
                picker 插入格式一致 —— 用户粘到 chat 即被 hover preview /
                双击导航识别。短任务标题免敲全角 IME 是这条 entry 的核心 ergo。 */}
            <button
              type="button"
              style={itemBtn}
              onMouseOver={itemBtnHoverIn}
              onMouseOut={itemBtnHoverOut}
              onClick={async () => {
                setTaskCtxMenu(null);
                const refToken = `「${m.title}」`;
                try {
                  await navigator.clipboard.writeText(refToken);
                  setBulkResultMsg(`已复制 ref：${refToken}`);
                } catch (e) {
                  setBulkResultMsg(`复制失败：${e}`);
                }
                window.setTimeout(() => setBulkResultMsg(""), 3000);
              }}
            >
              🔗 复制为 ref（「title」）
            </button>
            {t && (
              <button
                type="button"
                style={itemBtn}
                onMouseOver={itemBtnHoverIn}
                onMouseOut={itemBtnHoverOut}
                onClick={async () => {
                  setTaskCtxMenu(null);
                  try {
                    await navigator.clipboard.writeText(formatTaskAsMarkdown(t));
                    setBulkResultMsg(`已复制 "${t.title}" 为 markdown 到剪贴板`);
                  } catch (e) {
                    setBulkResultMsg(`复制失败：${e}`);
                  }
                  window.setTimeout(() => setBulkResultMsg(""), 3000);
                }}
              >
                📑 复制为 Markdown
              </button>
            )}
          </div>
        );
      })()}
    </div>
  );
}
