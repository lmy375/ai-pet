import { useEffect, useRef, useState, useCallback } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";
import { Live2DCharacter } from "./components/Live2DCharacter";
import { ChatMini } from "./components/ChatMini";
import { ChatPanel } from "./components/ChatPanel";
import { useChat } from "./hooks/useChat";
import { useAutoHide } from "./hooks/useAutoHide";
import { useSettings } from "./hooks/useSettings";
import { usePollingState } from "./hooks/usePollingState";
import { useThemeChangeSync } from "./hooks/useThemeChangeSync";
import { useMoodAnimation, playPetMotion } from "./hooks/useMoodAnimation";
import { applyTheme, getStoredTheme, getStoredAccent, setStoredTheme } from "./theme";
import { extractText } from "./utils/messageContent";
import {
  formatImageHelpText,
  parseSlashCommand,
} from "./components/panel/slashCommands";

// 应用 CSS 变量到桌面宠物窗口的 document.documentElement。PanelApp 在
// 它自己的 window 已 applyTheme，但桌面宠物窗口是独立 webview，没人替
// 它落 token —— 导致 ChatMini 用 var(--pet-color-accent) 渲染 user 气泡
// 时拿到空字符串 + 白字看不清。模块加载时一次性 apply（与 PanelApp
// useState initializer 同模式）。
applyTheme(getStoredTheme(), getStoredAccent());

interface CurrentMood {
  text: string;
  motion: string | null;
  raw: string;
}

const MOOD_GLYPH: Record<string, string> = {
  Tap: "😊",
  Flick: "✨",
  Flick3: "💢",
  Idle: "💤",
};

/// motion → 颜色（与 PanelPersona MOTION_META 同款配色，但本地不复用，
/// 避免桌面层 import 进 panel-only 文件造成 bundle 偶联）。
const MOOD_COLOR: Record<string, string> = {
  Tap: "#ec4899",
  Flick: "#f59e0b",
  Flick3: "#ea580c",
  Idle: "#64748b",
};
const MOOD_COLOR_FALLBACK = "#cbd5e1";

/// 与 PanelPersona DailyMotion 同形：后端 get_mood_daily_motions 的 JSON 输出。
interface DailyMotionPayload {
  date: string;
  motions: Record<string, number>;
  total: number;
}

/// 从 motions map 找占比最大的 motion key。空 map → null。同票时 keys 排
/// 序后取第一个让结果稳定（不依赖 JS 对象迭代顺序）。
function topMotion(motions: Record<string, number>): string | null {
  let best: string | null = null;
  let bestN = -1;
  // 稳定排序保证同票决定性
  const keys = Object.keys(motions).sort();
  for (const k of keys) {
    if (motions[k] > bestN) {
      bestN = motions[k];
      best = k;
    }
  }
  return best;
}

/// 桌面 Live2D 区右下角的心情展示位。轮询 get_current_mood 每 5s（与
/// PanelPersona 同节奏），有心情才渲染（空 / 未记录直接 null）。
/// 顺带留一条最近 6 条心情快照的 ring buffer，hover 主气泡时浮上方迷你 chart
/// 让用户感受心情曲线（脸色变化历程，不必等下次 panel 翻 persona）。
const MOOD_HISTORY_MAX = 6;
interface MoodSnapshot {
  glyph: string;
  text: string;
  motion: string | null;
  ts: number; // 采样的本地 ms
}
function moodSnapshotKey(m: CurrentMood): string {
  // motion + text 组合做去重 key。同一心情连 5s 轮询多次只入一条 —— 用户
  // 想看"变化曲线"而非"无聊重复"。
  return `${m.motion ?? "_"}|${m.text}`;
}
function formatMoodElapsed(ms: number, nowMs: number): string {
  const diffSec = Math.max(0, Math.round((nowMs - ms) / 1000));
  if (diffSec < 60) return `${diffSec}s 前`;
  const min = Math.floor(diffSec / 60);
  if (min < 60) return `${min} 分前`;
  const h = Math.floor(min / 60);
  return `${h} 小时前`;
}
function MoodWidget() {
  const [mood, setMood] = useState<CurrentMood | null>(null);
  const [history, setHistory] = useState<MoodSnapshot[]>([]);
  const [historyVisible, setHistoryVisible] = useState(false);
  /// 双击 widget 浮"最近 7 天心情" sparkline 浮窗。轻量 IPC：仅在首次打开
  /// 时拉一次 get_mood_daily_motions(days=7)；关闭后保留缓存，再次打开走
  /// 缓存秒开。loadingDaily 防 race 重复 fetch；errorDaily 显简短失败提示。
  const [sparklineOpen, setSparklineOpen] = useState(false);
  const [daily7, setDaily7] = useState<DailyMotionPayload[] | null>(null);
  const [loadingDaily, setLoadingDaily] = useState(false);
  const sparklineFetchOnceRef = useRef(false);
  useEffect(() => {
    if (!sparklineOpen) return;
    if (sparklineFetchOnceRef.current) return;
    sparklineFetchOnceRef.current = true;
    setLoadingDaily(true);
    invoke<DailyMotionPayload[]>("get_mood_daily_motions", { days: 7 })
      .then((arr) => setDaily7(Array.isArray(arr) ? arr : []))
      .catch((e) => {
        console.error("get_mood_daily_motions failed:", e);
        setDaily7([]);
      })
      .finally(() => setLoadingDaily(false));
  }, [sparklineOpen]);
  /// 点窗外 / 按 Esc 关闭。mousedown 而非 click 让"按下即关"跟手；popover
  /// 内 stopPropagation 防自关。
  useEffect(() => {
    if (!sparklineOpen) return;
    const onDoc = () => setSparklineOpen(false);
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setSparklineOpen(false);
    };
    window.addEventListener("mousedown", onDoc);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", onDoc);
      window.removeEventListener("keydown", onKey);
    };
  }, [sparklineOpen]);
  // nowMs 慢节奏 tick（10s）让 hover tooltip 里的"X 分前"自然刷新，无需
  // 重订 polling 频率。
  const [nowMs, setNowMs] = useState(() => Date.now());
  useEffect(() => {
    const id = window.setInterval(() => setNowMs(Date.now()), 10_000);
    return () => window.clearInterval(id);
  }, []);
  useEffect(() => {
    let cancelled = false;
    const fetchMood = async () => {
      try {
        const m = await invoke<CurrentMood>("get_current_mood");
        if (cancelled) return;
        setMood(m);
        if (!m.text.trim() && !m.motion) return;
        // ring buffer push：与上一条同 motion+text 不入；满 6 条丢最早。
        setHistory((prev) => {
          const last = prev[prev.length - 1];
          if (last && moodSnapshotKey(m) === `${last.motion ?? "_"}|${last.text}`) {
            return prev;
          }
          const glyph = m.motion ? MOOD_GLYPH[m.motion] ?? "🐾" : "🐾";
          const next = [
            ...prev,
            { glyph, text: m.text, motion: m.motion, ts: Date.now() },
          ];
          return next.length > MOOD_HISTORY_MAX
            ? next.slice(next.length - MOOD_HISTORY_MAX)
            : next;
        });
      } catch (e) {
        console.error("get_current_mood failed:", e);
      }
    };
    void fetchMood();
    const id = window.setInterval(fetchMood, 5000);
    return () => {
      cancelled = true;
      window.clearInterval(id);
    };
  }, []);
  if (!mood || (!mood.text.trim() && !mood.motion)) return null;
  const glyph = mood.motion ? MOOD_GLYPH[mood.motion] ?? "🐾" : "🐾";
  const text = mood.text.length > 24 ? mood.text.slice(0, 24) + "…" : mood.text;
  // 历史去掉最末一条（与当前 mood 等价）让 chart 真显"过去的脸色"。
  // length <= 1 时不渲染 chart（要么没历史，要么仅有当前一条）。
  const past = history.length > 1 ? history.slice(0, -1) : [];
  return (
    <div
      onMouseEnter={() => setHistoryVisible(true)}
      onMouseLeave={() => setHistoryVisible(false)}
      style={{
        position: "absolute",
        bottom: "8px",
        left: "8px",
        maxWidth: "calc(100% - 16px)",
        userSelect: "none",
        zIndex: 5,
      }}
    >
      {/* 心情历史 mini chart：仅 hover 主气泡时浮出。past 倒序渲染（最新在
          右、最早在左 → 与"时间轴自左向右流动"直觉一致；最右 fade-in 提示
          "刚发生"）。一行 emoji 圆点 + 颜色更柔，避免抢主气泡视觉。 */}
      {historyVisible && past.length > 0 && (
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 3,
            marginBottom: 4,
            padding: "2px 8px",
            borderRadius: 12,
            background: "var(--pet-color-card)",
            border: "1px solid var(--pet-color-border)",
            boxShadow: "var(--pet-shadow-sm)",
            fontSize: 10,
            color: "var(--pet-color-muted)",
            whiteSpace: "nowrap",
          }}
        >
          <span style={{ marginRight: 2 }} title="最近心情变化（从左到右越新）">
            最近
          </span>
          {past.map((snap, i) => (
            <span
              key={`${snap.ts}-${i}`}
              title={`${formatMoodElapsed(snap.ts, nowMs)}：${snap.text}${
                snap.motion ? `（${snap.motion}）` : ""
              }`}
              style={{
                opacity: 0.5 + (0.5 * (i + 1)) / past.length,
                fontSize: 12,
                lineHeight: 1,
              }}
            >
              {snap.glyph}
            </span>
          ))}
        </div>
      )}
      {/* 「最近 7 天心情」双击浮窗：每天一个 dot，色取自当日 top motion 的
          MOOD_COLOR；opacity 跟 total 缩放（多记录 = 实色，0 记录 = 灰透）；
          hover 单 dot 显日期 + motions 分布；空数据兜底"还没攒到 7 天历史"。
          mousedown stopPropagation 阻自关；窗外 click / Esc 由 effect 监听。 */}
      {sparklineOpen && (
        <div
          onMouseDown={(e) => e.stopPropagation()}
          onClick={(e) => e.stopPropagation()}
          style={{
            position: "absolute",
            bottom: "calc(100% + 6px)",
            left: 0,
            minWidth: 200,
            padding: "8px 12px",
            background: "var(--pet-color-card)",
            border: "1px solid var(--pet-color-border)",
            borderRadius: 10,
            boxShadow: "var(--pet-shadow-md)",
            whiteSpace: "nowrap",
            zIndex: 80,
          }}
        >
          <div
            style={{
              fontSize: 10,
              color: "var(--pet-color-muted)",
              marginBottom: 6,
              letterSpacing: 0.3,
            }}
          >
            最近 7 天心情（最旧 ← → 最新）
          </div>
          {loadingDaily && (
            <div style={{ fontSize: 10, color: "var(--pet-color-muted)" }}>
              加载中…
            </div>
          )}
          {!loadingDaily && daily7 !== null && daily7.length === 0 && (
            <div style={{ fontSize: 10, color: "var(--pet-color-muted)" }}>
              还没攒到 7 天心情历史；先一起聊聊。
            </div>
          )}
          {!loadingDaily && daily7 && daily7.length > 0 && (() => {
            const maxTotal = Math.max(1, ...daily7.map((d) => d.total));
            return (
              <div style={{ display: "flex", gap: 8, alignItems: "flex-end" }}>
                {daily7.map((d) => {
                  const top = topMotion(d.motions);
                  const color =
                    top && MOOD_COLOR[top]
                      ? MOOD_COLOR[top]
                      : MOOD_COLOR_FALLBACK;
                  // opacity 0.25 起步让"0 记录的日子"也可见但很淡；满记录 1.0
                  const opacity =
                    d.total === 0 ? 0.25 : 0.45 + (d.total / maxTotal) * 0.55;
                  // size 也按 total 缩 8..14，让"重磅日"更显眼
                  const size = d.total === 0 ? 8 : 8 + Math.min(6, d.total);
                  // tooltip：日期 + 各 motion 分布；按计数降序
                  const breakdown = Object.entries(d.motions)
                    .sort((a, b) => b[1] - a[1])
                    .map(([k, v]) => `${MOOD_GLYPH[k] ?? k}×${v}`)
                    .join(" ");
                  const dateLabel = d.date.slice(5); // MM-DD
                  return (
                    <div
                      key={d.date}
                      style={{
                        display: "flex",
                        flexDirection: "column",
                        alignItems: "center",
                        gap: 3,
                      }}
                      title={
                        d.total === 0
                          ? `${d.date}：无心情记录`
                          : `${d.date}：${breakdown}`
                      }
                    >
                      <div
                        style={{
                          width: size,
                          height: size,
                          borderRadius: "50%",
                          background: color,
                          opacity,
                          transition: "transform 120ms ease-out",
                        }}
                      />
                      <span
                        style={{
                          fontSize: 9,
                          color: "var(--pet-color-muted)",
                          fontFamily: "'SF Mono', 'Menlo', monospace",
                        }}
                      >
                        {dateLabel}
                      </span>
                    </div>
                  );
                })}
              </div>
            );
          })()}
        </div>
      )}
      <div
        title={`当前心情：${mood.text}${mood.motion ? `（${mood.motion}）` : ""}\n（hover 看最近 ${MOOD_HISTORY_MAX} 条变化；双击展开 7 天浮窗）`}
        onDoubleClick={(e) => {
          // 防"双击 widget = 双击 Live2D = happy motion"双触发：自身 stopPropagation。
          e.stopPropagation();
          setSparklineOpen((v) => !v);
        }}
        onMouseDown={(e) => {
          // 防 widget 上 mousedown 冒泡触发 window onDoc → 自关。stop 让 effect 里
          // 的 onDoc 不命中本 widget。historyVisible hover 不挡。
          e.stopPropagation();
        }}
        style={{
          display: "flex",
          alignItems: "center",
          gap: 6,
          padding: "4px 10px",
          borderRadius: 16,
          background: "var(--pet-color-card)",
          border: "1px solid var(--pet-color-border)",
          boxShadow: "var(--pet-shadow-sm)",
          fontSize: 11,
          color: "var(--pet-color-muted)",
          cursor: "pointer",
        }}
      >
        <span style={{ fontSize: 14, lineHeight: 1 }}>{glyph}</span>
        {text && (
          <span style={{ whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>
            {text}
          </span>
        )}
      </div>
    </div>
  );
}

