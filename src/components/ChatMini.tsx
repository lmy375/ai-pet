import { Fragment, useEffect, useMemo, useRef, useState } from "react";
import { bubbleStyle } from "./panel/panelChatBits";
import { EmptyState } from "./panel/EmptyState";
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
  /// 当前 session 累积 LLM context token 量（system-excluded；后端
  /// `get_active_session_context_stats` 同源）。> MINI_TOKEN_WARN_THRESHOLD
  /// 时顶部浮 "上下文" chip + 一键 reset 入口。undefined / 0 = 不显。
  sessionTokens?: number;
  /// 一键 reset 当前 session 的 LLM context + 可见 items。armed-confirm 二次
  /// 确认（与桌面 ChatPanel 顶部「清空」按钮同模式），单次误点不会丢历史。
  /// 不传则 chip 仅显信息无 reset 按钮（兜底降级）。
  onResetContext?: () => void;
  /// 双击气泡内的 `「title」` ref token → 跳到 PanelTasks 该任务行。
  /// selection-based 检测：双击命中点向左找「，向右找」，提取 title 传出。
  /// 与 PanelChat 内同名 onRefDoubleClick 同 pipeline；ChatMini 通过 deeplink
  /// + openPanel 跨窗口送过去，PanelApp 端接受 taskFocusTitle 字段消费。
  /// 不传 → 双击气泡走 onOpenPanel fallback（既有行为）。
  onRefDoubleClick?: (title: string) => void;
  /// hover bubble 时浮的 "💾 转 task" 按钮触发：把本条消息文本作为 task
  /// body 传出，由 App.tsx 写跨窗口 deeplink + 开 panel + 弹 quickAdd modal
  /// 预填。让 owner 觉得"宠物说了好东西，存为 task" 一键搞定。
  onSaveAsTask?: (text: string) => void;
}

/// 最近 N 条的硬上限。窗口很小，DOM 太长既不好读也耗渲染。
const MINI_CHAT_MAX_ITEMS = 20;

/// 上下文 token 提示阈值。与 PanelDebugStats 的 `SESSION_TOKEN_WARN_THRESHOLD`
/// 同值 —— 让"DebugApp 显警告" 和 "桌面 chip 显警告" 触发条件一致。
/// 4000 是经验值：8k-128k context 都有，留 50%+ 给后续对话不至于撞墙。
const MINI_TOKEN_WARN_THRESHOLD = 4000;

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
/* 底相对时间小角标：与 .pet-mini-row-time 顶时钟同 hover-reveal 模式。
   信号优先级比顶时钟还低（相对时间是 ambient），默认透明 + hover 升 0.45。 */
