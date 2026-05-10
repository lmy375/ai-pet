import { useEffect, useMemo, useRef, useState } from "react";
import { bubbleStyle } from "./panel/panelChatBits";
import { parseMarkdown } from "../utils/inlineMarkdown";

interface ChatMessage {
  role: "user" | "assistant" | "system" | "tool";
  content: string;
}

interface Props {
  /// 来自 useChat 的完整消息数组（含 system / tool）；本组件自己过滤展示。
  messages: ChatMessage[];
  /// 流式中的当前 chunk 累积。空串表示无 streaming。
  currentResponse: string;
  isLoading: boolean;
  visible: boolean;
  /// 最新 assistant 行 👍 按钮的回调。写 Liked 反馈。流式中或 history 模式
  /// 不传，按钮不渲染（避免误触没读完的内容）。
  onLike?: () => void;
  /// 「最大化」按钮 → 打开 Panel chat 页。点击调用此回调；不传则按钮
  /// 不渲染。替代旧 ChatPanel 底栏的 💬 按钮，让用户从 mini chat 顶角直
  /// 接进入 panel。
  onOpenPanel?: () => void;
}

/// 最近 N 条的硬上限。窗口很小，DOM 太长既不好读也耗渲染。
const MINI_CHAT_MAX_ITEMS = 20;

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
.pet-mini-bubble-like-btn {
  border: none;
  background: transparent;
  color: #94a3b8;
  font-size: 11px;
  line-height: 1;
  padding: 0 2px;
  cursor: pointer;
  opacity: 0.55;
  transition: opacity 120ms ease-out, color 120ms ease-out, transform 120ms ease-out;
}
.pet-mini-bubble-like-btn:hover {
  opacity: 1;
  color: #ec4899;
  transform: scale(1.15);
}
`;

/// 容器底部 8px 内视为"贴底"，用于决定 follow-tail 是否成立。给浮点偏差一
/// 点缓冲，避免微小量误判。
const FOLLOW_BOTTOM_THRESHOLD_PX = 8;

export function ChatMini({
  messages,
  currentResponse,
  isLoading,
  visible,
  onLike,
  onOpenPanel,
}: Props) {
  const scrollRef = useRef<HTMLDivElement>(null);
  // followTail：用户是否处于"自动跟随最新"状态。挂载时默认 true（贴底）。
  // 用 ref 让 auto-scroll effect 拿到最新值而不必加进 deps；同名 state
  // 仅供「跳到底浮标」按钮可见态用。两者由 onScroll 同步更新。
  const followTailRef = useRef(true);
  const [notAtBottom, setNotAtBottom] = useState(false);

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
      {/* 「最大化」按钮：固定在 mini chat 容器右上角内侧。点击调用
          onOpenPanel —— 替代过去 ChatPanel 底栏的 💬 按钮，让用户从聊天
          列表顶角直接跳到 Panel chat 页。stopPropagation 防止冒泡。 */}
      {onOpenPanel && (
        <button
          type="button"
          onClick={(e) => {
            e.stopPropagation();
            onOpenPanel();
          }}
          title="在面板中打开聊天（看完整历史 / 多会话切换）"
          aria-label="open panel chat"
          style={{
            position: "absolute",
            // 容器 bottom:60 + maxHeight:35%；按钮浮在容器右上角内侧。
            // 视觉上像 macOS 窗口的「全屏 / 最大化」三色钮里的绿色那枚。
            right: "20px",
            bottom: "calc(60px + 35% - 26px)",
            width: "20px",
            height: "20px",
            borderRadius: "50%",
            border: "1px solid rgba(148,163,184,0.4)",
            background: "rgba(255,255,255,0.95)",
            color: "#475569",
            fontSize: "11px",
            lineHeight: 1,
            cursor: "pointer",
            zIndex: 12,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            padding: 0,
            boxShadow: "0 1px 3px rgba(0,0,0,0.1)",
          }}
        >
          ⛶
        </button>
      )}
      <div
        className="pet-mini-chat"
        ref={scrollRef}
        onScroll={handleScroll}
        style={{
          position: "absolute",
          // 锚到底部、贴在 ChatPanel 输入框上方 —— 让 Live2D 形象在窗口
          // 上半部分自然展示，聊天列表压在人像脚边以下，避免覆盖身体。
          // bottom 60px 给底部输入框留位（输入框 bottom: 12px + 高度 36px ≈
          // 48px，再补一点 gap）。
          bottom: "60px",
          left: "12px",
          right: "12px",
          maxHeight: "35%",
          overflowY: "auto",
          padding: "8px 10px",
          background: "rgba(255,255,255,0.92)",
          borderRadius: "12px",
          border: "1px solid #bae6fd",
          fontSize: "12px",
          lineHeight: "1.5",
          color: "#333",
          zIndex: 10,
          boxShadow: "0 2px 8px rgba(0,0,0,0.08)",
          animation: "pet-mini-chat-fade-in 220ms ease-out",
        }}
      >
        {visibleItems.map((m, idx) => {
          const isLast = idx === lastIdx;
          const isAssistant = m.role === "assistant";
          return (
            <div
              key={`${m.role}-${idx}-${m.content.length}`}
              style={{
                display: "flex",
                justifyContent: m.role === "user" ? "flex-end" : "flex-start",
                marginBottom: 6,
                position: "relative",
              }}
            >
              <div style={{ ...bubbleStyle(m.role as "user" | "assistant"), maxWidth: "85%", padding: "6px 10px", fontSize: "12px", lineHeight: 1.45 }}>
                {parseMarkdown(m.content)}
              </div>
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
                    background: "rgba(255,255,255,0.85)",
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
          );
        })}
        {showStreamingBubble && (
          <div
            style={{
              display: "flex",
              justifyContent: "flex-start",
              marginBottom: 6,
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
          </div>
        )}
      </div>
      {/* 跳到底浮标：仅当用户向上滚翻历史时显（notAtBottom=true）。位置贴
          chat 容器右下角再往下偏一点，避开 list 内容。点击滚到底 + 重启
          follow-tail。流式中如果用户向上读旧内容也保留这个出口。 */}
      {notAtBottom && (
        <button
          type="button"
          onClick={handleJumpToBottom}
          title="跳到最新（点后新消息会自动跟随）"
          aria-label="jump to bottom"
          style={{
            position: "absolute",
            // 浮在 chat list 容器右下角内侧；容器 bottom:60 + maxHeight:35%
            // 之内贴底，所以按钮 bottom 就稍高于 60，让它正好贴 list 底缘。
            right: "20px",
            bottom: "68px",
            width: "28px",
            height: "28px",
            borderRadius: "50%",
            border: "1px solid #7dd3fc",
            background: "rgba(255,255,255,0.95)",
            color: "#0ea5e9",
            fontSize: "14px",
            lineHeight: 1,
            cursor: "pointer",
            zIndex: 11,
            boxShadow: "0 2px 6px rgba(0,0,0,0.15)",
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
    </>
  );
}