/// 任务完成 sparkle 飘飞粒子表。手工排出 6 颗的 top/left/dx/rot/delay/size，
/// 让弧线感漂亮而非纯随机散点。各粒子独立 animation-delay 形成涟漪式
/// 涌现，1.5s 内全部 fade out。glyph 取自 emoji ✨ / ⭐ / 🌟 三种，让视觉
/// 不机械重复。
const SPARKLE_PARTICLES: Array<{
  top: string;
  left: string;
  dx: string;
  rot: string;
  delay: number;
  size: number;
  glyph: string;
}> = [
  { top: "62%", left: "32%", dx: "-14px", rot: "-12deg", delay: 0,   size: 22, glyph: "✨" },
  { top: "55%", left: "68%", dx: "14px",  rot: "12deg",  delay: 80,  size: 22, glyph: "✨" },
  { top: "44%", left: "48%", dx: "-2px",  rot: "0deg",   delay: 160, size: 26, glyph: "🌟" },
  { top: "58%", left: "18%", dx: "-22px", rot: "-18deg", delay: 240, size: 18, glyph: "⭐" },
  { top: "50%", left: "82%", dx: "22px",  rot: "18deg",  delay: 320, size: 18, glyph: "⭐" },
  { top: "36%", left: "62%", dx: "8px",   rot: "6deg",   delay: 400, size: 20, glyph: "✨" },
];