.pet-mini-row .pet-mini-row-rel {
  opacity: 0;
  transition: opacity 120ms ease-out;
}
.pet-mini-row:hover .pet-mini-row-rel {
  opacity: 0.5;
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

/// 完整时间戳：`YYYY-MM-DD HH:MM:SS 周X (今天/昨天/N 天前)`。给 ts label
/// hover tooltip 用 —— 折叠后的 `[HH:MM]` 没日期 / 没秒级精度，owner 想精确
/// 看时间需要这个 fuller variant。`now` 注入便于测试（缺省取系统时钟）。
/// 无 ts / 解析失败返空串 —— caller 应 fallback 不挂 title。
function formatFullTimestamp(ts: string | undefined, now: Date = new Date()): string {
  if (!ts) return "";
  const d = new Date(ts);
  if (Number.isNaN(d.getTime())) return "";
  const y = d.getFullYear();
  const mo = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  const hh = String(d.getHours()).padStart(2, "0");
  const mm = String(d.getMinutes()).padStart(2, "0");
  const ss = String(d.getSeconds()).padStart(2, "0");
  const weekdays = ["周日", "周一", "周二", "周三", "周四", "周五", "周六"];
  const wd = weekdays[d.getDay()];
  // 相对天数：与 now 比较 0=今天 / 1=昨天 / 2+=N 天前 / -1=明天（罕见，
  // 由 ts 早于现在不超过 1 天但跨午夜出现）。
  const startOfDay = (dt: Date) =>
    new Date(dt.getFullYear(), dt.getMonth(), dt.getDate()).getTime();
  const diffDays = Math.round(
    (startOfDay(now) - startOfDay(d)) / (24 * 60 * 60 * 1000),
  );
  let rel: string;
  if (diffDays === 0) rel = "今天";
  else if (diffDays === 1) rel = "昨天";
  else if (diffDays === -1) rel = "明天";
  else if (diffDays > 1 && diffDays < 30) rel = `${diffDays} 天前`;
  else if (diffDays < -1 && diffDays > -30) rel = `${-diffDays} 天后`;
  else rel = "";
  return `${y}-${mo}-${day} ${hh}:${mm}:${ss} ${wd}${rel ? ` · ${rel}` : ""}`;
}

/// Bubble 底部 hover chip 用的相对时间格式。短串：刚刚 / N 分前 / N 时前 /
/// 昨天 / N 天前。与顶部 [HH:MM] 时钟 chip 对偶 —— 顶给绝对时刻 / 底给"距
/// 现在多久"。`now` 注入便于测试。无 ts / 解析失败返空串。
///
/// 与 PanelMemory `formatRelativeAgeBuckets` 思路一致但单位更短（适合
/// chip 紧凑显示）：去掉 "分钟前" 改 "分前"，"小时前" 改 "时前"。
function formatBubbleRelative(ts: string | undefined, now: Date = new Date()): string {
  if (!ts) return "";
  const d = new Date(ts);
  if (Number.isNaN(d.getTime())) return "";
  const ageMs = now.getTime() - d.getTime();
  if (ageMs < 60_000) return "刚刚";
  if (ageMs < 3_600_000) return `${Math.floor(ageMs / 60_000)} 分前`;
  if (ageMs < 86_400_000) return `${Math.floor(ageMs / 3_600_000)} 时前`;
  // 跨日判定：用 startOfDay 比对，相邻日历日就是"昨天"而非"24+ 时前"
  const startOfDay = (dt: Date) =>
    new Date(dt.getFullYear(), dt.getMonth(), dt.getDate()).getTime();
  const diffDays = Math.round((startOfDay(now) - startOfDay(d)) / 86_400_000);
  if (diffDays === 1) return "昨天";
  if (diffDays >= 2) return `${diffDays} 天前`;
  return ""; // 未来时刻：罕见 / 系统时钟回拨，不显
}

/// 把 ts 转成"YYYY-MM-DD" 日期键，给"跨日分隔条" 分组用。无效 / 缺失返
/// null —— caller 不该插分隔（与上一条同处理）。
function dateKeyFromTs(ts: string | undefined): string | null {
  if (!ts) return null;
  const d = new Date(ts);
  if (Number.isNaN(d.getTime())) return null;
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${y}-${m}-${day}`;
}

/// 把"YYYY-MM-DD" 日期键转成给用户看的相对文案：今天 / 昨天 / 本年内 MM-DD
/// / 跨年 YYYY-MM-DD。`now` 注入便于将来单测；不传则取系统时钟。
function formatDateDividerLabel(
  dateKey: string,
  now: Date = new Date(),
): string {
  const ny = now.getFullYear();
  const nm = String(now.getMonth() + 1).padStart(2, "0");
  const nd = String(now.getDate()).padStart(2, "0");
  if (dateKey === `${ny}-${nm}-${nd}`) return "今天";
  // yesterday
  const y = new Date(now);
  y.setDate(y.getDate() - 1);
  const yy = y.getFullYear();
  const ym = String(y.getMonth() + 1).padStart(2, "0");
  const yd = String(y.getDate()).padStart(2, "0");
  if (dateKey === `${yy}-${ym}-${yd}`) return "昨天";
  // 同年走 MM-DD；跨年走完整 YYYY-MM-DD（防"01-15" 在 2 年后看不出年代）
  if (dateKey.startsWith(`${ny}-`)) return dateKey.slice(5);
  return dateKey;
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
  sessionTokens,
  onResetContext,
  onRefDoubleClick,
  onSaveAsTask,
}: Props) {
  // armed-confirm: 第一次点击进 "再点确认" 态 + 3s 内不点就回 idle，防误触。
  // 与桌面 ChatPanel 顶部「清空」按钮 / 任务面板「清结束」按钮同模式。
  const [resetArmed, setResetArmed] = useState(false);
  useEffect(() => {
    if (!resetArmed) return;
    const id = window.setTimeout(() => setResetArmed(false), 3000);
    return () => window.clearTimeout(id);
  }, [resetArmed]);
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
  // 🌐 时区 chip click 复制 IANA 名后的 ✓ 1.5s 反馈态。
  const [tzCopyOk, setTzCopyOk] = useState(false);
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
  /// 顶 ts chip click 复制 ISO timestamp 后的"✓ 已复制"短暂 visual feedback
  /// 状态。同 bubbleCopyIdx 模式：1.5s 自清。null = 无；非空 = 显 ✓ 的 idx。
  const [tsCopyIdx, setTsCopyIdx] = useState<number | null>(null);
  /// 底 ⏱ 相对时间 chip click 复制 "N 分前 / 昨天 / N 天前" 串后的 ✓ 反馈
  /// 状态。与 tsCopyIdx 同模板。
  const [relCopyIdx, setRelCopyIdx] = useState<number | null>(null);
  /// 右键菜单状态：聚合"复制 / 带时间戳复制 / 针对这条再问 / 在 Panel 打开"
  /// 几个原本散在双击 / 小按钮里的动作到一个发现入口。idx 是 visibleItems
  /// 下标；x/y 是 viewport 坐标（fixed 定位）。null = 关闭。
  const [ctxMenu, setCtxMenu] = useState<{
    idx: number;
    x: number;
    y: number;
  } | null>(null);
  /// 点击 菜单外 / 按 Esc 关菜单。与 PanelTasks taskCtxMenu 同模式。
  useEffect(() => {
    if (!ctxMenu) return;
    const onDocClick = () => setCtxMenu(null);
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setCtxMenu(null);
    };
    // mousedown 而非 click 让 user 鼠标按下那一刻就关；click 还得等 mouseup
    // 周期才关，菜单跟手感差。菜单内的 onMouseDown 自身 stopPropagation 防
    // 自关。
    window.addEventListener("mousedown", onDocClick);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", onDocClick);
      window.removeEventListener("keydown", onKey);
    };
  }, [ctxMenu]);
  /// 选中文字浮 mini toolbar 状态：scrollRef 内有非空 selection 时浮起，含
  /// text + viewport 坐标（x = 选区中心 / y = 选区上沿）。点 toolbar 内
  /// 按钮触发动作（💾 转 task / 📋 复制 / 🔄 让 AI 改写）。选区清空 / 滚动
  /// / Esc 关。
  const [selectionToolbar, setSelectionToolbar] = useState<{
    text: string;
    x: number;
    y: number;
  } | null>(null);
  /// selection toolbar 内 📋 复制后的 ✓ 反馈，1.5s 自清。
  const [selectionCopyOk, setSelectionCopyOk] = useState(false);
  /// 选区监听：mouseup（鼠标松开时一次性 settle）+ selectionchange（清空 /
  /// 滚动 / 点空白时同步关）。仅在 visible 时挂；scrollRef 限定到 chat 列表
  /// 区，避免捕获其它窗口区域（如输入框）的选区。
  useEffect(() => {
    if (!visible) return;
    const computeToolbar = () => {
      const sel = window.getSelection?.();
      if (!sel || sel.rangeCount === 0) {
        setSelectionToolbar(null);
        return;
      }
      const text = sel.toString().trim();
      if (text.length === 0) {
        setSelectionToolbar(null);
        return;
      }
      const range = sel.getRangeAt(0);
      const container = scrollRef.current;
      if (!container) {
        setSelectionToolbar(null);
        return;
      }
      // commonAncestorContainer 在 chat list 区域内才显 toolbar；input /
      // 顶部 chip 区域的选区不弹（与 ⌘C 同模式 — 仅 chat 内容相关）。
      if (!container.contains(range.commonAncestorContainer)) {
        setSelectionToolbar(null);
        return;
      }
      const rect = range.getBoundingClientRect();
      if (rect.width === 0 && rect.height === 0) {
        setSelectionToolbar(null);
        return;
      }
      setSelectionToolbar({
        text,
        x: rect.left + rect.width / 2,
        y: rect.top,
      });
    };
    const onMouseUp = () => {
      // mouseup 时 selection 已 settle；用 setTimeout 0 等 onSelectionChange
      // 先跑完（顺序无强保证，简单稳妥）
      window.setTimeout(computeToolbar, 0);
    };
    const onSelChange = () => {
      // selection 被清掉时同步关，避免 toolbar 卡在屏幕上
      const sel = window.getSelection?.();
      if (!sel || sel.toString().trim().length === 0) {
        setSelectionToolbar(null);
      }
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && selectionToolbar) {
        setSelectionToolbar(null);
      }
    };
    const onScroll = () => setSelectionToolbar(null);
    document.addEventListener("mouseup", onMouseUp);
    document.addEventListener("selectionchange", onSelChange);
    window.addEventListener("keydown", onKey);
    scrollRef.current?.addEventListener("scroll", onScroll);
    return () => {
      document.removeEventListener("mouseup", onMouseUp);
      document.removeEventListener("selectionchange", onSelChange);
      window.removeEventListener("keydown", onKey);
      scrollRef.current?.removeEventListener("scroll", onScroll);
    };
  }, [visible, selectionToolbar]);
  // 静默淡出：N 秒无新消息 & 无 hover 时，整段聊天列表淡到半透明，让
  // Live2D 宠物在桌面成为视觉焦点；hover / 新消息 / streaming 立即回满。
  // 60s 是经验值：长到不会在用户看消息时偷偷淡掉，短到"放置一会儿"就生效。
  // localStorage 旁路：用户嫌烦可写 "pet-chatmini-idle-fade" = "off"。
  const [idleFaded, setIdleFaded] = useState(false);
  const idleFadeTimerRef = useRef<number | null>(null);
  const idleFadeEnabled = useMemo(() => {
    try {
      return window.localStorage.getItem("pet-chatmini-idle-fade") !== "off";
    } catch {
      return true;
    }
  }, []);
  const scheduleIdleFade = () => {
    if (!idleFadeEnabled) return;
    if (idleFadeTimerRef.current !== null) {
      window.clearTimeout(idleFadeTimerRef.current);
    }
    idleFadeTimerRef.current = window.setTimeout(() => {
      setIdleFaded(true);
      idleFadeTimerRef.current = null;
    }, 60_000);
  };
  const wakeIdleFade = () => {
    setIdleFaded(false);
    scheduleIdleFade();
  };
  // 任何"活动信号"重置倒计时：新消息追加、streaming chunk 来、tool 状态变化。
  // 三个 dep 一起监听让 effect 一处兜底，避免漏。
  useEffect(() => {
    wakeIdleFade();
    return () => {
      if (idleFadeTimerRef.current !== null) {
        window.clearTimeout(idleFadeTimerRef.current);
        idleFadeTimerRef.current = null;
      }
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [messages.length, currentResponse, toolStatus, isLoading]);
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

  /// 时间戳自适应折叠：在"密集同方对话"中只保留首末 ts，省视觉切碎。
  /// 规则：某条消息的 prev AND next 都满足"同 role + ts 差距 < 60s" 时，
  /// 视为"burst 中间"隐藏 ts；burst 首尾（一端不同 role / 一端超出 60s
  /// / 一端无）保留 ts。单条消息 / 两端不连续 → 永远显。
  /// hover tooltip 仍把完整时间写在 title attr 里，用户想看精确时间总能拿到。
  const TIMESTAMP_BURST_GAP_MS = 60_000;
  const hiddenTimestampIdx = useMemo(() => {
    const out = new Set<number>();
    const ts = (i: number): number | null => {
      const raw = visibleItems[i]?.ts;
      if (!raw) return null;
      const t = Date.parse(raw);
      return Number.isNaN(t) ? null : t;
    };
    for (let i = 0; i < visibleItems.length; i++) {
      const cur = visibleItems[i];
      if (!cur) continue;
      const curTs = ts(i);
      if (curTs === null) continue; // 时间无效自然不显，不用进 hide 集
      const prev = i > 0 ? visibleItems[i - 1] : null;
      const next = i < visibleItems.length - 1 ? visibleItems[i + 1] : null;
      const prevTs = i > 0 ? ts(i - 1) : null;
      const nextTs = i < visibleItems.length - 1 ? ts(i + 1) : null;
      const tightPrev =
        prev !== null &&
        prev.role === cur.role &&
        prevTs !== null &&
        curTs - prevTs < TIMESTAMP_BURST_GAP_MS;
      const tightNext =
        next !== null &&
        next.role === cur.role &&
        nextTs !== null &&
        nextTs - curTs < TIMESTAMP_BURST_GAP_MS;
      if (tightPrev && tightNext) out.add(i);
    }
    return out;
  }, [visibleItems]);

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

  /// ⌘C / Ctrl+C 快捷复制最近一条消息（user 或 assistant）。让位条件：
  /// - 选区非空：用户在拷选区内文本 → 走 browser native copy，不抢键
  /// - 输入控件聚焦：textarea / input / contentEditable 内 ⌘C 是 native copy
  ///   textarea 选区文本，不该被覆盖
  /// - ChatMini 不 visible（panel 模式 / pet hidden）：disable 监听
  /// copyRecentN 每 render 重建但读 messages 等 props，用 ref 避免空 deps 闭包
  /// 拿到旧 messages。
  const copyRecentNRef = useRef(copyRecentN);
  useEffect(() => {
    copyRecentNRef.current = copyRecentN;
  }, [copyRecentN]);
  useEffect(() => {
    if (!visible) return;
    const handler = (e: KeyboardEvent) => {
      if (!(e.metaKey || e.ctrlKey)) return;
      if (e.shiftKey || e.altKey) return;
      if (e.key.toLowerCase() !== "c") return;
      const sel = window.getSelection();
      if (sel && sel.toString().length > 0) return;
      const ae = document.activeElement;
      if (
        ae instanceof HTMLInputElement ||
        ae instanceof HTMLTextAreaElement ||
        (ae instanceof HTMLElement && ae.isContentEditable)
      ) {
        return;
      }
      e.preventDefault();
      copyRecentNRef.current(1);
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
          剩余空间，与三段堆叠布局对齐。
          idleFaded 时整段半透明 — 透出后面的 Live2D，让放置态桌面更干净；
          移到列表上立刻回满（onMouseEnter wakeIdleFade）。 */}
      <div
        onMouseEnter={wakeIdleFade}
        onMouseMove={idleFaded ? wakeIdleFade : undefined}
        style={{
          flex: 1,
          position: "relative",
          padding: "8px 12px 0",
          minHeight: 0,
          opacity: idleFaded ? 0.45 : 1,
          transition: "opacity 600ms ease-out",
        }}
      >
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
            title="在面板中打开聊天（看完整历史 / 多会话切换）— 也可按 ⌘O / Ctrl+O"
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
        {/* 🌐 当前时区 mini chip：跨时区出差 / 远程 owner 想知道"宠物记下的
            时间是哪个 tz" 时一眼可见。click 把 IANA 名（"Asia/Shanghai"）
            复制到剪贴板，方便在 task description / chat 里写绝对时区
            （"明天 14:00（Asia/Shanghai）"）。位置在 ⛶ / 📋 之左。 */}
        {(() => {
          const tzName =
            Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC";
          const offsetMin = -new Date().getTimezoneOffset();
          const sign = offsetMin >= 0 ? "+" : "-";
          const absMin = Math.abs(offsetMin);
          const hr = Math.floor(absMin / 60);
          const min = absMin % 60;
          const offsetShort =
            min === 0 ? `${sign}${hr}` : `${sign}${hr}:${String(min).padStart(2, "0")}`;
          const offsetFull = `UTC${sign}${String(hr).padStart(2, "0")}:${String(min).padStart(2, "0")}`;
          return (
            <button
              type="button"
              className="pet-mini-maxbtn"
              onMouseDown={(e) => e.stopPropagation()}
              onClick={async (e) => {
                e.stopPropagation();
                try {
                  await navigator.clipboard.writeText(tzName);
                  setTzCopyOk(true);
                  window.setTimeout(() => setTzCopyOk(false), 1500);
                } catch (err) {
                  console.error("tz chip copy failed:", err);
                }
              }}
              title={`本机当前时区：${tzName}（${offsetFull}）· 点击复制 IANA 名到剪贴板`}
              aria-label="copy current timezone IANA name"
              style={{
                position: "absolute",
                top: "14px",
                right: onOpenPanel ? "76px" : "48px",
                height: "20px",
                padding: "0 6px",
                borderRadius: 10,
                border: "1px solid var(--pet-color-border)",
                background: "var(--pet-color-card)",
                color: tzCopyOk
                  ? "var(--pet-tint-green-fg)"
                  : "var(--pet-color-muted)",
                fontSize: "10px",
                lineHeight: 1,
                cursor: "pointer",
                zIndex: 12,
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                gap: 2,
                boxShadow: "var(--pet-shadow-sm)",
                fontFamily: "inherit",
              }}
            >
              {tzCopyOk ? "✓" : `🌐${offsetShort}`}
            </button>
          );
        })()}
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
        {/* 上下文 token 警示 chip：与 DebugApp 统计 tab 「当前会话 LLM 上下
            文」卡片同源信号（4000 阈值）。一键 reset 走 armed-confirm 二次
            确认（首点变"再点确认"+3s 自清）防止误触丢历史。 */}
        {sessionTokens !== undefined &&
          sessionTokens > MINI_TOKEN_WARN_THRESHOLD && (
            <div
              style={{
                marginBottom: 6,
                padding: "5px 9px",
                borderRadius: 8,
                background: "var(--pet-tint-yellow-bg)",
                color: "var(--pet-tint-yellow-fg)",
                border:
                  "1px solid color-mix(in srgb, var(--pet-tint-yellow-fg) 30%, transparent)",
                fontSize: 11,
                lineHeight: 1.4,
                display: "flex",
                alignItems: "center",
                gap: 6,
              }}
              title={`当前 session LLM 上下文累计 ~${sessionTokens} tokens；超过 ${MINI_TOKEN_WARN_THRESHOLD} 通常意味着 prompt 在膨胀。建议 /reset 清掉以省 token + 让宠物注意力回到当前话题。`}
            >
              <span style={{ flex: 1 }}>
                💭 上下文 ~{sessionTokens} tok（已超 {MINI_TOKEN_WARN_THRESHOLD}，建议
                <strong> /reset</strong>）
              </span>
              {onResetContext && (
                <button
                  type="button"
                  onClick={() => {
                    if (resetArmed) {
                      onResetContext();
                      setResetArmed(false);
                    } else {
                      setResetArmed(true);
                    }
                  }}
                  style={{
                    fontSize: 10,
                    fontWeight: 600,
                    padding: "2px 8px",
                    borderRadius: 6,
                    border: resetArmed
                      ? "1px solid var(--pet-tint-red-fg)"
                      : "1px solid color-mix(in srgb, var(--pet-tint-yellow-fg) 50%, transparent)",
                    background: resetArmed
                      ? "var(--pet-tint-red-fg)"
                      : "var(--pet-color-card)",
                    color: resetArmed ? "#fff" : "var(--pet-tint-yellow-fg)",
                    cursor: "pointer",
                    whiteSpace: "nowrap",
                  }}
                  title={
                    resetArmed
                      ? "再点确认：清掉本 session 的 LLM context + 可见 mini chat 历史（系统提示词保留）。3s 内不点自动收起。"
                      : "清掉本 session 的 LLM context + 可见 mini chat 历史。点击进入二次确认。"
                  }
                >
                  {resetArmed ? "再点确认 (3s)" : "/reset"}
                </button>
              )}
            </div>
          )}
        {visibleItems.length === 0 && !currentResponse && (
          // 首次启动 / 全部 dismissed 后空态：给一行轻量 hint 让用户知道这里
          // 会显示什么、可以做什么。compact 模式 padding 较小，与 pet 窗 300px
          // 窄宽度匹配。
          <EmptyState
            icon="🐾"
            title="等宠物开口"
            hint="底部输入框敲字开始聊天；宠物也会在 proactive 时主动找你。"
            compact
          />
        )}
        {visibleItems.map((m, idx) => {
          // 跨日分隔：当前 ts 有效 + 与前一条 ts 的日期键不同时插一条分隔条。
          // 第一条（idx === 0）有有效 ts 时也显（让"对话起点是哪天"清楚）；
          // ts 缺失静默跳（与 ts 标签"无效 → 不显" 同语义边界）。
          const curDateKey = dateKeyFromTs(m.ts);
          const prevDateKey =
            idx > 0 ? dateKeyFromTs(visibleItems[idx - 1].ts) : null;
          const showDateDivider =
            curDateKey !== null && curDateKey !== prevDateKey;
          const dateLabel = showDateDivider
            ? formatDateDividerLabel(curDateKey!)
            : "";
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
          /// 💾 转 task：bubble 内 text 非空时显，hover row 才浮起（与 copyBtn
          /// 同 pet-mini-row-copy 透明度类）。点击调 onSaveAsTask 让 App.tsx
          /// 写跨窗口 deeplink + 开 panel + 弹 quickAdd modal 预填 body。owner
          /// 觉得"宠物说了好东西，存为 task 防忘" 一键搞定。
          const saveAsTaskBtn =
            text && onSaveAsTask ? (
              <button
                type="button"
                className="pet-mini-row-copy"
                onClick={(e) => {
                  e.stopPropagation();
                  onSaveAsTask(text);
                }}
                title="把本条消息转为新 task（跨窗口开 Panel + quickAdd modal 预填 body）"
                aria-label="save this message as task"
                style={{
                  alignSelf: "flex-end",
                  whiteSpace: "nowrap",
                  flexShrink: 0,
                }}
              >
                💾
              </button>
            ) : null;
          return (
            <Fragment key={`${m.role}-${idx}-${text.length}-${imgs.length}`}>
              {showDateDivider && (
                <div
                  aria-hidden
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: 8,
                    fontSize: 9,
                    color: "var(--pet-color-muted)",
                    letterSpacing: 0.5,
                    margin: "8px 4px 4px",
                    userSelect: "none",
                  }}
                  title={`本组消息从 ${curDateKey} 开始`}
                >
                  <span
                    style={{
                      flex: 1,
                      height: 1,
                      background:
                        "color-mix(in srgb, var(--pet-color-border) 70%, transparent)",
                    }}
                  />
                  <span style={{ flexShrink: 0 }}>{dateLabel}</span>
                  <span
                    style={{
                      flex: 1,
                      height: 1,
                      background:
                        "color-mix(in srgb, var(--pet-color-border) 70%, transparent)",
                    }}
                  />
                </div>
              )}
            <div
              className="pet-mini-row"
              data-mini-idx={idx}
              onContextMenu={(e) => {
                // 右键菜单：聚合发现入口。preventDefault 吃掉 webview 默认
                // 右键（Tauri 已禁默认 context-menu，但保险一道）；
                // stopPropagation 防被 wake-up / drag handlers 抢走。
                e.preventDefault();
                e.stopPropagation();
                setCtxMenu({ idx, x: e.clientX, y: e.clientY });
              }}
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
                  的"附加信息"，靠 bubble 自身一边更直观）。
                  burst 折叠：hiddenTimestampIdx 含此 idx 时跳过渲染 ——
                  连续 < 60s 同 role 消息中间的 ts 标签合并为"仅首末显"，
                  让密集对话不被时间戳切碎；hover bubble 自身的 title attr 仍
                  能拿到完整时间。 */}
              {hasValidTime && !hiddenTimestampIdx.has(idx) && (() => {
                const isTsCopied = tsCopyIdx === idx;
                return (
                  <span
                    className="pet-mini-row-time"
                    title={`${formatFullTimestamp(m.ts)} · 单击复制完整 ISO timestamp · 双击复制 "MM-DD HH:MM" 友好短格式`}
                    onClick={(e) => {
                      // 点击复制 raw ISO timestamp（m.ts）—— debug / 报错 /
                      // 跨工具时常用精确时间。stopPropagation 防 bubble 内
                      // ⌘+click 复制 / dblclick ref 跳等其它 click 路径误
                      // 触发。1.5s ✓ 视觉反馈与 bubbleCopyIdx 同模板。
                      e.stopPropagation();
                      if (!m.ts) return;
                      navigator.clipboard
                        .writeText(m.ts)
                        .then(() => {
                          setTsCopyIdx(idx);
                          window.setTimeout(
                            () =>
                              setTsCopyIdx((cur) =>
                                cur === idx ? null : cur,
                              ),
                            1500,
                          );
                        })
                        .catch((err) =>
                          console.error("ts chip copy failed:", err),
                        );
                    }}
                    onDoubleClick={(e) => {
                      // 双击复制 "MM-DD HH:MM" 友好短格式（贴日程 / 发同事
                      // / 写笔记时不要 ISO 那么长）。解析 m.ts → Date 再切
                      // 切；失败兜底 formatBubbleTimestamp（"[HH:MM]"）让
                      // owner 至少拿到点东西。stopPropagation 防触发 onClick
                      // 二次（dblclick 不会自动取消 click — 我们手动拦）。
                      e.stopPropagation();
                      e.preventDefault();
                      if (!m.ts) return;
                      const d = new Date(m.ts);
                      const short = isNaN(d.getTime())
                        ? formatBubbleTimestamp(m.ts).replace(/[\[\]]/g, "")
                        : `${String(d.getMonth() + 1).padStart(2, "0")}-${String(
                            d.getDate(),
                          ).padStart(2, "0")} ${String(d.getHours()).padStart(
                            2,
                            "0",
                          )}:${String(d.getMinutes()).padStart(2, "0")}`;
                      navigator.clipboard
                        .writeText(short)
                        .then(() => {
                          setTsCopyIdx(idx);
                          window.setTimeout(
                            () =>
                              setTsCopyIdx((cur) =>
                                cur === idx ? null : cur,
                              ),
                            1500,
                          );
                        })
                        .catch((err) =>
                          console.error("ts chip dblclick copy failed:", err),
                        );
                    }}
                    style={{
                      position: "absolute",
                      top: -12,
                      [m.role === "user" ? "right" : "left"]: 8,
                      fontSize: 9,
                      color: isTsCopied
                        ? "var(--pet-tint-green-fg)"
                        : "var(--pet-color-muted)",
                      fontWeight: isTsCopied ? 600 : undefined,
                      fontFamily: "'SF Mono', 'Menlo', monospace",
                      whiteSpace: "nowrap",
                      // pointerEvents 默认 auto —— 让 hover 触发 title 的 native
                      // tooltip 显完整时间戳（YYYY-MM-DD HH:MM:SS 周X · 相对天）。
                      // 折叠后 [HH:MM] 没日期 / 秒级精度，hover full ts 补全。
                      background: "var(--pet-color-card)",
                      padding: "0 4px",
                      borderRadius: 3,
                      lineHeight: "12px",
                      cursor: "pointer",
                    }}
                  >
                    {isTsCopied ? "✓ " : ""}{timeLabel}
                  </span>
                );
              })()}
              {/* bubble 底相对时间 chip：与顶 [HH:MM] 时钟 chip 对偶 ——
                  顶给绝对时刻、底给"距现在多久"。row hover 才显（pet-mini-
                  row-rel CSS 类透明度 0 → 0.5），存在感比顶 chip 还低
                  （ambient 信号）。user 行靠右底 / assistant 行靠左底，与 bubble
                  对齐方向同侧。relText 解析失败时不渲染（无 ts / 未来时刻）。
                  hiddenTimestampIdx 折叠时跳过（与顶 chip 同 gate）—— 密集
                  burst 中间也合并。 */}
              {hasValidTime &&
                !hiddenTimestampIdx.has(idx) &&
                (() => {
                  const rel = formatBubbleRelative(m.ts);
                  if (!rel) return null;
                  const isRelCopied = relCopyIdx === idx;
                  return (
                    <span
                      className="pet-mini-row-rel"
                      title={`相对时间 · ${formatFullTimestamp(m.ts)} · 点击复制 "${rel}"`}
                      onClick={(e) => {
                        // 点击复制相对时间字符串（"5 分前" / "昨天" / "3 天前"
                        // 等），与顶 ts chip 复制 ISO 对偶。stopPropagation 防
                        // bubble ⌘+click 复制全文 / dblclick ref 跳等其它路径
                        // 误触发。1.5s ✓ 视觉反馈与 tsCopyIdx 同模板。
                        e.stopPropagation();
                        navigator.clipboard
                          .writeText(rel)
                          .then(() => {
                            setRelCopyIdx(idx);
                            window.setTimeout(
                              () =>
                                setRelCopyIdx((cur) =>
                                  cur === idx ? null : cur,
                                ),
                              1500,
                            );
                          })
                          .catch((err) =>
                            console.error("rel chip copy failed:", err),
                          );
                      }}
                      style={{
                        position: "absolute",
                        bottom: -10,
                        [m.role === "user" ? "right" : "left"]: 8,
                        fontSize: 9,
                        color: isRelCopied
                          ? "var(--pet-tint-green-fg)"
                          : "var(--pet-color-muted)",
                        fontWeight: isRelCopied ? 600 : undefined,
                        fontFamily: "'SF Mono', 'Menlo', monospace",
                        whiteSpace: "nowrap",
                        background: "var(--pet-color-card)",
                        padding: "0 4px",
                        borderRadius: 3,
                        lineHeight: "12px",
                        pointerEvents: "auto",
                        cursor: "pointer",
                      }}
                    >
                      {isRelCopied ? "✓ " : ""}⏱ {rel}
                    </span>
                  );
                })()}
              {/* user 右对齐 → 复制按钮在 bubble 左侧 */}
              {m.role === "user" && saveAsTaskBtn}
              {m.role === "user" && copyBtn}
              <div
                onClick={(e) => {
                  // ⌘/Ctrl + click 复制本条 bubble 文本：与既有 ⌘C copy
                  // last + 角标 copy 按钮 + ctx menu 复制项 形成"键盘党
                  // 精准复制中段消息"路径。无 modifier 时 onClick 不抢用
                  // （让选区 / drag / 普通单击行为照旧）。⌥ / ⇧ 同按时不
                  // 触发避免与系统级 modifier 冲突。
                  if (!(e.metaKey || e.ctrlKey)) return;
                  if (e.altKey || e.shiftKey) return;
                  if (!text) return;
                  e.preventDefault();
                  e.stopPropagation();
                  handleBubbleCopy(idx, text);
                }}
                onDoubleClick={(e) => {
                  // 先尝试 ref-token 跳转：dblclick 命中点 selection 起点向左
                  // 找「，向右找」，命中 → onRefDoubleClick；否则 fallback 走
                  // onOpenPanel（双击 bubble 既有行为）。
                  if (onRefDoubleClick) {
                    const sel = window.getSelection();
                    if (sel && sel.rangeCount > 0) {
                      const range = sel.getRangeAt(0);
                      const node = range.startContainer;
                      // text node 才走 ref 探测；element 节点 dblclick（如点击 emoji
                      // span 边界）走 fallback
                      if (node.nodeType === Node.TEXT_NODE) {
                        const text = node.textContent ?? "";
                        const start = range.startOffset;
                        const end = range.endOffset;
                        // 向左 / 右扫，碰另一边引号 / 换行先停 = 不命中（双击点
                        // 已离开 ref token 区）
                        let lb = -1;
                        for (let i = start - 1; i >= 0; i--) {
                          const ch = text[i];
                          if (ch === "「") {
                            lb = i;
                            break;
                          }
                          if (ch === "」" || ch === "\n") break;
                        }
                        if (lb >= 0) {
                          let rb = -1;
                          for (let i = end; i < text.length; i++) {
                            const ch = text[i];
                            if (ch === "」") {
                              rb = i;
                              break;
                            }
                            if (ch === "「" || ch === "\n") break;
                          }
                          if (rb > lb) {
                            const title = text.slice(lb + 1, rb).trim();
                            if (title) {
                              e.preventDefault();
                              e.stopPropagation();
                              // 触发前播一声轻量 200ms beep（Web Audio API
                              // oscillator，无 audio asset 依赖）。让 owner
                              // 听觉确认"ref 跳转触发了" —— 跨窗口 deeplink
                              // 视觉切到 PanelTasks 有延迟时尤其有用。catch
                              // 容忍 AudioContext 不可用 / 无声效环境，silent
                              // 退化不破坏跳转主流程。
                              try {
                                const AC =
                                  window.AudioContext ||
                                  (window as unknown as {
                                    webkitAudioContext?: typeof AudioContext;
                                  }).webkitAudioContext;
                                if (AC) {
                                  const ac = new AC();
                                  const osc = ac.createOscillator();
                                  const gain = ac.createGain();
                                  osc.type = "sine";
                                  osc.frequency.value = 880; // A5
                                  gain.gain.setValueAtTime(
                                    0.06,
                                    ac.currentTime,
                                  );
                                  gain.gain.exponentialRampToValueAtTime(
                                    0.0001,
                                    ac.currentTime + 0.15,
                                  );
                                  osc.connect(gain).connect(ac.destination);
                                  osc.start();
                                  osc.stop(ac.currentTime + 0.16);
                                  // 关 AudioContext 释放 ：300ms 后（够 stop 完整）
                                  window.setTimeout(() => ac.close(), 300);
                                }
                              } catch {
                                // 静默退化 —— 跳转主流程仍走。
                              }
                              onRefDoubleClick(title);
                              return;
                            }
                          }
                        }
                      }
                    }
                  }
                  onOpenPanel?.();
                }}
                title={
                  // hover tooltip 把时间戳 + 多个交互 hint 拼一起。原生
                  // tooltip 只能挂一条 → 分多个不好。
                  `${formatBubbleTimestamp(m.ts)}${
                    text ? " · ⌘+点击 复制本条文本" : ""
                  }${
                    onRefDoubleClick
                      ? " · 双击「title」跳任务面板该卡片"
                      : ""
                  }${
                    onOpenPanel
                      ? " · 双击气泡空白处进入面板聊天（看完整历史 / 多会话切换）"
                      : ""
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
              {/* assistant 左对齐 → 复制 + 再回应 + 存 task 按钮在 bubble 右侧 */}
              {m.role === "assistant" && respondBtn}
              {m.role === "assistant" && saveAsTaskBtn}
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
            </Fragment>
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
      {/* 右键菜单：fixed 定位到 click 坐标；夹紧 viewport 右/下边界避免被
          切。子菜单按 role 条件化渲染（💭 仅 assistant）。所有 item 点击后
          自身关菜单；菜单外 mousedown / Esc 也关（effect 里挂全局监听）。 */}
      {ctxMenu && (() => {
        const m = visibleItems[ctxMenu.idx];
        if (!m) return null;
        const text = extractText(m.content);
        const hasText = text.length > 0;
        const isAssistant = m.role === "assistant";
        // 经验值夹紧：菜单大约 200×180px；超出 right/bottom 时向上 / 左挪。
        const MAX_W = 220;
        const MAX_H = 180;
        const vw = typeof window !== "undefined" ? window.innerWidth : 800;
        const vh = typeof window !== "undefined" ? window.innerHeight : 600;
        const x = Math.min(ctxMenu.x, vw - MAX_W - 4);
        const y = Math.min(ctxMenu.y, vh - MAX_H - 4);
        const item: React.CSSProperties = {
          padding: "6px 12px",
          fontSize: 12,
          textAlign: "left",
          background: "transparent",
          color: "var(--pet-color-fg)",
          border: "none",
          cursor: "pointer",
          fontFamily: "inherit",
          whiteSpace: "nowrap",
        };
        const itemHoverIn = (e: React.MouseEvent<HTMLButtonElement>) => {
          (e.currentTarget as HTMLButtonElement).style.background =
            "var(--pet-color-bg)";
        };
        const itemHoverOut = (e: React.MouseEvent<HTMLButtonElement>) => {
          (e.currentTarget as HTMLButtonElement).style.background = "transparent";
        };
        return (
          <div
            onMouseDown={(e) => e.stopPropagation()}
            onClick={(e) => e.stopPropagation()}
            style={{
              position: "fixed",
              left: x,
              top: y,
              minWidth: 180,
              maxWidth: MAX_W,
              background: "var(--pet-color-card)",
              border: "1px solid var(--pet-color-border)",
              borderRadius: 8,
              boxShadow: "var(--pet-shadow-md)",
              padding: "4px 0",
              display: "flex",
              flexDirection: "column",
              zIndex: 90,
              fontFamily: "inherit",
            }}
          >
            <button
              type="button"
              style={item}
              onMouseOver={itemHoverIn}
              onMouseOut={itemHoverOut}
              disabled={!hasText}
              onClick={() => {
                setCtxMenu(null);
                if (hasText) handleBubbleCopy(ctxMenu.idx, text);
              }}
            >
              📋 复制本条
            </button>
            <button
              type="button"
              style={item}
              onMouseOver={itemHoverIn}
              onMouseOut={itemHoverOut}
              disabled={!hasText}
              onClick={() => {
                setCtxMenu(null);
                if (!hasText) return;
                const ts = formatBubbleTimestamp(m.ts);
                const payload = ts === "[?]" ? text : `${ts} ${text}`;
                navigator.clipboard
                  .writeText(payload)
                  .then(() => {
                    setBubbleCopyIdx(ctxMenu.idx);
                    window.setTimeout(
                      () =>
                        setBubbleCopyIdx((cur) =>
                          cur === ctxMenu.idx ? null : cur,
                        ),
                      1500,
                    );
                  })
                  .catch((err) =>
                    console.error("bubble copy w/ timestamp failed:", err),
                  );
              }}
            >
              ⌚ 复制 · 含时间戳
            </button>
            {isAssistant && hasText && (
              <button
                type="button"
                style={item}
                onMouseOver={itemHoverIn}
                onMouseOut={itemHoverOut}
                onClick={() => {
                  setCtxMenu(null);
                  const excerpt =
                    text.length > 30 ? text.slice(0, 30) + "…" : text;
                  window.dispatchEvent(
                    new CustomEvent("pet-mini-respond-to", { detail: excerpt }),
                  );
                }}
              >
                💭 针对这条再问
              </button>
            )}
            {onOpenPanel && (
              <button
                type="button"
                style={item}
                onMouseOver={itemHoverIn}
                onMouseOut={itemHoverOut}
                onClick={() => {
                  setCtxMenu(null);
                  onOpenPanel();
                }}
              >
                ⛶ 在 Panel 中打开聊天
              </button>
            )}
            {/* "在 Panel 定位本条"：写 deeplink (chatMatch.excerpt) + 打开
                Panel。PanelChat 反向扫 items 找最近 substr 命中 → 滚到该
                bubble + 1.5s 高亮。仅 text 非空（hasText）才显 —— 纯图
                bubble 没文字给 PanelChat 匹配。 */}
            {onOpenPanel && hasText && (
              <button
                type="button"
                style={item}
                onMouseOver={itemHoverIn}
                onMouseOut={itemHoverOut}
                onClick={() => {
                  setCtxMenu(null);
                  // excerpt: 取前 80 字符（按 Unicode code point 算）作 substring
                  // 关键字。够独特命中、又够短不挤 localStorage。
                  const excerpt = Array.from(text).slice(0, 80).join("");
                  try {
                    window.localStorage.setItem(
                      "pet-panel-deeplink",
                      JSON.stringify({
                        chatMatch: { excerpt },
                        ts: Date.now(),
                      }),
                    );
                  } catch {
                    // localStorage 不可用：仍 onOpenPanel；用户至少进 Panel
                  }
                  onOpenPanel();
                }}
              >
                ⛶ 在 Panel 中定位本条
              </button>
            )}
          </div>
        );
      })()}
      {/* 选区浮 mini toolbar：text 非空 + selection 落在 chat 列表区时显，
          fixed 定位浮在 selection 上方。3 个按钮：💾 转 task（如有
          onSaveAsTask） / 📋 复制 / 🔄 让 AI 改写后重发（dispatch
          pet-mini-rewrite-selection 让 ChatPanel 在输入框预填）。viewport
          clamp 防超边缘。 */}
      {selectionToolbar && (() => {
        const TOOLBAR_W = 132;
        const TOOLBAR_H = 32;
        const vw = typeof window !== "undefined" ? window.innerWidth : 800;
        const vh = typeof window !== "undefined" ? window.innerHeight : 600;
        // 默认浮在 selection 上方；上方空间不够时翻到下方
        const desiredX = selectionToolbar.x - TOOLBAR_W / 2;
        const clampedX = Math.max(4, Math.min(desiredX, vw - TOOLBAR_W - 4));
        const aboveY = selectionToolbar.y - TOOLBAR_H - 6;
        const clampedY = aboveY < 4 ? selectionToolbar.y + 24 : aboveY;
        const finalY = Math.max(4, Math.min(clampedY, vh - TOOLBAR_H - 4));
        const btnStyle: React.CSSProperties = {
          fontSize: 12,
          padding: "3px 8px",
          background: "transparent",
          border: "none",
          color: "var(--pet-color-fg)",
          cursor: "pointer",
          fontFamily: "inherit",
          whiteSpace: "nowrap",
        };
        return (
          <div
            onMouseDown={(e) => e.stopPropagation()}
            onClick={(e) => e.stopPropagation()}
            style={{
              position: "fixed",
              left: clampedX,
              top: finalY,
              height: TOOLBAR_H,
              display: "flex",
              alignItems: "center",
              gap: 0,
              background: "var(--pet-color-card)",
              border: "1px solid var(--pet-color-border)",
              borderRadius: 8,
              boxShadow: "var(--pet-shadow-md)",
              padding: "0 2px",
              zIndex: 95,
              fontFamily: "inherit",
            }}
          >
            {onSaveAsTask && (
              <button
                type="button"
                style={btnStyle}
                title="把这段选中文字转为新 task（开 Panel + quickAdd 预填）"
                onClick={() => {
                  const text = selectionToolbar.text;
                  setSelectionToolbar(null);
                  onSaveAsTask(text);
                }}
              >
                💾
              </button>
            )}
            <button
              type="button"
              style={{
                ...btnStyle,
                color: selectionCopyOk
                  ? "var(--pet-tint-green-fg)"
                  : btnStyle.color,
              }}
              title="复制选中文字到剪贴板"
              onClick={async () => {
                const text = selectionToolbar.text;
                try {
                  await navigator.clipboard.writeText(text);
                  setSelectionCopyOk(true);
                  window.setTimeout(() => setSelectionCopyOk(false), 1500);
                } catch (e) {
                  console.error("clipboard write failed:", e);
                }
              }}
            >
              {selectionCopyOk ? "✓" : "📋"}
            </button>
            <button
              type="button"
              style={btnStyle}
              title="让 AI 改写这段（输入框预填「请改写：...」让你确认 / 微调后发送）"
              onClick={() => {
                const text = selectionToolbar.text;
                setSelectionToolbar(null);
                window.dispatchEvent(
                  new CustomEvent("pet-mini-rewrite-selection", {
                    detail: text,
                  }),
                );
              }}
            >
              🔄
            </button>
          </div>
        );
      })()}
    </>
  );
}
