import { Fragment, useState, useEffect, useCallback, useMemo, useRef } from "react";
import type { ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { parseMarkdown } from "../../utils/inlineMarkdown";
import { formatRelativeAgeBuckets } from "../../utils/formatRelativeAge";
import { useSearchHistory } from "../../hooks/useSearchHistory";
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
  /** 任务依赖：description 里 `[blockedBy: ...]` 解析出的引用 title 列表
   * （raw，不 cross-reference）。前端用 `computeUnresolvedBlockers` 拿仍卡
   * 着的子集渲染 🔒 chip。后端缺省 → `[]`（兼容老 session）。 */
  blocked_by?: string[];
  /** 任务 snooze：description 里 `[snooze: YYYY-MM-DD HH:MM]` 解析的最后一
   * 个有效时刻。后端仅在 `now < until` 时填字符串（`YYYY-MM-DDThh:mm`，与
   * `due` 协议同形）；过点后 = null/undefined 让 💤 chip 自动消失。 */
  snoozed_until?: string | null;
  /** 是否被 owner 标记 `[pinned]`。前端用此字段在「📌 钉住」chip filter 下专门
   * 列出，让长 pending 队列里关键条目不被淹。后端缺省 → false（兼容老 session）。 */
  pinned?: boolean;
}

/** 给定全部 tasks，返回每条 pending/error 任务仍未解决的 blocker title 列表。
 * "未解决"= blocker title 仍在 tasks 里且其 status 不是 done / cancelled。
 *
 * 与后端 `task_queue::unresolved_blockers` 同算法（独立实现一份避免 IPC 往返
 * + 让 UI 即时反映本地状态变更）。typo / 已删除的 blocker 视作已解决，避免
 * 永久卡死。done / cancelled 任务自身不计算 blocker —— 终态行没有"等待"语义。
 */