function App() {
  const { settings, soul, loaded } = useSettings();
  const { messages, currentResponse, toolStatus, isLoading, sendMessage, cancel, appendAssistant, resetContext } = useChat(soul);
  const modelRef = useRef<any>(null);
  const { hidden, handleMouseEnter, collapse } = useAutoHide();
  // 把 settings.motion_mapping 传给动画 hook，让用户在「设置」改了映射立即
  // 生效（hook 内部用 ref 跟随，无需重订阅 listen）。
  useMoodAnimation(modelRef, settings.motion_mapping);

  // hidden 期间的 proactive 消息计数：用于左侧 tab indicator 角标。
  // 用 ref + setState 同步：listener 在 useEffect 里挂一次，需要拿到最新
  // hidden 值而不要每次重订阅。Clear 在 hidden→false 时（用户已经回到桌面）。
  const hiddenRef = useRef(hidden);
  useEffect(() => {
    hiddenRef.current = hidden;
  }, [hidden]);
  const [unreadWhileHidden, setUnreadWhileHidden] = useState(0);
  // 拖图进 pet 窗口（非 ChatPanel 输入区）时显短暂 toast 让用户立刻看到
  // "图已收"。光靠 ChatPanel 的 pendingImages 缩略图条不够直观 —— 那个
  // 条贴在窗口底部，用户从 Finder 拖到 Live2D 区时眼睛在顶部，根本看不见
  // 反馈。toast 浮在顶部，1.8s 自清。
  const [imageDropToast, setImageDropToast] = useState<number>(0);
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    (async () => {
      unlisten = await listen("proactive-message", () => {
        if (hiddenRef.current) {
          setUnreadWhileHidden((n) => n + 1);
        }
      });
    })();
    return () => {
      if (unlisten) unlisten();
    };
  }, []);
  useEffect(() => {
    if (!hidden) setUnreadWhileHidden(0);
  }, [hidden]);

  // 当前 active 的 NOW 标记 → 过期 ms 时戳。pet 端独立持有副本（与
  // PanelTasks 的 Set 跨窗口），让 ChatMini hover 可显出当前专注队列。
  const [nowTasks, setNowTasks] = useState<Map<string, number>>(new Map());
  /// pet 窗顶部 pill 数据源：逾期 + 今日完成两个维度。走后端 task_stats 单
  /// SoT（同函数也供桌面 /stats / PanelDebug strip）。60s 轮询（panel 30s，
  /// pet 窗常驻 → 稀疏一点省 IPC 噪声）。失败保持上次值不闪 0（hook 默认行为）。
  /// initial = {0, 0} 让首帧 pill 不挂（!hasOverdue && !hasDone → 不渲染分支）。
  const { data: taskStats } = usePollingState(
    () =>
      invoke<{ overdue: number; done_today: number; snoozed: number }>(
        "task_stats",
      ),
    60_000,
    { overdue: 0, done_today: 0, snoozed: 0 },
  );
  /// 陪伴天数：桌面 Live2D 区右上角 ✦ N chip 数据源。day-granular，10 min
  /// 轮询足够（midnight 跨日 ≤ 10min 内更新）。`-1` 兜底"未 fetch / 失败"
  /// → chip 不渲染（避免空 chip 占位）。
  const { data: companionshipDays } = usePollingState(
    () => invoke<number>("get_companionship_days"),
    600_000,
    -1,
  );

  /// 今天宠物主动开口次数。60s 轮一次（与 chip 顶部其它 ambient 数同节奏）。
  /// > 0 时桌面 Live2D 区右上 ✦ 陪伴天数 chip 左侧渲一个 🐾 chip，让 owner
  /// 一眼看到"今天宠物来找我了多少次"。`-1` fallback 兜未抓到 → chip 不显
  /// （避免空 chip 占视觉位）。轮询节奏与 companionshipDays 同（10min）—
  /// proactive 一天最多 ~10 次，60s 轮太频繁；600s 足以让 chip 跟得上变化。
  const { data: todaySpeechCount } = usePollingState(
    () => invoke<number>("get_today_speech_count"),
    600_000,
    -1,
  );

  /// 今日 mute proactive 累计次数。10 分钟轮一次（mute 是用户低频操作，
  /// 不必更密）。> 0 时桌面 Live2D 区右上 🐾 chip 右侧渲一个 🔕 N chip，
  /// 让 owner 一眼看到"今天我让宠物闭嘴几次" — 自我 audit 是否过度打断。
  /// `-1` fallback 兜未抓到 → chip 不显。
  const { data: todayMuteCount } = usePollingState(
    () => invoke<number>("get_today_mute_count"),
    600_000,
    -1,
  );

  /// 当前 session 上下文 token 量。60 秒轮一次（变化频率与 user 聊天节奏一致；
  /// 更频会浪费 IPC，更稀疏会漏报"刚刚才聊几句就过线"）。> 4000 时 ChatMini
  /// 顶部浮出"该 /reset 一下" chip，与 DebugApp 统计 tab 同源信号 +
  /// SESSION_TOKEN_WARN_THRESHOLD 同阈值。fallback 0 = 未抓到不显 chip。
  const { data: sessionTokens } = usePollingState(
    async () => {
      try {
        const stats = await invoke<{ tokens: number }>(
          "get_active_session_context_stats",
        );
        return stats.tokens;
      } catch (e) {
        // 偶发 IPC 异常静默兜 0 = chip 不显，不打扰用户操作流。
        console.error("get_active_session_context_stats failed:", e);
        return 0;
      }
    },
    60_000,
    0,
  );

  /// pet 主区 hover 3s 浮 ambient 微卡片：聚合「今日 / 本周 / 累计」三段
  /// 主动开口数 + ✦ 陪伴天数 一瞥全景。3s = 让"路过 cursor 经过"不触发
  /// （区分"真的停在 pet 上端详"）。lazy fetch：只在 3s 计时器到点才发
  /// 3 个 IPC（首次显卡片可能闪 ~50ms 等三命令返回，但避免 owner 从不
  /// hover 的人白白持续轮询）。mouseleave 立刻清，下次 hover 重新启 3s
  /// 重新发请求 = 数据始终新鲜。pointerEvents none 让卡片不接 hover，
  /// 移到卡片上 cursor 实际仍在 wrapper 内 → 不触发 mouseleave。
  const [ambientStats, setAmbientStats] = useState<
    { today: number; week: number; lifetime: number } | null
  >(null);
  const petHoverTimerRef = useRef<number | null>(null);
  const handlePetAmbientEnter = useCallback(() => {
    if (petHoverTimerRef.current !== null) return;
    if (ambientStats !== null) return;
    petHoverTimerRef.current = window.setTimeout(async () => {
      petHoverTimerRef.current = null;
      try {
        const [today, week, lifetime] = await Promise.all([
          invoke<number>("get_today_speech_count"),
          invoke<number>("get_week_speech_count"),
          invoke<number>("get_lifetime_speech_count"),
        ]);
        setAmbientStats({ today, week, lifetime });
      } catch (e) {
        console.error("ambient stats fetch failed:", e);
      }
    }, 3000);
  }, [ambientStats]);
  const handlePetAmbientLeave = useCallback(() => {
    if (petHoverTimerRef.current !== null) {
      window.clearTimeout(petHoverTimerRef.current);
      petHoverTimerRef.current = null;
    }
    setAmbientStats(null);
  }, []);
  useEffect(
    () => () => {
      if (petHoverTimerRef.current !== null) {
        window.clearTimeout(petHoverTimerRef.current);
      }
    },
    [],
  );

  /// 任务完成庆祝 sparkle：done_today 计数从 prev → cur 单调上升时，触发
  /// pet 右键 "⏰ 设倒计时 N 分" 启动的 timeout id 集合。每次 ctx menu 点击
  /// 加一个 setTimeout；到点 push 一条 appendAssistant 软提醒；unmount 时
  /// 清全部防 timer 漏。Set 而非 Map：timer id 是 caller 唯一标识，cancel
  /// 在本 iter 范围外（owner 想取消可重启 pet 或忽略）；future iter 加显式
  /// 取消 UI 时再 lift 到 Map<id, {minutes, startedAt}>。
  const countdownTimersRef = useRef<Set<number>>(new Set());
  useEffect(
    () => () => {
      for (const tid of countdownTimersRef.current) {
        window.clearTimeout(tid);
      }
      countdownTimersRef.current.clear();
    },
    [],
  );
  const startCountdownNudge = useCallback(
    (minutes: number) => {
      if (minutes <= 0) return;
      appendAssistant(`⏰ 已设 ${minutes} 分倒计时（到点会浮一条提醒）`);
      const id = window.setTimeout(
        () => {
          appendAssistant(`⏰ ${minutes} 分倒计时到了 — 该回来看看了 🐾`);
          countdownTimersRef.current.delete(id);
        },
        minutes * 60_000,
      );
      countdownTimersRef.current.add(id);
    },
    [appendAssistant],
  );

  /// butler_task `[reminderMin: N]` 软提醒：60s 轮询 butler_tasks，找到点
  /// 前 N 分钟内未触发过的项 → appendAssistant 软提醒（不打开 Live2D 主动
  /// 模式）。dedup Set 用 `${title}::${fireTimeIso}` —— 同 fire-cycle 同 task
  /// 只触发一次；every 类型跨日产生新 fireTimeIso 自动允许下次。Set 仅活在
  /// 进程内（重启后 fresh）—— 重启就重新提醒一次也是可接受的（owner 重启
  /// 频率低 + 重启时多半正在用电脑）。
  const reminderFiredRef = useRef<Set<string>>(new Set());
  useEffect(() => {
    const tick = async () => {
      try {
        const tasks = await invoke<
          Array<{ title: string; description: string; status: string }>
        >("db_butler_tasks_list");
        // pending 才提醒 —— done / error / cancelled 任务 fire 时刻已过 / 无意义
        const pending = tasks.filter((t) => t.status === "pending");
        const { findRemindersToFire } = await import("./utils/butlerReminder");
        const toFire = findRemindersToFire(
          pending,
          new Date(),
          reminderFiredRef.current,
        );
        for (const r of toFire) {
          reminderFiredRef.current.add(r.dedupKey);
          const remainMin = Math.max(
            1,
            Math.round((new Date(r.fireTimeIso).getTime() - Date.now()) / 60_000),
          );
          appendAssistant(
            `🔔 提醒：「${r.title}」将在约 ${remainMin} 分钟后到点（reminderMin=${r.reminderMin}）`,
          );
        }
        // GC：dedup Set 不会无限增长 —— 一天 every 类型最多多一个 key；
        // once / deadline 一旦 fire 过 fireTime 就不再命中。但为了保险，
        // 若 Set 超过 200 项时清空（极端 ages-running pet 时止血）。
        if (reminderFiredRef.current.size > 200) {
          reminderFiredRef.current.clear();
        }
      } catch (e) {
        console.error("butler reminder tick failed:", e);
      }
    };
    void tick();
    const id = window.setInterval(() => void tick(), 60_000);
    return () => window.clearInterval(id);
  }, [appendAssistant]);

  /// 一次 ~1.5s ✨ 飘飞动画覆盖 Live2D 区。首次观测仅作 baseline 不点燃
  /// （初始 0 → 真实 N 不是"刚完成"信号）；午夜回到 0 也不触发（cur > prev
  /// 条件兜住）。多次连发用 key 自增让 React remount 动画从头跑。
  const lastDoneTodayRef = useRef<number | null>(null);
  const [sparkleKey, setSparkleKey] = useState(0);
  useEffect(() => {
    const cur = taskStats.done_today;
    const prev = lastDoneTodayRef.current;
    lastDoneTodayRef.current = cur;
    if (prev === null) return;
    if (cur > prev) {
      setSparkleKey((k) => k + 1);
    }
  }, [taskStats.done_today]);
  // 监听 PanelTasks "⚡ 标 NOW" 事件：立刻给桌面 ChatMini 推一条 ack +
  // 60s 后再推一条 nudge 提醒。每次 mark 起独立 timer（多任务并存 OK），
  // 组件 unmount 时清。同时更新 nowTasks Map 让 ChatMini 显倒计时浮窗。
  useEffect(() => {
    const pendingTimers = new Set<number>();
    let unlistenTaskNow: (() => void) | undefined;
    (async () => {
      unlistenTaskNow = await listen<{ title?: string }>(
        "task-now-mark",
        (event) => {
          const title = event.payload?.title;
          if (!title || typeof title !== "string") return;
          appendAssistant(`✨ 标记「${title}」为 NOW · 1 分钟内 nudge 提醒`);
          const expiresAt = Date.now() + 60_000;
          setNowTasks((prev) => {
            const next = new Map(prev);
            next.set(title, expiresAt);
            return next;
          });
          const id = window.setTimeout(() => {
            appendAssistant(`⏰ 提醒：「${title}」还没动手吗？`);
            setNowTasks((prev) => {
              if (!prev.has(title)) return prev;
              const next = new Map(prev);
              next.delete(title);
              return next;
            });
            pendingTimers.delete(id);
          }, 60_000);
          pendingTimers.add(id);
        },
      );
    })();
    return () => {
      if (unlistenTaskNow) unlistenTaskNow();
      for (const id of pendingTimers) window.clearTimeout(id);
      pendingTimers.clear();
    };
  }, [appendAssistant]);

  // 桌面 pet 窗口的 window-level 图片拖入。ChatPanel 内的 onDrop 已经处理
  // 落在输入框附近的 drop；但 Live2D 大区 / ChatMini 历史区不挂监听 → 用户
  // 把图从 Finder 拖到宠物身上会被浏览器默认行为打开（直接导航到 file://
  // URL）。这里在 window 层兜底：拦 dragover preventDefault 让 drop 能触
  // 发，drop 时校验是 image 文件后 FileReader → CustomEvent 推到 ChatPanel
  // 的 pendingImages。
  //
  // ChatPanel 内 onDrop 已经 preventDefault，所以这里读 e.defaultPrevented
  // 守门 —— 避免和 inner onDrop 双触发把同一张图入两次。
  useEffect(() => {
    const onDragOver = (e: DragEvent) => {
      const types = Array.from(e.dataTransfer?.types ?? []);
      if (!types.includes("Files")) return;
      e.preventDefault();
      if (e.dataTransfer) e.dataTransfer.dropEffect = "copy";
    };
    const onDrop = (e: DragEvent) => {
      if (e.defaultPrevented) return; // ChatPanel inner handler 已消化
      const types = Array.from(e.dataTransfer?.types ?? []);
      if (!types.includes("Files")) return;
      e.preventDefault();
      const files = e.dataTransfer?.files;
      if (!files || files.length === 0) return;
      const blobs: Blob[] = [];
      for (let i = 0; i < files.length; i++) {
        const f = files[i];
        if (f.type.startsWith("image/")) blobs.push(f);
      }
      if (blobs.length === 0) return;
      const urls: string[] = [];
      let done = 0;
      for (const blob of blobs) {
        const reader = new FileReader();
        reader.onload = () => {
          if (typeof reader.result === "string") urls.push(reader.result);
          done += 1;
          if (done === blobs.length) {
            window.dispatchEvent(
              new CustomEvent("pet-pending-image-drop", { detail: urls }),
            );
            // 顶部 toast 反馈：让用户知道 N 张图已收。状态写本地 ms 戳让
            // 短时间内连续 drop 时 timer 顺势更新（不会被旧 timer 提早清）。
            setImageDropToast(urls.length);
            const myTs = Date.now();
            window.setTimeout(() => {
              setImageDropToast((cur) =>
                Date.now() - myTs >= 1700 ? 0 : cur,
              );
            }, 1800);
          }
        };
        reader.readAsDataURL(blob);
      }
    };
    window.addEventListener("dragover", onDragOver);
    window.addEventListener("drop", onDrop);
    return () => {
      window.removeEventListener("dragover", onDragOver);
      window.removeEventListener("drop", onDrop);
    };
  }, []);

  // 跨窗口主题 / 强调色同步：监听 PanelApp 发的 emit。逻辑全在共享 hook
  // 里（与 DebugApp 用同一份）。
  useThemeChangeSync();

  // 👍 反馈：写 Liked 信号到 feedback_history。excerpt 取消息列表里最近一
  // 条 assistant 内容（来自 useChat.messages，含 proactive 推过来的）。
  // mini chat 里的 👍 按钮挂在最新 assistant 行，所以这里就用 messages
  // 末尾的 assistant 即可。
  const handleBubbleLike = useCallback(() => {
    const lastAssistant = [...messages].reverse().find((m) => m.role === "assistant");
    if (!lastAssistant) return;
    // assistant 实时路径 content 恒为 string，但 messages 类型已扩成多模态；
    // extractText 兼容拿文本，以免数组形态滑进来时把 JSON.stringify 形态发到后端。
    const excerpt = extractText(lastAssistant.content);
    if (!excerpt) return;
    invoke("record_bubble_liked", { excerpt }).catch(console.error);
  }, [messages]);

  const handleModelReady = useCallback((model: any) => {
    modelRef.current = model;
  }, []);

  /// 双击 Live2D 区触发"happy / 活泼"动作（Tap motion group）作为
  /// 用户互动的可见反馈，符合 GOAL「UI 美观可爱」+「自我进化」的陪伴感。
  /// 600ms cooldown 防连点刷动画 —— 短于 mood 自然 motion 的最低节奏，
  /// 长到给一次动作播完的时间。settings.motion_mapping 仍生效，让用户
  /// 自定义模型也能命中 happy 等价的 group 名。
  const lastTapAtRef = useRef<number>(0);
  /// 双击 happy 后偶尔（30%）push 一句鼓励 line 到 ChatMini，让"双击 pet"
  /// 不只是动作还伴随宠物 mini reaction。30% 概率防每次双击都触发 → 过密噪
  /// 音。lines 是文学短句库，2-3 字 emoji 起头 + 7-12 字主体，与 ChatMini
  /// 既有 systemNote 风格短消息一致。
  const happyLinesRef = useRef([
    "🐾 嘿，你看到我啦 ✨",
    "✨ 摸摸我会让我心情更好的～",
    "🐾 双击我也是一种问候呢",
    "💫 看你心情不错的样子",
    "🌸 想我啦？我也想你",
    "🐾 谢谢你来打个招呼",
    "✨ 难得你这么主动找我玩",
  ]);
  const handlePetDoubleClick = useCallback(
    (e: React.MouseEvent<HTMLDivElement>) => {
      // 状态 pill / MoodWidget / 收起按钮等绝对定位子元素自身已挂 onMouseDown
      // 的 stopPropagation；这里再守一道避免点 pill 时被解读为 Live2D 双击。
      const target = e.target as HTMLElement;
      if (target?.closest?.("[data-no-pet-dblclick]")) return;
      const now = Date.now();
      if (now - lastTapAtRef.current < 600) return;
      lastTapAtRef.current = now;
      playPetMotion(modelRef.current, "Tap", settings.motion_mapping);
      // ~30% 概率 push 鼓励 line；不每次都 push 防过密噪音。
      if (Math.random() < 0.3) {
        const lines = happyLinesRef.current;
        const line = lines[Math.floor(Math.random() * lines.length)];
        appendAssistant(line);
      }
    },
    [settings.motion_mapping, appendAssistant],
  );

  /// 桌面输入路由：先 parse 看是否 slash 命令。当前桌面仅支持 /image / /image -h，
  /// 其它 slash 命令（/clear / /tasks / /sleep 等）面板专属，桌面下落到 LLM 自
  /// 然处理（让宠物用文字回应"为啥不识别这个命令"也是体验的一部分）。
  const handleSend = useCallback(
    async (msg: string, images?: string[]) => {
      const trimmed = msg.trim();
      if (trimmed.startsWith("/")) {
        const action = parseSlashCommand(trimmed);
        if (action?.kind === "imageHelp") {
          appendAssistant(formatImageHelpText());
          return;
        }
        if (action?.kind === "image") {
          // -r 引用最近 assistant：与 PanelChat 同算法（messages 倒序找首条
          // assistant，文本拼到 prompt 前）。
          let effectivePrompt = action.prompt;
          if (action.referenceLastAssistant) {
            const lastA = [...messages]
              .reverse()
              .find((m) => m.role === "assistant");
            const refText = lastA ? extractText(lastA.content).trim() : "";
            if (!refText) {
              appendAssistant(
                "⚠ /image -r：当前会话还没有 assistant 回复可引用。直接走 /image <prompt> 即可。",
              );
              return;
            }
            effectivePrompt = action.prompt
              ? `${refText}\n\n${action.prompt}`
              : refText;
          }
          // 立即给个占位反馈 —— 桌面没有 PanelChat 的 pending row 概念，直接
          // append 一条 "🎨 正在生成…"，成功 / 失败时再追加结果行。
          appendAssistant(`🎨 正在生成图片：${action.prompt} …`);
          try {
            const result = await invoke<{ urls: string[]; errors: string[] }>(
              "image_generate",
              {
                prompt: effectivePrompt,
                n: action.n,
                size: action.sizeOverride ?? undefined,
              },
            );
            const nLabel = action.n > 1 ? `（${result.urls.length}/${action.n} 张）` : "";
            const sizeLabel = action.sizeOverride ? `（${action.sizeOverride}）` : "";
            const partialNote =
              result.errors.length > 0
                ? `\n\n⚠ ${result.errors.length}/${action.n} 失败：${result.errors.join("; ")}`
                : "";
            if (result.urls.length === 0) {
              appendAssistant(
                `🎨 图片生成失败：${result.errors.join("; ") || "未知"}`,
              );
            } else {
              appendAssistant(
                `🎨 ${action.prompt}${nLabel}${sizeLabel}${partialNote}`,
                result.urls,
              );
            }
          } catch (e) {
            appendAssistant(`🎨 图片生成失败：${e}`);
          }
          return;
        }
      }
      sendMessage(msg, images);
    },
    [sendMessage, appendAssistant, messages],
  );

  const handleDrag = (e: React.MouseEvent) => {
    const tag = (e.target as HTMLElement).tagName;
    if (tag === "INPUT" || tag === "BUTTON" || tag === "TEXTAREA") return;
    e.preventDefault();
    getCurrentWindow().startDragging();
  };

  /// Live2D 区右键聚合菜单：聚合"打开 panel / 切主题 / mute 30/60 分钟 / 重启
  /// 窗口"等快捷动作，当前 pet 窗口右键没反应（仅 textarea 等输入控件有默认菜单）。
  /// 状态：null = 关闭；非 null = viewport 坐标位置。useEffect 处理外部点击 +
  /// Esc 关闭，与既有 ctxMenu 模式一致。
  const [petCtxMenu, setPetCtxMenu] = useState<{ x: number; y: number } | null>(null);
  /// "🎲 摇一摇主动开口"按钮的 armed 二次确认态。armed=true 时 3s 内再点
  /// 才真触发，否则 timer 自动还原 false。防误触绕过 proactive cooldown
  /// 把宠物搞炸。fireShakeBusy 是请求 in-flight 期间禁用按钮防双触。
  const [shakeArmed, setShakeArmed] = useState(false);
  const shakeArmedTimerRef = useRef<number | null>(null);
  const [fireShakeBusy, setFireShakeBusy] = useState(false);
  const armShake = () => {
    if (shakeArmedTimerRef.current !== null) {
      window.clearTimeout(shakeArmedTimerRef.current);
    }
    setShakeArmed(true);
    shakeArmedTimerRef.current = window.setTimeout(() => {
      setShakeArmed(false);
      shakeArmedTimerRef.current = null;
    }, 3000);
  };
  /// Esc 全局键盘快捷：触发 collapse() 把宠物滑到桌边只露 tab。替代手点右
  /// 上角 ▶| 按钮，让键盘党 / mac trackpad 用户少一次定位。让位条件：
  /// - hidden（已收起）：noop 避免反复触发
  /// - petCtxMenu 开着：让右键菜单自己的 Esc 关 menu 先（不抢键）
  /// - 输入控件聚焦：让 textarea / input / contentEditable 自己处理 Esc
  ///   （桌面 ChatPanel textarea 在编辑回复 / cancel 流式等场景有 Esc 行为）
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      if (hidden || petCtxMenu) return;
      const ae = document.activeElement;
      if (
        ae instanceof HTMLInputElement ||
        ae instanceof HTMLTextAreaElement ||
        (ae instanceof HTMLElement && ae.isContentEditable)
      ) {
        return;
      }
      e.preventDefault();
      collapse();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [hidden, petCtxMenu, collapse]);
  useEffect(() => {
    if (!petCtxMenu) return;
    const close = () => setPetCtxMenu(null);
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        close();
      }
    };
    // 用 setTimeout 0 让本次 onContextMenu 完成后再挂监听 —— 同次事件触发
    // contextmenu + 立即捕获到自身的 click 会让菜单"刚开就关"。
    const t = window.setTimeout(() => {
      window.addEventListener("mousedown", close);
      window.addEventListener("keydown", onKey);
    }, 0);
    return () => {
      window.clearTimeout(t);
      window.removeEventListener("mousedown", close);
      window.removeEventListener("keydown", onKey);
    };
  }, [petCtxMenu]);

  /// ChatMini 气泡内双击「title」ref → 跨窗口跳到 PanelTasks 该任务行。
  /// 走既有 pet-panel-deeplink localStorage 通道：tab=任务 + taskFocusTitle +
  /// ts。PanelApp 端 consumePanelDeeplink 接受 taskFocusTitle 字段 →
  /// requestFocusTask(title) 既有 pipeline。失败静默 console（既然写不进
  /// storage 至少把 panel 打开让 owner 手动找）。
  const handleMiniRefDoubleClick = useCallback((title: string) => {
    try {
      window.localStorage.setItem(
        "pet-panel-deeplink",
        JSON.stringify({ tab: "任务", taskFocusTitle: title, ts: Date.now() }),
      );
    } catch (e) {
      console.error("write pet-panel-deeplink failed:", e);
    }
    invoke("open_panel").catch(console.error);
  }, []);

  /// ChatMini 气泡 "💾 转 task" 按钮：把本条消息文本作为 task body 传过去
  /// 跨窗口写 deeplink + 开 panel + 让 PanelApp 推到 PanelTasks 弹 quickAdd
  /// modal 预填。让 owner 把宠物刚说的好内容"存为 task 防忘" 一键搞定。
  /// body 字段会被前 30 字 → title default 让 owner 进 modal 后直接 ⌘Enter
  /// 创建。
  const handleMiniSaveAsTask = useCallback((text: string) => {
    const body = text.trim();
    if (!body) return;
    try {
      window.localStorage.setItem(
        "pet-panel-deeplink",
        JSON.stringify({ tab: "任务", quickAddBody: body, ts: Date.now() }),
      );
    } catch (e) {
      console.error("write pet-panel-deeplink quickAddBody failed:", e);
    }
    invoke("open_panel").catch(console.error);
    appendAssistant("💾 已把这条消息发去 Panel → 任务面板 quickAdd 预填");
  }, [appendAssistant]);

  /// ChatMini 选区 toolbar "📝 记到 note" 按钮：把选中文字作 general
  /// memory item 存盘。title 自动按本地秒级时间生成（与 TG /note 同模板，
  /// note-YYYY-MM-DDTHH-MM-SS 唯一防撞）；description = trim 后的 text。
  /// 与 💾 转 task 互补 — task 是要做的事，note 是想记的事。
  const handleMiniSaveAsNote = useCallback(
    async (text: string) => {
      const body = text.trim();
      if (!body) return;
      const now = new Date();
      const y = now.getFullYear();
      const mo = String(now.getMonth() + 1).padStart(2, "0");
      const d = String(now.getDate()).padStart(2, "0");
      const hh = String(now.getHours()).padStart(2, "0");
      const mm = String(now.getMinutes()).padStart(2, "0");
      const ss = String(now.getSeconds()).padStart(2, "0");
      const title = `note-${y}-${mo}-${d}T${hh}-${mm}-${ss}`;
      try {
        await invoke<string>("memory_edit", {
          action: "create",
          category: "general",
          title,
          description: body,
          detailContent: null,
        });
        appendAssistant(`📝 已记到 general/${title}`);
      } catch (e) {
        appendAssistant(`📝 记 note 失败：${e}`);
      }
    },
    [appendAssistant],
  );

  const openPanel = () => {
    invoke("open_panel").catch(console.error);
    // 跨窗口"刚从桌面跳过来"信号：panel 聊天 tab 上线后给当前 session bar
    // 加一段黄底脉冲，让用户进 panel 后立刻知道"我桌面聊的是这条会话"。
    //
    // 两条路径并行：
    // 1. emit('pet-focus-from-mini')：panel 已开时即时触发（无 cold start）
    // 2. localStorage 时戳：panel 刚建（首次打开）时 listener 还没挂上，事件
    //    会丢；用 localStorage 写一个 ts，PanelChat 挂载时读到并在 3s 时窗
    //    内同样触发动画。两次最多放一次（PanelChat 读后清 key）。
    const ts = Date.now();
    try {
      window.localStorage.setItem("pet-focus-from-mini-ts", String(ts));
    } catch {
      // 私密浏览 / 配额满；fallback 仍走 emit 路径
    }
    void emit("pet-focus-from-mini", { ts }).catch(() => {
      // 事件总线失败不影响主流程（panel 已经 invoke 打开）
    });
  };

  /// ⌘O / Ctrl+O 全局打开面板快捷键：与 Esc 收起对偶 —— Esc 走，⌘O 来。
  /// 替代鼠标点 mini chat ⛶ / 底部 💬 / 右键菜单"打开面板"三条路径之一。
  /// 让位条件与 Esc 同：输入控件聚焦时让它们自己处理（textarea 中 ⌘O 可能
  /// 是浏览器默认"打开文件"行为—我们让它继续走默认；用户敲消息时不该被
  /// 抢键）。已 hidden 时仍允许触发 —— owner 可能想从 collapse 态直接打开
  /// panel；invoke("open_panel") 与 hidden 互不依赖。
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!(e.metaKey || e.ctrlKey)) return;
      if (e.shiftKey || e.altKey) return;
      if (e.key.toLowerCase() !== "o") return;
      const ae = document.activeElement;
      if (
        ae instanceof HTMLInputElement ||
        ae instanceof HTMLTextAreaElement ||
        (ae instanceof HTMLElement && ae.isContentEditable)
      ) {
        return;
      }
      e.preventDefault();
      openPanel();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [openPanel]);

  if (!loaded) return null;

  return (
    <div
      onMouseDown={handleDrag}
      onMouseEnter={handleMouseEnter}
      style={{
        width: "100%",
        height: "100vh",
        background: "transparent",
        userSelect: "none",
        position: "relative",
        overflow: "hidden",
        display: "flex",
        flexDirection: "column",
      }}
    >
      {/* Tab indicator：hidden 时左侧露出的 12px 召回条。slide-in 入场动画
          + hover widen + 箭头脉冲 + 未读角标，与既有视觉一致。 */}
      {hidden && (
        <>
          <style>{`
            @keyframes pet-tab-slide-in {
              from { left: -16px; opacity: 0; }
              to   { left: 0; opacity: 1; }
            }
            @keyframes pet-tab-arrow-bob {
              0%, 100% { transform: translateX(0); }
              50%      { transform: translateX(-2px); }
            }
            .pet-tab:hover {
              width: 22px;
            }
            .pet-tab-arrow {
              animation: pet-tab-arrow-bob 1.6s ease-in-out infinite;
            }
            .pet-tab:hover .pet-tab-arrow {
              animation-play-state: paused;
            }
          `}</style>
          <div
            className="pet-tab"
            style={{
              position: "absolute",
              left: 0,
              top: "50%",
              transform: "translateY(-50%)",
              width: "16px",
              height: "50px",
              background: "linear-gradient(180deg, #7dd3fc 0%, #38bdf8 50%, #0ea5e9 100%)",
              borderRadius: "10px 0 0 10px",
              boxShadow: "-2px 0 8px rgba(56,189,248,0.3)",
              zIndex: 50,
              cursor: "pointer",
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              animation: "pet-tab-slide-in 280ms ease-out",
              transition: "width 120ms ease-out",
            }}
          >
            <div
              className="pet-tab-arrow"
              style={{
                width: "0",
                height: "0",
                borderTop: "6px solid transparent",
                borderBottom: "6px solid transparent",
                borderRight: "6px solid rgba(255,255,255,0.8)",
              }}
            />
            {unreadWhileHidden > 0 && (
              <div
                style={{
                  position: "absolute",
                  top: "-4px",
                  right: "-4px",
                  minWidth: "14px",
                  height: "14px",
                  padding: "0 3px",
                  background: "var(--pet-tint-red-fg)",
                  color: "#fff",
                  fontSize: "10px",
                  fontWeight: 700,
                  borderRadius: "7px",
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "center",
                  border: "1.5px solid var(--pet-color-card)",
                  boxShadow: "var(--pet-shadow-sm)",
                }}
                title={`pet 在隐藏期间主动开口了 ${unreadWhileHidden} 次（mouse-enter 让 pet 回来后会自动消失）`}
              >
                {unreadWhileHidden > 9 ? "9+" : unreadWhileHidden}
              </div>
            )}
          </div>
        </>
      )}

      {/* Layout: Live2D 形象 / 聊天列表 / 输入框 三段竖直堆叠，互不重叠。
          整窗 flex column；Live2D 自身 350px 高度的 wrapper 顺势占顶部；
          ChatMini 用 flex: 1 占剩余空间；ChatPanel 自身高度紧贴底部。
          `hidden`（窗口收到桌边）时整体不渲染主体，只剩左侧召回 tab。 */}
      {!hidden && (
        <>
          {/* 拖图反馈 toast：window-level drop 命中外部区（Live2D / ChatMini）
              时弹一条顶部信息，1.8s 自清。pointerEvents none 不挡 Live2D
              交互。位置在窗口最顶居中，z 高优先于其它元素。 */}
          {imageDropToast > 0 && (
            <div
              style={{
                position: "absolute",
                top: 6,
                left: "50%",
                transform: "translateX(-50%)",
                padding: "5px 12px",
                fontSize: 11,
                fontWeight: 600,
                color: "#fff",
                background: "var(--pet-color-accent)",
                borderRadius: 14,
                boxShadow: "var(--pet-shadow-md)",
                zIndex: 60,
                pointerEvents: "none",
                whiteSpace: "nowrap",
                animation: "pet-drop-toast-fade 1.8s ease-out forwards",
              }}
            >
              📎 收到 {imageDropToast} 张图 · 输入文本后回车发送
            </div>
          )}
          {imageDropToast > 0 && (
            <style>{`
              @keyframes pet-drop-toast-fade {
                0%   { opacity: 0; transform: translate(-50%, -8px); }
                15%  { opacity: 1; transform: translate(-50%, 0); }
                80%  { opacity: 1; transform: translate(-50%, 0); }
                100% { opacity: 0; transform: translate(-50%, -4px); }
              }
            `}</style>
          )}
          {/* Live2D 区 220px：窗口默认 450px 高，给 ChatMini 留 ≥ 150px 显
              4 行左右对话；输入框 ~ 60px。260 / 150 / 60 = 470 不够，220
              / 170 / 60 = 450 平衡。
              onDoubleClick：双击空白处触发 happy motion（见 handlePetDoubleClick）。
              子级浮标（pill / MoodWidget / 收起按钮 / sparkle）渲染时不挂
              [data-no-pet-dblclick]，但它们自身的 onClick / onMouseDown 已
              stopPropagation —— 双击事件冒泡到 wrapper 时这些 hit area 自然
              不响应。 */}
          <div
            style={{ position: "relative", flexShrink: 0, height: "220px" }}
            onDoubleClick={handlePetDoubleClick}
            onMouseEnter={handlePetAmbientEnter}
            onMouseLeave={handlePetAmbientLeave}
            onContextMenu={(e) => {
              // 仅在 Live2D 主区域响应；子级浮标（pill / chip / 按钮）右键时
              // 不抢菜单，让用户对那些 hit area 仍可走系统默认 / 子元素自定义。
              const tag = (e.target as HTMLElement).tagName;
              if (tag === "BUTTON" || tag === "INPUT" || tag === "TEXTAREA") return;
              e.preventDefault();
              setPetCtxMenu({ x: e.clientX, y: e.clientY });
            }}
          >
            <Live2DCharacter
              key={settings.live_2d_model_path}
              modelPath={settings.live_2d_model_path}
              onModelReady={handleModelReady}
            />
            {/* 任务状态 pill：钉在 Live2D 区左上角（与右上 ▶| 收起钮错开）。
                显示策略：含逾期 → 红 pill（紧迫感优先），仅 done_today → 绿
                pill（庆祝），都为 0 不渲染。点击调 openPanel + deeplink：含逾
                期跳 overdue filter，仅 done 时跳 all filter 让用户回看队列。
                stopPropagation 防穿透到 Live2D 拖动。 */}
            {(taskStats.overdue > 0 ||
              taskStats.done_today > 0 ||
              taskStats.snoozed > 0) &&
              (() => {
              const overdue = taskStats.overdue;
              const done = taskStats.done_today;
              const snoozed = taskStats.snoozed;
              const overdueLabel = overdue > 99 ? "99+" : `${overdue}`;
              const doneLabel = done > 99 ? "99+" : `${done}`;
              const snoozedLabel = snoozed > 99 ? "99+" : `${snoozed}`;
              const hasOverdue = overdue > 0;
              const hasDone = done > 0;
              const hasSnoozed = snoozed > 0;
              // 拼分段：≥ 2 段时只显简短 emoji + 数字；单段时附"逾期 /
              // 今日完成 / 暂停" 后缀让初见也能读懂。tint 按紧迫度优先：
              // 红（逾期）> 绿（完成）> 蓝（暂停）。
              const segments: string[] = [];
              if (hasOverdue) segments.push(`🔴 ${overdueLabel}`);
              if (hasDone) segments.push(`✓ ${doneLabel}`);
              if (hasSnoozed) segments.push(`💤 ${snoozedLabel}`);
              let text = segments.join(" · ");
              if (segments.length === 1) {
                text = hasOverdue
                  ? `🔴 ${overdueLabel} 逾期`
                  : hasDone
                    ? `✓ ${doneLabel} 今日完成`
                    : `💤 ${snoozedLabel} 暂停`;
              }
              const tipParts: string[] = [];
              if (hasOverdue) tipParts.push(`${overdue} 条任务已过期`);
              if (hasDone) tipParts.push(`今日完成 ${done} 条`);
              if (hasSnoozed) tipParts.push(`${snoozed} 条暂停中`);
              const tooltip = `${tipParts.join(" · ")} · 点开「任务」tab`;
              const tint: "red" | "green" | "blue" = hasOverdue
                ? "red"
                : hasDone
                  ? "green"
                  : "blue";
              return (
                <div
                  onClick={(e) => {
                    e.stopPropagation();
                    // deeplink：含逾期 → overdue filter；仅 done → all filter
                    // 让用户自由看队列。ts 是 TTL 戳，PanelApp 仅认 10s 内的。
                    try {
                      localStorage.setItem(
                        "pet-panel-deeplink",
                        JSON.stringify({
                          tab: "任务",
                          dueFilter: hasOverdue ? "overdue" : "all",
                          ts: Date.now(),
                        }),
                      );
                    } catch {
                      // localStorage 不可用 → 至少打开面板
                    }
                    openPanel();
                  }}
                  onMouseDown={(e) => e.stopPropagation()}
                  title={tooltip}
                  style={{
                    position: "absolute",
                    top: "8px",
                    left: "8px",
                    padding: "3px 9px",
                    background: `var(--pet-tint-${tint}-bg)`,
                    color: `var(--pet-tint-${tint}-fg)`,
                    border: `1px solid var(--pet-tint-${tint}-fg)`,
                    borderRadius: "12px",
                    fontSize: "11px",
                    fontWeight: 600,
                    lineHeight: 1.2,
                    cursor: "pointer",
                    zIndex: 60,
                    boxShadow: "var(--pet-shadow-sm)",
                    userSelect: "none",
                    whiteSpace: "nowrap",
                  }}
                >
                  {text}
                </div>
              );
            })()}
            {/* 任务完成 sparkle 庆祝：done_today 单调 +1 时点燃一次。key
                自增让多次连发也能 remount 从头跑。粒子表 SPARKLE_PARTICLES
                在模块顶；reduced-motion 下整段不渲染（CSS @media 兜底）。
                pointerEvents: none 保 Live2D 拖动 / pill 点击穿透不被遮。 */}
            {sparkleKey > 0 && (
              <div
                key={sparkleKey}
                aria-hidden
                style={{
                  position: "absolute",
                  inset: 0,
                  pointerEvents: "none",
                  zIndex: 70,
                  overflow: "hidden",
                }}
              >
                <style>{`
                  @keyframes pet-sparkle {
                    0%   { opacity: 0; transform: translate(0, 0) scale(0.4) rotate(var(--rot, 0deg)); }
                    25%  { opacity: 1; transform: translate(var(--dx, 0), -10px) scale(1.2) rotate(var(--rot, 0deg)); }
                    70%  { opacity: 1; transform: translate(var(--dx, 0), -28px) scale(1.0) rotate(var(--rot, 0deg)); }
                    100% { opacity: 0; transform: translate(var(--dx, 0), -44px) scale(0.6) rotate(var(--rot, 0deg)); }
                  }
                  @media (prefers-reduced-motion: reduce) {
                    .pet-sparkle-particle { animation: none !important; opacity: 0 !important; }
                  }
                `}</style>
                {SPARKLE_PARTICLES.map((p, i) => (
                  <span
                    key={i}
                    className="pet-sparkle-particle"
                    style={{
                      position: "absolute",
                      top: p.top,
                      left: p.left,
                      fontSize: p.size,
                      lineHeight: 1,
                      animation: `pet-sparkle 1500ms ${p.delay}ms ease-out forwards`,
                      ["--dx" as never]: p.dx,
                      ["--rot" as never]: p.rot,
                    } as React.CSSProperties}
                  >
                    {p.glyph}
                  </span>
                ))}
              </div>
            )}
            {/* 心情展示位：钉在 Live2D 区右下角，让用户随时看到宠物当前心情。
                空心情 → 不渲染（避免占视觉位）。motion → emoji 取自 PanelPersona
                的 MOTION_META 简化版；心情文字 trunc 到 ~24 字符。 */}
            <MoodWidget />
            {/* 今日主动开口 🐾 N chip：钉在 ✦ 陪伴 chip 左侧。todaySpeechCount
                = 后端 `get_today_speech_count` 仅数 proactive 主动开口次数（不
                含 user-initiated chat 回复）。> 0 才渲染 —— 等于 0 时（刚起或
                还没主动开口的清晨）chip 显 "🐾 0" 反成噪音。点击同 ✦ chip 跳
                Persona tab（含 speech stats 详情）。位置 `right: 76px` 给 ✦ chip
                的 36px + ~40px chip 宽留出空间。 */}
            {todaySpeechCount > 0 && (
              <div
                onClick={(e) => {
                  e.stopPropagation();
                  try {
                    window.localStorage.setItem(
                      "pet-panel-deeplink",
                      JSON.stringify({ tab: "人格", ts: Date.now() }),
                    );
                  } catch {
                    // localStorage 不可用 → 至少打开面板
                  }
                  openPanel();
                }}
                onMouseDown={(e) => e.stopPropagation()}
                title={`今天宠物主动来找你 ${todaySpeechCount} 次（不含你主动开口）。点开「人格」tab 看完整 speech 统计。`}
                style={{
                  position: "absolute",
                  top: "8px",
                  right: "76px",
                  padding: "3px 9px",
                  borderRadius: "12px",
                  background: "var(--pet-color-card)",
                  border: "1px solid var(--pet-color-border)",
                  color: "var(--pet-color-muted)",
                  fontSize: "11px",
                  fontWeight: 600,
                  lineHeight: 1.2,
                  cursor: "pointer",
                  zIndex: 60,
                  boxShadow: "var(--pet-shadow-sm)",
                  opacity: 0.6,
                  transition: "opacity 120ms ease-out",
                  userSelect: "none",
                  whiteSpace: "nowrap",
                }}
                onMouseOver={(e) => {
                  (e.currentTarget as HTMLDivElement).style.opacity = "1";
                }}
                onMouseOut={(e) => {
                  (e.currentTarget as HTMLDivElement).style.opacity = "0.6";
                }}
              >
                🐾 {todaySpeechCount}
              </div>
            )}
            {/* 今日 mute proactive 累计 🔕 N chip：钉在 🐾 chip 左侧。
                todayMuteCount = 后端 mute_count::get_today_mute_count
                （进程内每日聚合）。> 0 才渲染（无 mute 时不噪音）。
                位置 right 动态算：有 🐾 chip 时偏更左让两 chip 并排。 */}
            {todayMuteCount > 0 && (
              <div
                onMouseDown={(e) => e.stopPropagation()}
                title={`今天你让宠物闭嘴 ${todayMuteCount} 次（通过 PanelDebug "⚙️ mute" / PanelChat "/sleep" 等路径）。进程重启清零。`}
                style={{
                  position: "absolute",
                  top: "8px",
                  right:
                    todaySpeechCount > 0
                      ? `${76 + 56}px`
                      : "76px",
                  padding: "3px 9px",
                  borderRadius: "12px",
                  background: "var(--pet-color-card)",
                  border: "1px solid var(--pet-color-border)",
                  color: "var(--pet-color-muted)",
                  fontSize: "11px",
                  fontWeight: 600,
                  lineHeight: 1.2,
                  cursor: "default",
                  zIndex: 60,
                  boxShadow: "var(--pet-shadow-sm)",
                  opacity: 0.6,
                  transition: "opacity 120ms ease-out",
                  userSelect: "none",
                  whiteSpace: "nowrap",
                }}
                onMouseOver={(e) => {
                  (e.currentTarget as HTMLDivElement).style.opacity = "1";
                }}
                onMouseOut={(e) => {
                  (e.currentTarget as HTMLDivElement).style.opacity = "0.6";
                }}
              >
                🔕 {todayMuteCount}
              </div>
            )}
            {/* 陪伴天数 ✦ N chip：紧贴收起按钮左侧。默认 opacity 0.6 让它
                半透不抢戏；hover 上去满灰度。companionshipDays === -1 = 未
                fetch / 失败兜底 → 不渲染。点击跳到 Panel 「人格」tab（用户
                想看更多陪伴 stats / persona summary 的自然路径）。 */}
            {companionshipDays >= 0 && (
              <div
                onClick={(e) => {
                  e.stopPropagation();
                  try {
                    window.localStorage.setItem(
                      "pet-panel-deeplink",
                      JSON.stringify({ tab: "人格", ts: Date.now() }),
                    );
                  } catch {
                    // localStorage 不可用 → 至少打开面板
                  }
                  openPanel();
                }}
                onMouseDown={(e) => e.stopPropagation()}
                title={
                  companionshipDays === 0
                    ? "今天与你初识 🐾（点开「人格」tab 看陪伴 / 心情 / 工具）"
                    : `已陪伴 ${companionshipDays} 天 🐾（点开「人格」tab 看陪伴 / 心情 / 工具）`
                }
                style={{
                  position: "absolute",
                  top: "8px",
                  right: "36px",
                  padding: "3px 9px",
                  borderRadius: "12px",
                  background: "var(--pet-color-card)",
                  border: "1px solid var(--pet-color-border)",
                  color: "var(--pet-color-muted)",
                  fontSize: "11px",
                  fontWeight: 600,
                  lineHeight: 1.2,
                  cursor: "pointer",
                  zIndex: 60,
                  boxShadow: "var(--pet-shadow-sm)",
                  opacity: 0.6,
                  transition: "opacity 120ms ease-out",
                  userSelect: "none",
                  whiteSpace: "nowrap",
                }}
                onMouseOver={(e) => {
                  (e.currentTarget as HTMLDivElement).style.opacity = "1";
                }}
                onMouseOut={(e) => {
                  (e.currentTarget as HTMLDivElement).style.opacity = "0.6";
                }}
              >
                ✦ {companionshipDays}
              </div>
            )}
            {/* Ambient 微卡片：pet 主区 hover 3s 后浮出。聚合 今日 / 本周 /
                累计 主动开口数 + ✦ 陪伴天数 4 段一行（陪伴 -1 = 未抓到时
                少一段）。pointerEvents none = 不挡 Live2D 拖动 / 双击 / 收起按钮
                hover；cursor 移到卡片实际仍在 wrapper 内 → mouseLeave 不
                误触发。底居中：避开顶部已挂的任务 pill / 🐾 / ✦ / ▶| chip
                + 右下心情 widget，居中下方是 pet 视觉留白区。lifetime 大数
                自动 1.2k 简化避免占位过宽。 */}
            {ambientStats !== null && (
              <div
                style={{
                  position: "absolute",
                  bottom: "10px",
                  left: "50%",
                  transform: "translateX(-50%)",
                  padding: "4px 10px",
                  borderRadius: "10px",
                  background: "var(--pet-color-card)",
                  border: "1px solid var(--pet-color-border)",
                  color: "var(--pet-color-fg)",
                  fontSize: "11px",
                  lineHeight: 1.3,
                  zIndex: 55,
                  boxShadow: "var(--pet-shadow-sm)",
                  opacity: 0.92,
                  pointerEvents: "none",
                  userSelect: "none",
                  whiteSpace: "nowrap",
                  display: "flex",
                  gap: "8px",
                  alignItems: "center",
                }}
              >
                <span>
                  <span style={{ color: "var(--pet-color-muted)" }}>今日</span>{" "}
                  🐾 {ambientStats.today}
                </span>
                <span style={{ color: "var(--pet-color-border)" }}>·</span>
                <span>
                  <span style={{ color: "var(--pet-color-muted)" }}>本周</span>{" "}
                  {ambientStats.week}
                </span>
                <span style={{ color: "var(--pet-color-border)" }}>·</span>
                <span>
                  <span style={{ color: "var(--pet-color-muted)" }}>累计</span>{" "}
                  {ambientStats.lifetime >= 10000
                    ? `${(ambientStats.lifetime / 1000).toFixed(0)}k`
                    : ambientStats.lifetime >= 1000
                      ? `${(ambientStats.lifetime / 1000).toFixed(1)}k`
                      : ambientStats.lifetime}
                </span>
                {companionshipDays >= 0 && (
                  <>
                    <span style={{ color: "var(--pet-color-border)" }}>·</span>
                    <span>✦ {companionshipDays} 天</span>
                  </>
                )}
                {/* 🌐 本机时区 chip：HH:MM + IANA timezone short label。
                    远程办公 / 时区切换场景给 owner ambient 锚定 "我现在
                    几点 / 在哪个时区"。Intl.DateTimeFormat 取 IANA TZ
                    （fallback 用 offset 串）。本字段是 pet hover 3s 卡片
                    最右侧加一段，与既有 4 段 (今日/本周/累计/✦) 同行排列。 */}
                <span style={{ color: "var(--pet-color-border)" }}>·</span>
                <span
                  style={{ color: "var(--pet-color-muted)" }}
                  title={`本机时间 + 时区。Intl.DateTimeFormat 取系统 IANA 时区；远程办公 / 跨时区时 ambient 锚定。`}
                >
                  🌐{" "}
                  {(() => {
                    const now = new Date();
                    const hh = String(now.getHours()).padStart(2, "0");
                    const mm = String(now.getMinutes()).padStart(2, "0");
                    const tz =
                      (() => {
                        try {
                          return Intl.DateTimeFormat().resolvedOptions().timeZone;
                        } catch {
                          // fallback：拿数字 offset (-480 → "+08:00")
                          const off = -now.getTimezoneOffset();
                          const sign = off >= 0 ? "+" : "-";
                          const oh = String(Math.floor(Math.abs(off) / 60)).padStart(2, "0");
                          const om = String(Math.abs(off) % 60).padStart(2, "0");
                          return `UTC${sign}${oh}:${om}`;
                        }
                      })();
                    // IANA tz like "Asia/Shanghai" 简化为最后一段 "Shanghai"
                    const tzShort = tz.split("/").pop() ?? tz;
                    return `${hh}:${mm} ${tzShort}`;
                  })()}
                </span>
              </div>
            )}
            {/* 收起按钮：钉在 Live2D 区右上角；调 useAutoHide.collapse 把窗口
                滑到桌边只露 tab。 */}
            <div
              onClick={(e) => {
                e.stopPropagation();
                collapse();
              }}
              onMouseDown={(e) => e.stopPropagation()}
              title="收起到桌边（也可按 Esc；mouse-enter 左侧 tab 召回）"
              style={{
                position: "absolute",
                top: "8px",
                right: "8px",
                width: "22px",
                height: "22px",
                borderRadius: "50%",
                background: "var(--pet-color-card)",
                border: "1px solid var(--pet-color-border)",
                color: "var(--pet-color-muted)",
                fontSize: "13px",
                lineHeight: 1,
                cursor: "pointer",
                zIndex: 60,
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                boxShadow: "var(--pet-shadow-sm)",
                opacity: 0.6,
                transition: "opacity 120ms ease-out",
                userSelect: "none",
              }}
              onMouseOver={(e) => {
                (e.currentTarget as HTMLDivElement).style.opacity = "1";
              }}
              onMouseOut={(e) => {
                (e.currentTarget as HTMLDivElement).style.opacity = "0.6";
              }}
            >
              ▶|
            </div>
          </div>
          <ChatMini
            messages={messages}
            currentResponse={currentResponse}
            toolStatus={toolStatus}
            isLoading={isLoading}
            visible
            onLike={!isLoading ? handleBubbleLike : undefined}
            onOpenPanel={openPanel}
            onCancel={cancel}
            userGlyph={settings.user_glyph}
            assistantGlyph={settings.assistant_glyph}
            nowTasks={nowTasks}
            sessionTokens={sessionTokens}
            onResetContext={resetContext}
            onRefDoubleClick={handleMiniRefDoubleClick}
            onSaveAsTask={handleMiniSaveAsTask}
            onSaveAsNote={handleMiniSaveAsNote}
          />
          <ChatPanel onSend={handleSend} isLoading={isLoading} />
        </>
      )}
      {petCtxMenu &&
        (() => {
          // 视口右下越界时把菜单往回挪；经验值宽 180 / 高 ~230 足够 6 个 item +
          // separator。clamp `Math.max(8, Math.min(...))` 让贴边时留 8px 安全
          // 边距。mousedown e.stopPropagation 防菜单内部点击被 useEffect outside-
          // close 误关。
          const W = 180;
          // H 经验值 ~ 13 个 button (button ≈ 26px) + 6 个 separator (≈ 9px) +
          // 8px padding ≈ 400；加点余量到 470 给字体放大 / 不同主题边距浮动。
          const H = 470;
          const left = Math.max(8, Math.min(petCtxMenu.x, window.innerWidth - W - 8));
          const top = Math.max(8, Math.min(petCtxMenu.y, window.innerHeight - H - 8));
          const itemStyle: React.CSSProperties = {
            display: "block",
            width: "100%",
            textAlign: "left",
            padding: "6px 12px",
            fontSize: 12,
            lineHeight: 1.35,
            border: "none",
            background: "transparent",
            color: "var(--pet-color-fg)",
            cursor: "pointer",
            fontFamily: "inherit",
            borderRadius: 4,
          };
          const itemHoverIn = (e: React.MouseEvent<HTMLButtonElement>) => {
            (e.currentTarget as HTMLButtonElement).style.background =
              "var(--pet-color-bg)";
          };
          const itemHoverOut = (e: React.MouseEvent<HTMLButtonElement>) => {
            (e.currentTarget as HTMLButtonElement).style.background =
              "transparent";
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
          const runMute = async (minutes: number) => {
            setPetCtxMenu(null);
            try {
              await invoke<string | null>("set_mute_minutes", { minutes });
            } catch (e) {
              console.error("set_mute_minutes failed:", e);
            }
          };
          return (
            <div
              onMouseDown={(e) => e.stopPropagation()}
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
                borderRadius: 8,
                boxShadow: "0 4px 12px rgba(0,0,0,0.18)",
                padding: 4,
                zIndex: 100,
                fontFamily: "inherit",
              }}
            >
              <button
                type="button"
                style={itemStyle}
                onMouseOver={itemHoverIn}
                onMouseOut={itemHoverOut}
                onClick={() => {
                  setPetCtxMenu(null);
                  openPanel();
                }}
                title="打开 Panel（完整聊天 / 任务 / 记忆 / 设置）— 全局快捷键 ⌘O / Ctrl+O"
              >
                📋 打开面板
              </button>
              {/* 📂 打开宠物数据目录：复用既有 `open_pet_data_dir` Tauri 命令
                  （PanelSettings「在 Finder 中打开」同后端），让 owner 不开
                  Panel 也能直奔 `~/.config/pet/` 浏览 config / SOUL / memories /
                  sessions。错误静默 console；macOS Finder 打开本身视觉反馈足够。 */}
              <button
                type="button"
                style={itemStyle}
                onMouseOver={itemHoverIn}
                onMouseOut={itemHoverOut}
                onClick={async () => {
                  setPetCtxMenu(null);
                  try {
                    await invoke("open_pet_data_dir");
                  } catch (e) {
                    console.error("open_pet_data_dir failed:", e);
                  }
                }}
                title="在系统文件管理器里打开宠物数据目录（~/.config/pet/）—— 含 config.yaml / SOUL.md / memories/ / sessions/ 等。"
              >
                📂 打开数据目录
              </button>
              {sep}
              <button
                type="button"
                style={itemStyle}
                onMouseOver={itemHoverIn}
                onMouseOut={itemHoverOut}
                onClick={() => {
                  // 直接读 storage 当前值翻转 —— 比绑 React state 简单：theme 不
                  // 经 React 渲染（CSS var 自动 propagate），不必同步局部 state。
                  setPetCtxMenu(null);
                  const next = getStoredTheme() === "dark" ? "light" : "dark";
                  setStoredTheme(next);
                  applyTheme(next, getStoredAccent());
                }}
                title="切换 light / dark 主题（CSS var 即时生效；偏好持久化到 localStorage）"
              >
                {getStoredTheme() === "dark" ? "☀️ 切到 light 主题" : "🌙 切到 dark 主题"}
              </button>
              {sep}
              <button
                type="button"
                style={itemStyle}
                onMouseOver={itemHoverIn}
                onMouseOut={itemHoverOut}
                onClick={() => void runMute(30)}
                title="让宠物 30 分钟内不主动开口（proactive 暂停）"
              >
                😴 mute 30 分
              </button>
              <button
                type="button"
                style={itemStyle}
                onMouseOver={itemHoverIn}
                onMouseOut={itemHoverOut}
                onClick={() => void runMute(60)}
                title="让宠物 60 分钟内不主动开口"
              >
                😴 mute 60 分
              </button>
              <button
                type="button"
                style={{ ...itemStyle, color: "var(--pet-color-accent)" }}
                onMouseOver={itemHoverIn}
                onMouseOut={itemHoverOut}
                onClick={() => void runMute(0)}
                title="解除 mute（minutes=0 撤销 proactive 暂停）"
              >
                ☀️ 解除 mute
              </button>
              {sep}
              {/* 倒计时 nudge presets：与 mute 邻近因都涉及"时间软控制"，
                  但语义反向 —— mute 让宠物安静、倒计时让宠物提醒。四档
                  覆盖典型场景：5 分子段微歇 / 15 分短番茄 / 25 分标准番茄
                  / 30 分半小时块。多个并存 OK，timer id Set 在 unmount 时
                  一次 clear 防漏。 */}
              {[5, 15, 25, 30].map((m) => (
                <button
                  key={m}
                  type="button"
                  style={itemStyle}
                  onMouseOver={itemHoverIn}
                  onMouseOut={itemHoverOut}
                  onClick={() => {
                    setPetCtxMenu(null);
                    startCountdownNudge(m);
                  }}
                  title={`设 ${m} 分钟倒计时，到点 ChatMini 浮一条 "⏰ ${m} 分倒计时到了" 软提醒（不打开 Live2D 主动模式）。多个倒计时可并存。`}
                >
                  ⏰ 倒计时 {m} 分
                </button>
              ))}
              {sep}
              {/* 📋 复制当前 mood：调 get_current_mood 拿到当前文本 + motion，
                  format 成 "心情：X · 动作：Y · 时间：..." 字符串复制到剪
                  贴板。让 owner 想"抄给别人 / 写日记" 时一键 export 宠物
                  当前情绪。失败用 toast 替代而非 throw —— mood file 缺失
                  / 老 session 都是常见 OK 边界。 */}
              <button
                type="button"
                style={itemStyle}
                onMouseOver={itemHoverIn}
                onMouseOut={itemHoverOut}
                onClick={async () => {
                  setPetCtxMenu(null);
                  try {
                    const m = await invoke<CurrentMood>("get_current_mood");
                    if (!m || (!m.text?.trim() && !m.motion)) {
                      appendAssistant("📋 当前 mood 为空，无可复制");
                      return;
                    }
                    const parts: string[] = [];
                    if (m.text?.trim()) parts.push(`心情：${m.text.trim()}`);
                    if (m.motion) parts.push(`动作：${m.motion}`);
                    const text = parts.join(" · ");
                    await navigator.clipboard.writeText(text);
                    appendAssistant(`📋 已复制当前 mood：${text}`);
                  } catch (e) {
                    appendAssistant(`📋 复制 mood 失败：${e}`);
                  }
                }}
                title="把宠物当前 mood (text + motion) 复制到剪贴板，方便 owner 抄给别人 / 写日记 / 上 issue 截图配文等。"
              >
                📋 复制当前 mood
              </button>
              {sep}
              {/* 📡 ping LLM: 调后端 ping_llm command 测 settings.api_base
                  /models 端点连通 + 延迟。结果 push 到 ChatMini 让 owner
                  立刻看到（✅ 通 / ⚠ 通但 status N / ❌ 不通）+ api_base /
                  model echo / elapsed_ms。owner "宠物不回应" 排查时第一步。 */}
              <button
                type="button"
                style={itemStyle}
                onMouseOver={itemHoverIn}
                onMouseOut={itemHoverOut}
                onClick={async () => {
                  setPetCtxMenu(null);
                  appendAssistant("📡 ping LLM...");
                  try {
                    const r = await invoke<{
                      ok: boolean;
                      elapsed_ms: number;
                      status_code: number;
                      api_base: string;
                      model: string;
                    }>("ping_llm");
                    const icon = r.ok ? "✅" : "⚠️";
                    appendAssistant(
                      `${icon} ping ${r.api_base} → HTTP ${r.status_code} · ${r.elapsed_ms}ms · model=${r.model}`,
                    );
                  } catch (e) {
                    appendAssistant(`❌ ping LLM 失败：${e}`);
                  }
                }}
                title="测当前 model.api_base 连通 + 延迟 (ms)。owner 排查'宠物不回应'时第一步：看是网络挂了 / api_base 错了 / api_key 错了。"
              >
                📡 ping LLM
              </button>
              {sep}
              {/* 🎲 摇一摇主动开口：armed 二次确认。首点 → 红字"再点确认 3s"
                  + 启 3s 计时器；二次点 → 真调 trigger_proactive_turn 绕过
                  cooldown 立即跑一次 proactive turn。fireShakeBusy 在 invoke
                  期间禁用，防双触。结果走 appendAssistant 让 owner 在
                  ChatMini 看到执行 outcome 摘要。 */}
              <button
                type="button"
                style={{
                  ...itemStyle,
                  color: shakeArmed ? "var(--pet-tint-red-fg)" : itemStyle.color,
                  fontWeight: shakeArmed ? 600 : itemStyle.fontWeight,
                }}
                onMouseOver={itemHoverIn}
                onMouseOut={itemHoverOut}
                disabled={fireShakeBusy}
                onClick={async () => {
                  if (!shakeArmed) {
                    armShake();
                    return;
                  }
                  // armed 状态下二次点击 → 真触发
                  if (shakeArmedTimerRef.current !== null) {
                    window.clearTimeout(shakeArmedTimerRef.current);
                    shakeArmedTimerRef.current = null;
                  }
                  setShakeArmed(false);
                  setPetCtxMenu(null);
                  setFireShakeBusy(true);
                  appendAssistant("🎲 摇一摇 → 触发宠物主动开口…");
                  try {
                    const status = await invoke<string>(
                      "trigger_proactive_turn",
                    );
                    appendAssistant(`✅ ${status}`);
                  } catch (e) {
                    appendAssistant(`❌ 触发失败：${e}`);
                  } finally {
                    setFireShakeBusy(false);
                  }
                }}
                title={
                  fireShakeBusy
                    ? "正在跑 proactive turn…"
                    : shakeArmed
                      ? "再点确认（3s 内有效）：立即跑一次 proactive turn，绕过 cooldown / quiet hours"
                      : "摇一摇让宠物现在主动开口（绕过 proactive cooldown / quiet hours）。点击进入二次确认。"
                }
              >
                {fireShakeBusy
                  ? "🎲 跑中…"
                  : shakeArmed
                    ? "🎲 再点确认 (3s)"
                    : "🎲 摇一摇 主动开口"}
              </button>
              {sep}
              <button
                type="button"
                style={{ ...itemStyle, color: "var(--pet-color-muted)" }}
                onMouseOver={itemHoverIn}
                onMouseOut={itemHoverOut}
                onClick={async () => {
                  setPetCtxMenu(null);
                  try {
                    await invoke("restart_pet_window");
                  } catch (e) {
                    console.error("restart_pet_window failed:", e);
                  }
                }}
                title="重启桌面 pet 窗口（仅刷新此窗口，session / 后端不动）"
              >
                🔄 重启窗口
              </button>
            </div>
          );
        })()}
    </div>
  );
}

export default App;
