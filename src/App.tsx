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
import { useMoodAnimation } from "./hooks/useMoodAnimation";
import { applyTheme, getStoredTheme, setStoredTheme, getStoredAccent, setStoredAccent, type Accent } from "./theme";
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
      <div
        title={`当前心情：${mood.text}${mood.motion ? `（${mood.motion}）` : ""}\n（hover 看最近 ${MOOD_HISTORY_MAX} 条变化）`}
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

function App() {
  const { settings, soul, loaded } = useSettings();
  const { messages, currentResponse, toolStatus, isLoading, sendMessage, cancel, appendAssistant } = useChat(soul);
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

  // 监听 panel 发的主题切换：setStoredTheme 持久化 + applyTheme 刷 CSS vars，
  // 让桌面 ChatMini / ChatPanel / MoodWidget 跟随。emit 来自 PanelApp.toggleTheme
  // 或 PanelSettings 的 toggle（待加）。本 window 是接收方，不回 emit 防循环。
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let unlistenAccent: (() => void) | undefined;
    (async () => {
      unlisten = await listen<string>("theme-change", (event) => {
        const next = event.payload === "dark" ? "dark" : "light";
        if (getStoredTheme() === next) return;
        setStoredTheme(next);
        applyTheme(next, getStoredAccent());
      });
      unlistenAccent = await listen<string>("accent-change", (event) => {
        const valid: Accent[] = ["default", "green", "purple", "orange", "rose"];
        const raw = event.payload as Accent;
        const next = valid.includes(raw) ? raw : "default";
        if (getStoredAccent() === next) return;
        setStoredAccent(next);
        applyTheme(getStoredTheme(), next);
      });
    })();
    return () => {
      if (unlisten) unlisten();
      if (unlistenAccent) unlistenAccent();
    };
  }, []);

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
              / 170 / 60 = 450 平衡。 */}
          <div style={{ position: "relative", flexShrink: 0, height: "220px" }}>
            <Live2DCharacter
              key={settings.live_2d_model_path}
              modelPath={settings.live_2d_model_path}
              onModelReady={handleModelReady}
            />
            {/* 心情展示位：钉在 Live2D 区右下角，让用户随时看到宠物当前心情。
                空心情 → 不渲染（避免占视觉位）。motion → emoji 取自 PanelPersona
                的 MOTION_META 简化版；心情文字 trunc 到 ~24 字符。 */}
            <MoodWidget />
            {/* 收起按钮：钉在 Live2D 区右上角；调 useAutoHide.collapse 把窗口
                滑到桌边只露 tab。 */}
            <div
              onClick={(e) => {
                e.stopPropagation();
                collapse();
              }}
              onMouseDown={(e) => e.stopPropagation()}
              title="收起到桌边（mouse-enter 左侧 tab 召回）"
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
          />
          <ChatPanel onSend={handleSend} isLoading={isLoading} />
        </>
      )}
    </div>
  );
}

export default App;