function computeUnresolvedBlockers(
  tasks: TaskView[],
): Map<string, string[]> {
  const activeTitles = new Set<string>();
  for (const t of tasks) {
    if (t.status !== "done" && t.status !== "cancelled") {
      activeTitles.add(t.title);
    }
  }
  const out = new Map<string, string[]>();
  for (const t of tasks) {
    if (t.status === "done" || t.status === "cancelled") continue;
    const raw = t.blocked_by ?? [];
    if (raw.length === 0) continue;
    const unresolved = raw.filter((b) => activeTitles.has(b));
    if (unresolved.length > 0) out.set(t.title, unresolved);
  }
  return out;
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

interface TaskTemplate {
  label: string;
  title: string;
  body: string;
}

/// localStorage key for user-defined templates；与内置 TASK_TEMPLATES 合并
/// 后构成下拉选项总集合。shape：`Array<TaskTemplate>`。读失败 / 解析错 /
/// 非数组 → 静默退到空数组（功能性降级）。
const CUSTOM_TEMPLATES_LS_KEY = "pet-task-templates-custom";

/// 自定义模板上限。20 是经验值：再多用户也不可能从下拉里挑得动；超过强
/// 制让用户先在管理 modal 里清掉旧的。防止 localStorage 无界增长。
const CUSTOM_TEMPLATES_MAX = 20;

/// 自定义模板 label 最大字数。20 与任务标题 max 对齐，让 dropdown 不被拉宽。
const CUSTOM_TEMPLATE_LABEL_MAX = 20;

/// 读 localStorage 自定义模板。失败 / 解析错 → 空数组（不抛、不弹错）；
/// 每条 entry 做 shape guard 防 hand-edit / 老版本字段漂移。
function loadCustomTemplates(): TaskTemplate[] {
  try {
    const raw = window.localStorage.getItem(CUSTOM_TEMPLATES_LS_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(
      (x): x is TaskTemplate =>
        x !== null &&
        typeof x === "object" &&
        typeof x.label === "string" &&
        typeof x.title === "string" &&
        typeof x.body === "string" &&
        x.label.trim().length > 0,
    );
  } catch {
    return [];
  }
}

/// 写 localStorage 自定义模板。失败静默吞 —— localStorage 满 / 禁用都
/// 不该阻塞表单交互；下次启动恢复为空。
function saveCustomTemplates(list: TaskTemplate[]): void {
  try {
    window.localStorage.setItem(CUSTOM_TEMPLATES_LS_KEY, JSON.stringify(list));
  } catch (e) {
    console.error("saveCustomTemplates failed:", e);
  }
}

/// PanelTasks 创建表单的"📋 从模板"内置预填项。每条 = 一个 one-shot 任务范
/// 例，引导用户写出宠物易执行的形态（明确动作 + 明确产物 + 明确范围）。
/// label 是 dropdown 显示文案，title / body 是 prefill 值。priority 默认
/// 全 3（无信号偏置）；due 全空（用户决定）。
///
/// 用户可在 dropdown 旁边「💾 存为」按钮把当前表单内容存为自定义模板，
/// 与本内置列表合并显示。自定义模板可通过「管理…」入口删除；内置不可删。
const TASK_TEMPLATES_BUILTIN: TaskTemplate[] = [
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

/** 把 `Date` 渲染为 datetime-local 输入框接受的 `YYYY-MM-DDThh:mm`（无时区，
 * 本地时间）。后端 / `formatDue` 协议同形，不引入 timezone offset。
 *
 * Date 实例化用本地时间组件（getFullYear / getMonth + 1 / getDate / getHours
 * / getMinutes），避免 toISOString 走 UTC 偏移 8 小时（夏令时换日界还会更乱）。
 */
export function formatDueInput(d: Date): string {
  const y = d.getFullYear();
  const mo = String(d.getMonth() + 1).padStart(2, "0");
  const da = String(d.getDate()).padStart(2, "0");
  const h = String(d.getHours()).padStart(2, "0");
  const mi = String(d.getMinutes()).padStart(2, "0");
  return `${y}-${mo}-${da}T${h}:${mi}`;
}

/** 计算"今晚 18:00"对应的 datetime-local 值。如果 now 已过 18:00（晚上加班场
 * 景）跳到明晚同点，避免一点就退回过去时间的 footgun。 */
export function dueTonight(now: Date): string {
  const d = new Date(now.getFullYear(), now.getMonth(), now.getDate(), 18, 0, 0);
  if (d.getTime() <= now.getTime()) {
    d.setDate(d.getDate() + 1);
  }
  return formatDueInput(d);
}

/** 计算"明天 HH:MM"。默认 09:00，对应"明早开工"。 */
export function dueTomorrow(now: Date, hour = 9, minute = 0): string {
  const d = new Date(
    now.getFullYear(),
    now.getMonth(),
    now.getDate() + 1,
    hour,
    minute,
    0,
  );
  return formatDueInput(d);
}

/** 计算"下个周一 09:00"。如果今天就是周一且 09:00 还未到，仍跳到下周一 ——
 * "周一" 的语义里"下周第一天"比"今天"自然，避免今天周一上午点了直接 due 几小
 * 时后的歧义。 */
export function dueNextMonday(now: Date): string {
  // JS getDay(): 0 = Sun, 1 = Mon, ...
  const today = now.getDay();
  // 距离下一个周一的天数：今天周日 → 1；周一 → 7；周二 → 6；...
  const daysAhead = today === 0 ? 1 : 7 - today + 1;
  const d = new Date(
    now.getFullYear(),
    now.getMonth(),
    now.getDate() + daysAhead,
    9,
    0,
    0,
  );
  return formatDueInput(d);
}

/** 计算"一周后" —— 今天的 +7 日，本地时间组件用 now 的小时分（不强制 09:00）。
 * 既保留 +1 week 的语义，又让"现在加一周"的用户预期不被改写。 */
export function dueOneWeek(now: Date): string {
  const d = new Date(
    now.getFullYear(),
    now.getMonth(),
    now.getDate() + 7,
    now.getHours(),
    now.getMinutes(),
    0,
  );
  return formatDueInput(d);
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

/** 把任务格式化为 markdown blockquote —— 与 `formatTaskAsMarkdown`（H2 + bullets
 * 完整段）形成"简短 quote" vs "完整 block" 双选项。粘到 detail.md / chat /
 * 别的 task 描述时直接显成引用块，比一整段 H2 更轻量。
 *
 * 形式：
 * ```
 * > ✓ **标题** (P3 · ⏰ 2026-05-20 18:00 · #tag1 #tag2)
 * >
 * > 描述内容（裁剪到 200 字）
 * ```
 *
 * 规则：
 * - 第 1 行：status emoji + 加粗标题 + (paren 内 meta 串)；meta 为空时省略 paren
 * - 描述 body 非空时：空 `>` 隔开，每行加 `> ` 前缀；> 200 字裁剪 + `…`
 * - body 内换行保留（多行也每行加前缀），让代码 / 列表结构不被破坏
 *
 * 纯字符串拼装，幂等 —— 同一 task 永远产出同一段。 */
export function formatTaskAsBlockquote(t: TaskView): string {
  const STATUS_EMOJI: Record<TaskStatus, string> = {
    pending: "📋",
    done: "✅",
    error: "❌",
    cancelled: "🚫",
  };
  const emoji = STATUS_EMOJI[t.status] ?? "📋";
  const meta: string[] = [];
  meta.push(`P${t.priority}`);
  if (t.due) meta.push(`⏰ ${formatDue(t.due)}`);
  for (const tag of t.tags) meta.push(`#${tag}`);
  const metaStr = meta.length > 0 ? ` (${meta.join(" · ")})` : "";
  const lines: string[] = [`> ${emoji} **${t.title}**${metaStr}`];
  const body = t.body.trim();
  if (body) {
    const preview = body.length > 200 ? body.slice(0, 200) + "…" : body;
    lines.push(">");
    for (const ln of preview.split("\n")) {
      lines.push(ln.length > 0 ? `> ${ln}` : ">");
    }
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

/** detail.md 粘贴图片自动压缩门限（字节）。≤ 该阈值的 blob 直接 base64，原 mime
 * 保留（含透明 PNG）；> 阈值 → 走 canvas resize + JPEG 0.85 重编码。256 KiB 选取
 * 经验值：常见 markdown 单图段保留视觉无损 + detail.md 不会被几张大截图撑爆。 */
const DETAIL_IMG_SKIP_BYTES = 256 * 1024;
/** 长边像素 cap：1600 px 已能覆盖 4K 截图缩放后的可读性；canvas 输出像素超出
 * 这条只是浪费 detail.md 体积。仅在触发压缩时生效，small blob 直通不缩。 */
const DETAIL_IMG_MAX_DIM = 1600;
const DETAIL_IMG_JPEG_QUALITY = 0.85;

function readBlobAsDataUrl(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      if (typeof reader.result === "string") resolve(reader.result);
      else reject(new Error("FileReader result is not a string"));
    };
    reader.onerror = () => reject(reader.error);
    reader.readAsDataURL(blob);
  });
}

/** 任务详情粘贴 / drop 图片时的压缩入口。返回 dataUrl + 原始字节 + 最终字节 +
 * 是否触发压缩。失败时回退到原图 base64 —— 用户图比报错更重要。 */
async function compressImageForDetail(blob: Blob): Promise<{
  dataUrl: string;
  originalBytes: number;
  finalBytes: number;
  didCompress: boolean;
}> {
  const originalBytes = blob.size;
  if (originalBytes <= DETAIL_IMG_SKIP_BYTES) {
    const dataUrl = await readBlobAsDataUrl(blob);
    return { dataUrl, originalBytes, finalBytes: dataUrl.length, didCompress: false };
  }
  const url = URL.createObjectURL(blob);
  try {
    const img = await new Promise<HTMLImageElement>((resolve, reject) => {
      const i = new Image();
      i.onload = () => resolve(i);
      i.onerror = () => reject(new Error("image load failed"));
      i.src = url;
    });
    const ratio = Math.min(
      DETAIL_IMG_MAX_DIM / img.width,
      DETAIL_IMG_MAX_DIM / img.height,
      1,
    );
    const w = Math.max(1, Math.round(img.width * ratio));
    const h = Math.max(1, Math.round(img.height * ratio));
    const canvas = document.createElement("canvas");
    canvas.width = w;
    canvas.height = h;
    const ctx = canvas.getContext("2d");
    if (!ctx) throw new Error("no 2d ctx");
    ctx.drawImage(img, 0, 0, w, h);
    const dataUrl = canvas.toDataURL("image/jpeg", DETAIL_IMG_JPEG_QUALITY);
    return {
      dataUrl,
      originalBytes,
      finalBytes: dataUrl.length,
      didCompress: true,
    };
  } catch (e) {
    console.error("compressImageForDetail failed, falling back to raw:", e);
    const dataUrl = await readBlobAsDataUrl(blob);
    return { dataUrl, originalBytes, finalBytes: dataUrl.length, didCompress: false };
  } finally {
    URL.revokeObjectURL(url);
  }
}

function formatBytes(n: number): string {
  if (n >= 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`;
  if (n >= 1024) return `${(n / 1024).toFixed(0)} KB`;
  return `${n} B`;
}

/// 根据 heading 计数（1-indexed，按 emit 顺序，与 parseMarkdown 内的
/// headingCounter 同源）从 markdown 提取该节：从第 N 个 heading 开始，到下一
/// 个同级或更高级别 heading 之前（exclusive）。找不到返空串。
///
/// 例：
///   ## A         ← counter=1，level=2
///   text...
///   ### B        ← counter=2，level=3（A 的子节）
///   ...          ← B 的内容
///   ## C         ← counter=3，level=2（结束 A）
/// extractSection(md, 1) → "## A\ntext...\n### B\n..."
/// extractSection(md, 2) → "### B\n..."
function extractSectionFromMarkdown(md: string, counter: number): string {
  const lines = md.split("\n");
  let seen = 0;
  let startIdx = -1;
  let startLevel = 0;
  for (let i = 0; i < lines.length; i++) {
    const m = lines[i].match(/^(#{1,3})\s+/);
    if (m) {
      seen += 1;
      if (seen === counter) {
        startIdx = i;
        startLevel = m[1].length;
        break;
      }
    }
  }
  if (startIdx < 0) return "";
  let endIdx = lines.length;
  for (let i = startIdx + 1; i < lines.length; i++) {
    const m = lines[i].match(/^(#{1,3})\s+/);
    if (m && m[1].length <= startLevel) {
      endIdx = i;
      break;
    }
  }
  return lines.slice(startIdx, endIdx).join("\n").trimEnd();
}

/// detail.md textarea 中文 typography 配对表。仅 Chinese 全角 / typography
/// 字符 —— 不含 ASCII `(` / `[` / `{`，那些容易误触（用户写代码 / 数学表达式
/// 时不期待自动配对）。中文场景下「」『』（）等是 owner 明确想成对出现的
/// 引用 / 引号符号，自动配可大幅省手。
const BRACKET_PAIRS: Record<string, string> = {
  "「": "」",
  "『": "』",
  "（": "）",
  "【": "】",
  "《": "》",
  "“": "”",
  "‘": "’",
};

/** 域名 → emoji 映射表。匹配按"完全相等"或"以 `.<key>` 结尾"双语义 ——
 * 让 `gist.github.com` 也能命中 github.com 的 🐙。常用引用源（dev / docs /
 * video / social / package）覆盖到。未命中 → 📎 通用附件 emoji。
 *
 * 顺序无关：Map iteration 顺序按插入序，但匹配逻辑取首个命中即返回；同一
 * URL 不会命中多 key 因为子域名收敛规则唯一。 */
const DOMAIN_EMOJI_MAP: Record<string, string> = {
  "github.com": "🐙",
  "gitlab.com": "🦊",
  "linear.app": "📐",
  "figma.com": "🎨",
  "notion.so": "📓",
  "notion.site": "📓",
  "youtube.com": "▶️",
  "youtu.be": "▶️",
  "docs.google.com": "📄",
  "drive.google.com": "🗂️",
  "twitter.com": "🐦",
  "x.com": "🐦",
  "stackoverflow.com": "📚",
  "npmjs.com": "📦",
  "news.ycombinator.com": "🟧",
  "reddit.com": "👽",
  "arxiv.org": "📜",
  "wikipedia.org": "🌐",
  "medium.com": "✍️",
};

/// pure：取 URL 的"语义 host"（去 `www.` 前缀）+ 选 emoji。invalid URL 返
/// `{ emoji: 📎, label: 原文 }`。完整 hostname 匹配优先；不命中再走"以
/// `.<key>` 结尾"判断让子域名也命中（如 `api.github.com` / `gist.github.com`
/// 都算 🐙）。
function pickLinkEmojiAndLabel(url: string): { emoji: string; label: string } {
  let host: string;
  try {
    host = new URL(url).hostname.toLowerCase();
  } catch {
    return { emoji: "📎", label: url };
  }
  const cleaned = host.startsWith("www.") ? host.slice(4) : host;
  // 完全相等优先（"github.com" 命中 "github.com"）
  const direct = DOMAIN_EMOJI_MAP[cleaned];
  if (direct) return { emoji: direct, label: cleaned };
  // 子域名 fallback："gist.github.com" / "api.github.com" 命中 "github.com"
  for (const key of Object.keys(DOMAIN_EMOJI_MAP)) {
    if (cleaned.endsWith("." + key)) {
      return { emoji: DOMAIN_EMOJI_MAP[key], label: cleaned };
    }
  }
  return { emoji: "📎", label: cleaned };
}

/** detail.md 行内 bare https/http 链接的 chip 卡片。比 parseMarkdown 里的纯
 * 蓝色下划线 UrlLink 更显眼：emoji + hostname 形态让 detail.md 里的引用链接
 * 看起来像附件而非散文里的 URL。点击调 plugin-opener 打开默认浏览器（与
 * UrlLink 同后端）。`title` attr 显完整 URL 让 owner 可 hover 验证地址。
 *
 * 域名特化 emoji（DOMAIN_EMOJI_MAP）让常用引用源（GitHub 🐙 / Linear 📐 /
 * Figma 🎨 / Notion 📓 / YouTube ▶️ / docs.google 📄 / Twitter / X 🐦 等）
 * 一眼可分。未命中域名退化 📎 通用 emoji。
 *
 * 解析失败（无效 URL）→ pickLinkEmojiAndLabel 兜底返 📎 + 原文 URL 作 label，
 * 避免渲染空字符串。 */
function LinkCard({ url }: { url: string }) {
  const { emoji, label } = pickLinkEmojiAndLabel(url);
  return (
    <a
      href={url}
      onClick={(e) => {
        e.preventDefault();
        e.stopPropagation();
        openUrl(url).catch((err) => console.error("openUrl failed:", err));
      }}
      title={url}
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: 4,
        padding: "1px 7px",
        margin: "0 2px",
        borderRadius: 6,
        background: "var(--pet-color-card)",
        border: "1px solid var(--pet-color-border)",
        color: "var(--pet-color-fg)",
        fontSize: "0.92em",
        textDecoration: "none",
        cursor: "pointer",
        whiteSpace: "nowrap",
        maxWidth: 240,
        overflow: "hidden",
        textOverflow: "ellipsis",
      }}
    >
      {emoji} {label}
    </a>
  );
}

/** task 状态 → 视觉 emoji。pending 默认（不渲 emoji）/ done ✅ / error ❌ /
 * cancelled 🚫。与 PanelMemory / TG `/tasks` 同 status 字符对偶语义。 */
function statusEmojiForTask(status: string | undefined): string {
  switch (status) {
    case "done":
      return "✅ ";
    case "error":
      return "❌ ";
    case "cancelled":
      return "🚫 ";
    default:
      return "📋 "; // pending / unknown
  }
}

/** detail.md 行内 `[task: 标题]` 语法的 chip 卡片。让 owner 在 detail.md 引用
 * 其它 task 时一眼看到对方的 status emoji + title，点击切换焦点过去（与 chat
 * 里 `「标题」` ref token 同 cross-link 思路）。
 *
 * - taskInfo === null（lookup 失败）→ muted 灰底 + "(未找到)" 后缀；click 仍
 *   触发 onClick 让 owner 知道是 typo（也可能 owner 删了 task 留了引用）。
 * - taskInfo === undefined（无 lookup callback）→ 与 null 同视觉但 click no-op
 *   （仅展示态）。
 *
 * 视觉风格与 LinkCard 同 chip 集群：inline-flex / borderRadius 6 / 行内浮起
 * 仍可读。`flexShrink: 0` + `maxWidth: 240` ellipsis 防长 title 撑爆 layout。 */
function TaskRefChip({
  title,
  taskInfo,
  onClick,
}: {
  title: string;
  taskInfo: { status: string; pinned?: boolean } | null | undefined;
  onClick?: (title: string) => void;
}) {
  const found = taskInfo !== null && taskInfo !== undefined;
  const emoji = found ? statusEmojiForTask(taskInfo!.status) : "❓ ";
  const pinPrefix = found && taskInfo!.pinned ? "📌 " : "";
  return (
    <button
      type="button"
      onClick={(e) => {
        e.preventDefault();
        e.stopPropagation();
        if (onClick) onClick(title);
      }}
      title={
        found
          ? `跳到任务「${title}」（status: ${taskInfo!.status}${taskInfo!.pinned ? " · pinned" : ""}）`
          : `引用了任务「${title}」，但未在当前 task 列表找到（可能已删除 / typo）`
      }
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: 2,
        padding: "1px 7px",
        margin: "0 2px",
        borderRadius: 6,
        background: found
          ? "var(--pet-tint-blue-bg)"
          : "color-mix(in srgb, var(--pet-color-muted) 12%, transparent)",
        border: found
          ? "1px solid color-mix(in srgb, var(--pet-tint-blue-fg) 30%, transparent)"
          : "1px dashed color-mix(in srgb, var(--pet-color-muted) 35%, transparent)",
        color: found ? "var(--pet-tint-blue-fg)" : "var(--pet-color-muted)",
        fontSize: "0.92em",
        fontFamily: "inherit",
        textDecoration: "none",
        cursor: onClick && found ? "pointer" : "default",
        whiteSpace: "nowrap",
        maxWidth: 240,
        overflow: "hidden",
        textOverflow: "ellipsis",
      }}
    >
      {pinPrefix}
      {emoji}
      {title}
      {!found && (
        <span style={{ marginLeft: 4, fontStyle: "italic" }}>(未找到)</span>
      )}
    </button>
  );
}

/** detail.md 文本段：把 bare https/http URL 切出来用 LinkCard 渲，把
 * `[task: 标题]` 语法切出来用 TaskRefChip 渲，其它子段交给 parseMarkdown。
 *
 * negative lookbehind `(?<!\]\()` 排除 markdown 链接 `[text](url)` 里的 url —
 * 那种已经有显式锚文本，渲染为 LinkCard 反而丢失用户表达。trailing 标点（句
 * 号 / 逗号 / 引号）会被 char 范围排除自然落到后续文本里，与既有 parseUrls
 * 路径同思路。
 *
 * `[task: ...]` 与 task header `[task pri=...]` 不冲突 —— description 里的
 * marker 不出现在 detail.md body，且 `[task:` 要求**冒号后立刻一个空格**
 * 才匹配，与 `[task pri=...]` 形态错开。 */
function renderDetailTextWithLinkCards(
  text: string,
  keyPrefix: string,
  /// 非 URL 子段的渲染模式：
  /// - `"markdown"`（默认）：走 parseMarkdown，渲 **bold** / `code` / lists 等
  ///   富格式 —— 用于 detail.md 展开 / preview 模式的正式渲染。
  /// - `"raw"`：保持原文文本（newline 由父级 `pre-wrap` 处理）—— 用于行
  ///   hover preview 这种轻量场景：要 LinkCard chip 化但不希望 hover 闪动时
  ///   重跑 markdown 引擎，也避免改变既有"raw markdown 字面"视觉。
  textMode: "markdown" | "raw" = "markdown",
  /// task ref 查表 callback。`[task: 标题]` 命中时调用拿 status + pinned 等
  /// 信息。返回 null = 未找到（chip 走 muted 态）；不传 = 不识别 `[task:]`
  /// 语法整体（保持原文本走 parseMarkdown）。
  taskLookup?: (title: string) => { status: string; pinned?: boolean } | null,
  /// task chip 点击 callback。仅 lookup 命中时才挂；用于 cross-link 跳焦点。
  /// 不传时 chip 视觉同但 click 无副作用。
  onTaskClick?: (title: string) => void,
): ReactNode[] {
  // URL 与 task ref 并行匹配：alternation 单次扫描定位所有特殊段。
  // group 1 (`[task: x]` 的内 title 段) 命中时本 match 是 task ref；
  // 否则当 URL 处理。
  const COMBINED_RE = taskLookup
    ? /(?<!\]\()https?:\/\/[^\s)\]<>"']+|\[task:\s+([^\]]+?)\]/g
    : /(?<!\]\()https?:\/\/[^\s)\]<>"']+/g;
  const renderChunk = (s: string, key: string): ReactNode =>
    textMode === "markdown" ? (
      <Fragment key={key}>{parseMarkdown(s)}</Fragment>
    ) : (
      <Fragment key={key}>{s}</Fragment>
    );
  const out: ReactNode[] = [];
  let lastIdx = 0;
  let urlKey = 0;
  let taskKey = 0;
  let m: RegExpExecArray | null;
  while ((m = COMBINED_RE.exec(text)) !== null) {
    if (m.index > lastIdx) {
      out.push(
        renderChunk(
          text.slice(lastIdx, m.index),
          `${keyPrefix}-pre-${m.index}`,
        ),
      );
    }
    // group 1 命中 → 本 match 是 `[task: title]` task ref（caller 注了 lookup
    // 时 alternation 才生效）；否则当 URL 处理。
    const taskTitle = m[1];
    if (taskTitle !== undefined && taskLookup) {
      const trimmed = taskTitle.trim();
      out.push(
        <TaskRefChip
          key={`${keyPrefix}-task-${taskKey++}`}
          title={trimmed}
          taskInfo={taskLookup(trimmed)}
          onClick={onTaskClick}
        />,
      );
      lastIdx = m.index + m[0].length;
      continue;
    }
    // 剥句末标点（与 parseUrls 同 trail-trim 思路）：让 "看这里 https://a.com。"
    // 不把"。"吃进 URL。
    let url = m[0];
    let tail = "";
    while (
      url.length > 8 &&
      /[.,;:!?。,;:!?)）"'”“]/.test(url[url.length - 1])
    ) {
      tail = url[url.length - 1] + tail;
      url = url.slice(0, -1);
    }
    out.push(<LinkCard key={`${keyPrefix}-url-${urlKey++}`} url={url} />);
    if (tail) {
      out.push(
        <Fragment key={`${keyPrefix}-tail-${urlKey}`}>{tail}</Fragment>,
      );
    }
    lastIdx = m.index + m[0].length;
  }
  if (lastIdx < text.length) {
    out.push(renderChunk(text.slice(lastIdx), `${keyPrefix}-tail`));
  }
  // 全无 URL 时退化到 markdown / raw 单条 path（避免 splice 空白 ReactNode）。
  if (out.length === 0) {
    return textMode === "markdown" ? [parseMarkdown(text)] : [text];
  }
  return out;
}

/** 解析 detail.md：把 markdown image 语法 `![alt](url)` 切出来用 ImageThumb 渲，
 * 文本段进一步把 bare https/http URL 渲成「📎 hostname」link card；其它走
 * 既有 parseMarkdown。让任务详情里贴的截图直接可见 + 可点开 + 可复制，引用
 * 链接以附件形态独立呈现，不必切到 markdown 编辑器。
 *
 * 不识别带 title 的形式 `![alt](url "title")` —— 大模型 / 用户实际写的几乎全是
 * 朴素双段，复杂语法后续再扩。markdown 链接 `[text](url)` 仍走 parseMarkdown
 * 自身的 anchor 渲染（保留显式锚文本），不会被 LinkCard 抢走。 */
function parseDetailMdWithImages(
  md: string,
  onOpenImage: (src: string) => void,
  /// 可选：task ref `[task: 标题]` 查表 callback。同 renderDetailTextWithLinkCards
  /// 的同名参数；不传则不识别 task ref 语法。
  taskLookup?: (title: string) => { status: string; pinned?: boolean } | null,
  /// 可选：task ref chip click callback。
  onTaskClick?: (title: string) => void,
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
          {renderDetailTextWithLinkCards(
            md.slice(lastIdx, m.index),
            `txt-${m.index}`,
            "markdown",
            taskLookup,
            onTaskClick,
          )}
        </Fragment>,
      );
    }
    const url = m[2];
    if (isImageUrl(url)) {
      out.push(
        <div key={`img-${imgKey++}`} style={{ margin: "6px 0" }}>
          <ImageThumb src={url} onOpen={() => onOpenImage(url)} lazy />
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
      <Fragment key={`txt-tail`}>
        {renderDetailTextWithLinkCards(
          md.slice(lastIdx),
          "txt-tail",
          "markdown",
          taskLookup,
          onTaskClick,
        )}
      </Fragment>,
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
  return formatRelativeAgeBuckets(age);
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
  /// 跨窗口 deeplink（pet 窗 🔴 逾期 pill 等入口）把目标 due filter 推到这里。
  /// 挂载后 useEffect 一次性消费 → setDueFilter，再调 onConsumePendingDueFilter
  /// 清空 → 用户后续手改 filter 不会被 stale 值反复覆盖。
  pendingDueFilter?: "all" | "today" | "overdue" | "createdToday" | null;
  onConsumePendingDueFilter?: () => void;
  /// detail.md 编辑器选段 → "🧠 ask LLM about <selection>" 按钮触发：把
  /// 选段送到 PanelApp 上层，由那里 prefill PanelChat textarea + 切 tab。
  onAskLLMAbout?: (text: string) => void;
  /// 桌面 ChatMini "💾 转 task" 按钮 → 跨窗口 deeplink → 在本 mount 时
  /// setBody + setTitle (前 30 字 default) + setQuickAddOpen(true)。
  pendingQuickAddBody?: string | null;
  onConsumePendingQuickAddBody?: () => void;
}

export function PanelTasks({
  pendingFocusTitle,
  onConsumeFocus,
  pendingDueFilter,
  onConsumePendingDueFilter,
  onAskLLMAbout,
  pendingQuickAddBody,
  onConsumePendingQuickAddBody,
}: PanelTasksProps = {}) {
  const [tasks, setTasks] = useState<TaskView[]>([]);
  /// 任务依赖未解决映射：title → 仍卡着的 blocker title 列表。tasks 变化时
  /// O(n) 计算一次；行渲染时 .has(title) 决定是否显 🔒 chip。useMemo 让 tasks
  /// 不变时引用稳定，避免每次 re-render 都重算 Map（虽然 n 通常 < 几十）。
  const blockedMap = useMemo<Map<string, string[]>>(
    () => computeUnresolvedBlockers(tasks),
    [tasks],
  );
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
  /// 归档区搜索查询。仅 archive tab + expanded + loaded 时可用。空串显
  /// 全集；非空按 title / description 子串匹配（大小写不敏感）。session
  /// 内有效，切 tab / 折叠时不清空（用户可能想再回来 refine）。
  const [archiveQuery, setArchiveQuery] = useState("");
  /// 最近 5 个 task 搜索 keyword 历史 —— 走共享 useSearchHistory hook（与
  /// PanelMemory 同模式）。Enter 时 push；datalist 浮动。
  const { history: taskSearchHistory, push: pushTaskSearchHistory } =
    useSearchHistory("pet-tasks-search-history");
  /// 「🗑 清理」二次确认。null = 未 armed；number = armed 时的 setTimeout id
  /// （用于 disarm 倒计时；5s 内再点真执行）。armed 期间按钮文案 / 颜色变红。
  const [archivePurgeArmed, setArchivePurgeArmed] = useState(false);
  const archivePurgeArmTimerRef = useRef<number | null>(null);
  const [archivePurging, setArchivePurging] = useState(false);
  const armArchivePurge = () => {
    if (archivePurgeArmTimerRef.current !== null) {
      window.clearTimeout(archivePurgeArmTimerRef.current);
    }
    setArchivePurgeArmed(true);
    archivePurgeArmTimerRef.current = window.setTimeout(() => {
      setArchivePurgeArmed(false);
      archivePurgeArmTimerRef.current = null;
    }, 5000);
  };
  const disarmArchivePurge = () => {
    if (archivePurgeArmTimerRef.current !== null) {
      window.clearTimeout(archivePurgeArmTimerRef.current);
      archivePurgeArmTimerRef.current = null;
    }
    setArchivePurgeArmed(false);
  };
  const doArchivePurge = async () => {
    disarmArchivePurge();
    setArchivePurging(true);
    try {
      const n = await invoke<number>("task_archive_purge_older_than", { days: 30 });
      setBulkResultMsg(`已清理 ${n} 条 >30 天归档`);
      window.setTimeout(() => setBulkResultMsg(""), 4000);
      await reloadArchive();
    } catch (e) {
      setBulkResultMsg(`清理失败：${e}`);
      window.setTimeout(() => setBulkResultMsg(""), 4000);
    } finally {
      setArchivePurging(false);
    }
  };
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
  /// 📌 钉住过滤：true 时只显 pinned 任务。跨 session 持久化 —— 用户开过滤
  /// 后切走再回到面板，状态保留；解决"chip 状态丢"的体验割裂。localStorage
  /// 解析失败 / 旧用户首次升级时缺 key → fallback false（不打扰新用户）。
  const [pinnedFilter, setPinnedFilter] = useState<boolean>(() => {
    try {
      return window.localStorage.getItem("pet-task-pinned-filter") === "true";
    } catch {
      return false;
    }
  });
  useEffect(() => {
    try {
      window.localStorage.setItem(
        "pet-task-pinned-filter",
        pinnedFilter ? "true" : "false",
      );
    } catch (e) {
      console.error("pinnedFilter localStorage save failed:", e);
    }
  }, [pinnedFilter]);

  // 创建表单
  const [title, setTitle] = useState("");
  const [body, setBody] = useState("");
  const [priority, setPriority] = useState(3);
  const [due, setDue] = useState(""); // datetime-local 原始值，可空
  const [creating, setCreating] = useState(false);
  const [errMsg, setErrMsg] = useState("");
  /// 用户自定义任务模板（与内置 TASK_TEMPLATES_BUILTIN 合并显示在「📋
  /// 从模板」下拉里）。localStorage 持久。变更时通过 effect 写盘；首屏
  /// 走 lazy initializer 读盘一次避免每次 render 重新 parse。
  const [customTemplates, setCustomTemplates] = useState<TaskTemplate[]>(() =>
    loadCustomTemplates(),
  );
  useEffect(() => {
    saveCustomTemplates(customTemplates);
  }, [customTemplates]);
  /// 「管理自定义模板」modal 显隐。仅 customTemplates.length > 0 时入口
  /// 渲染（empty 状态下连入口都没有，避免空 modal）。
  const [templatesManagerOpen, setTemplatesManagerOpen] = useState(false);
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
  // 三态偏好跨 session 持久化：偏好 split 的用户每次开新任务都要点切换太烦。
  // 不合法 / 解析失败 / 老用户首次升级时缺 key → fallback "edit"（默认对最大
  // 多数用户的预期）。下次切换时 useEffect 自动写回。
  const [detailViewMode, setDetailViewMode] = useState<DetailViewMode>(() => {
    try {
      const raw = window.localStorage.getItem("pet-task-detail-view-mode");
      if (raw === "edit" || raw === "split" || raw === "preview") return raw;
    } catch {
      // localStorage 不可用（私密模式 / 容量满）→ fallback edit
    }
    return "edit";
  });
  useEffect(() => {
    try {
      window.localStorage.setItem("pet-task-detail-view-mode", detailViewMode);
    } catch (e) {
      // 配额满 → 用户至少这次切换仍生效，下次启动回 edit；不阻塞
      console.error("detailViewMode localStorage save failed:", e);
    }
  }, [detailViewMode]);
  const [savingDetail, setSavingDetail] = useState(false);
  /// detail.md textarea 光标位置（selectionStart UTF-16 offset）。给底部状态
  /// 栏算"行 N / 共 M"。0 = 无 / 编辑器未打开 / cursor 在文首。两个 textarea
  /// （edit / split 模式）共用一个 state —— 互斥编辑保证不竞争。
  const [detailCursorPos, setDetailCursorPos] = useState<number>(0);
  /// detail.md textarea selection 终点（selectionEnd UTF-16 offset）。配合
  /// detailCursorPos = selectionStart 算选区长度。`end > start` 时字数 chip
  /// 切到 "选 N 字 · 〜M 词" 显示，与 IDE / Pages 同 selection-aware UX。
  /// 无选区时（start == end）chip 显总字数。事件来源与 cursor pos 同 4 路：
  /// onChange / onSelect / onKeyUp / onClick。
  const [detailSelectionEnd, setDetailSelectionEnd] = useState<number>(0);
  /// dirty badge "● 未保存" stale tracking：记录 dirty 起始时刻，超 60s 仍未
  /// 保存时把 badge 染红 + 微 pulse 提醒 owner 该 ⌘S。content 回到 original /
  /// 编辑器关闭都清状态。`dirtyTickKey` 周期性 +1 触发重渲染让 elapsedSec 推
  /// 进；不存 elapsed 进 state 避免每 5s 多余 render（只读 ref + tick key 让
  /// 重渲发生即可，badge 内部读 ref 算最新值）。
  const dirtySinceRef = useRef<number | null>(null);
  const [dirtyTickKey, setDirtyTickKey] = useState(0);
  useEffect(() => {
    const dirty = editingDetailContent !== editingDetailOriginalRef.current;
    if (dirty) {
      if (dirtySinceRef.current === null) dirtySinceRef.current = Date.now();
    } else {
      dirtySinceRef.current = null;
    }
  }, [editingDetailContent]);
  useEffect(() => {
    if (editingDetailTitle === null) {
      dirtySinceRef.current = null;
      return;
    }
    const id = window.setInterval(() => setDirtyTickKey((k) => k + 1), 5000);
    return () => window.clearInterval(id);
  }, [editingDetailTitle]);

  /// 自动草稿：每 60s 把当前 editingDetailContent 写到 localStorage 防意外关
  /// 闭丢内容。key 形如 `pet-detail-draft-${title}` —— task 标题已 unique 唯
  /// 一，重名不可能进 butler_tasks。draft 仅在 dirty 时写（content 与磁盘版
  /// 一致时无意义）。值是 `{content, ts}` JSON。
  /// 编辑器打开时 handleEnterEditDetail 检查 draft；存在且与文件版本不同时
  /// 弹"恢复 / 忽略"banner。保存成功 → 删 draft；取消 / 关掉 panel → 留 draft
  /// 给下次开同任务时恢复。
  const DRAFT_AUTO_INTERVAL_MS = 60_000;
  const draftKeyFor = (taskTitle: string) =>
    `pet-detail-draft-${taskTitle}`;
  /// banner state：存"上次保存的 draft 时间戳 + 内容"。null = 无 draft 待恢复。
  const [pendingDraft, setPendingDraft] = useState<{
    title: string;
    content: string;
    ts: number;
  } | null>(null);
  useEffect(() => {
    if (editingDetailTitle === null) return;
    const id = window.setInterval(() => {
      const dirty =
        editingDetailContent !== editingDetailOriginalRef.current;
      if (!dirty) return;
      try {
        window.localStorage.setItem(
          draftKeyFor(editingDetailTitle),
          JSON.stringify({ content: editingDetailContent, ts: Date.now() }),
        );
      } catch (e) {
        // 配额满 / 私密模式：静默失败 —— 自动草稿是 backup safety net，主
        // 路径（⌘S / 关闭二次确认）仍能保数据。
        console.error("detail draft autosave failed:", e);
      }
    }, DRAFT_AUTO_INTERVAL_MS);
    return () => window.clearInterval(id);
  }, [editingDetailTitle, editingDetailContent]);

  /// detail.md 大纲浮窗开关。split / preview 模式下 view-mode 行 📑 按钮 toggle；
  /// 仅 1 个浮窗共用 state（同时只展开一个任务的 detail 编辑器，互斥保证）。
  /// 编辑器关闭 → 自动关浮窗（与 cursor pos 重置同模式）。
  const [detailOutlineOpen, setDetailOutlineOpen] = useState(false);
  useEffect(() => {
    if (editingDetailTitle === null) setDetailOutlineOpen(false);
  }, [editingDetailTitle]);

  /// 大纲浮窗 active heading 跟踪：IntersectionObserver 监听 preview pane 渲
  /// 染的所有 `pet-detail-${title}-h${counter}` 元素，把"当前最靠上可见"的
  /// heading 高亮在浮窗对应 item 上，让 owner 滚 preview 时一眼知道"我在哪节"。
  /// rootMargin `-70%` 让观察区缩到视口顶部 30% —— 只有 heading 滚进顶部 30%
  /// 才算 active（更稳定 + 避免视口尾部的多个 heading 同时算 active）。
  const [activeHeadingCounter, setActiveHeadingCounter] = useState<number | null>(null);
  useEffect(() => {
    if (!detailOutlineOpen || !editingDetailTitle) {
      setActiveHeadingCounter(null);
      return;
    }
    if (detailViewMode === "edit") {
      // edit 模式没 preview pane 渲染 heading（id 不存在）；跳过观察。
      setActiveHeadingCounter(null);
      return;
    }
    const prefix = `pet-detail-${editingDetailTitle}-h`;
    const elements: HTMLElement[] = [];
    let counter = 1;
    while (true) {
      const el = document.getElementById(`${prefix}${counter}`);
      if (!el) break;
      elements.push(el);
      counter += 1;
    }
    if (elements.length === 0) return;
    const visibility = new Map<number, number>();
    const obs = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          const m = entry.target.id.match(/-h(\d+)$/);
          if (!m) continue;
          const n = parseInt(m[1], 10);
          if (entry.isIntersecting) {
            visibility.set(n, entry.intersectionRatio);
          } else {
            visibility.delete(n);
          }
        }
        if (visibility.size === 0) return;
        // 取最小 counter（emit 顺序最早 = DOM 中最靠上）作 active —— 与 IDE
        // 通用 "topmost visible heading" 模式一致。
        let minCounter = Infinity;
        for (const k of visibility.keys()) {
          if (k < minCounter) minCounter = k;
        }
        if (Number.isFinite(minCounter)) {
          setActiveHeadingCounter(minCounter);
        }
      },
      {
        rootMargin: "0px 0px -70% 0px",
        threshold: [0, 0.1, 0.5, 1],
      },
    );
    for (const el of elements) obs.observe(el);
    return () => obs.disconnect();
  }, [
    detailOutlineOpen,
    editingDetailTitle,
    editingDetailContent,
    detailViewMode,
  ]);

  /// detail.md 内 `[task: 标题]` 语法 → TaskRefChip 的查表 callback。读
  /// 当前 tasks state，命中返 { status, pinned }；未命中返 null（chip 显
  /// muted "(未找到)"）。useCallback 让 chip 渲染稳定不触发不必要重渲。
  const taskLookupForRefs = useCallback(
    (title: string) => {
      const found = tasks.find((t) => t.title === title);
      if (!found) return null;
      return { status: found.status, pinned: !!found.pinned };
    },
    [tasks],
  );

  /// task ref chip 点击 → 复用既有 pendingTitleFocus 路径：清 filter / 显
  /// finished / 写 title → 下一帧 effect 找到 row → scrollIntoView + focus。
  /// 与"完成小卡 click title 跳行"同一条 jump-to-task pipeline，UX 一致。
  const handleTaskRefClick = useCallback(
    (title: string) => {
      // 命中检查：找不到的 task ref 点击仍可走 jump-to 但 setPendingTitleFocus
      // 会在下一帧失败（找不到 idx）—— 静默 no-op，无副作用。
      setSearch("");
      setSelectedTags(new Set());
      setDueFilter("all");
      setPriorityFilter(new Set());
      setOriginFilter(new Set());
      setPinnedFilter(false);
      setShowFinished(true);
      setPendingTitleFocus(title);
    },
    [],
  );

  /// preview / split 模式 heading 旁的 📋 复制本节 callback。parseMarkdown opts
  /// 传入 —— heading 计数同 parseMarkdown 内部 counter，extractSectionFromMarkdown
  /// 走同算法定位起止行。复用 setBulkResultMsg toast channel（与 📋 复制全文
  /// / 📤 export 同 UI）。
  const handleCopyHeadingSection = useCallback(
    (counter: number) => {
      const section = extractSectionFromMarkdown(
        editingDetailContent,
        counter,
      );
      if (!section) {
        setBulkResultMsg("未找到节内容");
        window.setTimeout(() => setBulkResultMsg(""), 3000);
        return;
      }
      void navigator.clipboard
        .writeText(section)
        .then(() => {
          setBulkResultMsg(
            `已复制本节 markdown（${section.length} 字符）`,
          );
        })
        .catch((e: unknown) => {
          setBulkResultMsg(`复制失败：${e}`);
        })
        .finally(() => {
          window.setTimeout(() => setBulkResultMsg(""), 4000);
        });
    },
    [editingDetailContent],
  );
  // 编辑器关闭 → 重置 cursor pos + selection end，避免下次打开新任务沿用
  // 旧值闪烁。
  useEffect(() => {
    if (editingDetailTitle === null) {
      setDetailCursorPos(0);
      setDetailSelectionEnd(0);
    }
  }, [editingDetailTitle]);

  /// preview 模式下点击 `- [ ]` / `- [x]` 复选框时切换源 description 该行的
  /// marker。functional setState 让多次连点（不同行）都基于最新值，避免闭包
  /// 拿到旧 content 误覆盖。不直接 save —— 用户保存按钮按下时一并写盘；
  /// 「未保存」chip 自然就会显出来提示。匹配大小写 `[ ]` / `[x]` / `[X]` 三种；
  /// row 不含 marker（理论上不会发生：onToggle 只在 parseMarkdown 命中
  /// taskMatch 时触发）时 noop。
  const toggleEditChecklistLine = useCallback(
    (lineIdx: number, checked: boolean) => {
      setEditingDetailContent((cur) => {
        const lines = cur.split("\n");
        if (lineIdx < 0 || lineIdx >= lines.length) return cur;
        const replaced = lines[lineIdx].replace(
          /- \[[ xX]\]/,
          checked ? "- [x]" : "- [ ]",
        );
        if (replaced === lines[lineIdx]) return cur;
        lines[lineIdx] = replaced;
        return lines.join("\n");
      });
    },
    [],
  );
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
  /// 💤 snooze chip click 弹的 mini popover state：哪个 task 的 chip 被点。
  /// null = 关。与 priorityPickerTitle 同 outside-click-close + Esc 模式（在
  /// 既有 useEffect 内 union close）。复用 task_set_snooze preset 入参（与
  /// /snooze tonight / monday 等 backend 同源）。
  const [snoozePickerTitle, setSnoozePickerTitle] = useState<string | null>(null);
  /// 📅 调期 chip click 弹的 mini popover：从现在起 +1h / +1d / +3d / +1w
  /// / +2w preset 微调 due_at（与 quickAdd "今晚 18:00" preset 同精神，只是
  /// 是相对增量而非绝对锚点）。调用 task_set_due 走单字段原子修改。snooze
  /// 是"暂时藏到 N 时之后"，调期是"改 due_at 截止时刻"——两条 chip 不冲突。
  const [dueShiftPickerTitle, setDueShiftPickerTitle] = useState<string | null>(
    null,
  );
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
    if (
      !priorityPickerTitle &&
      !statusPickerTitle &&
      !taskCtxMenu &&
      !tagColorPicker &&
      !snoozePickerTitle &&
      !dueShiftPickerTitle
    )
      return;
    const close = () => {
      setPriorityPickerTitle(null);
      setStatusPickerTitle(null);
      setTaskCtxMenu(null);
      setTagColorPicker(null);
      setSnoozePickerTitle(null);
      setDueShiftPickerTitle(null);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        setPriorityPickerTitle(null);
        setStatusPickerTitle(null);
        setTaskCtxMenu(null);
        setTagColorPicker(null);
        setSnoozePickerTitle(null);
        setDueShiftPickerTitle(null);
      }
    };
    window.addEventListener("mousedown", close);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", close);
      window.removeEventListener("keydown", onKey);
    };
  }, [
    priorityPickerTitle,
    statusPickerTitle,
    taskCtxMenu,
    tagColorPicker,
    snoozePickerTitle,
    dueShiftPickerTitle,
  ]);

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

  /// detail.md 编辑器内 ⌘F 全文搜浮 bar：长 detail.md（每条 ≥ 数千字）owner
  /// 想快速跳到某关键词位置，PanelTasks 顶部 search 是按 task 标题 / 描述搜，
  /// 不进 detail。这里加 in-textarea find — 与 Chrome / VS Code 找一致。
  /// open 时聚焦 input；Enter / ↑↓ 切 match；textarea.setSelectionRange 选中
  /// match 让 textarea 自动滚到位；Esc 关。editingDetailTitle === null 时
  /// listener 不挂；切 task 时清空 query 重置 idx。
  const [detailSearchOpen, setDetailSearchOpen] = useState(false);
  const [detailSearchQuery, setDetailSearchQuery] = useState("");
  const [detailSearchActiveIdx, setDetailSearchActiveIdx] = useState(0);
  const detailSearchInputRef = useRef<HTMLInputElement>(null);
  /// 切到不同 task 的 detail 时关搜索 + 清查询（不然旧 query 跨 task 错位）。
  useEffect(() => {
    if (editingDetailTitle === null) {
      setDetailSearchOpen(false);
      setDetailSearchQuery("");
      setDetailSearchActiveIdx(0);
    } else {
      // 切到另一 task：仅重置 idx（保留 query 让 owner 用同关键词跨 task 搜）
      setDetailSearchActiveIdx(0);
    }
  }, [editingDetailTitle]);
  /// detail.md 内匹配位置（case-insensitive substring）。空 query → 空数组。
  /// 含 0-length 防御 —— 不该出现（query 已 trim 检查），但同步 indexOf
  /// 循环必须保护防无限递归。
  const detailSearchMatches = useMemo(() => {
    const q = detailSearchQuery;
    if (!q) return [] as { start: number; end: number }[];
    const out: { start: number; end: number }[] = [];
    const haystack = editingDetailContent.toLowerCase();
    const needle = q.toLowerCase();
    let from = 0;
    while (from < haystack.length) {
      const idx = haystack.indexOf(needle, from);
      if (idx < 0) break;
      out.push({ start: idx, end: idx + needle.length });
      from = idx + Math.max(1, needle.length);
    }
    return out;
  }, [detailSearchQuery, editingDetailContent]);
  /// ⌘F 在 detail 编辑器 textarea / 搜索 input 内时 → 拦下，开 / 聚焦本 bar；
  /// 不在 detail 编辑器内时让 ⌘F 走 useTaskKeyboardNav 默认路径（聚焦顶部
  /// task 搜索框）。capture: true + stopImmediatePropagation 保证比 nav hook
  /// 先跑 + 让 nav hook 的 listener 不会再处理。
  useEffect(() => {
    if (editingDetailTitle === null) return;
    const onKey = (e: KeyboardEvent) => {
      if (!(e.metaKey || e.ctrlKey)) return;
      if (e.shiftKey || e.altKey) return;
      if (e.key.toLowerCase() !== "f") return;
      const ae = document.activeElement;
      const ta = detailEditorRef.current;
      const si = detailSearchInputRef.current;
      // 仅 detail 编辑器 textarea / 自身 search input 内才劫持 ⌘F；
      // 其他位置（如顶部 task 搜索框已聚焦）让默认行为走。
      if (ae !== ta && ae !== si) return;
      e.preventDefault();
      e.stopImmediatePropagation();
      setDetailSearchOpen(true);
      setDetailSearchActiveIdx(0);
      window.setTimeout(() => {
        detailSearchInputRef.current?.focus();
        detailSearchInputRef.current?.select();
      }, 0);
    };
    window.addEventListener("keydown", onKey, { capture: true });
    return () =>
      window.removeEventListener("keydown", onKey, { capture: true });
  }, [editingDetailTitle]);
  /// activeIdx / matches 变化 → 把对应 range 选中并把 input 焦点保留。
  /// textarea.focus() + setSelectionRange 触发 webview 内 textarea 自动滚到
  /// 选区位置；rAF 等浏览器滚完再 refocus input，避免连按 Enter 时焦点跳乱。
  useEffect(() => {
    if (!detailSearchOpen) return;
    if (detailSearchMatches.length === 0) return;
    const safeIdx = Math.max(
      0,
      Math.min(detailSearchActiveIdx, detailSearchMatches.length - 1),
    );
    const m = detailSearchMatches[safeIdx];
    if (!m) return;
    const ta = detailEditorRef.current;
    if (!ta) return;
    ta.focus();
    try {
      ta.setSelectionRange(m.start, m.end);
    } catch {
      // 极少数情况下 m.end 超出当前 textarea value 长度（content 还在改）
      // — 忽略，下次 activeIdx 变化时重试
    }
    window.requestAnimationFrame(() => {
      detailSearchInputRef.current?.focus();
    });
  }, [detailSearchActiveIdx, detailSearchMatches, detailSearchOpen]);
  /// 循环切 match：next / prev wrap。matches 空时 noop。
  const cycleDetailSearchMatch = useCallback(
    (dir: "next" | "prev") => {
      setDetailSearchActiveIdx((cur) => {
        const n = detailSearchMatches.length;
        if (n === 0) return 0;
        const safe = Math.max(0, Math.min(cur, n - 1));
        if (dir === "next") return (safe + 1) % n;
        return (safe - 1 + n) % n;
      });
    },
    [detailSearchMatches.length],
  );

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

  /// 在光标所在行首插入"✓ 完成行"模板：`- [x] YYYY-MM-DD HH:MM `。
  /// 让 owner / 宠物记"我刚做完了什么 + 何时"成 1 步操作 —— 紧凑结合既有
  /// `- [x]` GFM checklist 语法（被 parseMarkdown 渲成 disabled checkbox）+
  /// 既有 `[snooze:]` / `[once:]` marker 同形的时间戳协议。光标落尾让用户立
  /// 即接着敲"任务摘要"完成完整一行。
  ///
  /// 与 line-prefix mode 区别：line-prefix 是"每选中行加前缀"；此 helper 是
  /// "光标所在行首插一段固定模板（时间戳是 final-form）"，更适合 quick log。
  const insertDoneLineAtCursor = useCallback(() => {
    const ta = detailEditorRef.current;
    if (!ta) return;
    const start = ta.selectionStart ?? 0;
    const value = ta.value;
    // 光标当前行的行首位置：从 start 往前找最近的 `\n`，行首 = idx + 1。
    const lineStart = value.lastIndexOf("\n", start - 1) + 1;
    const now = new Date();
    const y = now.getFullYear();
    const mo = String(now.getMonth() + 1).padStart(2, "0");
    const d = String(now.getDate()).padStart(2, "0");
    const hh = String(now.getHours()).padStart(2, "0");
    const mm = String(now.getMinutes()).padStart(2, "0");
    const stamp = `- [x] ${y}-${mo}-${d} ${hh}:${mm} `;
    // 若当前行已有内容且光标在行首之后，把模板插在行首前面（保留原内容）;
    // 行首已经有 `- [` 或 `- [x]` 的话不重复加（避免连点变 `- [x] ... - [x] ...`)
    const rest = value.slice(lineStart);
    const alreadyChecklist = /^\s*- \[[ xX]\] /.test(rest);
    if (alreadyChecklist) {
      // 已是 checklist 行 → 不插模板（让用户用既有 ☐ 按钮，避免重复符号叠加）
      // 但用 toast 通知用户为什么没动 —— 静默 noop 让人误以为按钮坏了。
      setActionErr("当前行已是 checklist；想改时间戳请删后重插或手动编辑。");
      window.setTimeout(() => setActionErr(""), 3500);
      return;
    }
    const next = value.slice(0, lineStart) + stamp + value.slice(lineStart);
    const cursorPos = lineStart + stamp.length;
    setEditingDetailContent(next);
    requestAnimationFrame(() => {
      const cur = detailEditorRef.current;
      if (!cur) return;
      cur.focus();
      cur.selectionStart = cur.selectionEnd = cursorPos;
    });
  }, []);

  /// 在光标位置插入当前本地时间，格式 `YYYY-MM-DD HH:MM`（与 [snooze:] /
  /// [once:] marker 协议同形，方便后续直接复制成 marker 或时间戳引用）。
  /// 插入后光标落到字符串末尾，方便用户接着敲"完成了 X"等后续文字。
  /// 与 insertMarkdownAtCursor 的 wrap / line-prefix 模式独立 —— 这是
  /// "纯插入 + 光标落尾"，没 selection wrap 语义。
  const insertCurrentTimeAtCursor = useCallback(() => {
    const ta = detailEditorRef.current;
    if (!ta) return;
    const start = ta.selectionStart ?? 0;
    const end = ta.selectionEnd ?? start;
    const value = ta.value;
    const now = new Date();
    const y = now.getFullYear();
    const mo = String(now.getMonth() + 1).padStart(2, "0");
    const d = String(now.getDate()).padStart(2, "0");
    const hh = String(now.getHours()).padStart(2, "0");
    const mm = String(now.getMinutes()).padStart(2, "0");
    const stamp = `${y}-${mo}-${d} ${hh}:${mm}`;
    const next = value.slice(0, start) + stamp + value.slice(end);
    const cursorPos = start + stamp.length;
    setEditingDetailContent(next);
    requestAnimationFrame(() => {
      const cur = detailEditorRef.current;
      if (!cur) return;
      cur.focus();
      cur.selectionStart = cur.selectionEnd = cursorPos;
    });
  }, []);

  /// 在光标位置插入 3×3 GFM table 骨架。需独占整段：若光标前一字符不是
  /// 换行，先补一个 `\n` 让表头不被前文 "吞" 进同段。插入后把"列 1" 设为
  /// 当前 selection —— 用户立刻可敲 / 选 / 删，不必先手动 select 占位文。
  /// 既有 insertMarkdownAtCursor 的 wrap / line-prefix 双模式无法表达
  /// "块级模板 + 落点为内部 selection"，故单独写一份而非扩第三个 mode。
  const insertTableSkeletonAtCursor = useCallback(() => {
    const ta = detailEditorRef.current;
    if (!ta) return;
    const start = ta.selectionStart ?? 0;
    const end = ta.selectionEnd ?? start;
    const value = ta.value;
    const needLeadingNL = start > 0 && value[start - 1] !== "\n";
    const lead = needLeadingNL ? "\n" : "";
    const skeleton =
      `${lead}| 列 1 | 列 2 | 列 3 |\n| --- | --- | --- |\n|  |  |  |\n|  |  |  |\n`;
    const next = value.slice(0, start) + skeleton + value.slice(end);
    // "列 1" 在第一行的 "| " 之后；UTF-16 长度 3（列 + 空格 + 1）。
    const headerCellStart = start + lead.length + 2;
    const headerCellEnd = headerCellStart + 3;
    setEditingDetailContent(next);
    requestAnimationFrame(() => {
      const cur = detailEditorRef.current;
      if (!cur) return;
      cur.focus();
      cur.selectionStart = headerCellStart;
      cur.selectionEnd = headerCellEnd;
    });
  }, []);

  /// 把一组 image blob 异步压缩 + 读为 data URL，统一拼成 markdown
  /// `![](data:...)` 行插到当前 textarea 光标位置。> 256 KiB 的 blob 走 canvas
  /// resize（长边 cap 1600 px） + JPEG 0.85 重编码，小图保留原 mime。一次性
  /// Promise.all 后单次 setState，避免多个 reader.onload 并发改 selectionStart
  /// 漂移。压缩到任何一张时 toast 显原 / 后总体积。
  const insertImageBlobsIntoDetail = useCallback(async (blobs: Blob[]) => {
    if (blobs.length === 0) return;
    const ta = detailEditorRef.current;
    if (!ta) return;
    const results = await Promise.all(blobs.map((b) => compressImageForDetail(b)));
    const compressed = results.filter((r) => r.didCompress);
    if (compressed.length > 0) {
      const totalOriginal = compressed.reduce((s, r) => s + r.originalBytes, 0);
      const totalFinal = compressed.reduce((s, r) => s + r.finalBytes, 0);
      setBulkResultMsg(
        `已压缩 ${compressed.length} 张图片（${formatBytes(totalOriginal)} → ${formatBytes(totalFinal)}）`,
      );
      window.setTimeout(() => setBulkResultMsg(""), 4000);
    }
    const start = ta.selectionStart ?? 0;
    const end = ta.selectionEnd ?? start;
    // 前后各加换行让 markdown 段落分隔清晰；同次粘贴的多图也各占一行。
    const insert =
      "\n" + results.map((r) => `![](${r.dataUrl})`).join("\n") + "\n";
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
  /// detail.md textarea 中文 typography 配对处理。返回 true 表示本事件已处理
  /// （caller 应 early-return 跳过 ⌘S / Esc 等后续分支）。规则：
  /// - 仅 BRACKET_PAIRS 已知 open 字符触发；其它键直接返 false。
  /// - IME composing 期间不响应（让输入法自处理；e.nativeEvent.isComposing
  ///   是 React SyntheticEvent 不暴露的 native flag）。
  /// - 空选区：插入 open + close，光标落 inner（pair 中间）。
  /// - 非空选区：把选区包裹为 open + selection + close，selection 仍是 inner
  ///   content（让用户能继续 typing / 嵌套包裹）。
  const handleDetailBracketPair = useCallback(
    (e: React.KeyboardEvent<HTMLTextAreaElement>): boolean => {
      const close = BRACKET_PAIRS[e.key];
      if (!close) return false;
      // IME composing：let the input method 自处理，不抢键。React 不直接暴露
      // `isComposing`；走 nativeEvent 取。
      if ((e.nativeEvent as KeyboardEvent).isComposing) return false;
      const ta = e.currentTarget;
      const start = ta.selectionStart ?? 0;
      const end = ta.selectionEnd ?? start;
      e.preventDefault();
      const value = ta.value;
      const selected = value.slice(start, end);
      const inserted = e.key + selected + close;
      const next = value.slice(0, start) + inserted + value.slice(end);
      setEditingDetailContent(next);
      // 同步 cursor pos state（行号 status chip 实时跟上）。
      const innerStart = start + e.key.length;
      const innerEnd = innerStart + selected.length;
      setDetailCursorPos(innerStart);
      requestAnimationFrame(() => {
        const cur = detailEditorRef.current;
        if (!cur) return;
        cur.focus();
        cur.selectionStart = innerStart;
        cur.selectionEnd = innerEnd;
      });
      return true;
    },
    [],
  );

  /// detail.md textarea ⌘D / Ctrl+D 复制当前行（或选区）。Sublime / JetBrains
  /// 通用 IDE 行为：
  /// - 选区非空：在选区之后立即重复一份选中文本，新副本仍 selected 让 owner
  ///   可继续 ⌘D 累积粘多份。
  /// - 选区空：把光标所在行复制一份插到下一行，光标落到新行的同 column 位置
  ///   （column = 原行 selectionStart - lineStart 的偏移）。
  /// 任何 shift / alt 修饰 → 不响应让位给未来扩展（⌘⇧D 删除当前行 / ⌘⌥D 复
  /// 制到上一行等可后续加）。IME composing 跳过。preventDefault 吃浏览器默认
  /// ⌘D（"Add bookmark"）—— Tauri webview 通常不弹书签栏，但兜底安全。
  const handleDetailDuplicateLine = useCallback(
    (e: React.KeyboardEvent<HTMLTextAreaElement>): boolean => {
      if (!(e.metaKey || e.ctrlKey)) return false;
      if (e.shiftKey || e.altKey) return false;
      if (e.key.toLowerCase() !== "d") return false;
      if ((e.nativeEvent as KeyboardEvent).isComposing) return false;
      const ta = e.currentTarget;
      const start = ta.selectionStart ?? 0;
      const end = ta.selectionEnd ?? start;
      const value = ta.value;
      e.preventDefault();
      if (start !== end) {
        // 选区非空：在选区末尾插一份相同文本，新副本 selected
        const selected = value.slice(start, end);
        const next = value.slice(0, end) + selected + value.slice(end);
        const newSelEnd = end + selected.length;
        setEditingDetailContent(next);
        setDetailCursorPos(end);
        requestAnimationFrame(() => {
          const t = detailEditorRef.current;
          if (!t) return;
          t.focus();
          t.selectionStart = end;
          t.selectionEnd = newSelEnd;
        });
        return true;
      }
      // 空选区：复制光标所在整行到下一行。光标落到新行的相对 column。
      const lineStart = value.lastIndexOf("\n", start - 1) + 1;
      const lineEnd = value.indexOf("\n", start);
      const lineEndIdx = lineEnd === -1 ? value.length : lineEnd;
      const lineText = value.slice(lineStart, lineEndIdx);
      const insertion = `\n${lineText}`;
      const next =
        value.slice(0, lineEndIdx) + insertion + value.slice(lineEndIdx);
      // 新光标位置 = 原 lineEnd + 1（跳过换行）+ 原 column offset
      const colOffset = start - lineStart;
      const newCursor = lineEndIdx + 1 + colOffset;
      setEditingDetailContent(next);
      setDetailCursorPos(newCursor);
      requestAnimationFrame(() => {
        const t = detailEditorRef.current;
        if (!t) return;
        t.focus();
        t.selectionStart = t.selectionEnd = newCursor;
      });
      return true;
    },
    [],
  );

  /// detail.md textarea Enter 自动续列表前缀。识别行首 list marker：
  ///   - `- text` / `* text` / `+ text`：无序列表
  ///   - `- [ ] text` / `- [x] text`：GFM checklist（新行总是 `- [ ] `，让 owner
  ///     自己改 done 状态）
  ///   - `<N>. text`：有序列表（N+1 自动递增）
  ///   - `> text`：blockquote
  /// 命中 + 当前行非空 → 阻止默认 + 插入 `\n` + 同 marker（含原 indent）。
  /// 命中 + 当前行**仅有 marker 无文本** → 阻止默认 + 删掉该 marker（escape
  /// list 语义，与 VSCode / Obsidian / Notion 同模式）。
  /// 任何 modifier 按下（shift / meta / ctrl / alt）→ 不响应，让 Shift+Enter
  /// soft 换行 / ⌘Enter 等其它语义不被抢。
  /// IME composing 期间不响应，与 bracket pair 同 guard。
  const handleDetailListContinue = useCallback(
    (e: React.KeyboardEvent<HTMLTextAreaElement>): boolean => {
      if (e.key !== "Enter") return false;
      if (e.shiftKey || e.metaKey || e.ctrlKey || e.altKey) return false;
      if ((e.nativeEvent as KeyboardEvent).isComposing) return false;
      const ta = e.currentTarget;
      const start = ta.selectionStart ?? 0;
      const end = ta.selectionEnd ?? start;
      // 选区非空：让 native Enter（替换选区为 \n）走，不抢
      if (start !== end) return false;
      const value = ta.value;
      const lineStart = value.lastIndexOf("\n", start - 1) + 1;
      const lineEnd = value.indexOf("\n", start);
      const cur = value.slice(lineStart, lineEnd === -1 ? value.length : lineEnd);
      // GFM checklist 必须放 unordered 前判（- [ ] 也匹配 `- ` regex）
      let match = cur.match(/^(\s*)(- \[[ xX]\] )(.*)$/);
      if (match) {
        const [, indent, , content] = match;
        const isEmpty = content.length === 0;
        if (isEmpty) {
          // 退出 list：删掉 indent + marker（保留 lineStart 之前 + lineEnd 之后）
          const next =
            value.slice(0, lineStart) +
            value.slice(lineStart + indent.length + match[2].length);
          const cursorPos = lineStart;
          e.preventDefault();
          setEditingDetailContent(next);
          setDetailCursorPos(cursorPos);
          requestAnimationFrame(() => {
            const t = detailEditorRef.current;
            if (!t) return;
            t.focus();
            t.selectionStart = t.selectionEnd = cursorPos;
          });
          return true;
        }
        // 续行：插入 `\n` + 同 indent + `- [ ] `（新条 default unchecked）
        const inserted = `\n${indent}- [ ] `;
        const next = value.slice(0, start) + inserted + value.slice(end);
        const cursorPos = start + inserted.length;
        e.preventDefault();
        setEditingDetailContent(next);
        setDetailCursorPos(cursorPos);
        requestAnimationFrame(() => {
          const t = detailEditorRef.current;
          if (!t) return;
          t.focus();
          t.selectionStart = t.selectionEnd = cursorPos;
        });
        return true;
      }
      // 有序列表 `<digit>. `
      match = cur.match(/^(\s*)(\d+)(\. )(.*)$/);
      if (match) {
        const [, indent, numStr, dot, content] = match;
        const isEmpty = content.length === 0;
        if (isEmpty) {
          const next =
            value.slice(0, lineStart) +
            value.slice(lineStart + indent.length + numStr.length + dot.length);
          const cursorPos = lineStart;
          e.preventDefault();
          setEditingDetailContent(next);
          setDetailCursorPos(cursorPos);
          requestAnimationFrame(() => {
            const t = detailEditorRef.current;
            if (!t) return;
            t.focus();
            t.selectionStart = t.selectionEnd = cursorPos;
          });
          return true;
        }
        const nextNum = parseInt(numStr, 10) + 1;
        const inserted = `\n${indent}${nextNum}. `;
        const next = value.slice(0, start) + inserted + value.slice(end);
        const cursorPos = start + inserted.length;
        e.preventDefault();
        setEditingDetailContent(next);
        setDetailCursorPos(cursorPos);
        requestAnimationFrame(() => {
          const t = detailEditorRef.current;
          if (!t) return;
          t.focus();
          t.selectionStart = t.selectionEnd = cursorPos;
        });
        return true;
      }
      // 无序列表 `- ` / `* ` / `+ ` + blockquote `> `
      match = cur.match(/^(\s*)([-*+] |> )(.*)$/);
      if (match) {
        const [, indent, marker, content] = match;
        const isEmpty = content.length === 0;
        if (isEmpty) {
          const next =
            value.slice(0, lineStart) +
            value.slice(lineStart + indent.length + marker.length);
          const cursorPos = lineStart;
          e.preventDefault();
          setEditingDetailContent(next);
          setDetailCursorPos(cursorPos);
          requestAnimationFrame(() => {
            const t = detailEditorRef.current;
            if (!t) return;
            t.focus();
            t.selectionStart = t.selectionEnd = cursorPos;
          });
          return true;
        }
        const inserted = `\n${indent}${marker}`;
        const next = value.slice(0, start) + inserted + value.slice(end);
        const cursorPos = start + inserted.length;
        e.preventDefault();
        setEditingDetailContent(next);
        setDetailCursorPos(cursorPos);
        requestAnimationFrame(() => {
          const t = detailEditorRef.current;
          if (!t) return;
          t.focus();
          t.selectionStart = t.selectionEnd = cursorPos;
        });
        return true;
      }
      return false;
    },
    [],
  );
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
    // 检查 localStorage 是否有未恢复的 auto-draft（上次没 ⌘S 就关掉 panel /
    // Esc 取消等情况下被自动写入的）。draft.content 与 currentMd（磁盘版）
    // 不同时弹"恢复"banner 让 owner 决策。相同则静默清掉（无意义残留）。
    try {
      const raw = window.localStorage.getItem(`pet-detail-draft-${taskTitle}`);
      if (raw) {
        const parsed = JSON.parse(raw) as { content?: unknown; ts?: unknown };
        if (
          typeof parsed.content === "string" &&
          typeof parsed.ts === "number" &&
          parsed.content !== currentMd
        ) {
          setPendingDraft({
            title: taskTitle,
            content: parsed.content,
            ts: parsed.ts,
          });
        } else {
          // 与磁盘版一致 / 格式坏 → 清掉 stale
          window.localStorage.removeItem(`pet-detail-draft-${taskTitle}`);
          setPendingDraft(null);
        }
      } else {
        setPendingDraft(null);
      }
    } catch {
      // 解析失败 / localStorage 不可用 → 静默
      setPendingDraft(null);
    }
    // 自动滚到最新 `- [x]` 行：打开 detail 时若末尾含完成行，让 owner 一眼看到
    // "最近一次动作"。光标落到该行末尾，按 Enter 即起新一行接着记。无 done
    // 行时不动 cursor / scroll，让用户从文首开始读 / 写。rAF 等 React 提交 +
    // textarea autoFocus 完成后再操作 selection / scrollTop。
    requestAnimationFrame(() => {
      if (!currentMd) return;
      const lines = currentMd.split("\n");
      let lastDoneLineStart = -1;
      let lineIdxOfLastDone = -1;
      let offset = 0;
      for (let i = 0; i < lines.length; i++) {
        if (/^\s*- \[[xX]\] /.test(lines[i])) {
          lastDoneLineStart = offset;
          lineIdxOfLastDone = i;
        }
        offset += lines[i].length + 1; // +1 for `\n` separator
      }
      if (lastDoneLineStart < 0) return;
      const ta = detailEditorRef.current;
      if (!ta) return;
      // 光标到该行末尾。用户敲 Enter 即新起一行写下一条完成记录。
      const lineEnd = currentMd.indexOf("\n", lastDoneLineStart);
      const cursor = lineEnd === -1 ? currentMd.length : lineEnd;
      ta.selectionStart = ta.selectionEnd = cursor;
      ta.focus();
      // 强制把那行滚到 textarea 中央 —— browser 默认 focus 仅在 selection 不
      // 在 viewport 时滚，已在则不动；我们想"显示并居中显示上下文"。lineHeight
      // 估算来自 CSS 配置（fontSize 12 * lineHeight 1.65 ≈ 19.8px），略保守。
      const lineHeight = 12 * 1.65;
      ta.scrollTop = Math.max(
        0,
        lineIdxOfLastDone * lineHeight - ta.clientHeight / 2,
      );
      setDetailCursorPos(cursor);
    });
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
        // 保存成功 → 清掉 auto-draft（磁盘已是真相，draft 不再有恢复价值）
        try {
          window.localStorage.removeItem(`pet-detail-draft-${taskTitle}`);
        } catch {
          // 私密 / 配额满 → noop（下次进编辑器时检测 stale draft 自动判定
          // 与 currentMd 相同后清掉）
        }
        setPendingDraft(null);
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

  /// 内置 + 自定义合并后的完整模板列表。dropdown 用此 index 作为 value，
  /// applyTaskTemplate 用 index 取出。`useMemo` 让 customTemplates 不变时
  /// 数组身份稳定（避免 dropdown 每次 PanelTasks render 都 remount option）。
  const allTemplates = useMemo<TaskTemplate[]>(
    () => [...TASK_TEMPLATES_BUILTIN, ...customTemplates],
    [customTemplates],
  );
  /// "📋 从模板" 下拉选中后调用：把所选模板的 title/body 填入表单 state，
  /// priority 重置默认 3、due 清空。inline create form / quickAdd modal /
  /// empty-state 三处共用一份 handler。
  const applyTaskTemplate = (idx: number) => {
    const tpl = allTemplates[idx];
    if (!tpl) return;
    setTitle(tpl.title);
    setBody(tpl.body);
    setPriority(3);
    setDue("");
  };

  /// 把当前表单 title/body 存为自定义模板。空 title 拒绝（没意义）；超
  /// limit 拒绝（强制用户先清理）；label 重名（含内置）拒绝（避免下拉
  /// 视觉碰撞）。window.prompt 是 native 输入 — 与 schedule / due preset
  /// 等其它 native 控件同级简朴，不必引入额外 Modal。errMsg 复用既有
  /// 表单错误条（红字浮在按钮下）。
  const saveCurrentAsTemplate = () => {
    const t = title.trim();
    const b = body.trim();
    if (!t) {
      setErrMsg("先填标题再存模板。");
      return;
    }
    if (customTemplates.length >= CUSTOM_TEMPLATES_MAX) {
      setErrMsg(`自定义模板上限 ${CUSTOM_TEMPLATES_MAX}，请先在管理面板删几个。`);
      return;
    }
    const proposed = window.prompt(
      `命名这个模板（≤ ${CUSTOM_TEMPLATE_LABEL_MAX} 字）`,
      t.slice(0, CUSTOM_TEMPLATE_LABEL_MAX),
    );
    if (proposed === null) return; // 用户取消
    const label = proposed.trim();
    if (!label) {
      setErrMsg("模板名不能为空。");
      return;
    }
    if (label.length > CUSTOM_TEMPLATE_LABEL_MAX) {
      setErrMsg(`模板名 ≤ ${CUSTOM_TEMPLATE_LABEL_MAX} 字。`);
      return;
    }
    if (allTemplates.some((c) => c.label === label)) {
      setErrMsg(`模板名「${label}」已存在。`);
      return;
    }
    setCustomTemplates((prev) => [...prev, { label, title: t, body: b }]);
    setErrMsg("");
  };
  /// 删除一条自定义模板。按 label 匹配（label 是 unique，前面 saveCurrent
  /// 已拒重名）。删完不需要二次确认 —— 用户可以再「存为」一次重建，损失
  /// 极低；多一道确认反而打扰。
  const deleteCustomTemplate = (label: string) => {
    setCustomTemplates((prev) => prev.filter((c) => c.label !== label));
  };

  const handleCreate = async (openDetailAfter: boolean = false) => {
    setErrMsg("");
    if (!title.trim()) {
      setErrMsg("标题不能为空");
      return;
    }
    setCreating(true);
    const titleTrimmed = title.trim();
    try {
      await invoke<string>("task_create", {
        args: {
          title: titleTrimmed,
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
      // ⌘⇧Enter 路径：建完立即打开 detail.md 编辑器空白态，让 owner 进
      // "写进度笔记" 流。task_create 后端默认不写 detail.md，初始 ""。
      // setPendingTitleFocus 顺带把焦点滚到新行 + scrollIntoView。
      if (openDetailAfter) {
        handleEnterEditDetail(titleTrimmed, "");
        setPendingTitleFocus(titleTrimmed);
      }
    } catch (e) {
      setErrMsg(`创建失败：${e}`);
    } finally {
      setCreating(false);
    }
  };

  // R120: 创建表单内 ⌘Enter / Ctrl+Enter 提交。仅在 input/textarea focus
  // 时触发（scoped 到 4 个表单字段的 onKeyDown），不挂全局；creating 守卫
  // 防 race 重复创建；preventDefault 让 textarea 内按 ⌘Enter 不换行。
  // ⌘⇧Enter / Ctrl+⇧+Enter：建完立即打开 detail.md 编辑器（键盘党"建+
  // 编辑" 一键 flow）；⌘Enter 仅创建（既有行为保留）。
  const handleFormKeyDown = (
    e: React.KeyboardEvent<HTMLInputElement | HTMLTextAreaElement>,
  ) => {
    if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
      e.preventDefault();
      if (creating) return;
      void handleCreate(e.shiftKey);
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
  /// `p` 键盘快捷反转 pinned：与桌面右键菜单「📌 钉住 / 📌 取消钉住」对偶 +
  /// 同 strip-before-write 后端命令。fire-and-forget 错误用 setActionErr 显
  /// 3.5s。pin 与 status 正交 → done / cancelled 也接受（让 owner 复盘时也能
  /// 标"这条 done 任务是经典作"）。
  const handleTogglePinned = async (taskTitle: string, nextPinned: boolean) => {
    setActionErr("");
    setBusyTitle(taskTitle);
    try {
      await invoke<void>("task_set_pinned", {
        title: taskTitle,
        pinned: nextPinned,
      });
      await reload();
    } catch (e) {
      setActionErr(`${nextPinned ? "钉住" : "取消钉住"}失败：${e}`);
      window.setTimeout(() => setActionErr(""), 3500);
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

  /// 批量钉住：跳过已 pinned 的（reload 后 chip / row 不变 = noop 不报错，但
  /// 计入 skipped 让 toast 文案清楚"我跳过了多少")。strip-before-write 后端
  /// 已保证幂等，predicate 这里仅是为了 toast 的"跳过 N 条已钉住"反馈。
  const handleBulkPin = useCallback(async () => {
    await runBulk(
      "钉住",
      (t) => !t.pinned,
      "已钉住",
      async (title) => {
        await invoke<void>("task_set_pinned", { title, pinned: true });
      },
    );
  }, [runBulk]);

  /// 批量取消钉住：跳过未 pinned 的（同上 — 后端 strip 也是 noop-friendly，
  /// predicate 是给用户的"跳过 N 条未钉住"清楚反馈）。
  const handleBulkUnpin = useCallback(async () => {
    await runBulk(
      "取消钉住",
      (t) => !!t.pinned,
      "未钉住",
      async (title) => {
        await invoke<void>("task_set_pinned", { title, pinned: false });
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
      const oldPri = source.priority;
      const newPri = target.priority;
      try {
        await invoke<void>("task_set_priority", {
          title: sourceTitle,
          priority: newPri,
        });
        await reload();
        // 1.5s toast 反馈让 owner 看到具体值变更 —— 拖拽是空间操作，没数字
        // 反馈时 owner 不确定"我刚拖到哪个 priority 上了"。bulkResultMsg
        // 也用作 inline-edit P pill click 等其它 priority 改动反馈，UX 一
        // 致；含箭头方向让"升 / 降"显式。
        const arrow = newPri > oldPri ? "↑ 升" : "↓ 降";
        setBulkResultMsg(
          `🎯 拖动「${sourceTitle}」P${oldPri} → P${newPri}（${arrow}）`,
        );
        window.setTimeout(() => setBulkResultMsg(""), 1500);
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
    .filter((t) => (pinnedFilter ? !!t.pinned : true))
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

  /// detail 编辑器 ↑ / ↓ 上一条 / 下一条任务导航。让 owner 连续 review 多 task
  /// 不必关 detail → click 下条 → 再开 detail 三步。dirty 内容 sync-flush 进
  /// draft localStorage（与 60s autosave 同 key），避免切走前 dirty 未持久化
  /// 丢内容（autosave 60s tick 可能还没跑到）。target detail 优先读 detailMap
  /// 缓存，未命中走 task_get_detail。target 切换后调 setPendingTitleFocus 让
  /// 既有"清 filter / 显 finished / scrollIntoView" pipeline 把目标 row 滚进
  /// 视野（与"完成小卡跳行" / task ref chip click 同 jump-to pipeline）。
  const handleNavigateDetail = useCallback(
    async (direction: "prev" | "next") => {
      const curTitle = editingDetailTitle;
      if (!curTitle) return;
      const curIdx = visibleTasks.findIndex((t) => t.title === curTitle);
      if (curIdx === -1) return;
      const targetIdx = direction === "prev" ? curIdx - 1 : curIdx + 1;
      if (targetIdx < 0 || targetIdx >= visibleTasks.length) return;
      const target = visibleTasks[targetIdx];
      // 1. dirty 内容 sync flush 到 draft（防 autosave 60s tick 没跑到丢内容）
      const dirty =
        editingDetailContent !== editingDetailOriginalRef.current;
      if (dirty) {
        try {
          window.localStorage.setItem(
            `pet-detail-draft-${curTitle}`,
            JSON.stringify({
              content: editingDetailContent,
              ts: Date.now(),
            }),
          );
        } catch (e) {
          console.error("flush draft on navigate failed:", e);
        }
      }
      // 2. 拉 target detail —— detailMap 缓存命中直接用；未命中走 IO。
      let targetMd: string;
      const cached = detailMap[target.title];
      if (cached) {
        targetMd = cached.detail_md;
      } else {
        try {
          const fresh = await invoke<TaskDetail>("task_get_detail", {
            title: target.title,
          });
          targetMd = fresh.detail_md;
          setDetailMap((prev) => ({ ...prev, [target.title]: fresh }));
        } catch (e) {
          console.error("task_get_detail on navigate failed:", e);
          targetMd = "";  // 拉失败用空内容开，让 owner 能写新内容
        }
      }
      // 3. 切换编辑器 + 滚 target row 进视野
      handleEnterEditDetail(target.title, targetMd);
      setPendingTitleFocus(target.title);
    },
    [
      editingDetailTitle,
      editingDetailContent,
      visibleTasks,
      detailMap,
      handleEnterEditDetail,
    ],
  );

  /// detail.md 编辑器 ⌘[ / ⌘] 快捷键：与 ↑/↓ 按钮同 handler。仅
  /// editingDetailTitle 非空（即正在编辑某 task 的 detail）时挂；textarea
  /// focused 时也响应（⌘[ 不冲突 textarea 内默认行为；macOS 系统级 ⌘[ 通常
  /// 用于"后退"，PanelTasks 非浏览器视图 → 这里抢用合理）。preventDefault
  /// 阻止任何潜在系统行为。Windows / Linux 走 Ctrl+[ / Ctrl+]（e.metaKey ||
  /// e.ctrlKey）。
  useEffect(() => {
    if (editingDetailTitle === null) return;
    const handler = (e: KeyboardEvent) => {
      if (!(e.metaKey || e.ctrlKey)) return;
      if (e.shiftKey || e.altKey) return;
      if (e.key === "[") {
        e.preventDefault();
        void handleNavigateDetail("prev");
      } else if (e.key === "]") {
        e.preventDefault();
        void handleNavigateDetail("next");
      } else if (e.key === "k" || e.key === "K") {
        // ⌘K 唤起 task quick-find palette。仅 editingDetailTitle 非空（在编
        // 辑 detail）时挂监听；与既有 ⌘[ / ⌘] 同 effect，共享 dependency。
        e.preventDefault();
        setTaskPaletteOpen(true);
        setTaskPaletteMode("jump");
        setPaletteQuery("");
        setPaletteSelectedIdx(0);
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [editingDetailTitle, handleNavigateDetail]);

  /// task quick-find palette 状态：⌘K 在 detail.md 编辑器内唤起。fuzzy
  /// 匹配 visibleTasks（含 filter / sort 后的视图），Enter 跳到选中 task 的
  /// detail.md 编辑器（复用 handleNavigateDetail-style 切换 pipeline）。
  /// Esc 关闭，↑↓ 移动 selectedIdx。
  /// mode === "jump"（默认 ⌘K 路径）：Enter switchToTaskDetail。
  /// mode === "insertRef"（toolbar 「」按钮路径）：Enter 把 `「title」` 插
  /// 到 textarea 光标处，不切 task。同一 palette UI 双用例分流。
  const [taskPaletteOpen, setTaskPaletteOpen] = useState(false);
  const [taskPaletteMode, setTaskPaletteMode] = useState<"jump" | "insertRef">(
    "jump",
  );
  const [paletteQuery, setPaletteQuery] = useState("");
  const [paletteSelectedIdx, setPaletteSelectedIdx] = useState(0);
  const paletteInputRef = useRef<HTMLInputElement>(null);
  /// 把 `「title」` 全角直角引号 ref token 插到 detail.md textarea 当前光标
  /// 位置（或替换选区）。光标落 token 末尾让 owner 接着敲。token 形态与
  /// renderContentWithTaskRefs / `🔗 拼为 ref` 同协议。
  const insertTaskRefAtCursor = useCallback(
    (title: string) => {
      const ta = detailEditorRef.current;
      if (!ta) return;
      const start = ta.selectionStart;
      const end = ta.selectionEnd;
      const token = `「${title}」`;
      const before = editingDetailContent.slice(0, start);
      const after = editingDetailContent.slice(end);
      const next = `${before}${token}${after}`;
      setEditingDetailContent(next);
      window.requestAnimationFrame(() => {
        const cur = detailEditorRef.current;
        if (!cur) return;
        const pos = start + token.length;
        cur.focus();
        cur.setSelectionRange(pos, pos);
      });
    },
    [editingDetailContent],
  );
  /// 一次切到任意 target title 的 detail 编辑器 helper：复用 handleNavigateDetail
  /// 的"dirty flush draft + detailMap cache / IO fallback + handleEnterEditDetail
  /// + setPendingTitleFocus" 五步链路，仅 target idx 取法不同（palette 是
  /// owner select，nav 是 prev/next）。
  const switchToTaskDetail = useCallback(
    async (targetTitle: string) => {
      const curTitle = editingDetailTitle;
      if (!curTitle || curTitle === targetTitle) return;
      const target = visibleTasks.find((t) => t.title === targetTitle);
      if (!target) return;
      const dirty =
        editingDetailContent !== editingDetailOriginalRef.current;
      if (dirty) {
        try {
          window.localStorage.setItem(
            `pet-detail-draft-${curTitle}`,
            JSON.stringify({
              content: editingDetailContent,
              ts: Date.now(),
            }),
          );
        } catch (e) {
          console.error("flush draft on palette switch failed:", e);
        }
      }
      let targetMd: string;
      const cached = detailMap[target.title];
      if (cached) {
        targetMd = cached.detail_md;
      } else {
        try {
          const fresh = await invoke<TaskDetail>("task_get_detail", {
            title: target.title,
          });
          targetMd = fresh.detail_md;
          setDetailMap((prev) => ({ ...prev, [target.title]: fresh }));
        } catch (e) {
          console.error("task_get_detail on palette switch failed:", e);
          targetMd = "";
        }
      }
      handleEnterEditDetail(target.title, targetMd);
      setPendingTitleFocus(target.title);
    },
    [
      editingDetailTitle,
      editingDetailContent,
      visibleTasks,
      detailMap,
      handleEnterEditDetail,
    ],
  );

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

  // 📌 钉住任务计数：只数活动态 —— 与 dueTodayCount / createdTodayCount 同
  // 语义。finished 行不参与 chip 提示（pinned 的 done 任务已没人关心了）。
  const pinnedCount = useMemo(() => {
    let n = 0;
    for (const t of tasks) {
      if (!isFinished(t.status) && t.pinned) n += 1;
    }
    return n;
  }, [tasks]);

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
  /// 🎯 紧迫任务（P0-P2 未完成）计数：高优先级 backlog 信号。tasks 全集
  /// （活动态）走过滤；done / cancelled 不计。0 时不渲染 chip。priorityCounts
  /// 已经按 priority 升序排好；reduce 前 3 档求和即得。
  const urgentTopPriorityCount = useMemo(() => {
    let n = 0;
    for (const t of tasks) {
      if (isFinished(t.status)) continue;
      if (t.priority <= 2) n += 1;
    }
    return n;
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
    originFilter.size > 0 ||
    pinnedFilter;

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
    handleTogglePinned,
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

  // 跨窗口 deeplink：pet 窗 🔴 逾期 pill 写过来的 due filter。挂载后立即
  // 应用一次 + consume 回 PanelApp 清空 state，避免用户后续手改 filter
  // 时被 stale 值反复覆盖。
  useEffect(() => {
    if (!pendingDueFilter) return;
    setDueFilter(pendingDueFilter);
    onConsumePendingDueFilter?.();
  }, [pendingDueFilter, onConsumePendingDueFilter]);

  /// 跨窗口 deeplink：pet 窗 ChatMini "💾 转 task" 按钮把 body 推过来。挂
  /// 载后立即 setTitle (前 30 字 default 让 owner 可改) + setBody (全文) +
  /// setQuickAddOpen(true) + consume 清。前 30 字作 title 让 owner ⌘Enter
  /// 即创建；若觉得 default 不合适在 modal 内手改。
  useEffect(() => {
    if (!pendingQuickAddBody) return;
    const body = pendingQuickAddBody;
    // title default: 第一行（防换行 / markdown noise），cap 30 字符（与 backend
    // task title input 上限对齐）。trim 前导空白
    const titleDefault = body
      .split("\n")[0]
      .replace(/^\s+/, "")
      .slice(0, 30);
    setTitle(titleDefault);
    setBody(body);
    setQuickAddOpen(true);
    onConsumePendingQuickAddBody?.();
  }, [pendingQuickAddBody, onConsumePendingQuickAddBody]);

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
    container: { padding: 22, overflowY: "auto" as const, height: "100%" },
    section: { marginBottom: 20 },
    sectionTitle: {
      fontSize: 14,
      fontWeight: 600,
      color: "var(--pet-color-fg)",
      marginBottom: 12,
      paddingBottom: 10,
      // 渐变 hairline 与 SectionTitle / PanelMemory.s.sectionTitle 一致
      backgroundImage:
        "linear-gradient(90deg, transparent, var(--pet-color-border) 12%, var(--pet-color-border) 88%, transparent)",
      backgroundRepeat: "no-repeat",
      backgroundSize: "100% 1px",
      backgroundPosition: "bottom",
      letterSpacing: 0.2,
    },
    formCard: {
      padding: "16px 18px",
      background:
        "linear-gradient(180deg, color-mix(in srgb, var(--pet-color-accent) 3%, var(--pet-color-card)) 0%, var(--pet-color-card) 55%)",
      border: "1px solid var(--pet-color-border)",
      borderRadius: 12,
      marginBottom: 16,
      boxShadow: "var(--pet-shadow-sm)",
    },
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
        /* detail.md "● 未保存" badge 持续 dirty > 60s 时的脉冲。柔和不抢
           主视觉但让 owner 余光能瞄到 —— 解决"写到一半切走没 ⌘S 丢内容"
           的真实痛点。 */
        @keyframes pet-detail-dirty-pulse {
          0%, 100% { opacity: 1; }
          50%      { opacity: 0.55; }
        }
        @media (prefers-reduced-motion: reduce) {
          [style*="pet-task-now-pulse"] { animation: none !important; }
          [style*="pet-detail-dirty-pulse"] { animation: none !important; }
        }
      `}</style>
      <div style={s.section}>
        <div
          style={{ ...s.sectionTitle, display: "flex", alignItems: "center", gap: 6, cursor: "pointer", userSelect: "none" }}
          onClick={() => setCreateFormExpanded((v) => !v)}
          title={
            createFormExpanded
              ? "点击折叠新建任务表单（节省垂直空间，跨 session 记忆） · ⌘N 任意时刻弹快速建任务 modal"
              : "点击展开新建任务表单 · ⌘N 任意时刻弹快速建任务 modal"
          }
        >
          <span style={{ width: 10, fontFamily: "monospace", color: "var(--pet-color-muted)" }}>
            {createFormExpanded ? "▾" : "▸"}
          </span>
          <span>新建任务</span>
          {/* ⌘N hint chip：与 ⌘F / ⌘[ 等 PanelTasks 内既有快捷键一致的 hint
              style。让 owner 一眼发现 ⌘N 可以全局唤起 quickAdd modal（不必
              先点本 section header）。fontSize 10 / muted color 不喧宾夺
              主；只在 createFormExpanded === false 时显，展开时该 section
              下面已有大 input 区，hotkey hint 显得多余。 */}
          {!createFormExpanded && (
            <span
              style={{
                fontSize: 10,
                color: "var(--pet-color-muted)",
                fontWeight: 400,
                marginLeft: 4,
                fontFamily: "'SF Mono', 'Menlo', monospace",
                background: "var(--pet-color-border)",
                borderRadius: 4,
                padding: "1px 5px",
                opacity: 0.7,
              }}
              title="按 ⌘N（macOS）/ Ctrl+N（Windows/Linux）随时弹快速建任务 modal —— 无需先展开本 section"
            >
              ⌘N
            </span>
          )}
          {/* 队列健康 chip：collapsed 时显当前 🔴 逾期 + ❌ 失败计数。让
              owner 想"先加一条新 task"前一眼看到队列 backlog 健康 —— "先
              清掉旧任务还是先派新单"。两者都为 0 时 chip 不显（不打扰
              clean state）。点击不抢 createFormExpanded toggle —— 仅信息
              性显示，e.stopPropagation 防 click 触发 section 折叠 / 展开。 */}
          {!createFormExpanded && (overdueCount > 0 || errorTaskCount > 0) && (
            <span
              style={{
                fontSize: 10,
                fontWeight: 600,
                marginLeft: 4,
                fontFamily: "'SF Mono', 'Menlo', monospace",
                background: overdueCount > 0
                  ? "var(--pet-tint-red-bg)"
                  : "var(--pet-tint-orange-bg)",
                color: overdueCount > 0
                  ? "var(--pet-tint-red-fg)"
                  : "var(--pet-tint-orange-fg)",
                borderRadius: 4,
                padding: "1px 6px",
                whiteSpace: "nowrap",
              }}
              onClick={(e) => e.stopPropagation()}
              title={`队列里还有未处理任务：${overdueCount > 0 ? `${overdueCount} 条逾期` : ""}${overdueCount > 0 && errorTaskCount > 0 ? " · " : ""}${errorTaskCount > 0 ? `${errorTaskCount} 条失败` : ""}。先看 backlog 再加新单 / 看一眼队列健康再决定。`}
            >
              {overdueCount > 0 && `🔴 ${overdueCount}`}
              {overdueCount > 0 && errorTaskCount > 0 && " · "}
              {errorTaskCount > 0 && `❌ ${errorTaskCount}`}
            </span>
          )}
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
            <div style={{ display: "flex", alignItems: "center", gap: 4 }}>
              {/* 📋 从模板 下拉：选中后 prefill title/body/priority/due。
                  value="" 是 disabled placeholder，选完立刻 reset 让下次能
                  重选同一个模板。内置 + 用户自定义合并显示；optgroup 分组
                  让用户一眼分辨「内置范例」vs「我存的」。 */}
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
                <optgroup label="内置范例">
                  {TASK_TEMPLATES_BUILTIN.map((tpl, i) => (
                    <option key={tpl.label} value={i}>
                      {tpl.label}
                    </option>
                  ))}
                </optgroup>
                {customTemplates.length > 0 && (
                  <optgroup label="我存的">
                    {customTemplates.map((tpl, j) => (
                      <option
                        key={tpl.label}
                        value={TASK_TEMPLATES_BUILTIN.length + j}
                      >
                        {tpl.label}
                      </option>
                    ))}
                  </optgroup>
                )}
              </select>
              {/* 💾 把当前表单 title/body 存为自定义模板。title 空时禁用
                  （saveCurrentAsTemplate 内也守一次），让 button 不可点防误触。 */}
              <button
                type="button"
                onClick={saveCurrentAsTemplate}
                disabled={!title.trim()}
                title={
                  title.trim()
                    ? "把当前 title/body 存为我的模板"
                    : "先填标题再存模板"
                }
                style={{
                  padding: "2px 8px",
                  fontSize: 11,
                  border: "1px solid var(--pet-color-border)",
                  borderRadius: 4,
                  background: title.trim()
                    ? "var(--pet-color-card)"
                    : "var(--pet-color-bg)",
                  color: title.trim()
                    ? "var(--pet-color-fg)"
                    : "var(--pet-color-muted)",
                  cursor: title.trim() ? "pointer" : "not-allowed",
                  fontFamily: "inherit",
                  whiteSpace: "nowrap",
                }}
              >
                💾 存为
              </button>
              {customTemplates.length > 0 && (
                <button
                  type="button"
                  onClick={() => setTemplatesManagerOpen(true)}
                  title={`管理 ${customTemplates.length} 条自定义模板`}
                  style={{
                    padding: "2px 8px",
                    fontSize: 11,
                    border: "1px solid var(--pet-color-border)",
                    borderRadius: 4,
                    background: "var(--pet-color-card)",
                    color: "var(--pet-color-muted)",
                    cursor: "pointer",
                    fontFamily: "inherit",
                    whiteSpace: "nowrap",
                  }}
                >
                  管理 {customTemplates.length}
                </button>
              )}
            </div>
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
              {/* 快捷预设 chips：高频 due 场景一键填，省手敲 datetime-local。
                  Date 在 click 时实时求值（每次点都按"现在"算）；helper 是纯
                  函数（formatDueInput / dueTonight / ...）跨 click 行为一致。
                  「清除」单独靠右，与"赋值"chip 视觉分离避免误点。 */}
              <div
                style={{
                  display: "flex",
                  flexWrap: "wrap",
                  gap: 6,
                  marginTop: 6,
                  alignItems: "center",
                }}
                aria-label="due 快捷预设"
              >
                {([
                  { label: "今晚", title: "今晚 18:00（若已过则明晚）", build: dueTonight },
                  { label: "明天", title: "明天 09:00", build: dueTomorrow },
                  { label: "周一", title: "下周一 09:00", build: dueNextMonday },
                  { label: "一周后", title: "+7 天（保留当前时分）", build: dueOneWeek },
                ] as const).map(({ label, title: tipText, build }) => (
                  <button
                    key={label}
                    type="button"
                    onClick={() => setDue(build(new Date()))}
                    title={tipText}
                    style={{
                      padding: "3px 10px",
                      fontSize: 11,
                      borderRadius: 999,
                      border: "1px solid var(--pet-color-border)",
                      background:
                        "color-mix(in srgb, var(--pet-color-accent) 6%, var(--pet-color-card))",
                      color: "var(--pet-color-fg)",
                      cursor: "pointer",
                      lineHeight: 1.4,
                      letterSpacing: 0.2,
                    }}
                  >
                    {label}
                  </button>
                ))}
                {due && (
                  <button
                    type="button"
                    onClick={() => setDue("")}
                    title="清除 due"
                    style={{
                      padding: "3px 10px",
                      fontSize: 11,
                      borderRadius: 999,
                      border:
                        "1px solid color-mix(in srgb, var(--pet-tint-red-fg) 35%, var(--pet-color-border))",
                      background: "var(--pet-color-card)",
                      color: "var(--pet-tint-red-fg)",
                      cursor: "pointer",
                      marginLeft: "auto",
                      lineHeight: 1.4,
                    }}
                  >
                    清除
                  </button>
                )}
              </div>
            </div>
          </div>
          <button
            style={creating || !title.trim() ? s.btnDisabled : s.btnPrimary}
            onClick={() => void handleCreate(false)}
            disabled={creating || !title.trim()}
            title="创建任务（⌘Enter / Ctrl+Enter 等价）。按 ⌘⇧Enter / Ctrl+⇧+Enter 创建并立即打开 detail.md 编辑器（键盘党'建+编辑'一键流）。"
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
                                setPinnedFilter(false);
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
            placeholder="按标题或内容搜索…（⌘F / ⌘K / `/` 聚焦 · Enter 入历史）"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            list="pet-tasks-search-history"
            onKeyDown={(e) => {
              // Esc：非空时清掉 query；空时让出键位（让全局 Esc 关 modal 等）。
              if (e.key === "Escape" && search) {
                e.preventDefault();
                setSearch("");
                return;
              }
              // Enter：把当前 query 入 history（与 PanelMemory pushSearchHistory
              // 同模式）。live filter 已在 onChange 即时生效，Enter 只是"我用
              // 这条 query 用得满意，记一下"的显式信号。
              if (e.key === "Enter" && search.trim()) {
                e.preventDefault();
                pushTaskSearchHistory(search);
              }
            }}
            style={s.searchInput}
          />
          {/* 最近 5 条搜索 keyword：native datalist 自动浮 dropdown。Enter
              成功的 query 入栈；空 history 时不渲染 option 即 noop。 */}
          {taskSearchHistory.length > 0 && (
            <datalist id="pet-tasks-search-history">
              {taskSearchHistory.map((kw) => (
                <option key={kw} value={kw} />
              ))}
            </datalist>
          )}
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
                  setPinnedFilter(false);
                }}
                style={s.searchClearBtn}
                title="一键清掉全部 active filter（search / tag / due / priority / origin / pinned）"
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
        {(dueTodayCount > 0 || overdueCount > 0 || createdTodayCount > 0 || pinnedCount > 0 || priorityCounts.length > 0 || originCounts.tg > 0 || errorTaskCount > 0 || finishedTaskCount > 0 || completionStats.today > 0 || urgentTopPriorityCount > 0) && (
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
            {/* 🎯 P0-P2 紧迫 chip：高优先级未完成 backlog。amber tint 视
                觉 = "需要注意" 介于 red overdue / blue stats 间。0 时不显。
                informational 不接 filter（既有 priority chip 已能逐档点 filter；
                此 chip 是 "高优先级总览" 信号）。 */}
            {urgentTopPriorityCount > 0 && (
              <span
                style={{
                  fontSize: 11,
                  padding: "2px 8px",
                  borderRadius: 8,
                  background: "var(--pet-tint-amber-bg, var(--pet-tint-yellow-bg))",
                  color: "var(--pet-tint-amber-fg, var(--pet-tint-yellow-fg))",
                  fontWeight: 600,
                  whiteSpace: "nowrap",
                }}
                title={`高优先级 (P0-P2) 未完成任务 ${urgentTopPriorityCount} 条。owner 应优先处理这些；queue 顶有积压时考虑暂缓低优先级。`}
              >
                🎯 紧迫 {urgentTopPriorityCount}
              </span>
            )}
            {/* ✓ 今日已完成 N 绿 chip：与 🔴 逾期 / 📅 今日到期 chip 同行
                显，让 owner 看到 "今天完成多少条" momentum。0 时不显（与
                其它计数 chip 同稀疏模板）。informational 不接 filter（点
                击不切 view —— 仅信息性显示）。 */}
            {completionStats.today > 0 && (
              <span
                style={{
                  fontSize: 11,
                  padding: "2px 8px",
                  borderRadius: 8,
                  background: "var(--pet-tint-green-bg)",
                  color: "var(--pet-tint-green-fg)",
                  fontWeight: 600,
                  whiteSpace: "nowrap",
                }}
                title={`今日完成 ${completionStats.today} 条任务${completionStats.week > completionStats.today ? `（近 7 天累计 ${completionStats.week} 条）` : ""}`}
              >
                ✓ 今日完成 {completionStats.today}
              </span>
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
            {/* 📌 钉住 chip：> 0 时常驻 chip 行。激活态走 amber tint
                （与 due / priority 系列色族错开 —— pinned 是"owner 标注"维度
                而非 due/时态/priority 维度，独立配色让识别更快）。pinnedFilter
                在 localStorage 持久 —— 用户开过滤后切走再回到面板仍保留。 */}
            {pinnedCount > 0 && (
              <span
                role="button"
                tabIndex={0}
                onClick={() => setPinnedFilter((v) => !v)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" || e.key === " ") {
                    e.preventDefault();
                    setPinnedFilter((v) => !v);
                  }
                }}
                aria-pressed={pinnedFilter}
                title={
                  pinnedFilter
                    ? `已仅显钉住任务（${pinnedCount} 条）。点击恢复全部。`
                    : `仅显示 owner 钉住的任务（${pinnedCount} 条）`
                }
                style={{
                  display: "inline-flex",
                  alignItems: "center",
                  gap: 4,
                  padding: "2px 8px",
                  fontSize: 11,
                  borderRadius: 999,
                  cursor: "pointer",
                  userSelect: "none",
                  background: pinnedFilter
                    ? "var(--pet-tint-amber-fg, #d97706)"
                    : "var(--pet-tint-amber-bg, #fef3c7)",
                  color: pinnedFilter
                    ? "#fff"
                    : "var(--pet-tint-amber-fg, #92400e)",
                  border: pinnedFilter
                    ? "1px solid var(--pet-tint-amber-fg, #d97706)"
                    : "1px solid color-mix(in srgb, var(--pet-tint-amber-fg, #d97706) 30%, transparent)",
                }}
              >
                📌 {pinnedCount}
              </span>
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
                onClick={handleBulkPin}
                title="批量钉住所有选中任务（写 [pinned] marker；已钉住的跳过）"
              >
                📌 钉住
              </button>
              <button
                style={bulkBusy ? s.bulkBtnDisabled : s.bulkBtn}
                disabled={bulkBusy}
                onClick={handleBulkUnpin}
                title="批量取消钉住所有选中任务（剥 [pinned] marker；未钉住的跳过）"
              >
                📌 取消钉
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
                  setPinnedFilter(false);
                }}
                title="清掉全部 active filter（search / tag / due / priority / origin / pinned）"
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
                title="点击打开新建表单，用一个具体任务范例预填 · 任意时刻 ⌘N 也可弹空白 modal"
              >
                📋 用范例预填一条 (⌘N 弹空白)
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
                {/* 行 idx / total hover 角标：长队列时 owner 看到当前
                    行在第几位 / 全队列多大。仅 hover 该行时显（pointerEvents
                    none 让 click 穿透到行 hit area）。右上角 absolute 浮
                    layout 无 reflow。idx +1 一基（display friendly），与
                    visibleTasks 顺序一致（含 filter / sort 后视图）。 */}
                {taskPreviewHoverTitle === t.title && visibleTasks.length > 5 && (
                  <span
                    style={{
                      position: "absolute",
                      top: 4,
                      right: 6,
                      fontSize: 9,
                      color: "var(--pet-color-muted)",
                      fontFamily: "'SF Mono', 'Menlo', monospace",
                      background: "var(--pet-color-card)",
                      padding: "0 4px",
                      borderRadius: 3,
                      lineHeight: "12px",
                      opacity: 0.6,
                      pointerEvents: "none",
                      zIndex: 5,
                    }}
                    aria-hidden
                  >
                    {idx + 1} / {visibleTasks.length}
                  </span>
                )}
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
                              {/* hover preview 容器自己挂 pointerEvents:none
                                  让 hover area 不抢 row 主 onClick；这条 chip
                                  显式 override 为 auto 让 click 仍 reach 这里
                                  调 memory_reveal_detail_in_finder 跳系统文件
                                  管理器。其它 hover preview 内容仍透明穿透。 */}
                              <button
                                type="button"
                                onClick={async (e) => {
                                  e.stopPropagation();
                                  if (!t.detail_path) return;
                                  try {
                                    await invoke<void>(
                                      "memory_reveal_detail_in_finder",
                                      { detailPath: t.detail_path },
                                    );
                                  } catch (err) {
                                    setActionErr(
                                      `在 Finder 打开失败：${err}（detail.md 可能尚未保存到磁盘）`,
                                    );
                                    window.setTimeout(
                                      () => setActionErr(""),
                                      3500,
                                    );
                                  }
                                }}
                                onMouseDown={(e) => e.stopPropagation()}
                                title={`在系统文件管理器里显示 detail.md（路径：memories/${t.detail_path}）。macOS Finder 会高亮选中文件，方便拖入附件 / git add / 用其它编辑器打开 / 重命名。`}
                                style={{
                                  pointerEvents: "auto",
                                  background: "transparent",
                                  border: "none",
                                  color: "var(--pet-color-muted)",
                                  fontFamily: "inherit",
                                  fontSize: "inherit",
                                  padding: 0,
                                  cursor: "pointer",
                                  textAlign: "left",
                                  textDecoration: "underline dotted",
                                  textUnderlineOffset: 2,
                                }}
                              >
                                📄 {t.detail_path}
                              </button>
                            </div>
                            <div style={{ whiteSpace: "pre-wrap" }}>
                              {/* hover preview 走 LinkCard "raw" 模式：保留原
                                  "pre-wrap markdown 字面" 视觉，但 bare https
                                  URL chip 化 emoji + hostname。让 owner hover
                                  时一眼分辨"这条 detail 引用了 GitHub / Linear
                                  / Figma" 等 —— 与展开详情段的 LinkCard 体验
                                  同源、视觉同源、性能更轻（不重跑 parseMarkdown）。 */}
                              {renderDetailTextWithLinkCards(
                                detailSnippet,
                                `hover-${t.title}`,
                                "raw",
                                taskLookupForRefs,
                                handleTaskRefClick,
                              )}
                            </div>
                          </>
                        )}
                        {/* 右键操作 onboarding hint：与 PanelMemory item hover
                            底脚 ✏️ hint 同模板（iter #201）。dashed-top divider
                            + fontSize 9 + opacity 0.7 italic muted，让"右键
                            查看所有操作 (mark done / 改 priority / snooze /
                            pin / silent / 复制 / ...)" 这条隐藏交互可被首次
                            用户发现。任何 hover 都显，老 owner 一行噪音可
                            忽略。 */}
                        <div
                          style={{
                            marginTop: 6,
                            paddingTop: 4,
                            borderTop: "1px dashed var(--pet-color-border)",
                            fontSize: 9,
                            color: "var(--pet-color-muted)",
                            fontStyle: "italic",
                            opacity: 0.7,
                          }}
                        >
                          🖱️ 右键查看所有操作（done / 改 priority / snooze / pin / silent / 复制 / ...）· 点击行 折叠/展开
                        </div>
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
                    {/* 📌 pinned chip：owner 标记 `[pinned]`。amber tint 与 chip
                        行的「📌 N」过滤 chip 同色族；显示状态不分 status（done
                        / cancelled 也保留视觉一致，但 chip 行计数只数活动态）。
                        右键菜单可 toggle 钉 / 取消钉。 */}
                    {t.pinned && (
                      <span
                        title="owner 已钉住本任务（描述含 [pinned]）。chip 行「📌 N」过滤可一键集中查看。右键 → 取消钉住。"
                        style={{
                          display: "inline-flex",
                          alignItems: "center",
                          gap: 2,
                          padding: "1px 7px",
                          fontSize: 10,
                          fontWeight: 600,
                          lineHeight: 1.4,
                          letterSpacing: 0.2,
                          borderRadius: 999,
                          background: "var(--pet-tint-amber-bg, #fef3c7)",
                          color: "var(--pet-tint-amber-fg, #92400e)",
                          border:
                            "1px solid color-mix(in srgb, var(--pet-tint-amber-fg, #d97706) 30%, transparent)",
                          whiteSpace: "nowrap",
                        }}
                        aria-label="已钉住"
                      >
                        📌
                      </span>
                    )}
                    {/* 任务 snooze 💤 chip：description 含 [snooze: ...] 且未过点。
                        后端 build_task_view 已做"过点 → 不填"过滤，前端只需判 truthy。
                        tooltip 显完整时刻；chip 文字短到 "至 MM-DD HH:MM"（13 字符
                        以内）。终态行不渲染 —— 暂停语义对结束态无意义。 */}
                    {t.snoozed_until && t.status !== "done" && t.status !== "cancelled" && (() => {
                      const until = t.snoozed_until!;
                      // `YYYY-MM-DDThh:mm` → 短串 `MM-DD HH:MM`
                      const short =
                        until.length >= 16
                          ? `${until.slice(5, 10)} ${until.slice(11, 16)}`
                          : until;
                      const open = snoozePickerTitle === t.title;
                      return (
                        <span
                          style={{ position: "relative", display: "inline-block" }}
                        >
                          <button
                            type="button"
                            onMouseDown={(e) => e.stopPropagation()}
                            onClick={(e) => {
                              e.stopPropagation();
                              setSnoozePickerTitle((cur) =>
                                cur === t.title ? null : t.title,
                              );
                            }}
                            title={`本任务已 [snooze:] 暂停，至 ${until.replace("T", " ")} 之前不会出现在 proactive 选单。点击改 / 解除`}
                            style={{
                              display: "inline-flex",
                              alignItems: "center",
                              gap: 2,
                              padding: "1px 7px",
                              fontSize: 10,
                              fontWeight: 600,
                              lineHeight: 1.4,
                              letterSpacing: 0.2,
                              borderRadius: 999,
                              background: "var(--pet-tint-purple-bg)",
                              color: "var(--pet-tint-purple-fg)",
                              border:
                                "1px solid color-mix(in srgb, var(--pet-tint-purple-fg) 30%, transparent)",
                              whiteSpace: "nowrap",
                              cursor: "pointer",
                              fontFamily: "inherit",
                            }}
                            aria-label="暂停至"
                          >
                            💤 至 {short}
                          </button>
                          {/* mini popover: 4 个 preset (30m / 今晚 / 明早 /
                              下周一) + 解除。click outside / Esc 关。复用
                              task_set_snooze backend preset 入参（iter
                              #200 加 EN/CJK 关键词解析）。busyTitle 守。 */}
                          {open && (
                            <div
                              onMouseDown={(e) => e.stopPropagation()}
                              onClick={(e) => e.stopPropagation()}
                              style={{
                                position: "absolute",
                                top: "calc(100% + 4px)",
                                left: 0,
                                minWidth: 160,
                                padding: 4,
                                background: "var(--pet-color-card)",
                                border: "1px solid var(--pet-color-border)",
                                borderRadius: 6,
                                boxShadow: "0 4px 12px rgba(0,0,0,0.18)",
                                zIndex: 30,
                                display: "flex",
                                flexDirection: "column",
                                gap: 2,
                              }}
                            >
                              {[
                                { key: "30m", label: "💤 暂停 30 分" },
                                { key: "tonight", label: "💤 至今晚 18:00" },
                                { key: "tomorrow", label: "💤 至明早 09:00" },
                                { key: "monday", label: "💤 至下周一 09:00" },
                              ].map((p) => (
                                <button
                                  key={p.key}
                                  type="button"
                                  style={{
                                    display: "block",
                                    width: "100%",
                                    textAlign: "left",
                                    padding: "5px 9px",
                                    fontSize: 11,
                                    border: "none",
                                    background: "transparent",
                                    color: "var(--pet-color-fg)",
                                    cursor: "pointer",
                                    fontFamily: "inherit",
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
                                  onClick={async () => {
                                    setSnoozePickerTitle(null);
                                    setActionErr("");
                                    setBusyTitle(t.title);
                                    try {
                                      await invoke<void>("task_set_snooze", {
                                        title: t.title,
                                        until: p.key,
                                      });
                                      await reload();
                                    } catch (e) {
                                      setActionErr(`设 snooze 失败：${e}`);
                                    } finally {
                                      setBusyTitle(null);
                                    }
                                  }}
                                >
                                  {p.label}
                                </button>
                              ))}
                              <div
                                style={{
                                  height: 1,
                                  background: "var(--pet-color-border)",
                                  margin: "2px 0",
                                }}
                              />
                              <button
                                type="button"
                                style={{
                                  display: "block",
                                  width: "100%",
                                  textAlign: "left",
                                  padding: "5px 9px",
                                  fontSize: 11,
                                  border: "none",
                                  background: "transparent",
                                  color: "var(--pet-color-accent)",
                                  cursor: "pointer",
                                  fontFamily: "inherit",
                                  borderRadius: 4,
                                  fontWeight: 600,
                                }}
                                onMouseOver={(e) => {
                                  (e.currentTarget as HTMLButtonElement).style.background =
                                    "var(--pet-color-bg)";
                                }}
                                onMouseOut={(e) => {
                                  (e.currentTarget as HTMLButtonElement).style.background =
                                    "transparent";
                                }}
                                onClick={async () => {
                                  setSnoozePickerTitle(null);
                                  setActionErr("");
                                  setBusyTitle(t.title);
                                  try {
                                    await invoke<void>("task_set_snooze", {
                                      title: t.title,
                                      until: null,
                                    });
                                    await reload();
                                  } catch (e) {
                                    setActionErr(`解除 snooze 失败：${e}`);
                                  } finally {
                                    setBusyTitle(null);
                                  }
                                }}
                              >
                                ☀️ 解除暂停
                              </button>
                            </div>
                          )}
                        </span>
                      );
                    })()}
                    {/* 任务依赖 🔒 chip：blockedBy 引用的 title 仍处 pending/error
                        时显。tooltip 列出仍卡着的 blocker。proactive prompt 已自动
                        过滤这些任务给 LLM，面板仍渲染让用户看到"为什么没人做这条"。
                        终态行（done / cancelled）computeUnresolvedBlockers 跳过。 */}
                    {(() => {
                      const blockers = blockedMap.get(t.title);
                      if (!blockers || blockers.length === 0) return null;
                      const preview =
                        blockers.length === 1
                          ? blockers[0]
                          : `${blockers[0]} +${blockers.length - 1}`;
                      return (
                        <span
                          title={`本任务被 [blockedBy: …] 依赖卡住，等下列任务完成或取消后才会出现在 proactive 选单：\n${blockers.map((b) => `· ${b}`).join("\n")}`}
                          style={{
                            display: "inline-flex",
                            alignItems: "center",
                            gap: 2,
                            padding: "1px 7px",
                            fontSize: 10,
                            fontWeight: 600,
                            lineHeight: 1.4,
                            letterSpacing: 0.2,
                            borderRadius: 999,
                            background: "var(--pet-tint-yellow-bg)",
                            color: "var(--pet-tint-yellow-fg)",
                            border:
                              "1px solid color-mix(in srgb, var(--pet-tint-yellow-fg) 30%, transparent)",
                            whiteSpace: "nowrap",
                            maxWidth: 180,
                            overflow: "hidden",
                            textOverflow: "ellipsis",
                          }}
                          aria-label={`等待 ${blockers.length} 条依赖`}
                        >
                          🔒 等 {preview}
                        </span>
                      );
                    })()}
                    {/* 🕰 老任务年龄 chip：created_at > 3 天且仍 pending/error
                        时显，配 📌 钉住 chip 帮 owner 判断「这条放了多久 / 要不
                        要拆 / 取消」。done / cancelled 不渲（静态终态，年龄无
                        actionable 信号）。muted gray 与 actionable chips（📌 /
                        💤 / 🔒）配色错开，弱化为"信息"非"动作"。tooltip 显完整
                        created_at + 相对值，让 owner 心里有数 + hover 验证。 */}
                    {(t.status === "pending" || t.status === "error") &&
                      (() => {
                        const ts = Date.parse(t.created_at);
                        if (Number.isNaN(ts)) return null;
                        const ageMs = nowMs - ts;
                        // 3 天阈值：< 3 天的任务"新"，不需要提醒；≥ 3 天
                        // 开始显，覆盖一周内 / 一两周 / 更久三个粒度。
                        if (ageMs < 3 * 86_400_000) return null;
                        const rel = formatRelativeAge(t.created_at, nowMs);
                        if (!rel) return null;
                        return (
                          <span
                            title={`创建于 ${t.created_at
                              .slice(0, 16)
                              .replace("T", " ")}（${rel}）—— 放了一阵了，要不要拆 / 改 priority / 取消？`}
                            style={{
                              display: "inline-flex",
                              alignItems: "center",
                              gap: 2,
                              padding: "1px 7px",
                              fontSize: 10,
                              fontWeight: 500,
                              lineHeight: 1.4,
                              letterSpacing: 0.2,
                              borderRadius: 999,
                              background:
                                "color-mix(in srgb, var(--pet-color-muted) 12%, transparent)",
                              color: "var(--pet-color-muted)",
                              border:
                                "1px solid color-mix(in srgb, var(--pet-color-muted) 25%, transparent)",
                              whiteSpace: "nowrap",
                            }}
                            aria-label={`已创建 ${rel}`}
                          >
                            🕰 {rel}
                          </span>
                        );
                      })()}
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
                    <div
                      style={s.itemBody}
                      // 折叠态加 native tooltip 显示全文 —— 用户不点展开就能
                      // hover 看长描述，对于"扫一眼判断"是否需要展开有用。展开
                      // 时不挂 title（避免 hover 弹一长串重复 content）。
                      title={folded ? t.body : undefined}
                    >
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
                  {/* ⏰ 还 N 分钟 倒计时 chip：仅 pending / error 状态 + due
                      在未来 ≤ 60 分钟时浮。让 owner 看长列表时一眼看到
                      "立即到期"急迫信号 —— 比"截止 YYYY-MM-DD HH:MM"更直觉。
                      红色 tint 表示"剩时间不多"。终态行不显（done / cancelled
                      没"还剩多久"语义）。 */}
                  {t.due &&
                    !isFinished(t.status) &&
                    (() => {
                      const ts = Date.parse(t.due);
                      if (Number.isNaN(ts)) return null;
                      const diffMs = ts - nowMs;
                      if (diffMs <= 0 || diffMs > 3_600_000) return null;
                      const mins = Math.ceil(diffMs / 60_000);
                      return (
                        <span
                          style={{
                            background: "var(--pet-tint-red-bg)",
                            color: "var(--pet-tint-red-fg)",
                            padding: "1px 6px",
                            borderRadius: 999,
                            fontWeight: 600,
                            fontFamily: "'SF Mono', 'Menlo', monospace",
                          }}
                          title={`due 在 ${mins} 分钟后到期 — 立即处理。`}
                        >
                          ⏰ 还 {mins} 分
                        </span>
                      );
                    })()}
                  {/* 📅 调期 chip：相对增量 preset 微调 due_at。终态行
                      （done / cancelled）不显——调期对结束态无意义。
                      popover 直接锚 chip 下方；与 💤 snooze 同 outside-click
                      + Esc 关闭模式。 */}
                  {!isFinished(t.status) && (() => {
                    const open = dueShiftPickerTitle === t.title;
                    const presets: { key: string; label: string; deltaMs: number | null }[] = [
                      { key: "+1h", label: "📅 现在 +1 小时", deltaMs: 3_600_000 },
                      { key: "+1d", label: "📅 现在 +1 天", deltaMs: 86_400_000 },
                      { key: "+3d", label: "📅 现在 +3 天", deltaMs: 3 * 86_400_000 },
                      { key: "+1w", label: "📅 现在 +1 周", deltaMs: 7 * 86_400_000 },
                      { key: "+2w", label: "📅 现在 +2 周", deltaMs: 14 * 86_400_000 },
                    ];
                    return (
                      <span style={{ position: "relative", display: "inline-block" }}>
                        <button
                          type="button"
                          onMouseDown={(e) => e.stopPropagation()}
                          onClick={(e) => {
                            e.stopPropagation();
                            setDueShiftPickerTitle((cur) =>
                              cur === t.title ? null : t.title,
                            );
                          }}
                          disabled={busyTitle === t.title}
                          title="调期 due_at：从现在起 +1h / +1d / +3d / +1w / +2w preset 微调，或清除 due。"
                          style={{
                            padding: "1px 7px",
                            fontSize: 10,
                            border: "1px solid var(--pet-color-border)",
                            borderRadius: 999,
                            background: "var(--pet-color-card)",
                            color: "var(--pet-color-fg)",
                            cursor:
                              busyTitle === t.title ? "default" : "pointer",
                            opacity: busyTitle === t.title ? 0.5 : 1,
                            fontFamily: "inherit",
                          }}
                          aria-label="调期"
                        >
                          📅 调期
                        </button>
                        {open && (
                          <div
                            onMouseDown={(e) => e.stopPropagation()}
                            onClick={(e) => e.stopPropagation()}
                            style={{
                              position: "absolute",
                              top: "calc(100% + 4px)",
                              left: 0,
                              minWidth: 160,
                              padding: 4,
                              background: "var(--pet-color-card)",
                              border: "1px solid var(--pet-color-border)",
                              borderRadius: 6,
                              boxShadow: "0 4px 12px rgba(0,0,0,0.18)",
                              zIndex: 30,
                              display: "flex",
                              flexDirection: "column",
                              gap: 2,
                            }}
                          >
                            {presets.map((p) => (
                              <button
                                key={p.key}
                                type="button"
                                style={{
                                  display: "block",
                                  width: "100%",
                                  textAlign: "left",
                                  padding: "5px 9px",
                                  fontSize: 11,
                                  border: "none",
                                  background: "transparent",
                                  color: "var(--pet-color-fg)",
                                  cursor: "pointer",
                                  fontFamily: "inherit",
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
                                onClick={async () => {
                                  setDueShiftPickerTitle(null);
                                  setActionErr("");
                                  setBusyTitle(t.title);
                                  try {
                                    const dueArg =
                                      p.deltaMs === null
                                        ? null
                                        : formatDueInput(
                                            new Date(Date.now() + p.deltaMs),
                                          );
                                    await invoke<void>("task_set_due", {
                                      title: t.title,
                                      due: dueArg,
                                    });
                                    await reload();
                                  } catch (e) {
                                    setActionErr(`调期失败：${e}`);
                                  } finally {
                                    setBusyTitle(null);
                                  }
                                }}
                              >
                                {p.label}
                              </button>
                            ))}
                            <div
                              style={{
                                height: 1,
                                background: "var(--pet-color-border)",
                                margin: "2px 0",
                              }}
                            />
                            <button
                              type="button"
                              style={{
                                display: "block",
                                width: "100%",
                                textAlign: "left",
                                padding: "5px 9px",
                                fontSize: 11,
                                border: "none",
                                background: "transparent",
                                color: "var(--pet-color-accent)",
                                cursor: "pointer",
                                fontFamily: "inherit",
                                borderRadius: 4,
                                fontWeight: 600,
                              }}
                              onMouseOver={(e) => {
                                (e.currentTarget as HTMLButtonElement).style.background =
                                  "var(--pet-color-bg)";
                              }}
                              onMouseOut={(e) => {
                                (e.currentTarget as HTMLButtonElement).style.background =
                                  "transparent";
                              }}
                              onClick={async () => {
                                setDueShiftPickerTitle(null);
                                setActionErr("");
                                setBusyTitle(t.title);
                                try {
                                  await invoke<void>("task_set_due", {
                                    title: t.title,
                                    due: null,
                                  });
                                  await reload();
                                } catch (e) {
                                  setActionErr(`清 due 失败：${e}`);
                                } finally {
                                  setBusyTitle(null);
                                }
                              }}
                            >
                              清除 due
                            </button>
                          </div>
                        )}
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
                  {/* origin chip：raw_description 带 `[origin:tg:<chat>]` 时
                      行内显「📨 TG」一片，让用户扫长队列时一眼分辨"哪条是
                      手机派的"。不带 origin marker 默认是面板创建，不显
                      chip 避免噪音。点击 chip 跳到顶部 origin filter 集中
                      看 TG 任务。 */}
                  {t.raw_description.includes("[origin:tg:") && (
                    <span
                      onClick={(e) => {
                        e.stopPropagation();
                        setOriginFilter((prev) => {
                          if (prev.has("tg")) return prev;
                          const next = new Set(prev);
                          next.add("tg");
                          return next;
                        });
                      }}
                      style={{
                        padding: "1px 6px",
                        borderRadius: 999,
                        background: "var(--pet-tint-blue-bg)",
                        color: "var(--pet-tint-blue-fg)",
                        fontSize: 10,
                        fontWeight: 600,
                        letterSpacing: 0.2,
                        cursor: "pointer",
                        userSelect: "none",
                      }}
                      title="本任务从 Telegram 派出。点击 chip → 顶部 origin filter 切到 TG，集中查看手机端派的任务。"
                    >
                      📨 TG
                    </span>
                  )}
                  {/* 更新于 X · Y 前 [· N 次更新]：与"创建于"对称展示活跃
                      度。updated_at 与 created_at 同 → 任务建后没动过，省
                      此 span 避免重复噪声。N 次更新依赖 detailMap[title] 已
                      经被 hover preview / expand 加载 —— 没加载就只显时间，
                      graceful degrade。
                      done/cancelled 终态时 label 用"完成于"/"取消于"——
                      该 ts 就是状态确定的时刻，比泛泛"更新于"更有信息量。 */}
                  {t.updated_at && t.updated_at !== t.created_at && (
                    <span>
                      {t.status === "done"
                        ? "完成于 "
                        : t.status === "cancelled"
                          ? "取消于 "
                          : "更新于 "}
                      {t.updated_at.slice(0, 16).replace("T", " ")}
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
                              {/* ⌘F detail.md 全文搜浮 bar：仅在 detail 编辑器
                                  textarea 内 / 自身 input 内 ⌘F 时打开。input
                                  + 命中计数 + ↑↓ 翻 match + ✕ 关。textarea
                                  以 setSelectionRange 选中当前 match 让 textarea
                                  内部自动滚到位。 */}
                              {detailSearchOpen && (() => {
                                const n = detailSearchMatches.length;
                                const safeIdx = n === 0
                                  ? 0
                                  : Math.max(
                                      0,
                                      Math.min(detailSearchActiveIdx, n - 1),
                                    );
                                return (
                                  <div
                                    style={{
                                      display: "flex",
                                      alignItems: "center",
                                      gap: 6,
                                      padding: "4px 8px",
                                      border:
                                        "1px solid var(--pet-color-border)",
                                      background: "var(--pet-color-card)",
                                      borderRadius: 6,
                                      fontSize: 12,
                                    }}
                                  >
                                    <span style={{ fontSize: 12, opacity: 0.7 }}>
                                      🔍
                                    </span>
                                    <input
                                      ref={detailSearchInputRef}
                                      type="text"
                                      autoFocus
                                      value={detailSearchQuery}
                                      onChange={(e) => {
                                        setDetailSearchQuery(e.target.value);
                                        setDetailSearchActiveIdx(0);
                                      }}
                                      onKeyDown={(e) => {
                                        if (e.key === "Escape") {
                                          e.preventDefault();
                                          setDetailSearchOpen(false);
                                          // 关闭后把焦点送回 textarea，让 owner
                                          // 继续敲字
                                          window.setTimeout(() => {
                                            detailEditorRef.current?.focus();
                                          }, 0);
                                          return;
                                        }
                                        if (e.key === "Enter") {
                                          e.preventDefault();
                                          cycleDetailSearchMatch(
                                            e.shiftKey ? "prev" : "next",
                                          );
                                          return;
                                        }
                                        if (e.key === "ArrowDown") {
                                          e.preventDefault();
                                          cycleDetailSearchMatch("next");
                                          return;
                                        }
                                        if (e.key === "ArrowUp") {
                                          e.preventDefault();
                                          cycleDetailSearchMatch("prev");
                                          return;
                                        }
                                      }}
                                      placeholder="在本 detail.md 内搜（⌘F · Enter 下 / ⇧Enter 上 · Esc 关）"
                                      style={{
                                        flex: 1,
                                        minWidth: 80,
                                        padding: "3px 6px",
                                        fontSize: 12,
                                        border:
                                          "1px solid var(--pet-color-border)",
                                        borderRadius: 4,
                                        background: "var(--pet-color-bg)",
                                        color: "var(--pet-color-fg)",
                                        fontFamily: "inherit",
                                        outline: "none",
                                      }}
                                    />
                                    <span
                                      style={{
                                        fontSize: 10,
                                        color:
                                          n === 0 && detailSearchQuery
                                            ? "var(--pet-tint-red-fg)"
                                            : "var(--pet-color-muted)",
                                        fontFamily: "'SF Mono', monospace",
                                        whiteSpace: "nowrap",
                                        minWidth: 36,
                                        textAlign: "right",
                                      }}
                                      title={
                                        n === 0
                                          ? detailSearchQuery
                                            ? `没有命中「${detailSearchQuery}」`
                                            : "输入关键词"
                                          : `第 ${safeIdx + 1} / ${n} 处命中`
                                      }
                                    >
                                      {detailSearchQuery
                                        ? n === 0
                                          ? "0/0"
                                          : `${safeIdx + 1}/${n}`
                                        : "—"}
                                    </span>
                                    <button
                                      type="button"
                                      onClick={() =>
                                        cycleDetailSearchMatch("prev")
                                      }
                                      disabled={n === 0}
                                      title="上一处（⇧Enter / ↑）"
                                      style={{
                                        padding: "2px 6px",
                                        fontSize: 11,
                                        border:
                                          "1px solid var(--pet-color-border)",
                                        borderRadius: 4,
                                        background: "var(--pet-color-card)",
                                        color: "var(--pet-color-fg)",
                                        cursor: n === 0 ? "default" : "pointer",
                                        opacity: n === 0 ? 0.4 : 1,
                                      }}
                                    >
                                      ↑
                                    </button>
                                    <button
                                      type="button"
                                      onClick={() =>
                                        cycleDetailSearchMatch("next")
                                      }
                                      disabled={n === 0}
                                      title="下一处（Enter / ↓）"
                                      style={{
                                        padding: "2px 6px",
                                        fontSize: 11,
                                        border:
                                          "1px solid var(--pet-color-border)",
                                        borderRadius: 4,
                                        background: "var(--pet-color-card)",
                                        color: "var(--pet-color-fg)",
                                        cursor: n === 0 ? "default" : "pointer",
                                        opacity: n === 0 ? 0.4 : 1,
                                      }}
                                    >
                                      ↓
                                    </button>
                                    <button
                                      type="button"
                                      onClick={() => {
                                        setDetailSearchOpen(false);
                                        window.setTimeout(() => {
                                          detailEditorRef.current?.focus();
                                        }, 0);
                                      }}
                                      title="关闭搜索（Esc）"
                                      style={{
                                        padding: "2px 6px",
                                        fontSize: 11,
                                        border:
                                          "1px solid var(--pet-color-border)",
                                        borderRadius: 4,
                                        background: "var(--pet-color-card)",
                                        color: "var(--pet-color-muted)",
                                        cursor: "pointer",
                                      }}
                                    >
                                      ✕
                                    </button>
                                  </div>
                                );
                              })()}
                              {/* 草稿恢复 banner：editor 上次没 ⌘S 关掉时
                                  autosave 把 content 写进 localStorage；本次
                                  进入时检测 draft.content !== currentMd 弹
                                  此条让 owner 选择恢复（覆盖到 textarea）/
                                  忽略（删 draft key）。两个 action 都立即
                                  setPendingDraft(null) 让 banner 隐藏。 */}
                              {pendingDraft && pendingDraft.title === t.title && (
                                <div
                                  style={{
                                    padding: "8px 12px",
                                    border: "1px solid var(--pet-tint-amber-fg, #d97706)",
                                    background: "var(--pet-tint-amber-bg, #fef3c7)",
                                    color: "var(--pet-tint-amber-fg, #92400e)",
                                    borderRadius: 6,
                                    fontSize: 11,
                                    lineHeight: 1.4,
                                    display: "flex",
                                    alignItems: "center",
                                    gap: 8,
                                  }}
                                >
                                  <span style={{ flex: 1 }}>
                                    📝 检测到上次未保存的草稿（
                                    {(() => {
                                      const ageMs = Date.now() - pendingDraft.ts;
                                      if (ageMs < 60_000) return "刚刚";
                                      if (ageMs < 3_600_000)
                                        return `${Math.floor(ageMs / 60_000)} 分钟前`;
                                      if (ageMs < 86_400_000)
                                        return `${Math.floor(ageMs / 3_600_000)} 小时前`;
                                      return `${Math.floor(ageMs / 86_400_000)} 天前`;
                                    })()}
                                    ）—— 与磁盘版差{" "}
                                    {Math.abs(
                                      pendingDraft.content.length -
                                        editingDetailContent.length,
                                    )}{" "}
                                    字符
                                  </span>
                                  <button
                                    type="button"
                                    onClick={() => {
                                      setEditingDetailContent(pendingDraft.content);
                                      setPendingDraft(null);
                                    }}
                                    style={{
                                      fontSize: 10,
                                      fontWeight: 600,
                                      padding: "2px 8px",
                                      border:
                                        "1px solid var(--pet-tint-amber-fg, #d97706)",
                                      borderRadius: 4,
                                      background:
                                        "var(--pet-tint-amber-fg, #d97706)",
                                      color: "#fff",
                                      cursor: "pointer",
                                      whiteSpace: "nowrap",
                                    }}
                                    title="把 localStorage 里的草稿 content 灌回 textarea（你仍然可以 ⌘S 真保存或 Esc 再次取消）"
                                  >
                                    🔄 恢复
                                  </button>
                                  <button
                                    type="button"
                                    onClick={() => {
                                      try {
                                        window.localStorage.removeItem(
                                          `pet-detail-draft-${t.title}`,
                                        );
                                      } catch {
                                        // noop
                                      }
                                      setPendingDraft(null);
                                    }}
                                    style={{
                                      fontSize: 10,
                                      fontWeight: 600,
                                      padding: "2px 8px",
                                      border:
                                        "1px solid color-mix(in srgb, var(--pet-tint-amber-fg, #d97706) 40%, transparent)",
                                      borderRadius: 4,
                                      background: "var(--pet-color-card)",
                                      color: "var(--pet-tint-amber-fg, #92400e)",
                                      cursor: "pointer",
                                      whiteSpace: "nowrap",
                                    }}
                                    title="删掉 localStorage 草稿，本次不恢复"
                                  >
                                    ✕ 忽略
                                  </button>
                                </div>
                              )}
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
                                {/* 📋 复制全文：与既有 PanelMemory 的 📋 detail.md
                                    全文复制对偶 —— 在 PanelTasks detail 编辑器顶
                                    部一键拷整段 markdown 到剪贴板。preview 模式
                                    下 owner 无法用 textarea 原生选中拷贝；edit /
                                    split 模式下也省"⌘A + ⌘C"两步。空内容不渲
                                    染避免噪音。toast 复用 bulkResultMsg 通道。 */}
                                {editingDetailContent.length > 0 && (
                                  <button
                                    type="button"
                                    onClick={async () => {
                                      try {
                                        await navigator.clipboard.writeText(
                                          editingDetailContent,
                                        );
                                        const len = Array.from(editingDetailContent)
                                          .length;
                                        setBulkResultMsg(
                                          `已复制 detail.md 全文（${len} 字）`,
                                        );
                                      } catch (e) {
                                        setBulkResultMsg(`复制失败：${e}`);
                                      }
                                      window.setTimeout(
                                        () => setBulkResultMsg(""),
                                        4000,
                                      );
                                    }}
                                    style={{
                                      fontSize: 11,
                                      padding: "2px 8px",
                                      border: "1px solid var(--pet-color-border)",
                                      borderRadius: 4,
                                      background: "var(--pet-color-card)",
                                      color: "var(--pet-color-muted)",
                                      cursor: "pointer",
                                    }}
                                    title="把当前 detail.md 全文写到系统剪贴板（含未保存改动 —— textarea 当前值，不是磁盘版本）。便于贴到外部 markdown 笔记 / chat / issue。"
                                    aria-label="copy detail.md content to clipboard"
                                  >
                                    📋
                                  </button>
                                )}
                                {/* 📤 导出整体 markdown：与既有 bulk 复制为 MD
                                    （handleBulkCopyAsMd）/ PanelMemory 📝 同思
                                    路 —— 但聚焦"当前编辑这条任务"的完整快照。
                                    复用 formatTaskAsMarkdown(t, detail) 拼 H2
                                    title + meta + body + detail.md + result 段；
                                    history 段单独追加（formatTaskAsMarkdown 不含）
                                    让 owner 把 share / issue / 周末复盘所需的
                                    所有元数据一键打包。detail.md 用 editing
                                    state（未保存改动也带上）。 */}
                                <button
                                  type="button"
                                  onClick={async () => {
                                    setBulkResultMsg("📤 正在拼 markdown…");
                                    // detail 走当前编辑值 + 已加载 history。
                                    // history 优先从既有 detailMap 缓存读；
                                    // 没缓存时 task_get_detail 走一次 IO。
                                    let history: TaskDetail["history"] = [];
                                    let historyIoError = false;
                                    const cached = detailMap[t.title];
                                    if (cached) {
                                      history = cached.history;
                                      historyIoError = !!cached.history_io_error;
                                    } else if (t.detail_path) {
                                      try {
                                        const fresh = await invoke<TaskDetail>(
                                          "task_get_detail",
                                          { title: t.title },
                                        );
                                        history = fresh.history;
                                        historyIoError = !!fresh.history_io_error;
                                      } catch (e) {
                                        console.error(
                                          "task_get_detail failed:",
                                          e,
                                        );
                                        // history 拉不到也继续 export；至少 detail.md
                                        // + meta 仍写得出。
                                      }
                                    }
                                    // 构造 synthetic TaskDetail 让 formatter
                                    // 把当前 editing 值（含未保存）作 detail.md
                                    // body —— owner 期望"导出我现在看到的"。
                                    const detailForFormat: TaskDetail = {
                                      title: t.title,
                                      raw_description: t.raw_description,
                                      detail_path: t.detail_path || "",
                                      detail_md: editingDetailContent,
                                      created_at: t.created_at,
                                      updated_at: t.updated_at,
                                      history,
                                      detail_md_io_error: false,
                                      history_io_error: historyIoError,
                                    };
                                    const lines = [
                                      formatTaskAsMarkdown(t, detailForFormat),
                                    ];
                                    if (history.length > 0) {
                                      lines.push("", "### 历史事件", "");
                                      for (const ev of history) {
                                        const ts =
                                          ev.timestamp
                                            ?.slice(0, 16)
                                            .replace("T", " ") ?? "?";
                                        const snippet =
                                          ev.snippet?.trim() || "(空)";
                                        lines.push(
                                          `- \`${ts}\` ${ev.action}：${snippet}`,
                                        );
                                      }
                                    }
                                    const md = lines.join("\n");
                                    try {
                                      await navigator.clipboard.writeText(md);
                                      setBulkResultMsg(
                                        `已导出整体 markdown 到剪贴板（${md.length} 字符）`,
                                      );
                                    } catch (e) {
                                      setBulkResultMsg(`导出失败：${e}`);
                                    }
                                    window.setTimeout(
                                      () => setBulkResultMsg(""),
                                      4000,
                                    );
                                  }}
                                  style={{
                                    fontSize: 11,
                                    padding: "2px 8px",
                                    border: "1px solid var(--pet-color-border)",
                                    borderRadius: 4,
                                    background: "var(--pet-color-card)",
                                    color: "var(--pet-color-muted)",
                                    cursor: "pointer",
                                  }}
                                  title="导出本任务完整 markdown 到剪贴板：title + 状态 / 优先级 / 截止 / 标签 / 时间戳 + body + detail.md（含未保存）+ result + 历史事件。便于 share / issue / 周末复盘。"
                                  aria-label="export task as markdown"
                                >
                                  📤
                                </button>
                                {/* ↑ ↓ 上 / 下一条任务导航：让 owner 连续 review
                                    多 task 不必关 detail。dirty 时切换前 sync-
                                    flush 进 draft localStorage 防丢内容；target
                                    detail 缓存命中即用、未命中走 task_get_detail。
                                    在 visibleTasks 头 / 尾时对应方向 disabled。 */}
                                {(() => {
                                  const curIdx = visibleTasks.findIndex(
                                    (vt) => vt.title === t.title,
                                  );
                                  const hasPrev = curIdx > 0;
                                  const hasNext =
                                    curIdx !== -1 &&
                                    curIdx < visibleTasks.length - 1;
                                  const navBtnStyle = (
                                    enabled: boolean,
                                  ): React.CSSProperties => ({
                                    fontSize: 11,
                                    padding: "2px 8px",
                                    border: "1px solid var(--pet-color-border)",
                                    borderRadius: 4,
                                    background: "var(--pet-color-card)",
                                    color: enabled
                                      ? "var(--pet-color-muted)"
                                      : "var(--pet-color-border)",
                                    cursor: enabled ? "pointer" : "default",
                                    opacity: enabled ? 1 : 0.5,
                                  });
                                  return (
                                    <>
                                      <button
                                        type="button"
                                        disabled={!hasPrev}
                                        onClick={() =>
                                          void handleNavigateDetail("prev")
                                        }
                                        style={navBtnStyle(hasPrev)}
                                        title={
                                          hasPrev
                                            ? `跳到上一条任务（visibleTasks 顺序 #${curIdx} → #${curIdx - 1}） · ⌘[。dirty 时自动 flush 进草稿不丢内容。`
                                            : "已是第一条"
                                        }
                                        aria-label="previous task detail"
                                      >
                                        ↑
                                      </button>
                                      <button
                                        type="button"
                                        disabled={!hasNext}
                                        onClick={() =>
                                          void handleNavigateDetail("next")
                                        }
                                        style={navBtnStyle(hasNext)}
                                        title={
                                          hasNext
                                            ? `跳到下一条任务（visibleTasks 顺序 #${curIdx} → #${curIdx + 1}） · ⌘]。dirty 时自动 flush 进草稿不丢内容。`
                                            : "已是最后一条"
                                        }
                                        aria-label="next task detail"
                                      >
                                        ↓
                                      </button>
                                    </>
                                  );
                                })()}
                                {/* 📑 大纲：仅 split / preview 模式渲染（edit 模
                                    式下没 preview pane，scrollIntoView 找不到
                                    heading id）。toggle 浮窗：扫 H1-H3 显锚点
                                    清单，点击 getElementById 跳节。content 含
                                    headings 才显（无 heading 时浮窗空，按钮
                                    存在感低，gate 去掉避免噪音）。 */}
                                {(detailViewMode === "split" ||
                                  detailViewMode === "preview") &&
                                  /^#{1,3}\s+/m.test(editingDetailContent) && (
                                    <button
                                      type="button"
                                      onClick={() =>
                                        setDetailOutlineOpen((v) => !v)
                                      }
                                      style={{
                                        fontSize: 11,
                                        padding: "2px 8px",
                                        border: `1px solid ${detailOutlineOpen ? "var(--pet-color-accent)" : "var(--pet-color-border)"}`,
                                        borderRadius: 4,
                                        background: detailOutlineOpen
                                          ? "var(--pet-tint-blue-bg)"
                                          : "var(--pet-color-card)",
                                        color: detailOutlineOpen
                                          ? "var(--pet-tint-blue-fg)"
                                          : "var(--pet-color-muted)",
                                        cursor: "pointer",
                                        fontWeight: detailOutlineOpen ? 600 : 400,
                                      }}
                                      title="切换大纲浮窗：扫 H1-H3 标题列出锚点，点击跳到对应位置。长 detail.md 用。"
                                      aria-label="toggle detail.md outline"
                                    >
                                      📑
                                    </button>
                                  )}
                                {/* R141: dirty marker — content !== original 时
                                    显 "● 未保存"；marginLeft: auto 在字数 counter
                                    上，dirty marker 紧贴字数左侧（gap 4 分隔）。
                                    持续 dirty > 60s 时染红 + pulse 提醒 owner
                                    该 ⌘S（防长编辑场景下忘保存丢内容）。
                                    dirtyTickKey 仅是 trigger 重渲染，read ref
                                    取最新 elapsed 而非 state（省 5s 一次 state
                                    set）。 */}
                                {editingDetailContent !==
                                  editingDetailOriginalRef.current && (() => {
                                  const since = dirtySinceRef.current;
                                  const elapsedSec = since
                                    ? Math.floor((Date.now() - since) / 1000)
                                    : 0;
                                  const stale = elapsedSec > 60;
                                  // 引用一下 dirtyTickKey 让 ESLint 看到 hook 关联，
                                  // 也防 dead-code elimination 不渲（实际值不用）。
                                  void dirtyTickKey;
                                  return (
                                    <span
                                      style={{
                                        marginLeft: "auto",
                                        fontSize: 10,
                                        color: stale
                                          ? "var(--pet-tint-red-fg)"
                                          : "var(--pet-color-muted)",
                                        fontFamily:
                                          "'SF Mono', 'Menlo', monospace",
                                        fontWeight: stale ? 600 : 400,
                                        animation: stale
                                          ? "pet-detail-dirty-pulse 1.8s ease-in-out infinite"
                                          : undefined,
                                      }}
                                      title={
                                        stale
                                          ? `textarea 内容已改但未保存超 ${elapsedSec}s ⚠️ —— 按 ⌘S 保存 / 关掉编辑器走 Esc 二次确认`
                                          : "textarea 内容已改但未保存（⌘S 保存 / Esc 取消触发 dirty 二次确认）"
                                      }
                                    >
                                      ● 未保存{stale ? ` ${elapsedSec}s` : ""}
                                    </span>
                                  );
                                })()}
                                {/* 行号状态栏：「行 N / 共 M」与 IDE 状态栏同体
                                    验。仅在编辑模式（textarea 存在 → 光标存在）
                                    显；preview 纯渲染态下无 cursor 概念省略。
                                    line 计算：value.slice(0, cursor).split("\n").length
                                    给 1-indexed 行号；total = split 全文行数。 */}
                                {detailViewMode !== "preview" && (() => {
                                  const cursor = Math.max(
                                    0,
                                    Math.min(
                                      detailCursorPos,
                                      editingDetailContent.length,
                                    ),
                                  );
                                  const before = editingDetailContent.slice(
                                    0,
                                    cursor,
                                  );
                                  const line = before.split("\n").length;
                                  const total = editingDetailContent.length === 0
                                    ? 1
                                    : editingDetailContent.split("\n").length;
                                  return (
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
                                      title="当前光标所在行号 / 文档总行数（与 IDE 状态栏同源）。调试 markdown 时方便对照。"
                                    >
                                      行 {line} / 共 {total}
                                    </span>
                                  );
                                })()}
                                {/* 📅 创建 X 前 · 🔄 更新 Y 前 时间段：与
                                    PanelMemory item hover tooltip 同源信号。
                                    编辑长 task 时 owner 一眼看到任务年龄 +
                                    最近改动；与底部既有"行 N/M" + "☑ done/total"
                                    + 字数 chip 形成完整 status bar。created /
                                    updated ≤ 60s 视为同动作合并；解析失败
                                    跳整段不渲染空白。 */}
                                {(() => {
                                  const nowMs = Date.now();
                                  const cMs = t.created_at
                                    ? Date.parse(t.created_at)
                                    : NaN;
                                  const uMs = t.updated_at
                                    ? Date.parse(t.updated_at)
                                    : NaN;
                                  const fmt = (ms: number) => {
                                    const age = nowMs - ms;
                                    return age < 60_000
                                      ? "刚刚"
                                      : formatRelativeAgeBuckets(age);
                                  };
                                  const parts: string[] = [];
                                  if (!Number.isNaN(cMs)) {
                                    parts.push(`📅 ${fmt(cMs)}创建`);
                                  }
                                  if (
                                    !Number.isNaN(uMs) &&
                                    (Number.isNaN(cMs) ||
                                      Math.abs(uMs - cMs) > 60_000)
                                  ) {
                                    parts.push(`🔄 ${fmt(uMs)}改`);
                                  }
                                  if (parts.length === 0) return null;
                                  return (
                                    <span
                                      style={{
                                        fontSize: 10,
                                        color: "var(--pet-color-muted)",
                                        fontFamily:
                                          "'SF Mono', 'Menlo', monospace",
                                      }}
                                      title={
                                        `created_at: ${t.created_at || "（缺）"}\n` +
                                        `updated_at: ${t.updated_at || "（缺）"}`
                                      }
                                    >
                                      {parts.join(" · ")}
                                    </span>
                                  );
                                })()}
                                {/* ☑ checkbox 进度 chip：扫 detail.md 里的
                                    `- [ ]` / `- [x]` / `- [X]` 行计数。total > 0
                                    才显（无 checklist 时是噪音）；done == total
                                    时变绿，鼓励"全勾完"。与既有 GFM checklist
                                    渲染 + ✓ 完成行按钮同 marker 协议，让"清单
                                    完成度"成为一眼可见的反馈。 */}
                                {(() => {
                                  // 全文 scan：multiline regex 一次走完。换行
                                  // 大小写都覆盖（与 toggleEditChecklistLine
                                  // 接受的形态一致）。
                                  const lines = editingDetailContent.split("\n");
                                  let total = 0;
                                  let done = 0;
                                  for (const line of lines) {
                                    const m = line.match(/^\s*- \[([ xX])\] /);
                                    if (m) {
                                      total += 1;
                                      if (m[1] !== " ") done += 1;
                                    }
                                  }
                                  if (total === 0) return null;
                                  const allDone = done === total;
                                  return (
                                    <span
                                      style={{
                                        fontSize: 10,
                                        color: allDone
                                          ? "var(--pet-tint-green-fg)"
                                          : "var(--pet-color-muted)",
                                        fontWeight: allDone ? 600 : undefined,
                                        fontFamily: "'SF Mono', 'Menlo', monospace",
                                      }}
                                      title={
                                        allDone
                                          ? `全部 ${total} 条 checklist 都已勾完 ✓`
                                          : `本 detail.md 含 ${total} 条 GFM checklist；已勾 ${done} 条。点工具栏 ☐ 加新条 / ✓ 完成行加"做完一条 + 时间戳"。`
                                      }
                                    >
                                      ☑ {done} / 共 {total}
                                    </span>
                                  );
                                })()}
                                {(() => {
                                  // 编辑态 counter 三档配色：与阅读态 counter
                                  // （Array.from length；> 2000 amber）共一套
                                  // 阈值语义但更激进 —— edit 是 user 主动写，
                                  // > 5000 字进 red banner（下一行）。
                                  const editCount = Array.from(editingDetailContent).length;
                                  const longish = editCount > 2000;
                                  const danger = editCount > 5000;
                                  // 选区感知：detailSelectionEnd > detailCursorPos
                                  // 时切到选区子串。selection 的字数 / 词数 独立
                                  // 算 —— 让 owner 在 IDE / Pages-style "选 N 字"
                                  // 即时反馈下知道选了多少。颜色 / 配色 / 阈值
                                  // 仍按文档全体的 editCount 走（避免选了 100 字
                                  // 就跳红 banner 这种误导）。
                                  const selStart = Math.min(
                                    detailCursorPos,
                                    detailSelectionEnd,
                                  );
                                  const selEnd = Math.max(
                                    detailCursorPos,
                                    detailSelectionEnd,
                                  );
                                  const hasSelection =
                                    selEnd > selStart &&
                                    selStart >= 0 &&
                                    selEnd <= editingDetailContent.length &&
                                    detailViewMode !== "preview";
                                  const countSource = hasSelection
                                    ? editingDetailContent.slice(selStart, selEnd)
                                    : editingDetailContent;
                                  // 词数 heuristic：CJK 字符（中日韩 ideograph
                                  // 范围 U+3400-U+9FFF）每个算 1 词；非 CJK 段
                                  // split 非-字母数字 取 token 数。纯 CJK 文本
                                  // wordCount === charCount → 仅显字数避免冗
                                  // 余；混排 / 英文文本时多显一段 〜M 词 给
                                  // owner 看实际 token 数量。
                                  const charCount = Array.from(countSource).length;
                                  const cjkCount = (
                                    countSource.match(/[㐀-鿿]/g) || []
                                  ).length;
                                  const stripped = countSource.replace(
                                    /[㐀-鿿]/g,
                                    " ",
                                  );
                                  const enWords = stripped
                                    .split(/[^a-zA-Z0-9_'-]+/)
                                    .filter(Boolean).length;
                                  const wordCount = cjkCount + enWords;
                                  // marginLeft auto 由更早的 chip 抢占（dirty ●
                                  // 或 行号 chip）；只有 preview 模式 + clean 时
                                  // 字数 chip 自己成为"右推 spacer"。多 auto chip
                                  // 会让 flex 把空间平均分配，破坏布局。
                                  const spacerOnSelf =
                                    detailViewMode === "preview" &&
                                    editingDetailContent ===
                                      editingDetailOriginalRef.current;
                                  // 仅 word count 与 char count 不同时追加 ·〜M 词，
                                  // 纯 CJK 文本两者相等避免重复显示。
                                  const showWord =
                                    wordCount > 0 && wordCount !== charCount;
                                  const prefix = hasSelection ? "选 " : "";
                                  return (
                                    <span
                                      style={{
                                        marginLeft: spacerOnSelf ? "auto" : undefined,
                                        fontSize: 10,
                                        color: hasSelection
                                          ? "var(--pet-color-accent)"
                                          : danger
                                            ? "var(--pet-tint-red-fg)"
                                            : longish
                                              ? "var(--pet-tint-yellow-fg)"
                                              : "var(--pet-color-muted)",
                                        fontWeight:
                                          hasSelection || danger ? 600 : undefined,
                                        fontFamily: "'SF Mono', 'Menlo', monospace",
                                      }}
                                      title={
                                        (hasSelection
                                          ? `选区 ${charCount} 字 / 共 ${editCount} 字（Unicode code points 计）`
                                          : `${charCount} 字（Unicode code points 计；含换行 / 空白）`) +
                                        (showWord
                                          ? `\n${wordCount} 词（CJK 字符各算 1 词 + 非 CJK 段 split 标点 token 数；heuristic 估算）`
                                          : "")
                                      }
                                    >
                                      {prefix}{charCount} 字
                                      {showWord && ` · 〜${wordCount} 词`}
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
                                  <button
                                    type="button"
                                    onClick={() =>
                                      insertMarkdownAtCursor(
                                        "wrap",
                                        "```\n",
                                        "\n```",
                                      )
                                    }
                                    title="代码块（```\\n...\\n```）。选中后点击包裹；无选区时光标落在两道围栏之间让你直接敲。"
                                    style={{
                                      ...mdToolbarBtnStyle,
                                      fontFamily:
                                        "'SF Mono', 'Menlo', monospace",
                                    }}
                                  >
                                    {"</>"}
                                  </button>
                                  <button
                                    type="button"
                                    onClick={() =>
                                      insertMarkdownAtCursor(
                                        "line-prefix",
                                        "- [ ] ",
                                        "",
                                      )
                                    }
                                    title="待办（- [ ] ...）。每选中行的行首加 - [ ]。完成后手动改成 - [x] 即标记完成；GitHub / Obsidian / Notion 都识别。"
                                    style={mdToolbarBtnStyle}
                                  >
                                    ☐
                                  </button>
                                  <button
                                    type="button"
                                    onClick={() =>
                                      insertMarkdownAtCursor(
                                        "line-prefix",
                                        "> ",
                                        "",
                                      )
                                    }
                                    title="引用块（> ...）。每选中行的行首加 >；多行连续就是多行引用。粘别人的话 / 引用之前结论 / 提示框都常用。"
                                    style={mdToolbarBtnStyle}
                                  >
                                    ❝
                                  </button>
                                  <button
                                    type="button"
                                    onClick={insertTableSkeletonAtCursor}
                                    title="表格（3×3 GFM）。插入 | 列 1 | 列 2 | 列 3 | + 分隔行 + 2 空白数据行；光标自动选中『列 1』，直接敲即覆盖。需独占整段，按钮会自动补换行。"
                                    style={mdToolbarBtnStyle}
                                  >
                                    📊
                                  </button>
                                  <button
                                    type="button"
                                    onClick={insertCurrentTimeAtCursor}
                                    title="插入当前时间（YYYY-MM-DD HH:MM 本地，与 [snooze:] / [once:] marker 协议同形）。记录里程碑 / 进度笔记 / 调用时间戳都用得到。"
                                    style={mdToolbarBtnStyle}
                                  >
                                    📅
                                  </button>
                                  <button
                                    type="button"
                                    onClick={insertDoneLineAtCursor}
                                    title="✓ 完成行（- [x] YYYY-MM-DD HH:MM ）。在光标所在行首插入「已完成 checklist + 时间戳」模板，光标落尾让你直接敲『做了什么』。当前行已是 checklist 时跳过。"
                                    style={mdToolbarBtnStyle}
                                  >
                                    ✓
                                  </button>
                                  {/* 🔗 插 task ref：复用 ⌘K palette 但 mode
                                      = insertRef。fuzzy 选其他 task 后在光标
                                      位置插 `「title」`，token 与 bulk
                                      "🔗 拼为 ref" 协议同 — chat / detail
                                      渲染时 hover 显状态 / 双击跳源 task。 */}
                                  <button
                                    type="button"
                                    onClick={() => {
                                      setTaskPaletteOpen(true);
                                      setTaskPaletteMode("insertRef");
                                      setPaletteQuery("");
                                      setPaletteSelectedIdx(0);
                                    }}
                                    title="插 task ref token「title」。弹 task picker → 选要 ref 的 task → 自动在光标位置插入 `「title」`（chat / detail 渲染时是 hover-able / 双击跳源任务的 ref）。"
                                    style={mdToolbarBtnStyle}
                                  >
                                    「」
                                  </button>
                                  {/* 📂 在 Finder 显示 detail.md：让 owner 能在
                                      系统文件管理器里操作（拖图 / git add /
                                      用其它编辑器打开等）。macOS 用 `open -R`
                                      高亮选中；Windows `explorer /select,`；其它
                                      平台退化到打开父目录。文件还未存在（新任务
                                      首次保存前）→ 后端报错，setActionErr 显原
                                      因 toast 3.5s 自清。 */}
                                  {t.detail_path && (
                                    <button
                                      type="button"
                                      onClick={async () => {
                                        setActionErr("");
                                        try {
                                          await invoke<void>(
                                            "memory_reveal_detail_in_finder",
                                            { detailPath: t.detail_path },
                                          );
                                        } catch (e) {
                                          setActionErr(
                                            `在 Finder 打开失败：${e}（detail.md 可能尚未保存到磁盘 —— 先 ⌘S 一次再点）`,
                                          );
                                          window.setTimeout(
                                            () => setActionErr(""),
                                            5000,
                                          );
                                        }
                                      }}
                                      title={`在系统文件管理器里显示 detail.md（路径：memories/${t.detail_path}）。macOS Finder 会高亮选中文件，方便拖入附件 / git add / 重命名 / 用其它编辑器打开。`}
                                      style={mdToolbarBtnStyle}
                                    >
                                      📂
                                    </button>
                                  )}
                                  {/* 📤 复制 LLM consume 段：复用既有
                                      formatTaskAsMarkdown(t, detail) 拼 H2
                                      标题 + 状态/优先级/截止/标签 bullet 元数据
                                      + body + ### 进度笔记 (detail.md 当前
                                      编辑态内容) + ### 产物。让 owner 在编辑
                                      中想"把这条 task 完整上下文喂给外部
                                      LLM (ChatGPT / Claude / 其它)"时不必先
                                      关编辑 + 走 row 右键 + 复制为 Markdown
                                      三步。detail_md 用当前 editingDetailContent
                                      （而非磁盘版）让"边写边复制"反映最新。 */}
                                  <button
                                    type="button"
                                    onClick={async () => {
                                      setActionErr("");
                                      const stub: TaskDetail = {
                                        title: t.title,
                                        raw_description: t.raw_description,
                                        detail_path: t.detail_path ?? "",
                                        detail_md: editingDetailContent,
                                        created_at: t.created_at,
                                        updated_at: t.updated_at,
                                        history: [],
                                        detail_md_io_error: false,
                                        history_io_error: false,
                                      };
                                      const md = formatTaskAsMarkdown(t, stub);
                                      try {
                                        await navigator.clipboard.writeText(md);
                                        setBulkResultMsg(
                                          `已复制「${t.title}」完整 markdown（含当前 detail.md 编辑态）`,
                                        );
                                      } catch (e) {
                                        setActionErr(`复制失败：${e}`);
                                      }
                                      window.setTimeout(
                                        () => setBulkResultMsg(""),
                                        3000,
                                      );
                                    }}
                                    title="复制本任务的「LLM 喂养段」：H2 标题 + 状态/优先级/截止/标签 bullet 元数据 + body + ### 进度笔记 (含当前编辑器内容，不必先 ⌘S) + ### 产物，整段 markdown 进剪贴板。粘到 ChatGPT / Claude / Cursor / 别的 LLM 即作完整上下文。"
                                    style={mdToolbarBtnStyle}
                                  >
                                    📤
                                  </button>
                                  {/* 📋 选区 → 新 task：detail.md 编辑器中
                                      owner 选中一段（通常某个具体子项 / 待办
                                      / 思考片段），click 提取选区到 quickAdd
                                      modal 预填（首行 80 字符当 title，全段
                                      当 body）。让 "在长 detail 写到一半发现
                                      这段值得独立任务" 流不必先离开编辑器。
                                      仅 selection 长度 > 0 时 enabled。 */}
                                  {(() => {
                                    const selStart = Math.min(
                                      detailCursorPos,
                                      detailSelectionEnd,
                                    );
                                    const selEnd = Math.max(
                                      detailCursorPos,
                                      detailSelectionEnd,
                                    );
                                    const hasSel =
                                      selEnd > selStart &&
                                      selStart >= 0 &&
                                      selEnd <= editingDetailContent.length;
                                    return (
                                      <button
                                        type="button"
                                        disabled={!hasSel}
                                        onClick={() => {
                                          if (!hasSel) return;
                                          const text = editingDetailContent
                                            .slice(selStart, selEnd)
                                            .trim();
                                          if (!text) return;
                                          // title: 首行（去掉常见 markdown
                                          // 前缀 - * `> ` `- [ ] ` 等）+ cap
                                          // 80 chars 防 backend title 上限
                                          // (max 30 char in title input)；过
                                          // 长由 owner 在 modal 内手动缩。
                                          const firstLine = text.split("\n")[0]
                                            .replace(/^\s*(?:[-*+]\s+|\d+\.\s+|>\s+|\[[ xX]?\]\s+|-\s*\[[ xX]?\]\s+)/, "")
                                            .slice(0, 80);
                                          setTitle(firstLine);
                                          setBody(text);
                                          setQuickAddOpen(true);
                                          setBulkResultMsg(
                                            `📋 已把选中 ${text.length} 字带到新建任务（quickAdd modal 已展开）`,
                                          );
                                          window.setTimeout(
                                            () => setBulkResultMsg(""),
                                            3000,
                                          );
                                        }}
                                        title={
                                          hasSel
                                            ? `把选区 ${selEnd - selStart} 字带到「新建任务」modal 预填（首行作 title / 全段作 body）。让"长 detail 里看到值得独立的子项" 一键拆出新 task。`
                                            : "无选区。先在编辑器选一段文字"
                                        }
                                        style={{
                                          ...mdToolbarBtnStyle,
                                          opacity: hasSel ? 1 : 0.4,
                                          cursor: hasSel ? "pointer" : "default",
                                        }}
                                      >
                                        📋➕
                                      </button>
                                    );
                                  })()}
                                  {/* 📑 复制大纲：扫 detail.md H1-H3 拼成
                                      markdown 缩进列表（H1 = 0 indent / H2 =
                                      2 spaces / H3 = 4 spaces，前缀 "- "）+
                                      复制到剪贴板。让 owner 把任务的大纲
                                      作 TOC / 思维导图 root / 检查清单顶 paste
                                      到其它地方。无 heading 时按钮 disabled。 */}
                                  {(() => {
                                    const lines =
                                      editingDetailContent.split("\n");
                                    const headings: Array<{
                                      level: number;
                                      text: string;
                                    }> = [];
                                    for (const line of lines) {
                                      const m = line.match(
                                        /^(#{1,3})\s+(.*)$/,
                                      );
                                      if (m) {
                                        headings.push({
                                          level: m[1].length,
                                          text: m[2].trim(),
                                        });
                                      }
                                    }
                                    const hasHeadings = headings.length > 0;
                                    return (
                                      <button
                                        type="button"
                                        disabled={!hasHeadings}
                                        onClick={async () => {
                                          if (!hasHeadings) return;
                                          const indent = (lv: number) =>
                                            "  ".repeat(Math.max(0, lv - 1));
                                          const outline = headings
                                            .map(
                                              (h) =>
                                                `${indent(h.level)}- ${h.text}`,
                                            )
                                            .join("\n");
                                          try {
                                            await navigator.clipboard.writeText(
                                              outline,
                                            );
                                            setBulkResultMsg(
                                              `📑 已复制大纲（${headings.length} 条 heading）`,
                                            );
                                          } catch (e) {
                                            setActionErr(`复制失败：${e}`);
                                          }
                                          window.setTimeout(
                                            () => setBulkResultMsg(""),
                                            3000,
                                          );
                                        }}
                                        title={
                                          hasHeadings
                                            ? `扫 H1-H3 标题（共 ${headings.length} 条）拼缩进 markdown 列表复制到剪贴板，作 TOC / 思维导图根 / 检查清单顶。`
                                            : "无 heading（H1-H3）。先在编辑器加 # / ## / ### 标题"
                                        }
                                        style={{
                                          ...mdToolbarBtnStyle,
                                          opacity: hasHeadings ? 1 : 0.4,
                                          cursor: hasHeadings
                                            ? "pointer"
                                            : "default",
                                        }}
                                      >
                                        📑📋
                                      </button>
                                    );
                                  })()}
                                  {/* 🧠 ask LLM about selection：textarea 选
                                      中一段 → click 把选段封装成 "关于「...」"
                                      预填到 PanelChat textarea + 切到聊天
                                      tab。让 owner 在写 detail 时看到值得
                                      问 LLM 的段直接发问，不必离开编辑器
                                      手动复制 + 切 tab + 拼 prompt。无选区
                                      时按钮 disabled。仅 onAskLLMAbout 传入
                                      时显（PanelApp 端 wire；其它 caller 不
                                      显冗余 UI）。 */}
                                  {onAskLLMAbout && (() => {
                                    const selStart = Math.min(
                                      detailCursorPos,
                                      detailSelectionEnd,
                                    );
                                    const selEnd = Math.max(
                                      detailCursorPos,
                                      detailSelectionEnd,
                                    );
                                    const hasSel =
                                      selEnd > selStart &&
                                      selStart >= 0 &&
                                      selEnd <= editingDetailContent.length;
                                    return (
                                      <button
                                        type="button"
                                        disabled={!hasSel}
                                        onClick={() => {
                                          if (!hasSel) return;
                                          const text = editingDetailContent
                                            .slice(selStart, selEnd)
                                            .trim();
                                          if (!text) return;
                                          onAskLLMAbout(text);
                                          setBulkResultMsg(
                                            `🧠 已切到聊天 tab + 预填 "关于「...」" 让你立刻问 LLM`,
                                          );
                                          window.setTimeout(
                                            () => setBulkResultMsg(""),
                                            3000,
                                          );
                                        }}
                                        title={
                                          hasSel
                                            ? `把选区 ${selEnd - selStart} 字封装成 "关于「<excerpt 50 字>」 " 预填到 PanelChat textarea + 切到聊天 tab。owner 写 detail 时一键问 LLM 解释 / 评论 / 给建议这段。`
                                            : "无选区。先在编辑器选一段文字"
                                        }
                                        style={{
                                          ...mdToolbarBtnStyle,
                                          opacity: hasSel ? 1 : 0.4,
                                          cursor: hasSel
                                            ? "pointer"
                                            : "default",
                                        }}
                                      >
                                        🧠
                                      </button>
                                    );
                                  })()}
                                </div>
                              )}
                              {/* 📑 大纲浮窗：扫 H1-H3 显锚点列表 + click 跳节。
                                  仅 detailOutlineOpen 且 split / preview 模式
                                  渲染。inline panel（不浮 absolute），让 layout
                                  推开主编辑区一段；avoiding overlay coverage 让
                                  长 detail 选锚点时无视觉冲突。 */}
                              {detailOutlineOpen &&
                                (detailViewMode === "split" ||
                                  detailViewMode === "preview") &&
                                (() => {
                                  const lines = editingDetailContent.split("\n");
                                  const headings: Array<{
                                    level: number;
                                    text: string;
                                    counter: number;
                                  }> = [];
                                  let cnt = 0;
                                  for (const line of lines) {
                                    const m = line.match(/^(#{1,3})\s+(.*)$/);
                                    if (m) {
                                      cnt += 1;
                                      headings.push({
                                        level: m[1].length,
                                        text: m[2].trim(),
                                        counter: cnt,
                                      });
                                    }
                                  }
                                  if (headings.length === 0) {
                                    // 无 heading → 按钮 gate 已挡，这里防御
                                    return null;
                                  }
                                  return (
                                    <div
                                      style={{
                                        padding: "8px 10px",
                                        border:
                                          "1px solid var(--pet-color-border)",
                                        borderRadius: 6,
                                        background: "var(--pet-color-card)",
                                        boxShadow: "var(--pet-shadow-sm)",
                                        fontSize: 11,
                                        lineHeight: 1.4,
                                        maxHeight: 200,
                                        overflowY: "auto",
                                      }}
                                    >
                                      <div
                                        style={{
                                          fontSize: 10,
                                          fontWeight: 600,
                                          color: "var(--pet-color-muted)",
                                          marginBottom: 4,
                                          letterSpacing: 0.3,
                                        }}
                                      >
                                        📑 大纲（{headings.length} 节）
                                      </div>
                                      {headings.map((h) => {
                                        const isActive =
                                          h.counter === activeHeadingCounter;
                                        return (
                                        <button
                                          key={h.counter}
                                          type="button"
                                          onClick={() => {
                                            const id = `pet-detail-${t.title}-h${h.counter}`;
                                            const el =
                                              document.getElementById(id);
                                            if (el) {
                                              el.scrollIntoView({
                                                behavior: "smooth",
                                                block: "start",
                                              });
                                            }
                                          }}
                                          style={{
                                            display: "block",
                                            width: "100%",
                                            textAlign: "left",
                                            // active = IntersectionObserver 跟
                                            // 踪到的"最靠上可见"heading；tint
                                            // 蓝 bg + 加粗让 owner 滚 preview
                                            // 时一眼知道大纲里"我在哪节"。
                                            background: isActive
                                              ? "var(--pet-tint-blue-bg)"
                                              : "transparent",
                                            border: "none",
                                            padding: `2px 4px 2px ${(h.level - 1) * 12 + 4}px`,
                                            fontSize: 11,
                                            color: isActive
                                              ? "var(--pet-tint-blue-fg)"
                                              : "var(--pet-color-fg)",
                                            fontWeight: isActive ? 600 : 400,
                                            cursor: "pointer",
                                            fontFamily: "inherit",
                                            overflow: "hidden",
                                            textOverflow: "ellipsis",
                                            whiteSpace: "nowrap",
                                            borderRadius: 3,
                                          }}
                                          onMouseOver={(e) => {
                                            // hover 颜色仅在非 active 时覆盖
                                            // —— active tint 已显眼，再 hover
                                            // 覆盖会丢"我在哪节"信号。
                                            if (isActive) return;
                                            (
                                              e.currentTarget as HTMLButtonElement
                                            ).style.background =
                                              "var(--pet-color-bg)";
                                          }}
                                          onMouseOut={(e) => {
                                            if (isActive) return;
                                            (
                                              e.currentTarget as HTMLButtonElement
                                            ).style.background = "transparent";
                                          }}
                                          title={`跳到「${h.text}」（H${h.level}）${isActive ? " · 当前节" : ""}`}
                                        >
                                          <span
                                            style={{
                                              color: "var(--pet-color-muted)",
                                              marginRight: 4,
                                              fontFamily:
                                                "'SF Mono', 'Menlo', monospace",
                                              fontSize: 10,
                                            }}
                                          >
                                            {"#".repeat(h.level)}
                                          </span>
                                          {h.text || "（空标题）"}
                                        </button>
                                        );
                                      })}
                                    </div>
                                  );
                                })()}
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
                                onChange={(e) => {
                                  setEditingDetailContent(e.target.value);
                                  setDetailCursorPos(e.target.selectionStart);
                                  setDetailSelectionEnd(e.target.selectionEnd);
                                }}
                                onSelect={(e) => {
                                  const ta = e.target as HTMLTextAreaElement;
                                  setDetailCursorPos(ta.selectionStart);
                                  setDetailSelectionEnd(ta.selectionEnd);
                                }}
                                onKeyUp={(e) => {
                                  setDetailCursorPos(e.currentTarget.selectionStart);
                                  setDetailSelectionEnd(e.currentTarget.selectionEnd);
                                }}
                                onClick={(e) => {
                                  const ta = e.target as HTMLTextAreaElement;
                                  setDetailCursorPos(ta.selectionStart);
                                  setDetailSelectionEnd(ta.selectionEnd);
                                }}
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
                                  // Enter 自动续列表前缀（无序 / GFM checklist /
                                  // 有序 / blockquote）。优先级在 bracket 之前
                                  // —— Enter 不是 bracket pair 触发字符，互不
                                  // 冲突，但放第一防未来某 modifier+Enter 引入
                                  // 时的歧义。
                                  if (handleDetailListContinue(e)) return;
                                  // 中文 typography 配对（「『（【《""'）：成对
                                  // 自动插 close，光标落 inner。优先级最高 —
                                  // 字符级 intercept 不该让 ⌘S / Esc 抢走。
                                  if (handleDetailBracketPair(e)) return;
                                  // ⌘D 复制当前行 / 选区：Sublime / JetBrains 通用
                                  // IDE 行为。放在 ⌘S 之前 —— 二者不冲突但保
                                  // 一致 modifier handler 集群。
                                  if (handleDetailDuplicateLine(e)) return;
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
                                  // ⌘⇧Enter / Ctrl+⇧+Enter 保存并关闭（即
                                  // "完成本轮编辑"）。与 iter #215 新建表单
                                  // ⌘⇧Enter "建并打开 detail" 对偶 —— "⌘⇧Enter
                                  // = 一键完成本轮工作"心智一致。handleSaveDetail
                                  // 内部本就 setEditingDetailTitle(null) 关编
                                  // 辑器，复用即可。无 ⇧ 时是 textarea 原生
                                  // 换行，不抢。
                                  if (
                                    (e.metaKey || e.ctrlKey) &&
                                    e.shiftKey &&
                                    e.key === "Enter"
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
                                placeholder="在这里追加 / 修改进度笔记…保存后覆盖 detail.md。（⌘S 保存 / ⌘⇧Enter 保存并关闭 / ⌘[/⌘] 上 / 下一条 task / ⌘K 跳到任意 task detail / Esc 取消）"
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
                                      parseMarkdown(editingDetailContent, {
                                        checkboxToggle: {
                                          lineOffset: 0,
                                          onToggle: toggleEditChecklistLine,
                                        },
                                        headingIdPrefix: `pet-detail-${t.title}`,
                                        onHeadingCopySection: handleCopyHeadingSection,
                                      })
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
                                    parseMarkdown(editingDetailContent, {
                                      checkboxToggle: {
                                        lineOffset: 0,
                                        onToggle: toggleEditChecklistLine,
                                      },
                                      headingIdPrefix: `pet-detail-${t.title}`,
                                      onHeadingCopySection: handleCopyHeadingSection,
                                    })
                                  )}
                                </div>
                              ) : (
                              <textarea
                                ref={detailEditorRef}
                                value={editingDetailContent}
                                onChange={(e) => {
                                  setEditingDetailContent(e.target.value);
                                  setDetailCursorPos(e.target.selectionStart);
                                  setDetailSelectionEnd(e.target.selectionEnd);
                                }}
                                onSelect={(e) => {
                                  const ta = e.target as HTMLTextAreaElement;
                                  setDetailCursorPos(ta.selectionStart);
                                  setDetailSelectionEnd(ta.selectionEnd);
                                }}
                                onKeyUp={(e) => {
                                  setDetailCursorPos(e.currentTarget.selectionStart);
                                  setDetailSelectionEnd(e.currentTarget.selectionEnd);
                                }}
                                onClick={(e) => {
                                  const ta = e.target as HTMLTextAreaElement;
                                  setDetailCursorPos(ta.selectionStart);
                                  setDetailSelectionEnd(ta.selectionEnd);
                                }}
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
                                  if (handleDetailListContinue(e)) return;
                                  if (handleDetailBracketPair(e)) return;
                                  if (handleDetailDuplicateLine(e)) return;
                                  if (
                                    (e.metaKey || e.ctrlKey) &&
                                    e.key.toLowerCase() === "s"
                                  ) {
                                    e.preventDefault();
                                    if (savingDetail) return;
                                    handleSaveDetail(t.title);
                                    return;
                                  }
                                  // ⌘⇧Enter / Ctrl+⇧+Enter 保存并关闭，与 edit
                                  // 模式 textarea 同 handler。
                                  if (
                                    (e.metaKey || e.ctrlKey) &&
                                    e.shiftKey &&
                                    e.key === "Enter"
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
                                placeholder="在这里追加 / 修改进度笔记…保存后覆盖 detail.md。（⌘S 保存 / ⌘⇧Enter 保存并关闭 / ⌘[/⌘] 上 / 下一条 task / ⌘K 跳到任意 task detail / Esc 取消）"
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
                                    taskLookupForRefs,
                                    handleTaskRefClick,
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
                {/* 🗑 清理 >30 天：二次确认（首次进 armed 红字；5s 内再点真清）。
                    禁用条件：归档为空 / 加载中 / 正在清。armed 5s 后自动 disarm。 */}
                {archiveItems.length > 0 && (
                  <button
                    type="button"
                    onClick={(e) => {
                      e.stopPropagation();
                      if (archivePurgeArmed) void doArchivePurge();
                      else armArchivePurge();
                    }}
                    disabled={archiveLoading || archivePurging}
                    style={{
                      fontSize: 11,
                      padding: "2px 8px",
                      border: "1px solid",
                      borderColor: archivePurgeArmed
                        ? "var(--pet-tint-red-fg)"
                        : "var(--pet-color-border)",
                      borderRadius: 4,
                      background: archivePurgeArmed
                        ? "var(--pet-tint-red-bg)"
                        : "var(--pet-color-card)",
                      color: archivePurgeArmed
                        ? "var(--pet-tint-red-fg)"
                        : "var(--pet-color-muted)",
                      cursor:
                        archiveLoading || archivePurging ? "default" : "pointer",
                      fontWeight: archivePurgeArmed ? 600 : 400,
                    }}
                    title={
                      archivePurgeArmed
                        ? "再点一次真清；5 秒内不点自动取消"
                        : "清掉 task_archive 里 updated_at 超 30 天的条目（与归档进入窗口对齐）"
                    }
                  >
                    {archivePurging
                      ? "清理中…"
                      : archivePurgeArmed
                        ? "再点确认 ⚠️"
                        : "🗑 清理 >30 天"}
                  </button>
                )}
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
              ) : (() => {
                // 搜索过滤：空 query 直接走原列表；非空 case-insensitive 子串匹配
                // title / description。filteredItems 渲染传给下方 map。
                const q = archiveQuery.trim().toLowerCase();
                const filtered =
                  q.length === 0
                    ? archiveItems
                    : archiveItems.filter(
                        (it) =>
                          it.title.toLowerCase().includes(q) ||
                          it.description.toLowerCase().includes(q),
                      );
                return (
                  <>
                    {/* 归档搜索框：仅 archiveItems 非空时渲染（loading / empty
                        都不显，避免噪音）。空 query placeholder 提示总数；非
                        空时下方计数显"过滤后 / 总数"。 */}
                    <div style={{ marginBottom: 8, display: "flex", gap: 6, alignItems: "center" }}>
                      <input
                        type="text"
                        value={archiveQuery}
                        onChange={(e) => setArchiveQuery(e.target.value)}
                        onKeyDown={(e) => {
                          if (e.key === "Escape" && archiveQuery) {
                            e.preventDefault();
                            setArchiveQuery("");
                          }
                        }}
                        placeholder={`搜归档 title / description…（共 ${archiveItems.length} 条）`}
                        style={{
                          flex: 1,
                          padding: "5px 10px",
                          fontSize: 12,
                          border: "1px solid var(--pet-color-border)",
                          borderRadius: 6,
                          background: "var(--pet-color-card)",
                          color: "var(--pet-color-fg)",
                          fontFamily: "inherit",
                        }}
                      />
                      {archiveQuery.length > 0 && (
                        <>
                          <span style={{ fontSize: 11, color: "var(--pet-color-muted)", fontFamily: "'SF Mono', 'Menlo', monospace" }}>
                            {filtered.length} / {archiveItems.length}
                          </span>
                          <button
                            type="button"
                            onClick={() => setArchiveQuery("")}
                            style={{
                              fontSize: 11,
                              padding: "3px 8px",
                              border: "1px solid var(--pet-color-border)",
                              borderRadius: 4,
                              background: "var(--pet-color-card)",
                              color: "var(--pet-color-muted)",
                              cursor: "pointer",
                              fontFamily: "inherit",
                            }}
                            title="清空搜索"
                          >
                            ✕
                          </button>
                        </>
                      )}
                    </div>
                    {filtered.length === 0 ? (
                      <EmptyState
                        icon="🔍"
                        title="没有匹配的归档"
                        hint="试试更短的关键词，或清空搜索看全集"
                        compact
                      />
                    ) : (
                      filtered.map((it) => {
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
                        <span style={{ fontSize: 12, fontWeight: 600, color: "var(--pet-color-fg)", wordBreak: "break-word", flex: 1 }}>
                          {displayTitle}
                        </span>
                        {/* ↩ 恢复到队列：剥归档 / 终态 marker，重新创建为 pending
                            butler_task；老 archive 条目同时删除。不弹确认（恢复
                            操作是低风险 —— task_archive 条目还能再次手动创建）。 */}
                        <button
                          type="button"
                          onClick={async () => {
                            try {
                              const msg = await invoke<string>("task_unarchive", { title: it.title });
                              setBulkResultMsg(msg);
                              await reloadArchive();
                              await reload();
                            } catch (e) {
                              setBulkResultMsg(`恢复失败：${e}`);
                            }
                            window.setTimeout(() => setBulkResultMsg(""), 4000);
                          }}
                          style={{
                            flexShrink: 0,
                            padding: "2px 8px",
                            fontSize: 11,
                            border: "1px solid var(--pet-color-border)",
                            borderRadius: 4,
                            background: "var(--pet-color-card)",
                            color: "var(--pet-color-accent)",
                            cursor: "pointer",
                            fontFamily: "inherit",
                          }}
                          title="把这条归档剥光 done / archived / result 等标记，重建为 pending butler_task（detail.md 不带回，需要的话先手动复制内容）"
                        >
                          ↩ 恢复
                        </button>
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
                  </>
                );
              })()}
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
                <optgroup label="内置范例">
                  {TASK_TEMPLATES_BUILTIN.map((tpl, i) => (
                    <option key={tpl.label} value={i}>
                      {tpl.label}
                    </option>
                  ))}
                </optgroup>
                {customTemplates.length > 0 && (
                  <optgroup label="我存的">
                    {customTemplates.map((tpl, j) => (
                      <option
                        key={tpl.label}
                        value={TASK_TEMPLATES_BUILTIN.length + j}
                      >
                        {tpl.label}
                      </option>
                    ))}
                  </optgroup>
                )}
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
                onClick={() => void handleCreate(false)}
                disabled={creating || !title.trim()}
                title="创建任务（⌘Enter / Ctrl+Enter 等价）。⌘⇧Enter 创建并打开 detail 编辑器。"
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
      {/* 自定义模板管理 Modal：列每条 label + title 前缀 + body 前缀 + 删除按钮。
          customTemplates 空数组时 Modal 仍可被强行打开（理论不发生：入口按钮在
          length === 0 时根本不渲染），渲染兜底空态文案。 */}
      <Modal
        open={templatesManagerOpen}
        onClose={() => setTemplatesManagerOpen(false)}
        maxWidth={520}
      >
        <div
          style={{
            display: "flex",
            justifyContent: "space-between",
            alignItems: "baseline",
            marginBottom: 14,
          }}
        >
          <h2 style={{ margin: 0, fontSize: 15, fontWeight: 600 }}>
            自定义任务模板
          </h2>
          <span style={{ fontSize: 11, color: "var(--pet-color-muted)" }}>
            点背景或 Esc 关闭 · 共 {customTemplates.length} / {CUSTOM_TEMPLATES_MAX}
          </span>
        </div>
        {customTemplates.length === 0 ? (
          <div
            style={{
              fontSize: 12,
              color: "var(--pet-color-muted)",
              padding: "16px 0",
              textAlign: "center",
            }}
          >
            还没有自定义模板。填好新建任务的标题 / 内容后点「💾 存为」就能加一条。
          </div>
        ) : (
          <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
            {customTemplates.map((tpl) => (
              <div
                key={tpl.label}
                style={{
                  display: "flex",
                  alignItems: "flex-start",
                  gap: 10,
                  padding: "8px 10px",
                  border: "1px solid var(--pet-color-border)",
                  borderRadius: 8,
                  background: "var(--pet-color-card)",
                }}
              >
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div
                    style={{
                      fontSize: 12,
                      fontWeight: 600,
                      color: "var(--pet-color-fg)",
                      marginBottom: 2,
                    }}
                  >
                    {tpl.label}
                  </div>
                  <div
                    style={{
                      fontSize: 11,
                      color: "var(--pet-color-muted)",
                      whiteSpace: "nowrap",
                      overflow: "hidden",
                      textOverflow: "ellipsis",
                    }}
                    title={`标题：${tpl.title}\n\n内容：${tpl.body}`}
                  >
                    标题：{tpl.title}
                  </div>
                  {tpl.body && (
                    <div
                      style={{
                        fontSize: 11,
                        color: "var(--pet-color-muted)",
                        whiteSpace: "nowrap",
                        overflow: "hidden",
                        textOverflow: "ellipsis",
                        marginTop: 2,
                      }}
                    >
                      内容：{tpl.body.replace(/\n/g, "  ").slice(0, 80)}
                      {tpl.body.length > 80 ? "…" : ""}
                    </div>
                  )}
                </div>
                <button
                  type="button"
                  onClick={() => deleteCustomTemplate(tpl.label)}
                  title={`删除模板「${tpl.label}」`}
                  style={{
                    padding: "4px 10px",
                    fontSize: 11,
                    border:
                      "1px solid color-mix(in srgb, var(--pet-tint-red-fg) 40%, var(--pet-color-border))",
                    borderRadius: 6,
                    background: "var(--pet-color-card)",
                    color: "var(--pet-tint-red-fg)",
                    cursor: "pointer",
                    flexShrink: 0,
                  }}
                >
                  删除
                </button>
              </div>
            ))}
          </div>
        )}
      </Modal>
      {/* ⌘K task quick-find palette：detail.md 编辑器内任意时刻按 ⌘K 唤
          起。input 即时 fuzzy filter visibleTasks（含 filter/sort 后视图），
          ↑↓ 移动选中 idx，Enter 切换到该 task 的 detail 编辑器（复用
          switchToTaskDetail pipeline），Esc / outside-click 关。fixed 顶层
          浮卡，与 row hover preview / ctx menu 同 viewport-clamp 思路。 */}
      {taskPaletteOpen && (() => {
        const q = paletteQuery.trim().toLowerCase();
        const filtered =
          q === ""
            ? visibleTasks.slice(0, 30) // 空 query 时显前 30 条
            : visibleTasks
                .filter((t) => t.title.toLowerCase().includes(q))
                .slice(0, 30);
        const safeIdx = Math.max(
          0,
          Math.min(paletteSelectedIdx, filtered.length - 1),
        );
        return (
          <div
            onMouseDown={(e) => {
              // 点 backdrop（target === currentTarget）关；点内部 palette 不关
              if (e.target === e.currentTarget) {
                setTaskPaletteOpen(false);
              }
            }}
            style={{
              position: "fixed",
              inset: 0,
              background: "rgba(0,0,0,0.3)",
              zIndex: 200,
              display: "flex",
              alignItems: "flex-start",
              justifyContent: "center",
              paddingTop: "10vh",
            }}
          >
            <div
              onMouseDown={(e) => e.stopPropagation()}
              style={{
                width: 480,
                maxWidth: "90vw",
                background: "var(--pet-color-card)",
                border: "1px solid var(--pet-color-border)",
                borderRadius: 8,
                boxShadow: "var(--pet-shadow-md)",
                padding: 8,
                display: "flex",
                flexDirection: "column",
                gap: 4,
              }}
            >
              <input
                ref={paletteInputRef}
                type="text"
                autoFocus
                value={paletteQuery}
                onChange={(e) => {
                  setPaletteQuery(e.target.value);
                  setPaletteSelectedIdx(0);
                }}
                onKeyDown={(e) => {
                  if (e.key === "Escape") {
                    e.preventDefault();
                    setTaskPaletteOpen(false);
                    return;
                  }
                  if (e.key === "ArrowDown") {
                    e.preventDefault();
                    setPaletteSelectedIdx((i) =>
                      filtered.length === 0
                        ? 0
                        : Math.min(i + 1, filtered.length - 1),
                    );
                    return;
                  }
                  if (e.key === "ArrowUp") {
                    e.preventDefault();
                    setPaletteSelectedIdx((i) => Math.max(0, i - 1));
                    return;
                  }
                  if (e.key === "Enter") {
                    e.preventDefault();
                    const target = filtered[safeIdx];
                    if (!target) return;
                    setTaskPaletteOpen(false);
                    if (taskPaletteMode === "insertRef") {
                      insertTaskRefAtCursor(target.title);
                    } else {
                      void switchToTaskDetail(target.title);
                    }
                    return;
                  }
                }}
                placeholder={
                  taskPaletteMode === "insertRef"
                    ? `fuzzy 选 task 插 ref token「title」（共 ${visibleTasks.length}）· ↑↓ 选 · Enter 插 · Esc 关`
                    : `fuzzy 找 task （共 ${visibleTasks.length}）· ↑↓ 选 · Enter 切 · Esc 关`
                }
                style={{
                  padding: "6px 10px",
                  fontSize: 13,
                  border: "1px solid var(--pet-color-border)",
                  borderRadius: 6,
                  background: "var(--pet-color-bg)",
                  color: "var(--pet-color-fg)",
                  fontFamily: "inherit",
                  outline: "none",
                }}
              />
              <div
                style={{
                  maxHeight: 360,
                  overflowY: "auto",
                  display: "flex",
                  flexDirection: "column",
                  gap: 2,
                }}
              >
                {filtered.length === 0 ? (
                  <div
                    style={{
                      padding: "12px",
                      fontSize: 12,
                      color: "var(--pet-color-muted)",
                      fontStyle: "italic",
                      textAlign: "center",
                    }}
                  >
                    {q === ""
                      ? "（无任务）"
                      : `没有标题含「${paletteQuery}」的任务`}
                  </div>
                ) : (
                  filtered.map((t, i) => {
                    const active = i === safeIdx;
                    const isCurrent = t.title === editingDetailTitle;
                    // insertRef 模式允许插当前 task 的 ref（自引并不常见但合
                    // 法 —— 比如把已完成子任务列回主任务自身的 detail），
                    // 仅 jump 模式 disable current。
                    const disabled =
                      taskPaletteMode === "jump" && isCurrent;
                    return (
                      <button
                        key={t.title}
                        type="button"
                        onMouseEnter={() => setPaletteSelectedIdx(i)}
                        onClick={() => {
                          setTaskPaletteOpen(false);
                          if (taskPaletteMode === "insertRef") {
                            insertTaskRefAtCursor(t.title);
                          } else {
                            void switchToTaskDetail(t.title);
                          }
                        }}
                        style={{
                          padding: "6px 10px",
                          fontSize: 12,
                          border: "none",
                          background: active
                            ? "var(--pet-tint-blue-bg)"
                            : "transparent",
                          color: active
                            ? "var(--pet-tint-blue-fg)"
                            : disabled
                              ? "var(--pet-color-muted)"
                              : "var(--pet-color-fg)",
                          fontWeight: active ? 600 : 400,
                          cursor: disabled ? "default" : "pointer",
                          opacity: disabled ? 0.5 : 1,
                          borderRadius: 4,
                          textAlign: "left",
                          fontFamily: "inherit",
                          display: "flex",
                          alignItems: "center",
                          justifyContent: "space-between",
                          gap: 8,
                        }}
                        disabled={disabled}
                        title={
                          taskPaletteMode === "insertRef"
                            ? `插入 ref「${t.title}」到光标位置`
                            : isCurrent
                              ? "当前已在编辑此 task"
                              : `切到「${t.title}」detail 编辑器`
                        }
                      >
                        <span
                          style={{
                            overflow: "hidden",
                            textOverflow: "ellipsis",
                            whiteSpace: "nowrap",
                            flex: 1,
                          }}
                        >
                          {t.title}
                        </span>
                        <span
                          style={{
                            fontSize: 10,
                            color: "var(--pet-color-muted)",
                            fontFamily: "'SF Mono', monospace",
                            flexShrink: 0,
                          }}
                        >
                          P{t.priority}
                          {isCurrent ? " · 当前" : ""}
                        </span>
                      </button>
                    );
                  })
                )}
              </div>
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
            {/* 📌 钉住 toggle：done / cancelled 行也允许（owner 自标"重要"
                与状态正交），所以不放在 canMarkDone gate 后面。current pinned
                state 从 t.pinned 读，label / color 反映"将切换到的方向"。
                调 task_set_pinned 后 reload 让 chip 计数 / row chip 即时刷。 */}
            <button
              type="button"
              style={{
                ...itemBtn,
                color: t?.pinned
                  ? "var(--pet-color-muted)"
                  : "var(--pet-tint-amber-fg, #d97706)",
              }}
              onMouseOver={itemBtnHoverIn}
              onMouseOut={itemBtnHoverOut}
              onClick={async () => {
                setTaskCtxMenu(null);
                setActionErr("");
                setBusyTitle(m.title);
                try {
                  await invoke<void>("task_set_pinned", {
                    title: m.title,
                    pinned: !t?.pinned,
                  });
                  await reload();
                } catch (e) {
                  setActionErr(`钉住失败：${e}`);
                } finally {
                  setBusyTitle(null);
                }
              }}
              title={
                t?.pinned
                  ? "已钉住 —— 点击取消（剥 [pinned] marker）"
                  : "钉住任务（写 [pinned] marker） —— 「📌 N」chip 可一键过滤"
              }
            >
              {t?.pinned ? "📌 取消钉住" : "📌 钉住"}
            </button>
            {/* 🔇 silent toggle：与 📌 钉住 同模板。从 raw_description 里
                inline regex 探 `[silent]` 字面量；调 task_set_silent
                atomic add / strip marker。silent 任务从 LLM proactive cycle
                pick 队列消失（在 format_butler_tasks_block filter 同
                blockedBy / snooze union pipeline）；面板 / 手动触发不受影响。 */}
            {(() => {
              const isSilent = !!t?.raw_description?.includes("[silent]");
              return (
                <button
                  type="button"
                  style={{
                    ...itemBtn,
                    color: isSilent
                      ? "var(--pet-color-accent)"
                      : "var(--pet-color-muted)",
                  }}
                  onMouseOver={itemBtnHoverIn}
                  onMouseOut={itemBtnHoverOut}
                  onClick={async () => {
                    setTaskCtxMenu(null);
                    setActionErr("");
                    setBusyTitle(m.title);
                    try {
                      await invoke<void>("task_set_silent", {
                        title: m.title,
                        silent: !isSilent,
                      });
                      await reload();
                    } catch (e) {
                      setActionErr(`silent 切换失败：${e}`);
                    } finally {
                      setBusyTitle(null);
                    }
                  }}
                  title={
                    isSilent
                      ? "已标 [silent] —— 点击解除（剥 marker）让 LLM proactive cycle 重新看到此任务"
                      : "标 [silent] —— LLM 在 proactive cycle 不再主动选此任务（仍在面板可见 / 仍可手动触发）"
                  }
                >
                  {isSilent ? "🔇 解除 silent" : "🔇 标 silent"}
                </button>
              );
            })()}
            {/* ✨ LLM 重写标题：与 PanelChat session ctx menu 的"LLM 重写标题"
                按钮同模板。调用一次非流式 chat/completions（30s timeout /
                temperature 0.3 / max_tokens 30），让 LLM 看任务 title + 描述 +
                detail.md 前 600 字给一句 ≤ 10 字的新标题；返回后 atomic
                memory_rename 写回。中途让 toast 显"进行中"避免用户以为按钮坏了。 */}
            <button
              type="button"
              style={{ ...itemBtn, color: "var(--pet-color-accent)" }}
              onMouseOver={itemBtnHoverIn}
              onMouseOut={itemBtnHoverOut}
              onClick={async () => {
                setTaskCtxMenu(null);
                setActionErr("");
                setBulkResultMsg(`✨ 正在让 LLM 重写「${m.title}」的标题…`);
                setBusyTitle(m.title);
                try {
                  const newTitle = await invoke<string>(
                    "regenerate_task_title",
                    { title: m.title },
                  );
                  setBulkResultMsg(`✨ 已重写标题：${newTitle}`);
                  window.setTimeout(() => setBulkResultMsg(""), 4000);
                  await reload();
                } catch (e) {
                  setActionErr(`重写标题失败：${e}`);
                  setBulkResultMsg("");
                } finally {
                  setBusyTitle(null);
                }
              }}
              title="让 LLM 看任务标题 + 描述 + detail.md 前 600 字，给一句 ≤ 10 字新标题，并直接改名。免去手动想新名的脑力开销。"
            >
              ✨ LLM 重写标题
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
            {/* Snooze preset：把任务暂停到指定时刻 —— pending / error 都允许。
                与 [snooze: ...] marker 协议同源（YYYY-MM-DD HH:MM 空格分隔）。
                "今晚 18:00 已过" 自动跳明晚（与 due `今晚` chip 同行为）；
                "下周一" 永远跳下周（即使今日就是周一）—— 与 dueNextMonday 同
                语义。currentSnoozed 用 tasks.find 拿最新 snoozed_until；
                truthy 时多渲一个"解除暂停"行让用户随时撤销。 */}
            {canMarkDone && (() => {
              const cur = tasks.find((x) => x.title === m.title);
              const currentSnoozed = cur?.snoozed_until ?? null;
              const fmt = (d: Date) => {
                const y = d.getFullYear();
                const mo = String(d.getMonth() + 1).padStart(2, "0");
                const da = String(d.getDate()).padStart(2, "0");
                const hh = String(d.getHours()).padStart(2, "0");
                const mm = String(d.getMinutes()).padStart(2, "0");
                return `${y}-${mo}-${da} ${hh}:${mm}`;
              };
              const computeUntil = (preset: string): string => {
                const now = new Date();
                if (preset === "30m") {
                  return fmt(new Date(now.getTime() + 30 * 60 * 1000));
                }
                if (preset === "tonight") {
                  const d = new Date(
                    now.getFullYear(),
                    now.getMonth(),
                    now.getDate(),
                    18,
                    0,
                    0,
                  );
                  if (d.getTime() <= now.getTime()) {
                    d.setDate(d.getDate() + 1);
                  }
                  return fmt(d);
                }
                if (preset === "tomorrow") {
                  const d = new Date(
                    now.getFullYear(),
                    now.getMonth(),
                    now.getDate() + 1,
                    9,
                    0,
                    0,
                  );
                  return fmt(d);
                }
                // nextMonday
                const today = now.getDay();
                const daysAhead = today === 0 ? 1 : 7 - today + 1;
                const d = new Date(
                  now.getFullYear(),
                  now.getMonth(),
                  now.getDate() + daysAhead,
                  9,
                  0,
                  0,
                );
                return fmt(d);
              };
              const presets: Array<{ key: string; label: string }> = [
                { key: "30m", label: "💤 暂停 30 分" },
                { key: "tonight", label: "💤 暂停至今晚 18:00" },
                { key: "tomorrow", label: "💤 暂停至明早 09:00" },
                { key: "nextMonday", label: "💤 暂停至下周一 09:00" },
              ];
              const setSnooze = async (until: string | null) => {
                setTaskCtxMenu(null);
                setActionErr("");
                setBusyTitle(m.title);
                try {
                  await invoke<void>("task_set_snooze", {
                    title: m.title,
                    until,
                  });
                  await reload();
                } catch (e) {
                  setActionErr(`设 snooze 失败：${e}`);
                } finally {
                  setBusyTitle(null);
                }
              };
              return (
                <>
                  {presets.map((p) => (
                    <button
                      key={p.key}
                      type="button"
                      style={itemBtn}
                      onMouseOver={itemBtnHoverIn}
                      onMouseOut={itemBtnHoverOut}
                      onClick={() => void setSnooze(computeUntil(p.key))}
                    >
                      {p.label}
                    </button>
                  ))}
                  {currentSnoozed && (
                    <button
                      type="button"
                      style={{
                        ...itemBtn,
                        color: "var(--pet-color-accent)",
                      }}
                      onMouseOver={itemBtnHoverIn}
                      onMouseOut={itemBtnHoverOut}
                      onClick={() => void setSnooze(null)}
                      title={`当前 snooze 至 ${currentSnoozed.replace("T", " ")}`}
                    >
                      ☀️ 解除暂停
                    </button>
                  )}
                </>
              );
            })()}
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
            {/* ✦ +1 / ✦ -1 priority 微调按钮：邻近 priority submenu 之前。
                免开 submenu 单 click 升 / 降一档；clamp [0, PRIORITY_MAX]
                时禁用对应方向按钮 + opacity 视觉降级。复用 handleInlineSetPriority
                既有 backend pipeline（与 priority submenu 单 click 相同 IPC）。 */}
            {(() => {
              const canInc = m.priority < PRIORITY_MAX;
              const canDec = m.priority > 0;
              return (
                <div
                  style={{
                    display: "flex",
                    gap: 4,
                    padding: "0 4px",
                  }}
                >
                  <button
                    type="button"
                    disabled={!canDec}
                    style={{
                      ...itemBtn,
                      flex: 1,
                      textAlign: "center",
                      padding: "4px 8px",
                      opacity: canDec ? 1 : 0.4,
                      cursor: canDec ? "pointer" : "default",
                    }}
                    onMouseOver={(e) => {
                      if (canDec) itemBtnHoverIn(e);
                    }}
                    onMouseOut={itemBtnHoverOut}
                    onClick={() => {
                      if (!canDec) return;
                      setTaskCtxMenu(null);
                      void handleInlineSetPriority(m.title, m.priority - 1);
                    }}
                    title={
                      canDec
                        ? `把优先级从 P${m.priority} 降到 P${m.priority - 1}（更不紧急）`
                        : "已是最低 P0，无法再降"
                    }
                  >
                    ✦ -1 (→P{Math.max(0, m.priority - 1)})
                  </button>
                  <button
                    type="button"
                    disabled={!canInc}
                    style={{
                      ...itemBtn,
                      flex: 1,
                      textAlign: "center",
                      padding: "4px 8px",
                      opacity: canInc ? 1 : 0.4,
                      cursor: canInc ? "pointer" : "default",
                    }}
                    onMouseOver={(e) => {
                      if (canInc) itemBtnHoverIn(e);
                    }}
                    onMouseOut={itemBtnHoverOut}
                    onClick={() => {
                      if (!canInc) return;
                      setTaskCtxMenu(null);
                      void handleInlineSetPriority(m.title, m.priority + 1);
                    }}
                    title={
                      canInc
                        ? `把优先级从 P${m.priority} 升到 P${m.priority + 1}（更紧急）`
                        : `已是最高 P${PRIORITY_MAX}，无法再升`
                    }
                  >
                    ✦ +1 (→P{Math.min(PRIORITY_MAX, m.priority + 1)})
                  </button>
                </div>
              );
            })()}
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
            {/* 🔗 复制 detail.md 绝对路径：调后端 memory_detail_abs_path
                把相对路径拼出绝对路径，写剪贴板。owner 可 paste 进 IDE 文件
                打开框（VSCode ⌘P 接受绝对 path）/ Finder bar / shell `open`
                直奔 detail.md 本地文件。仅 t.detail_path 非空时浮（任务还没
                writeDetail 过则没 path 可复制）。 */}
            {t && t.detail_path && (
              <button
                type="button"
                style={itemBtn}
                onMouseOver={itemBtnHoverIn}
                onMouseOut={itemBtnHoverOut}
                onClick={async () => {
                  setTaskCtxMenu(null);
                  try {
                    const abs = await invoke<string>(
                      "memory_detail_abs_path",
                      { detailPath: t.detail_path },
                    );
                    await navigator.clipboard.writeText(abs);
                    setBulkResultMsg(`已复制 detail.md 绝对路径`);
                  } catch (e) {
                    setBulkResultMsg(`复制 path 失败：${e}`);
                  }
                  window.setTimeout(() => setBulkResultMsg(""), 3000);
                }}
                title="把 detail.md 的绝对路径（含 ~/.config/pet/memories/... 前缀）复制到剪贴板。粘到 VSCode ⌘P / IntelliJ ⇧⌘O / Finder ⇧⌘G / shell `open` 都能直接打开本地文件，比走「📂 在 Finder 显示」少一次定位点击。"
              >
                🔗 复制 detail.md 绝对路径
              </button>
            )}
            {/* 复制为 markdown 引用块 (> ...)：与 📑 完整段不同，blockquote
                轻量 quote 形态，适合 paste 到 detail.md / chat / 别的 task 描述
                里作为 "ref 到此任务" 一段。emoji + title + meta 单行 + 描述
                前 200 字。 */}
            {t && (
              <button
                type="button"
                style={itemBtn}
                onMouseOver={itemBtnHoverIn}
                onMouseOut={itemBtnHoverOut}
                onClick={async () => {
                  setTaskCtxMenu(null);
                  try {
                    await navigator.clipboard.writeText(
                      formatTaskAsBlockquote(t),
                    );
                    setBulkResultMsg(`已复制 "${t.title}" 为引用块`);
                  } catch (e) {
                    setBulkResultMsg(`复制失败：${e}`);
                  }
                  window.setTimeout(() => setBulkResultMsg(""), 3000);
                }}
                title="把任务复制为 markdown blockquote（> 起头）—— 适合 paste 进 detail.md / chat / 别的任务描述。比 「📑 完整段」 简短，比 「🔗 ref token」 多带状态 / 优先级 / due / tags / 描述预览。"
              >
                💬 复制为引用块（&gt; ）
              </button>
            )}
          </div>
        );
      })()}
    </div>
  );
}
