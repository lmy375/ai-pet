import { Fragment, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
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
  /// 选区 toolbar "📝 记到 note" 按钮触发：把选中文字作 general memory
  /// item 存盘（与 💾 转 task 互补 — task 是要做的事，note 是想记的事）。
  /// 由 App.tsx 调 memory_edit("create", "general") 同 TG /note 后端。
  onSaveAsNote?: (text: string) => void;
  /// 选区 toolbar "📚 加到 ai_insights" 按钮触发：把选中文字作
  /// ai_insights memory item 存盘 — 与 /reflect / PanelMemory AI 洞察
  /// 段同后端。与 📝 note（general cat 杂项 brain-dump）分流 — 本入
  /// 口是「反思 / 自我洞察」按信号类型分类避免 ai_insights 段被日常
  /// 杂项稀释。由 App.tsx 调 memory_edit("create", "ai_insights")。
  onSaveAsAiInsight?: (text: string) => void;
  /// ChatBubble 右键菜单 "📝 设 transient_note" 触发：把 assistant
  /// 这条 reply 文本作 transient_note（in-memory N 分钟有效上下文）。
  /// 由 App.tsx 调 set_transient_note Tauri 命令。与 iter #364
  /// PanelToneStrip ✍️ 写入口 / iter #363 TG /transient 同后端，第三
  /// 个 surface 让 owner "选 pet 说的这句话直接用" 免再敲字。
  onSetTransientNote?: (text: string, minutes: number) => void;
}

/// 最近 N 条的硬上限。窗口很小，DOM 太长既不好读也耗渲染。
const MINI_CHAT_MAX_ITEMS = 20;

/// 上下文 token 提示阈值。与 PanelDebugStats 的 `SESSION_TOKEN_WARN_THRESHOLD`
/// 同值 —— 让"DebugApp 显警告" 和 "桌面 chip 显警告" 触发条件一致。
/// 4000 是经验值：8k-128k context 都有，留 50%+ 给后续对话不至于撞墙。
const MINI_TOKEN_WARN_THRESHOLD = 4000;

/// session token tally 估算 cost 用的 USD / 百万 token blended rate。
/// 3.0 是经验中点：Claude Sonnet $3 input / $15 output 之间偏 input
/// （chat 场景输入占多）；Opus 更贵 / Haiku 更便宜，3.0 算个 mid-band
/// 兜底估。owner 想精算改这个常量即可。本 tally 显示 cost 是 ambient
/// awareness 用，不当账单 — tooltip 内已注明仅供参考。
const MINI_TOKEN_COST_PER_MILLION = 3.0;

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
/* 顶字数 chip：与 .pet-mini-row-time 顶时钟同 hover-reveal 模式但位
   置在 bubble 对侧（user 左 / assistant 右）— 让两顶 chip 不挤一边。
   信号优先级 ambient 级（看长度 audit / 复制前预估），默认透明 +
   hover 升 0.5。 */
