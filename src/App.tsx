import { useEffect, useRef, useState, useCallback } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Live2DCharacter } from "./components/Live2DCharacter";
import { ChatMini } from "./components/ChatMini";
import { ChatPanel } from "./components/ChatPanel";
import { useChat } from "./hooks/useChat";
import { useAutoHide } from "./hooks/useAutoHide";
import { useSettings } from "./hooks/useSettings";
import { useMoodAnimation } from "./hooks/useMoodAnimation";

function App() {
  const { settings, soul, loaded } = useSettings();
  const { messages, currentResponse, isLoading, sendMessage } = useChat(soul);
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

  // 👍 反馈：写 Liked 信号到 feedback_history。excerpt 取消息列表里最近一
  // 条 assistant 内容（来自 useChat.messages，含 proactive 推过来的）。
  // mini chat 里的 👍 按钮挂在最新 assistant 行，所以这里就用 messages
  // 末尾的 assistant 即可。
  const handleBubbleLike = useCallback(() => {
    const lastAssistant = [...messages].reverse().find((m) => m.role === "assistant");
    if (!lastAssistant) return;
    invoke("record_bubble_liked", { excerpt: lastAssistant.content }).catch(
      console.error,
    );
  }, [messages]);

  const handleModelReady = useCallback((model: any) => {
    modelRef.current = model;
  }, []);

  const handleSend = useCallback(
    (msg: string) => {
      sendMessage(msg);
    },
    [sendMessage],
  );

  const handleDrag = (e: React.MouseEvent) => {
    const tag = (e.target as HTMLElement).tagName;
    if (tag === "INPUT" || tag === "BUTTON" || tag === "TEXTAREA") return;
    e.preventDefault();
    getCurrentWindow().startDragging();
  };

  const openPanel = () => {
    invoke("open_panel").catch(console.error);
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
                  background: "#dc2626",
                  color: "#fff",
                  fontSize: "10px",
                  fontWeight: 700,
                  borderRadius: "7px",
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "center",
                  border: "1.5px solid #fff",
                  boxShadow: "0 1px 3px rgba(0,0,0,0.2)",
                }}
                title={`pet 在隐藏期间主动开口了 ${unreadWhileHidden} 次（mouse-enter 让 pet 回来后会自动消失）`}
              >
                {unreadWhileHidden > 9 ? "9+" : unreadWhileHidden}
              </div>
            )}
          </div>
        </>
      )}

      {/* 桌面迷你聊天列表：常驻显示，位于 Live2D 形象下方、输入框上方。
          流式中追加 ghost bubble；最新 assistant 行带 👍。右上角「⛶」最大化
          进 Panel chat。`hidden`（窗口收到桌边）时整体不渲染，省 paint。 */}
      <ChatMini
        messages={messages}
        currentResponse={currentResponse}
        isLoading={isLoading}
        visible={!hidden}
        onLike={!isLoading ? handleBubbleLike : undefined}
        onOpenPanel={openPanel}
      />
      <Live2DCharacter
        key={settings.live_2d_model_path}
        modelPath={settings.live_2d_model_path}
        onModelReady={handleModelReady}
      />
      {/* 收起按钮：右上角小圆，调 useAutoHide.collapse 把窗口滑到桌边只露
          tab。hidden 时不渲染（已收起，再点无意义；mouse-enter 左侧 tab
          才是召回入口）。 */}
      {!hidden && (
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
            background: "rgba(255,255,255,0.85)",
            border: "1px solid rgba(148,163,184,0.4)",
            color: "#475569",
            fontSize: "13px",
            lineHeight: 1,
            cursor: "pointer",
            zIndex: 60,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            boxShadow: "0 1px 3px rgba(0,0,0,0.12)",
            opacity: 0.6,
            transition: "opacity 120ms ease-out, background 120ms ease-out",
            userSelect: "none",
          }}
          onMouseOver={(e) => {
            (e.currentTarget as HTMLDivElement).style.opacity = "1";
            (e.currentTarget as HTMLDivElement).style.background = "rgba(255,255,255,0.98)";
          }}
          onMouseOut={(e) => {
            (e.currentTarget as HTMLDivElement).style.opacity = "0.6";
            (e.currentTarget as HTMLDivElement).style.background = "rgba(255,255,255,0.85)";
          }}
        >
          ▶|
        </div>
      )}
      {!hidden && <ChatPanel onSend={handleSend} isLoading={isLoading} />}
    </div>
  );
}

export default App;
