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

  /// 任务完成庆祝 sparkle：done_today 计数从 prev → cur 单调上升时，触发
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
    },
    [settings.motion_mapping],
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
          // H 经验值 ~ 7 个 button (button ≈ 26px) + 3 个 separator (≈ 9px) +
          // 8px padding ≈ 217；加点余量到 270 给字体放大 / 不同主题边距浮动。
          const H = 270;
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