.pet-mini-row .pet-mini-row-chars {
  opacity: 0;
  transition: opacity 120ms ease-out;
}
.pet-mini-row:hover .pet-mini-row-chars {
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

/// 长 assistant reply 折叠阈值。> THRESHOLD 字才会折叠（短 reply 不
/// 屠龙）；折叠后显前 PREVIEW_CHARS 字 + "…" + 「展开」按钮。chars 用
/// Array.from 计数确保 CJK / emoji 等多字节字符按字形计 1。
const LONG_BUBBLE_THRESHOLD = 2000;
const LONG_BUBBLE_PREVIEW_CHARS = 400;

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
  onSaveAsNote,
  onSaveAsAiInsight,
  onSetTransientNote,
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

  /// 💡 ambient hint 行：顶部一行显当前 transient_note + active alarms
  /// 数 + mute 剩余。30s 轮询足够（这三个信号都是分钟级粒度变化）。
  /// 三段全空时整行不渲，避免占垂直空间。
  /// - tn: { text, mins } | null
  /// - alarms: count
  /// - muteMins: number | null（None = 未静音）
  const [ambientTransient, setAmbientTransient] = useState<{
    text: string;
    mins: number;
  } | null>(null);
  const [ambientAlarms, setAmbientAlarms] = useState<number>(0);
  const [ambientMuteMins, setAmbientMuteMins] = useState<number | null>(null);
  useEffect(() => {
    if (!visible) return;
    let cancelled = false;
    const tick = async () => {
      try {
        const [tnTuple, reminders, muteUntil] = await Promise.all([
          invoke<[string, string]>("get_transient_note").catch(() => [
            "",
            "",
          ] as [string, string]),
          invoke<{ time: string; topic: string; title: string; due_now: boolean }[]>(
            "get_pending_reminders",
          ).catch(() => []),
          invoke<string>("get_mute_until").catch(() => ""),
        ]);
        if (cancelled) return;
        const [tnText, tnUntilIso] = tnTuple;
        if (tnText.length === 0) {
          setAmbientTransient(null);
        } else {
          const untilMs = Date.parse(tnUntilIso);
          const mins = Number.isNaN(untilMs)
            ? 0
            : Math.max(1, Math.ceil((untilMs - Date.now()) / 60000));
          setAmbientTransient({ text: tnText, mins });
        }
        setAmbientAlarms(reminders.length);
        if (muteUntil.length === 0) {
          setAmbientMuteMins(null);
        } else {
          const untilMs = Date.parse(muteUntil);
          if (Number.isNaN(untilMs)) {
            setAmbientMuteMins(null);
          } else {
            const mins = Math.max(1, Math.ceil((untilMs - Date.now()) / 60000));
            setAmbientMuteMins(mins);
          }
        }
      } catch (e) {
        console.error("ambient poll failed:", e);
      }
    };
    void tick();
    const id = window.setInterval(tick, 30_000);
    return () => {
      cancelled = true;
      window.clearInterval(id);
    };
  }, [visible]);

  /// ⏱ pet 沉默 N 分 chip：自上次 pet 主动 / 回复（role=assistant 含
  /// valid ts）算起的分钟数。让 owner 觉察「pet 是不是又卡住了 / proactive
  /// pipeline 是不是有问题」。仅 ≥ 5 分钟显（避免 pet 刚说完就闪 chip
  /// 噪音）；severity 分三档（5..30 muted / 30..90 黄 / >90 红）。
  ///
  /// `silentTick` 每 30s bump 让本 useMemo 重算 — 分钟级 display 的足够
  /// 节奏；与既有 nowTick（1s, NOW marks 专用）分开避免相互 dependency 干扰。
  const [silentTick, setSilentTick] = useState(0);
  useEffect(() => {
    const id = window.setInterval(() => setSilentTick((t) => t + 1), 30_000);
    return () => window.clearInterval(id);
  }, []);
  const petSilentMins = useMemo<number | null>(() => {
    void silentTick;
    for (let i = messages.length - 1; i >= 0; i--) {
      const m = messages[i];
      if (m.role !== "assistant") continue;
      const raw = m.ts;
      if (!raw) continue;
      const t = Date.parse(raw);
      if (Number.isNaN(t)) continue;
      return Math.floor((Date.now() - t) / 60_000);
    }
    return null;
  }, [messages, silentTick]);
  const showPetSilentChip = petSilentMins !== null && petSilentMins >= 5;
  /// 今日消息计数：scan `messages` prop 中 ts 落在本地今日的 user +
  /// assistant 总数。活跃度信号 — owner 想「今天和 pet 聊了多少」时
  /// ambient chip 给即时数。`messages` 是 raw prop，覆盖比 visibleItems
  /// 更全（visibleItems 还没在此 scope 声明 — TDZ）。session 切换 / 新
  /// 消息进来时自然 re-derive；跨午夜时 silentTick 30s 推过自然刷新。
  /// 仅 ≥ 1 时显（idle 态省垂直空间，与既有 ambient gates 一致）。
  const messagesToday = useMemo(() => {
    const todayStr = new Date().toLocaleDateString("sv-SE"); // YYYY-MM-DD 本地
    let count = 0;
    for (const m of messages) {
      if (!m.ts) continue;
      const d = new Date(m.ts);
      if (isNaN(d.getTime())) continue;
      const itemStr = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
      if (itemStr === todayStr) count += 1;
    }
    return count;
  }, [messages, silentTick]);
  const ambientHasContent =
    ambientTransient !== null ||
    ambientAlarms > 0 ||
    ambientMuteMins !== null ||
    showPetSilentChip ||
    messagesToday > 0;
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

  /// 长 assistant reply 折叠状态：visibleItems idx ∈ Set 表示"用户已展
  /// 开"。默认所有长 bubble 折叠到 LONG_BUBBLE_PREVIEW_CHARS（400 字）
  /// + "…" + 「展开」按钮；超过 LONG_BUBBLE_THRESHOLD（2000 字）才进
  /// 折叠流。idx 作 key 简单可靠（新消息总是 append，旧 idx 稳定）；
  /// 跨 session reset 时 stale idx 自然失效（Set 仍 in-memory 但不影
  /// 响渲染）。
  const [longBubblesExpanded, setLongBubblesExpanded] = useState<Set<number>>(
    () => new Set(),
  );
  const toggleLongBubble = (idx: number) => {
    setLongBubblesExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(idx)) next.delete(idx);
      else next.add(idx);
      return next;
    });
  };
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
  /// 📌 view marks popover：列当前 session 内 sel-* 标记（iter #412 写入）。
  /// marks 是从 localStorage 读出的快照 (key + ts + text)；refreshTrigger
  /// 让选区 📌 button 写入后能同步刷新视图。仅 popoverOpen 时实际 IO 加
  /// 载；关闭时仍存内存但不刷（节省 invoke）。
  const [marksPopoverOpen, setMarksPopoverOpen] = useState(false);
  const [marksRefreshTrigger, setMarksRefreshTrigger] = useState(0);
  const [marksList, setMarksList] = useState<
    Array<{ key: string; ts: number; text: string }>
  >([]);
  const [marksCount, setMarksCount] = useState(0);
  /// 读 localStorage 两个 key：`pet-chat-marked-messages` filter sel-* +
  /// `pet-chatmini-mark-texts` 取 text body。需要 active session id 让
  /// 跨 session marks 互不显。返回按 ts desc（最新在前）。
  const refreshMarks = useCallback(async () => {
    try {
      const idx = await invoke<{ active_id: string }>("list_sessions");
      const sid = idx.active_id?.trim();
      if (!sid) {
        setMarksList([]);
        setMarksCount(0);
        return;
      }
      const KEY = "pet-chat-marked-messages";
      const TEXTS_KEY = "pet-chatmini-mark-texts";
      let parsed: Record<string, unknown> = {};
      let texts: Record<string, string> = {};
      try {
        const raw = window.localStorage.getItem(KEY);
        if (raw) {
          const got = JSON.parse(raw);
          if (got && typeof got === "object" && !Array.isArray(got)) {
            parsed = got;
          }
        }
        const trawTexts = window.localStorage.getItem(TEXTS_KEY);
        if (trawTexts) {
          const got = JSON.parse(trawTexts);
          if (got && typeof got === "object" && !Array.isArray(got)) {
            texts = got;
          }
        }
      } catch {
        // 解析失败 → 视作空
      }
      const sessionPrefix = `${sid}::sel-`;
      const out: Array<{ key: string; ts: number; text: string }> = [];
      for (const [k, v] of Object.entries(parsed)) {
        if (typeof k !== "string" || !k.startsWith(sessionPrefix)) continue;
        if (typeof v !== "number") continue;
        out.push({
          key: k,
          ts: v,
          text: texts[k] ?? "（无文本快照 — iter #412 之前的旧 mark）",
        });
      }
      out.sort((a, b) => b.ts - a.ts);
      setMarksList(out);
      setMarksCount(out.length);
    } catch {
      // list_sessions 失败 → 不更新，保上次值
    }
  }, []);
  /// mount + popover 开关 + refreshTrigger 变化 → 拉新数据
  useEffect(() => {
    void refreshMarks();
  }, [refreshMarks, marksPopoverOpen, marksRefreshTrigger]);
  /// 删除单条 mark：从 pet-chat-marked-messages + sibling text key 一并
  /// 移除 + refreshTrigger 让 popover 重渲。
  const deleteMark = useCallback(
    (markKey: string) => {
      const KEY = "pet-chat-marked-messages";
      const TEXTS_KEY = "pet-chatmini-mark-texts";
      try {
        const raw = window.localStorage.getItem(KEY);
        if (raw) {
          const got = JSON.parse(raw);
          if (got && typeof got === "object" && !Array.isArray(got)) {
            delete got[markKey];
            window.localStorage.setItem(KEY, JSON.stringify(got));
          }
        }
      } catch {}
      try {
        const trawTexts = window.localStorage.getItem(TEXTS_KEY);
        if (trawTexts) {
          const got = JSON.parse(trawTexts);
          if (got && typeof got === "object" && !Array.isArray(got)) {
            delete got[markKey];
            window.localStorage.setItem(TEXTS_KEY, JSON.stringify(got));
          }
        }
      } catch {}
      setMarksRefreshTrigger((v) => v + 1);
    },
    [],
  );

  /// ⌘` 弹 transient_note 快速 popover：让 owner 不必发消息就能给 pet 留
  /// 临时上下文（如「半小时别打扰」「集中写文档」）。复用既有
  /// onSetTransientNote callback（同 set_transient_note Tauri 后端）。
  /// 与既有 ChatBubble 右键「📝 用此话设 transient_note」对偶 — 那个把
  /// 既有 pet 文本复用，本 popover 是 owner 写「全新文本」入口。
  const [transientPopoverOpen, setTransientPopoverOpen] = useState(false);
  const [transientPopoverDraft, setTransientPopoverDraft] = useState("");
  const transientPopoverInputRef = useRef<HTMLTextAreaElement>(null);
  /// ⌘` 全局快捷键：可见时切换 popover。\` 与 Esc 同 / 数字 / J/K 等已被各
  /// 处占用的键不冲突；macOS 上 ⌘\` 默认是「下一窗口」但 ChatMini webview
  /// 内不会触发系统级 — 在 webview 焦点内安全劫持。
  useEffect(() => {
    if (!visible) return;
    if (!onSetTransientNote) return;
    const onKey = (e: KeyboardEvent) => {
      if (!(e.metaKey || e.ctrlKey)) return;
      if (e.shiftKey || e.altKey) return;
      if (e.key !== "`") return;
      e.preventDefault();
      setTransientPopoverOpen((v) => !v);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [visible, onSetTransientNote]);
  /// 打开后聚焦 textarea + 选中既有内容（让连续 ⌘\` 弹起可立即重写）。
  useEffect(() => {
    if (!transientPopoverOpen) return;
    const id = window.setTimeout(() => {
      transientPopoverInputRef.current?.focus();
      transientPopoverInputRef.current?.select();
    }, 0);
    return () => window.clearTimeout(id);
  }, [transientPopoverOpen]);
  /// 提交：调 onSetTransientNote + 清 draft + 关 popover。空 trim 拒绝
  /// （避免给 pet 注水空 transient_note）。
  const submitTransientPopover = useCallback(
    (minutes: number) => {
      const body = transientPopoverDraft.trim();
      if (!body) return;
      onSetTransientNote?.(body, minutes);
      setTransientPopoverDraft("");
      setTransientPopoverOpen(false);
    },
    [transientPopoverDraft, onSetTransientNote],
  );

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

  /// iter #399: 💾 导出本会话 markdown — 与 PanelChat 既有
  /// exportSessionAsMarkdown 同格式（# 标题 + > 导出时间 · 共 N 条
  /// + 每条 ## 🧑/🐾 + content + 空行）。仅 user / assistant 入；
  /// system / tool 跳。复用 copyToast 反馈 mechanism。
  const handleExportSessionMarkdown = () => {
    const slice = messages.filter(
      (m) => m.role === "user" || m.role === "assistant",
    );
    if (slice.length === 0) {
      setCopyToast("err");
      window.setTimeout(() => setCopyToast("none"), 1500);
      return;
    }
    const lines: string[] = [];
    lines.push("# ChatMini Session");
    lines.push(
      `> 导出时间: ${new Date().toLocaleString()} · 共 ${slice.length} 条消息`,
    );
    lines.push("");
    for (const m of slice) {
      const glyph =
        m.role === "user" ? effectiveUserGlyph : effectiveAssistantGlyph;
      lines.push(`## ${glyph} ${m.role}`);
      lines.push("");
      lines.push(extractText(m.content));
      lines.push("");
    }
    const md = lines.join("\n");
    navigator.clipboard
      .writeText(md)
      .then(() => {
        setCopyToast("done");
        window.setTimeout(() => setCopyToast("none"), 1500);
      })
      .catch((err) => {
        console.error("export session markdown failed:", err);
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

  /// ⌘R / Ctrl+R re-roll：把**最后一条 user message** 重新发送给 pet。
  /// 与既有 ↺ 重发本条 ctx menu item 同 backend（dispatch
  /// `pet-mini-resend-message` 让 ChatPanel listener 跳过 textarea
  /// 直接 onSend）— 但 keyboard 路径不需要先右键找具体 bubble，最常用
  /// "上一句不满意，让 pet 再 reply 一次" 场景一键完成。
  ///
  /// ⌘R 本是 browser / Tauri webview 默认 reload 整页 — preventDefault
  /// + stopImmediatePropagation 双重吃键防 reload（owner 误触会丢
  /// session state）。仅在 visible + 找到 user message 时拦；textarea /
  /// input 内焦点放过（让 native re-paste / 其它编辑器 ⌘R 行为不被吞）。
  ///
  /// 取最后一条 user message 而非最后整体 — owner 想 "重发上句"，不是
  /// "重发 pet 的 reply"（assistant message resend 无语义）。
  const messagesRef = useRef(messages);
  useEffect(() => {
    messagesRef.current = messages;
  }, [messages]);
  useEffect(() => {
    if (!visible) return;
    const handler = (e: KeyboardEvent) => {
      if (!(e.metaKey || e.ctrlKey)) return;
      if (e.shiftKey || e.altKey) return;
      if (e.key.toLowerCase() !== "r") return;
      // textarea / input / contentEditable 焦点 → 放过（让 owner 在
      // ChatPanel input 写 prompt 时 ⌘R 仍能 reload — 误防御过度反而
      // 让 owner 抱怨）。其它焦点（chat scroll / pet)劫持。
      const ae = document.activeElement;
      if (
        ae instanceof HTMLInputElement ||
        ae instanceof HTMLTextAreaElement ||
        (ae instanceof HTMLElement && ae.isContentEditable)
      ) {
        return;
      }
      // 找最后一条 user message（从末向前扫）
      const msgs = messagesRef.current;
      let lastUserText = "";
      for (let i = msgs.length - 1; i >= 0; i--) {
        const m = msgs[i];
        if (m.role !== "user") continue;
        const t = extractText(m.content).trim();
        if (t.length === 0) continue;
        lastUserText = t;
        break;
      }
      if (!lastUserText) return; // 无 user message 不动 — 让 reload 自然走？
      // 拦截 reload + 触发 resend
      e.preventDefault();
      e.stopImmediatePropagation();
      window.dispatchEvent(
        new CustomEvent("pet-mini-resend-message", {
          detail: lastUserText,
        }),
      );
    };
    // capture phase 让本 handler 先于其它 ⌘R listener / 默认 reload 跑
    window.addEventListener("keydown", handler, { capture: true });
    return () =>
      window.removeEventListener("keydown", handler, { capture: true });
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
        {/* 💡 ambient hint 行：顶部一行显当前 pet 临时上下文 — transient_note
            preview + active alarms 数 + mute 剩余。让 owner 不必开 panel 即
            可一眼看「pet 现在感知到什么」。三段全空 → 整行不渲（idle 态省
            垂直空间）。30s 轮询足够（这三个信号都是分钟级粒度）。 */}
        {ambientHasContent && (
          <div
            style={{
              display: "flex",
              flexWrap: "wrap",
              gap: 6,
              padding: "2px 4px 6px",
              fontSize: 10,
              color: "var(--pet-color-muted)",
              fontFamily: "'SF Mono', 'Menlo', monospace",
              userSelect: "none",
            }}
            aria-label="pet 当前上下文 ambient hint"
          >
            {/* iter #395: chip click → deeplink 跳 PanelDebug 对应卡片。
                写 pet-debug-deeplink localStorage + invoke open_debug；
                DebugApp 读后切到 "应用" tab + scrollIntoView 锚点元素。
                与 pet-panel-deeplink 同 TTL=10s 模板。chips 改 button 以
                获 cursor: pointer + keyboard accessible。 */}
            {ambientTransient && (() => {
              const preview =
                ambientTransient.text.length > 30
                  ? ambientTransient.text.slice(0, 30) + "…"
                  : ambientTransient.text;
              return (
                <button
                  type="button"
                  onClick={() => {
                    try {
                      window.localStorage.setItem(
                        "pet-debug-deeplink",
                        JSON.stringify({
                          tab: "应用",
                          scrollAnchor: "tone-strip",
                          ts: Date.now(),
                        }),
                      );
                    } catch (e) {
                      console.error("write pet-debug-deeplink failed:", e);
                    }
                    invoke("open_debug").catch(console.error);
                  }}
                  title={`pet 当前 transient_note（剩 ${ambientTransient.mins} 分钟）：${ambientTransient.text}\n\n点击 → 打开 debug 窗 + 滚到 ToneStrip 查看 / 改 transient_note`}
                  style={{
                    display: "inline-flex",
                    alignItems: "center",
                    gap: 2,
                    padding: "1px 6px",
                    borderRadius: 8,
                    background: "color-mix(in srgb, #0891b2 14%, transparent)",
                    color: "#0891b2",
                    fontWeight: 500,
                    maxWidth: 220,
                    overflow: "hidden",
                    textOverflow: "ellipsis",
                    whiteSpace: "nowrap",
                    border: "none",
                    cursor: "pointer",
                    fontFamily: "inherit",
                    fontSize: "inherit",
                  }}
                >
                  📝 {preview} · {ambientTransient.mins}m
                </button>
              );
            })()}
            {ambientAlarms > 0 && (
              <button
                type="button"
                onClick={() => {
                  try {
                    window.localStorage.setItem(
                      "pet-debug-deeplink",
                      JSON.stringify({
                        tab: "应用",
                        scrollAnchor: "pending-reminders",
                        ts: Date.now(),
                      }),
                    );
                  } catch (e) {
                    console.error("write pet-debug-deeplink failed:", e);
                  }
                  invoke("open_debug").catch(console.error);
                }}
                title={`${ambientAlarms} 条 pending alarm（todo 段 [remind:] 条目）— pet proactive 扫到 due 时会软提醒。\n\n点击 → 打开 debug 窗 + 滚到「待提醒事项」卡片`}
                style={{
                  display: "inline-flex",
                  alignItems: "center",
                  gap: 2,
                  padding: "1px 6px",
                  borderRadius: 8,
                  background:
                    "color-mix(in srgb, var(--pet-tint-blue-fg) 14%, transparent)",
                  color: "var(--pet-tint-blue-fg)",
                  fontWeight: 500,
                  border: "none",
                  cursor: "pointer",
                  fontFamily: "inherit",
                  fontSize: "inherit",
                }}
              >
                ⏰ {ambientAlarms}
              </button>
            )}
            {ambientMuteMins !== null && (
              <button
                type="button"
                onClick={() => {
                  try {
                    window.localStorage.setItem(
                      "pet-debug-deeplink",
                      JSON.stringify({
                        tab: "应用",
                        scrollAnchor: "tone-strip",
                        ts: Date.now(),
                      }),
                    );
                  } catch (e) {
                    console.error("write pet-debug-deeplink failed:", e);
                  }
                  invoke("open_debug").catch(console.error);
                }}
                title={`pet 当前被静音 — 剩 ${ambientMuteMins} 分钟。期间 proactive 不主动开口；ChatMini / panel 仍可手动发起。\n\n点击 → 打开 debug 窗 + 滚到 ToneStrip 查看 / 改 mute`}
                style={{
                  display: "inline-flex",
                  alignItems: "center",
                  gap: 2,
                  padding: "1px 6px",
                  borderRadius: 8,
                  background: "color-mix(in srgb, #7c3aed 14%, transparent)",
                  color: "#7c3aed",
                  fontWeight: 500,
                  border: "none",
                  cursor: "pointer",
                  fontFamily: "inherit",
                  fontSize: "inherit",
                }}
              >
                🔇 {ambientMuteMins}m
              </button>
            )}
            {/* ⏱ pet 沉默 N 分 chip：自上次 assistant 消息（含 valid ts）
                算起的分钟数。让 owner 觉察「pet 是不是又卡住了 / proactive
                pipeline 没在跑」。severity 三档：muted / amber / red。仅
                ≥ 5 分时显（pet 刚说完就闪 chip 是噪音）。click → debug 窗
                看 proactive 状态。 */}
            {showPetSilentChip && petSilentMins !== null && (() => {
              const mins = petSilentMins;
              let bg: string;
              let fg: string;
              if (mins >= 90) {
                bg = "color-mix(in srgb, #dc2626 14%, transparent)";
                fg = "#dc2626";
              } else if (mins >= 30) {
                bg = "color-mix(in srgb, #d97706 14%, transparent)";
                fg = "#d97706";
              } else {
                bg = "color-mix(in srgb, var(--pet-color-fg) 8%, transparent)";
                fg = "var(--pet-color-muted)";
              }
              const label =
                mins < 60
                  ? `${mins}m`
                  : `${Math.floor(mins / 60)}h${mins % 60 > 0 ? ` ${mins % 60}m` : ""}`;
              const sev =
                mins >= 90
                  ? "🔴 长时间沉默 — 检查 proactive pipeline 是否卡住"
                  : mins >= 30
                    ? "🟡 偏久 — 可能正在等 mute / silent / 长 cron 间隔"
                    : "默认节奏";
              return (
                <button
                  type="button"
                  onClick={() => {
                    try {
                      window.localStorage.setItem(
                        "pet-debug-deeplink",
                        JSON.stringify({
                          tab: "应用",
                          scrollAnchor: "tone-strip",
                          ts: Date.now(),
                        }),
                      );
                    } catch (e) {
                      console.error(
                        "write pet-debug-deeplink failed:",
                        e,
                      );
                    }
                    invoke("open_debug").catch(console.error);
                  }}
                  title={`pet 自上次主动 / 回复以来已沉默 ${mins} 分钟。${sev}\n\n点击 → 打开 debug 窗 + 滚到 ToneStrip 看 mute / transient / proactive 状态。`}
                  style={{
                    display: "inline-flex",
                    alignItems: "center",
                    gap: 2,
                    padding: "1px 6px",
                    borderRadius: 8,
                    background: bg,
                    color: fg,
                    fontWeight: 500,
                    border: "none",
                    cursor: "pointer",
                    fontFamily: "inherit",
                    fontSize: "inherit",
                  }}
                >
                  ⏱ {label}
                </button>
              );
            })()}
            {/* 📊 今日消息计数 chip：scan messages 中 ts 落在本地今日的
                user + assistant 总数。活跃度信号 — 与既有 transient /
                alarms / mute / silent chip 同 ambient pattern。点击复制
                「今日 N 消息」一行到剪贴板（粘日记 / 同事 ping 场景）。
                ≥ 1 时浮（≥ 1 才有意义 — gate 与 ambientHasContent 一致
                避免 0 时单独显占垂直空间）。 */}
            {messagesToday > 0 && (
              <button
                type="button"
                onClick={async (e) => {
                  e.stopPropagation();
                  const todayStr = new Date().toLocaleDateString("sv-SE");
                  const line = `今日（${todayStr}）${messagesToday} 条消息`;
                  try {
                    await navigator.clipboard.writeText(line);
                    console.log(`📊 已复制：${line}`);
                  } catch (err) {
                    console.error("copy today msg count failed:", err);
                  }
                }}
                title={`本会话今日（本地日历日）共 ${messagesToday} 条 user + assistant 消息。点击复制「今日 N 消息」一行到剪贴板。`}
                aria-label="today messages count"
                style={{
                  display: "inline-flex",
                  alignItems: "center",
                  gap: 2,
                  padding: "1px 6px",
                  borderRadius: 8,
                  background:
                    "color-mix(in srgb, var(--pet-color-fg) 6%, transparent)",
                  color: "var(--pet-color-muted)",
                  fontWeight: 500,
                  border: "none",
                  cursor: "pointer",
                  fontFamily: "inherit",
                  fontSize: "inherit",
                }}
              >
                📊 今日 {messagesToday}
              </button>
            )}
          </div>
        )}
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
        {/* iter #399: 💾 导出本会话 markdown 按钮 — 与 PanelChat 既有
            导出对偶；ChatMini 内一键 export 当前 session 到剪贴板。位置
            在 🌐 时区 chip 之左（顺序 ⛶ → 📋 → 🌐 → 💾，每个 28px 间
            距），让两 copy/export 入口（📋 / 💾）+ 时区 / 最大化辅助
            chip 视觉成行。 */}
        <button
          type="button"
          className="pet-mini-maxbtn"
          onMouseDown={(e) => e.stopPropagation()}
          onClick={(e) => {
            e.stopPropagation();
            handleExportSessionMarkdown();
          }}
          title="导出本会话为 markdown 复制到剪贴板（# title + 每条 ## 🧑/🐾 + content，与桌面 PanelChat 既有导出同格式）"
          aria-label="export session as markdown"
          style={{
            position: "absolute",
            top: "14px",
            right: onOpenPanel ? "104px" : "76px",
            width: "20px",
            height: "20px",
            borderRadius: "50%",
            border: "1px solid var(--pet-color-border)",
            background: "var(--pet-color-card)",
            color: "var(--pet-color-muted)",
            fontSize: "10px",
            lineHeight: 1,
            cursor: "pointer",
            zIndex: 13,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            padding: 0,
            boxShadow: "var(--pet-shadow-sm)",
          }}
        >
          💾
        </button>
        {/* iter #417: 📌 view marks chip — 仅当前 session 有 sel-* mark 时
            浮起，badge 显计数。click toggle popover 列每条 mark + 删除按钮。
            位置在 💾 之左（onOpenPanel 132 / no-panel 104；每个 28px 间距
            与既有 chip 行节奏一致）。 */}
        {marksCount > 0 && (
          <button
            type="button"
            className="pet-mini-maxbtn"
            onMouseDown={(e) => e.stopPropagation()}
            onClick={(e) => {
              e.stopPropagation();
              setMarksPopoverOpen((v) => !v);
            }}
            title={`查看本会话标记 (${marksCount} 条) — 列每条 sel-* mark 内容 + 时间戳；click 🗑 删除单条`}
            aria-label={`view ${marksCount} marks in this session`}
            style={{
              position: "absolute",
              top: "14px",
              right: onOpenPanel ? "132px" : "104px",
              minWidth: "20px",
              height: "20px",
              padding: "0 5px",
              borderRadius: "10px",
              border: "1px solid var(--pet-tint-yellow-fg)",
              background: "var(--pet-tint-yellow-bg)",
              color: "var(--pet-tint-yellow-fg)",
              fontSize: "10px",
              fontWeight: 600,
              lineHeight: 1,
              cursor: "pointer",
              zIndex: 13,
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              gap: 2,
              boxShadow: "var(--pet-shadow-sm)",
            }}
          >
            📌 {marksCount}
          </button>
        )}
        {marksPopoverOpen && (
          <div
            onMouseDown={(e) => {
              // outside-click 关：内部 mousedown stopProp 防自关
              if (e.target === e.currentTarget) {
                setMarksPopoverOpen(false);
              }
            }}
            style={{
              position: "fixed",
              inset: 0,
              zIndex: 100000,
              display: "flex",
              alignItems: "flex-start",
              justifyContent: "center",
              paddingTop: 60,
              background:
                "color-mix(in srgb, var(--pet-color-bg) 35%, transparent)",
            }}
          >
            <div
              onMouseDown={(e) => e.stopPropagation()}
              style={{
                minWidth: 320,
                maxWidth: 440,
                maxHeight: "70vh",
                overflow: "auto",
                padding: 12,
                border: "1px solid var(--pet-color-border)",
                borderRadius: 8,
                background: "var(--pet-color-card)",
                boxShadow: "var(--pet-shadow-md)",
                display: "flex",
                flexDirection: "column",
                gap: 6,
              }}
            >
              <div
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 6,
                  fontSize: 12,
                  fontWeight: 600,
                  color: "var(--pet-color-fg)",
                  marginBottom: 4,
                }}
              >
                📌 本会话标记 ({marksList.length})
                <span style={{ flex: 1 }} />
                <button
                  type="button"
                  onClick={() => setMarksPopoverOpen(false)}
                  title="关闭"
                  style={{
                    padding: "2px 8px",
                    fontSize: 11,
                    border: "1px solid var(--pet-color-border)",
                    borderRadius: 4,
                    background: "var(--pet-color-card)",
                    color: "var(--pet-color-muted)",
                    cursor: "pointer",
                  }}
                >
                  ✕
                </button>
              </div>
              {marksList.length === 0 ? (
                <div
                  style={{
                    fontSize: 11,
                    color: "var(--pet-color-muted)",
                    padding: "12px 0",
                    textAlign: "center",
                  }}
                >
                  本会话暂无标记。
                  <br />
                  选中聊天里的文字 → 工具栏 📌 标记可加入。
                </div>
              ) : (
                marksList.map((m) => {
                  const d = new Date(m.ts);
                  const tsLabel = `${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")} ${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
                  return (
                    <div
                      key={m.key}
                      style={{
                        display: "flex",
                        gap: 6,
                        alignItems: "flex-start",
                        padding: "6px 8px",
                        border: "1px solid var(--pet-color-border)",
                        borderRadius: 4,
                        background: "var(--pet-color-bg)",
                      }}
                    >
                      <span
                        style={{
                          fontSize: 10,
                          color: "var(--pet-color-muted)",
                          flexShrink: 0,
                          fontFamily: "'SF Mono', monospace",
                          padding: "2px 0",
                        }}
                        title={new Date(m.ts).toLocaleString()}
                      >
                        {tsLabel}
                      </span>
                      <span
                        style={{
                          flex: 1,
                          fontSize: 12,
                          color: "var(--pet-color-fg)",
                          lineHeight: 1.5,
                          wordBreak: "break-word",
                        }}
                      >
                        {m.text}
                      </span>
                      <button
                        type="button"
                        onClick={() => deleteMark(m.key)}
                        title="删除这条标记"
                        aria-label="delete mark"
                        style={{
                          padding: "2px 6px",
                          fontSize: 11,
                          border: "1px solid var(--pet-color-border)",
                          borderRadius: 4,
                          background: "var(--pet-color-card)",
                          color: "var(--pet-tint-red-fg)",
                          cursor: "pointer",
                          flexShrink: 0,
                        }}
                      >
                        🗑
                      </button>
                    </div>
                  );
                })
              )}
            </div>
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
        {/* 🌡️ context 健康 mini progress bar：常态可见（仅 sessionTokens
            > 20% threshold = 800 时显，避免空 session 噪音），让 owner 在
            撞 4000 警示线前提前感知。色按 < 50% 绿 / 50-75% amber / ≥ 75%
            red 三档；cap 100% width。撞警示线后由下方更显眼的警示 chip
            接力（同一信号两层视觉权重 — bar 是 ambient peek，chip 是 CTA）。 */}
        {sessionTokens !== undefined &&
          sessionTokens > MINI_TOKEN_WARN_THRESHOLD * 0.2 &&
          sessionTokens <= MINI_TOKEN_WARN_THRESHOLD && (
            <div
              style={{
                marginBottom: 6,
                display: "flex",
                alignItems: "center",
                gap: 6,
                fontSize: 10,
                color: "var(--pet-color-muted)",
                fontFamily: "'SF Mono', 'Menlo', monospace",
              }}
              title={`当前 session 累计 ~${sessionTokens} / ${MINI_TOKEN_WARN_THRESHOLD} tokens（${Math.round((sessionTokens / MINI_TOKEN_WARN_THRESHOLD) * 100)}%）· 撞警示线后会浮 /reset CTA chip`}
            >
              <span>🌡️</span>
              <div
                style={{
                  flex: 1,
                  height: 4,
                  borderRadius: 2,
                  background: "var(--pet-color-border)",
                  overflow: "hidden",
                  position: "relative",
                }}
              >
                {(() => {
                  const pct = Math.min(
                    1,
                    sessionTokens / MINI_TOKEN_WARN_THRESHOLD,
                  );
                  const fg =
                    pct < 0.5
                      ? "var(--pet-tint-green-fg)"
                      : pct < 0.75
                        ? "var(--pet-tint-amber-fg, #d97706)"
                        : "var(--pet-tint-red-fg)";
                  return (
                    <div
                      style={{
                        width: `${pct * 100}%`,
                        height: "100%",
                        background: fg,
                        transition: "width 200ms ease-out",
                      }}
                    />
                  );
                })()}
              </div>
              <span style={{ fontVariantNumeric: "tabular-nums" }}>
                {sessionTokens}/{MINI_TOKEN_WARN_THRESHOLD}
              </span>
            </div>
          )}
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
              {/* 📊 字数 chip：bubble text 字数（Unicode code points 计数
                  via Array.from + length，让中文 / emoji 不被高估 / 低估）。
                  hover-reveal 与顶 ⏱ ts / 底 ⏱ rel chip 同模式但位置在
                  对侧（user 左顶 / assistant 右顶）— 让两顶 chip 不挤一
                  边。仅 hasText 时显（纯图 bubble 没有文本字数概念）。
                  click 复制「N chars」一行（粘 chat report / 写复制前
                  预估）。 */}
              {text && (() => {
                const chars = Array.from(text).length;
                if (chars === 0) return null;
                return (
                  <span
                    className="pet-mini-row-chars"
                    title={`本 bubble 字数 ${chars} 字（Unicode code points）— 点击复制「${chars} chars」`}
                    onClick={(e) => {
                      e.stopPropagation();
                      const line = `${chars} chars`;
                      navigator.clipboard
                        .writeText(line)
                        .catch((err) =>
                          console.error("chars chip copy failed:", err),
                        );
                    }}
                    style={{
                      position: "absolute",
                      top: -12,
                      // 对侧 — user 左 / assistant 右（与 ts chip 反向）
                      [m.role === "user" ? "left" : "right"]: 8,
                      fontSize: 9,
                      color: "var(--pet-color-muted)",
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
                    📊 {chars}
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
                {text && (() => {
                  // 长 assistant reply 折叠 — 默认折到 400 字让 chat 区
                  // 不被单条占满。仅 assistant 且总 chars > THRESHOLD 时
                  // 进折叠流；user 消息不折（用户输入 length 通常自控）；
                  // 流式 streaming bubble 也不折（owner 看 LLM 在打字时
                  // 不该突然折叠）。
                  const chars = Array.from(text);
                  const isLong =
                    isAssistant && chars.length > LONG_BUBBLE_THRESHOLD;
                  const isExpanded = longBubblesExpanded.has(idx);
                  if (!isLong || isExpanded) {
                    return (
                      <>
                        {parseMarkdown(text)}
                        {isLong && (
                          <button
                            type="button"
                            onClick={(e) => {
                              e.stopPropagation();
                              toggleLongBubble(idx);
                            }}
                            style={{
                              display: "inline-block",
                              marginTop: 4,
                              padding: "1px 6px",
                              fontSize: 10,
                              border: "1px dashed var(--pet-color-border)",
                              borderRadius: 4,
                              background: "transparent",
                              color: "var(--pet-color-muted)",
                              cursor: "pointer",
                              fontFamily: "inherit",
                            }}
                            title={`折叠回 ${LONG_BUBBLE_PREVIEW_CHARS} 字预览（再点展开）`}
                          >
                            📑 折叠（{chars.length} 字）
                          </button>
                        )}
                      </>
                    );
                  }
                  // 折叠态：显前 PREVIEW_CHARS 字 + "…" + 展开按钮
                  const preview =
                    chars.slice(0, LONG_BUBBLE_PREVIEW_CHARS).join("") + "…";
                  const remaining =
                    chars.length - LONG_BUBBLE_PREVIEW_CHARS;
                  return (
                    <>
                      {parseMarkdown(preview)}
                      <button
                        type="button"
                        onClick={(e) => {
                          e.stopPropagation();
                          toggleLongBubble(idx);
                        }}
                        style={{
                          display: "inline-block",
                          marginTop: 4,
                          padding: "1px 6px",
                          fontSize: 10,
                          border: "1px dashed var(--pet-color-accent)",
                          borderRadius: 4,
                          background: "transparent",
                          color: "var(--pet-color-accent)",
                          cursor: "pointer",
                          fontFamily: "inherit",
                          fontWeight: 500,
                        }}
                        title={`展开剩余 ${remaining} 字 — 完整长度 ${chars.length} 字`}
                      >
                        📑 展开剩余 {remaining} 字
                      </button>
                    </>
                  );
                })()}
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
        {/* 💰 session token tally 状态行：bubble 列表底部 ambient 信号，
            显累计 token + 估算 cost（基于 MINI_TOKEN_COST_PER_MILLION
            blended rate）。任 sessionTokens > 0 即显（无 threshold gate）—
            与顶部 🌡️ bar（仅 20-100% 区段）/ 💭 CTA chip（> threshold）
            互补，覆盖 0-20% 早期 ambient 信息盲区。
            数据局限：backend 无 input/output token 拆分（estimate_tokens
            是按 char count / 4 估全 session 上下文），cost 用单一 rate
            $3/1M 估算 — 不当账单用。tooltip 注明 caveat。 */}
        {sessionTokens !== undefined && sessionTokens > 0 && (() => {
          const costUsd = (sessionTokens * MINI_TOKEN_COST_PER_MILLION) / 1_000_000;
          const costLabel =
            costUsd < 0.01
              ? `<$0.01`
              : `≈ $${costUsd.toFixed(costUsd < 1 ? 3 : 2)}`;
          return (
            <div
              style={{
                marginTop: 4,
                paddingTop: 4,
                borderTop: "1px dashed var(--pet-color-border)",
                fontSize: 10,
                color: "var(--pet-color-muted)",
                fontFamily: "'SF Mono', 'Menlo', monospace",
                display: "flex",
                alignItems: "center",
                gap: 6,
                userSelect: "none",
              }}
              title={`本 session 累计 ~${sessionTokens} tokens（含 system + 历史 turns，按 4 chars/token 估），按 blended $${MINI_TOKEN_COST_PER_MILLION}/1M 估算 cost ${costLabel} USD。仅供 ambient awareness — 不区分 input/output，精确账单请看上游 API console。`}
            >
              <span>💰</span>
              <span style={{ fontVariantNumeric: "tabular-nums" }}>
                ~{sessionTokens} tok · {costLabel}
              </span>
            </div>
          );
        })()}
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
        // 📋 markdown 原文：与 extractText（stripMdImages 后的 plain）
        // 不同 — 保留 markdown image 语法 / ref tokens / code blocks 原
        // 状。让 owner 粘到 markdown 编辑器 / detail.md / TG /quick 保
        // 留所有渲染。pet reply 常含 「title」 ref + ```code``` block，
        // 走 extractText 后 image 语法被 strip；本变体保完整 raw。
        const getRawMarkdown = (content: unknown): string => {
          if (typeof content === "string") return content;
          if (!Array.isArray(content)) return "";
          return content
            .filter(
              (p): p is { type: "text"; text: string } =>
                !!p &&
                typeof p === "object" &&
                (p as { type?: string }).type === "text" &&
                typeof (p as { text?: unknown }).text === "string",
            )
            .map((p) => p.text)
            .join("\n");
        };
        const rawMarkdown = getRawMarkdown(m.content);
        const hasRawMarkdown = rawMarkdown.trim().length > 0;
        // 🔗 复制 task ref：扫 bubble 文本里的 `「title」` token，dedupe
        // 保留出现顺序，拼成空格分隔的 inline ref 串。owner 复制后可粘到
        // 新 task description（如 `[blockedBy: 「a」 「b」]`）/ detail.md /
        // TG /quick — refs 渲染保留双击跳源 task 的语义。pet 在 reply 里
        // 频繁带 ref token；本入口让 owner 不必手工拣字符。
        const refTitlesSet: string[] = [];
        if (hasText) {
          const seen = new Set<string>();
          const re = /「([^「」\n]+)」/g;
          let match: RegExpExecArray | null;
          while ((match = re.exec(text)) !== null) {
            const t = match[1].trim();
            if (t && !seen.has(t)) {
              seen.add(t);
              refTitlesSet.push(t);
            }
          }
        }
        const refTitles = refTitlesSet;
        const hasRefs = refTitles.length > 0;
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
            {/* 📋 markdown 原文：与「📋 复制本条」(plain text, stripMdImages
                抹掉) 对偶，保完整 markdown 语法 — 让 owner 粘到 markdown
                编辑器 / detail.md / TG /quick 保留 ref tokens / image /
                code blocks 渲染。仅 rawMarkdown 非空显（pure-image 消息
                content 数组无 text part 时空 string 时跳）。 */}
            <button
              type="button"
              style={item}
              onMouseOver={itemHoverIn}
              onMouseOut={itemHoverOut}
              disabled={!hasRawMarkdown}
              onClick={() => {
                setCtxMenu(null);
                if (!hasRawMarkdown) return;
                navigator.clipboard
                  .writeText(rawMarkdown)
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
                    console.error("markdown raw copy failed:", err),
                  );
              }}
              title={
                hasRawMarkdown
                  ? `复制 ${rawMarkdown.length} 字 markdown 原文（保 ref tokens / image / code block 等语法）`
                  : "本条无可复制 markdown 原文"
              }
            >
              📋 markdown 原文
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
            {/* 📋 复制 thread 5：本 bubble 之上 4 条（含本条共 5 条）user
                / assistant 消息拼 markdown 段。让 owner audit「上下文一
                段对话」时不必逐条复制。format 与 copyRecentN 同 glyph
                pattern（user 🧑 / assistant 🐾）+ 含 timestamp 与 copyIncludeTime
                偏好一致；用 \n\n 段隔保 markdown 阅读体验。 */}
            <button
              type="button"
              style={item}
              onMouseOver={itemHoverIn}
              onMouseOut={itemHoverOut}
              onClick={() => {
                setCtxMenu(null);
                const idx = ctxMenu.idx;
                const startIdx = Math.max(0, idx - 4);
                const slice = visibleItems.slice(startIdx, idx + 1);
                if (slice.length === 0) return;
                const text = slice
                  .map((mi) => {
                    const glyph =
                      mi.role === "user"
                        ? effectiveUserGlyph
                        : effectiveAssistantGlyph;
                    const prefix = copyIncludeTime
                      ? `${formatBubbleTimestamp(mi.ts)} ${glyph}`
                      : glyph;
                    return `${prefix} ${extractText(mi.content)}`.trim();
                  })
                  .filter((s) => s.length > 0)
                  .join("\n\n");
                navigator.clipboard
                  .writeText(text)
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
                    console.error("copy thread 5 failed:", err),
                  );
              }}
              title={`复制本 bubble + 之上 4 条（共 5 条 user / assistant 消息）拼 markdown — 上下文段 audit 场景。${copyIncludeTime ? "含 [HH:MM] timestamp prefix。" : "不含 timestamp（开顶部「⌚ 含时间戳」preference 切换）。"}`}
            >
              📋 复制 thread 5
            </button>
            {/* 🔗 复制 task ref：把 bubble 内的所有 「title」 token 收
                集 + dedupe + 空格拼接复制。无 ref 命中时 disabled + tooltip
                解释。粘到 task description / detail.md / TG /quick 仍是
                ref（owner 不必手工拣字符）。 */}
            <button
              type="button"
              style={item}
              onMouseOver={itemHoverIn}
              onMouseOut={itemHoverOut}
              disabled={!hasRefs}
              title={
                hasRefs
                  ? `复制 ${refTitles.length} 个 task ref：${refTitles
                      .map((t) => `「${t}」`)
                      .join(" ")}`
                  : "本条未提到任何 「title」 ref token"
              }
              onClick={() => {
                setCtxMenu(null);
                if (!hasRefs) return;
                const payload = refTitles.map((t) => `「${t}」`).join(" ");
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
                    console.error("copy task ref tokens failed:", err),
                  );
              }}
            >
              🔗 复制 task ref{hasRefs ? ` (${refTitles.length})` : ""}
            </button>
            {/* 💾 转 task：把 bubble plain text 一键塞进 butler_tasks 队
                列。title = 前 30 字（whitespace flat），body = 全文；
                priority=P3，无 due。owner 在 chat 中看到 pet 提出的待办
                / 自己的 brain-dump 一句想"先塞队列回头处理"时一键完
                成 — 比手工切到 PanelTasks + 复制 + 填表三步快。 */}
            {hasText && (
              <button
                type="button"
                style={item}
                onMouseOver={itemHoverIn}
                onMouseOut={itemHoverOut}
                onClick={async () => {
                  setCtxMenu(null);
                  const flat = text.replace(/\s+/g, " ").trim();
                  const titleRaw = flat.slice(0, 30);
                  if (!titleRaw) return;
                  try {
                    await invoke<string>("task_create", {
                      args: {
                        title: titleRaw,
                        body: text,
                        priority: 3,
                        due: null,
                      },
                    });
                    setBubbleCopyIdx(ctxMenu.idx);
                    window.setTimeout(
                      () =>
                        setBubbleCopyIdx((cur) =>
                          cur === ctxMenu.idx ? null : cur,
                        ),
                      1500,
                    );
                    console.log(`💾 转 task 成功：${titleRaw}`);
                  } catch (err) {
                    console.error("create task from bubble failed:", err);
                  }
                }}
                title={`一键把这条 bubble 转 task（P3，无 due）— 标题取前 30 字「${text.slice(0, 30).replace(/\s+/g, " ").trim()}」，body = 全文。后续在 PanelTasks 调 priority / 加 due / 改内容。`}
              >
                💾 转 task
              </button>
            )}
            {/* 📝 转 reflect：把 bubble plain text 一键存为 ai_insights
                memory item — 与 💾 转 task 对偶（task 是要做的事；reflect
                是反思 / 自我洞察）。复用既有 onSaveAsAiInsight callback
                （选区 toolbar 📚 同后端 — memory_edit("create",
                "ai_insights")，title 自动 reflect-YYYY-MM-DDTHH-MM-SS）。
                gate `onSaveAsAiInsight && hasText` — 与 💾 转 task 同
                hasText 防御 + 父无 callback 时不渲染。 */}
            {hasText && onSaveAsAiInsight && (
              <button
                type="button"
                style={item}
                onMouseOver={itemHoverIn}
                onMouseOut={itemHoverOut}
                onClick={() => {
                  setCtxMenu(null);
                  onSaveAsAiInsight(text);
                  setBubbleCopyIdx(ctxMenu.idx);
                  window.setTimeout(
                    () =>
                      setBubbleCopyIdx((cur) =>
                        cur === ctxMenu.idx ? null : cur,
                      ),
                    1500,
                  );
                }}
                title="把这条 bubble 转 ai_insights memory item — 反思 / 自我洞察分类（title 自动 reflect-YYYY-MM-DDTHH-MM-SS，body = 全文）。与 💾 转 task（要做的事）/ 📝 note 选区版（杂项 brain-dump）三件分类。同 /reflect TG 命令后端。"
              >
                📝 转 reflect
              </button>
            )}
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
            {/* ↺ 重发本条：把 user message 原文 dispatch 给 ChatPanel +
                自动触发 onSend → reroll 场景一键拿一份新 reply（不必先
                复制再粘贴再 ⌘Enter）。仅 user role + hasText 时显；
                assistant reply 重发无语义（owner 自己说话不能让 pet
                "再说一次"）。complement 已存 💭 针对这条再问（prefill
                only，不自动 send）。 */}
            {!isAssistant && hasText && (
              <button
                type="button"
                style={item}
                onMouseOver={itemHoverIn}
                onMouseOut={itemHoverOut}
                onClick={() => {
                  setCtxMenu(null);
                  // pet-mini-resend-message：ChatPanel listener 拿原文
                  // 直接 onSend(trimmed) — 跳过 textarea state 中转，
                  // 不污染 user 输入。reroll 场景已 sent 过 → history
                  // 不再 push（dedup）；多模态附件不复用（原 send 时已
                  // attached，再 send 已无 stage）。
                  window.dispatchEvent(
                    new CustomEvent("pet-mini-resend-message", {
                      detail: text,
                    }),
                  );
                }}
              >
                ↺ 重发本条
              </button>
            )}
            {/* 📝 用此话设 transient_note：把 pet 这条 reply 文本（去
                markdown 后的 plain）作 30min 临时上下文。与 iter #364
                PanelToneStrip ✍️ 写 / iter #363 TG /transient 同后端，
                第三个 surface 让 owner "选 pet 这句话当下轮 context"
                免再敲字。仅 assistant + hasText + 父级传 callback 时显。 */}
            {isAssistant && hasText && onSetTransientNote && (
              <button
                type="button"
                style={item}
                onMouseOver={itemHoverIn}
                onMouseOut={itemHoverOut}
                onClick={() => {
                  setCtxMenu(null);
                  onSetTransientNote(text, 30);
                }}
              >
                📝 用此话设 transient_note 30m
              </button>
            )}
            {/* 🎯 60m preset：与 30m 对偶，给「长会议 / deep work」场景
                更顺手的 1 小时窗口。pet 在 60 分钟内每次主动开口 / 选
                task 都会读到这条 transient_note 作 context。两 preset
                同 callback、不同 minutes — owner 想精确分钟数走桌面
                ToneStrip 自定义。 */}
            {isAssistant && hasText && onSetTransientNote && (
              <button
                type="button"
                style={item}
                onMouseOver={itemHoverIn}
                onMouseOut={itemHoverOut}
                onClick={() => {
                  setCtxMenu(null);
                  onSetTransientNote(text, 60);
                }}
              >
                🎯 用此话设 transient_note 60m
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
            {/* "🔍 search this session"：写 deeplink (chatSearch.keyword)
                + 打开 Panel。PanelChat 收到后开 search bar + scope=current
                + 填 query 让 owner 看本会话所有命中（与下方「定位本条」
                单点对偶 — 那个滚 1 处，本入口开搜索循环遍历多点）。
                仅 text 非空（hasText）+ onOpenPanel 传入时显。 */}
            {onOpenPanel && hasText && (
              <button
                type="button"
                style={item}
                onMouseOver={itemHoverIn}
                onMouseOut={itemHoverOut}
                onClick={() => {
                  setCtxMenu(null);
                  // keyword: 取前 60 字符 + flatten whitespace（与
                  // handleFindSimilarInSession line 540 同 limit）— 够独
                  // 特命中、又不挤 search bar input。
                  const keyword = text
                    .replace(/\s+/g, " ")
                    .trim()
                    .slice(0, 60);
                  if (!keyword) return;
                  try {
                    window.localStorage.setItem(
                      "pet-panel-deeplink",
                      JSON.stringify({
                        chatSearch: { keyword },
                        ts: Date.now(),
                      }),
                    );
                  } catch {
                    // localStorage 不可用：仍 onOpenPanel；owner 至少进 Panel
                  }
                  onOpenPanel();
                }}
              >
                🔍 在 Panel 内搜本会话
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
            {onSaveAsNote && (
              <button
                type="button"
                style={btnStyle}
                title="把这段选中文字作 general memory item 存（与 💾 转 task 互补 — task 是要做的事，note 是想记的事）"
                onClick={() => {
                  const text = selectionToolbar.text;
                  setSelectionToolbar(null);
                  onSaveAsNote(text);
                }}
              >
                📝
              </button>
            )}
            {onSaveAsAiInsight && (
              <button
                type="button"
                style={btnStyle}
                title="把这段选中文字作 ai_insights memory item 存 — 反思 / 自我洞察分类，与 📝 note（杂项 brain-dump）分流。同 /reflect TG 命令后端。"
                onClick={() => {
                  const text = selectionToolbar.text;
                  setSelectionToolbar(null);
                  onSaveAsAiInsight(text);
                }}
              >
                📚
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
              title={`把选中文字推到 ChatPanel — 预填「关于「...」」让你直接提问 / 评论。与「💾 转 task」（要做）/「📝 记到 note」（想记）/「🔄 让 AI 改写」（rewrite）互补 — 这条是「针对选段在 ChatPanel 继续问」。`}
              onClick={() => {
                const text = selectionToolbar.text;
                setSelectionToolbar(null);
                // 选段 flatten + 80 char cap：与 ChatMini bubble 内的 30 char
                // 短摘要不同 — 选区是 owner 显式挑的更长片段，给更宽 cap
                // 但仍防超长 prefix 撑爆 input UI。
                const flat = text.replace(/\s+/g, " ").trim();
                const TEXT_PREFIX_CAP = 80;
                const excerpt =
                  flat.length > TEXT_PREFIX_CAP
                    ? flat.slice(0, TEXT_PREFIX_CAP) + "…"
                    : flat;
                window.dispatchEvent(
                  new CustomEvent("pet-mini-respond-to", {
                    detail: excerpt,
                  }),
                );
              }}
            >
              💬
            </button>
            <button
              type="button"
              style={btnStyle}
              title="📌 标记选段：写入 markedMessages localStorage（与 PanelChat bookmark 同 channel — key 形如 sessionId::sel-<ms>）。配 PanelChat 既有 marks 一起 audit。"
              onClick={async () => {
                const text = selectionToolbar.text;
                setSelectionToolbar(null);
                try {
                  const idx = await invoke<{ active_id: string }>(
                    "list_sessions",
                  );
                  const sid = idx.active_id?.trim();
                  if (!sid) {
                    setCopyToast("err");
                    window.setTimeout(() => setCopyToast("none"), 1500);
                    return;
                  }
                  // 读既有 marks → append sel-<ms> 项 → 写回。复用与
                  // PanelChat 同 localStorage key `pet-chat-marked-messages`
                  // — PanelChat 的 idx-only filter 会跳过 sel-* 不影响其
                  // 渲染。承载 text snippet（first 120 字符）让未来 ChatMini
                  // own marks UI 渲它。
                  const KEY = "pet-chat-marked-messages";
                  let parsed: Record<string, unknown> = {};
                  try {
                    const raw = window.localStorage.getItem(KEY);
                    if (raw) {
                      const got = JSON.parse(raw);
                      if (got && typeof got === "object" && !Array.isArray(got)) {
                        parsed = got;
                      } else if (Array.isArray(got)) {
                        // 旧格式 Array<string>：迁到 Record<key, 0>
                        for (const s of got) {
                          if (typeof s === "string") parsed[s] = 0;
                        }
                      }
                    }
                  } catch {
                    // 解析失败 → 视作空，覆盖写
                  }
                  const ts = Date.now();
                  const markKey = `${sid}::sel-${ts}`;
                  // 同 key markedAt 数值（与 PanelChat 同 schema）。text
                  // snippet 不存（PanelChat read 路径只取 number value）—
                  // 保 schema 兼容；selection 内容 owner 用复制按钮另
                  // 走，本 mark 仅作 audit 标记 / count 信号。
                  parsed[markKey] = ts;
                  window.localStorage.setItem(KEY, JSON.stringify(parsed));
                  // 同步写 sibling key `pet-chatmini-mark-texts`：承载 text
                  // snippet（120 字 cap），让 view-marks popover (iter #417)
                  // 渲实际内容而非裸 timestamp。Schema：Record<markKey, text>。
                  // 与 markedMessages 解耦 — PanelChat 不读这条 key，不影响
                  // 既有渲染路径。
                  const TEXTS_KEY = "pet-chatmini-mark-texts";
                  let texts: Record<string, string> = {};
                  try {
                    const trawTexts = window.localStorage.getItem(TEXTS_KEY);
                    if (trawTexts) {
                      const got = JSON.parse(trawTexts);
                      if (got && typeof got === "object" && !Array.isArray(got)) {
                        texts = got;
                      }
                    }
                  } catch {
                    // 解析失败 → 覆盖写
                  }
                  texts[markKey] = text.length > 120
                    ? text.slice(0, 120) + "…"
                    : text;
                  try {
                    window.localStorage.setItem(
                      TEXTS_KEY,
                      JSON.stringify(texts),
                    );
                  } catch {
                    // quota full → markedMessages 已写成功，text 失败不致命
                  }
                  // 反馈：复用 copyToast 通道（与 📋 同视觉），1.5s 自清
                  setCopyToast("done");
                  window.setTimeout(() => setCopyToast("none"), 1500);
                  // 同步 marksRefreshTrigger 让 popover state 即时更新
                  setMarksRefreshTrigger((v) => v + 1);
                  console.info(
                    `[ChatMini] 📌 marked selection (${text.length} chars):`,
                    text.slice(0, 60),
                  );
                } catch (e) {
                  console.error("mark selection failed:", e);
                  setCopyToast("err");
                  window.setTimeout(() => setCopyToast("none"), 1500);
                }
              }}
            >
              📌
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
      {/* ⌘` 弹 transient_note popover：fixed 居中浮窗 + textarea + 4 个时
          长按钮。owner 写文本 → 选时长 → 一键挂 transient_note 给 pet
          临时上下文。Esc / 点 outside / 关按钮 都关；写时 Enter 是换行
          （文本可能多行），⌘Enter 用默认 60m 提交。 */}
      {transientPopoverOpen && onSetTransientNote && (
        <div
          onMouseDown={(e) => {
            // outside-click 关：内部 mousedown stopPropagation 防自关
            if (e.target === e.currentTarget) {
              setTransientPopoverOpen(false);
            }
          }}
          style={{
            position: "fixed",
            inset: 0,
            zIndex: 100000,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            background: "color-mix(in srgb, var(--pet-color-bg) 40%, transparent)",
          }}
        >
          <div
            onMouseDown={(e) => e.stopPropagation()}
            style={{
              minWidth: 320,
              maxWidth: 420,
              padding: 12,
              border: "1px solid var(--pet-color-border)",
              borderRadius: 8,
              background: "var(--pet-color-card)",
              boxShadow: "var(--pet-shadow-md)",
              display: "flex",
              flexDirection: "column",
              gap: 8,
            }}
          >
            <div
              style={{
                fontSize: 12,
                fontWeight: 600,
                color: "var(--pet-color-fg)",
                display: "flex",
                alignItems: "center",
                gap: 6,
              }}
            >
              📝 设 transient_note
              <span
                style={{
                  fontSize: 10,
                  color: "var(--pet-color-muted)",
                  fontWeight: 400,
                }}
              >
                · 给 pet 写一段临时上下文（不发消息）
              </span>
            </div>
            <textarea
              ref={transientPopoverInputRef}
              value={transientPopoverDraft}
              onChange={(e) => setTransientPopoverDraft(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Escape") {
                  e.preventDefault();
                  setTransientPopoverOpen(false);
                  return;
                }
                if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
                  e.preventDefault();
                  submitTransientPopover(60);
                }
              }}
              placeholder="比如：在开会，半小时别打扰 / 集中写文档别活泼 / 今晚 9 点后再 ping 我"
              rows={3}
              style={{
                padding: "6px 8px",
                fontSize: 12,
                lineHeight: 1.5,
                border: "1px solid var(--pet-color-border)",
                borderRadius: 4,
                background: "var(--pet-color-bg)",
                color: "var(--pet-color-fg)",
                fontFamily: "inherit",
                resize: "vertical",
                outline: "none",
              }}
            />
            <div
              style={{
                display: "flex",
                gap: 6,
                alignItems: "center",
                flexWrap: "wrap",
              }}
            >
              <span
                style={{
                  fontSize: 11,
                  color: "var(--pet-color-muted)",
                  marginRight: 4,
                }}
              >
                时长：
              </span>
              {[
                { mins: 30, label: "30m" },
                { mins: 60, label: "1h" },
                { mins: 120, label: "2h" },
                { mins: 360, label: "6h" },
              ].map(({ mins, label }) => {
                const disabled = !transientPopoverDraft.trim();
                return (
                  <button
                    key={mins}
                    type="button"
                    onClick={() => submitTransientPopover(mins)}
                    disabled={disabled}
                    title={
                      disabled
                        ? "请先在上方输入框写 transient_note 文本"
                        : `挂 ${label} 后自动清除（复用既有 set_transient_note 后端）`
                    }
                    style={{
                      padding: "4px 10px",
                      fontSize: 11,
                      border: "1px solid var(--pet-color-border)",
                      borderRadius: 4,
                      background: disabled
                        ? "var(--pet-color-bg)"
                        : "var(--pet-tint-blue-bg)",
                      color: disabled
                        ? "var(--pet-color-muted)"
                        : "var(--pet-tint-blue-fg)",
                      cursor: disabled ? "default" : "pointer",
                      fontFamily: "inherit",
                      fontWeight: 600,
                      opacity: disabled ? 0.5 : 1,
                    }}
                  >
                    {label}
                  </button>
                );
              })}
              <span style={{ flex: 1 }} />
              <button
                type="button"
                onClick={() => setTransientPopoverOpen(false)}
                title="关闭（Esc）"
                style={{
                  padding: "4px 10px",
                  fontSize: 11,
                  border: "1px solid var(--pet-color-border)",
                  borderRadius: 4,
                  background: "var(--pet-color-card)",
                  color: "var(--pet-color-muted)",
                  cursor: "pointer",
                  fontFamily: "inherit",
                }}
              >
                ✕
              </button>
            </div>
            <div
              style={{
                fontSize: 10,
                color: "var(--pet-color-muted)",
                lineHeight: 1.4,
              }}
            >
              快捷键：⌘` 切换浮窗 · ⌘Enter 提交（默认 1h） · Esc 关
            </div>
          </div>
        </div>
      )}
    </>
  );
}
