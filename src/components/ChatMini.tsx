import { useEffect, useMemo, useRef, useState } from "react";
import { bubbleStyle } from "./panel/panelChatBits";
import { parseMarkdown } from "../utils/inlineMarkdown";

interface ChatMessage {
  role: "user" | "assistant" | "system" | "tool";
  content: string;
}

interface HistoryControls {
  canPrev: boolean;
  canNext: boolean;
  onPrev: () => void;
  onNext: () => void;
  /// 形如 "2/10"。live 模式 null。
  indicator: string | null;
}

interface Props {
  /// 来自 useChat 的完整消息数组（含 system / tool）；本组件自己过滤展示。
  messages: ChatMessage[];
  /// 流式中的当前 chunk 累积。空串表示无 streaming。
  currentResponse: string;
  isLoading: boolean;
  visible: boolean;
  /// 仅最新 assistant 那条挂的反馈钩子（与之前 ChatBubble 同语义）：
  /// `onDismiss` 在 5s 内点 ✕ → R1b dismissed 反馈；`onLike` 写 Liked。
  /// 不传则不渲染对应按钮（如 history 模式）。
  onDismiss?: () => void;
  onLike?: () => void;
  /// 历史导航：渲染在最底的 footer，与 chat list 共存（不替代列表）。
  historyControls?: HistoryControls;
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
.pet-mini-nav-btn {
  background: rgba(241, 245, 249, 0.9);
  border: 1px solid #cbd5e1;
  border-radius: 8px;
  width: 18px;
  height: 16px;
  font-size: 10px;
  line-height: 1;
  color: #475569;
  cursor: pointer;
  padding: 0;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  transition: background 100ms ease-out, border-color 100ms ease-out;
}
.pet-mini-nav-btn:hover:not(:disabled) {
  background: #e0f2fe;
  border-color: #7dd3fc;
}
.pet-mini-nav-btn:disabled {
  opacity: 0.35;
  cursor: not-allowed;
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
  onDismiss,
  onLike,
  historyControls,
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

  // 反馈按钮挂在「最新那一条 assistant」上，沿用 ChatBubble 时代的
  // R1b dismissed / Liked 语义。streaming 中不挂（避免误点未读完的内容
  // 写反馈），history 模式 caller 不传 onDismiss / onLike 也不挂。
  const lastIdx = visibleItems.length - 1;
  const lastMsg = lastIdx >= 0 ? visibleItems[lastIdx] : null;
  const showFeedbackOnLast =
    !!lastMsg &&
    lastMsg.role === "assistant" &&
    !isLoading &&
    (!!onDismiss || !!onLike);

  const showStreamingBubble = isLoading && currentResponse.trim().length > 0;

  // 跳到底浮标的点击：滚到底 + 重置 followTail。鼠标点会 bubble 到容器
  // onClick 触发 onDismiss，所以 stopPropagation。
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
      <div
        className="pet-mini-chat"
        ref={scrollRef}
        onScroll={handleScroll}
        // 主体可点 → 触发 onDismiss（与原 ChatBubble click-to-dismiss
        // 同入口）。点子按钮各自 stopPropagation。
        onClick={() => {
          if (onDismiss) onDismiss();
        }}
        style={{
          position: "absolute",
          top: "12px",
          left: "12px",
          right: "12px",
          maxHeight: "55%",
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
          cursor: onDismiss ? "pointer" : "default",
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
                  // 阻止冒泡：点按钮不该触发容器的 onDismiss。
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
                  {onDismiss && (
                    <button
                      type="button"
                      className="pet-mini-bubble-like-btn"
                      aria-label="dismiss bubble"
                      title="点掉（5 秒内点 = 「别这条」信号；R1b dismissed feedback）"
                      onClick={(e) => {
                        e.stopPropagation();
                        onDismiss();
                      }}
                    >
                      ✕
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
        {historyControls && (
          <div
            // 阻止冒泡：点 nav 按钮不该触发主体 onDismiss。
            onClick={(e) => e.stopPropagation()}
            style={{
              display: "flex",
              alignItems: "center",
              justifyContent: "flex-end",
              gap: "4px",
              fontSize: "10px",
              color: "#64748b",
              userSelect: "none",
              marginTop: 4,
              paddingTop: 4,
              borderTop: "1px dashed #e2e8f0",
            }}
          >
            <button
              type="button"
              className="pet-mini-nav-btn"
              aria-label="previous bubble"
              title="上一句（往更早的主动开口翻）"
              disabled={!historyControls.canPrev}
              onClick={(e) => {
                e.stopPropagation();
                historyControls.onPrev();
              }}
            >
              ◀
            </button>
            {historyControls.indicator && (
              <span title="第几条 / 共几条最近主动开口">{historyControls.indicator}</span>
            )}
            {historyControls.canNext && (
              <button
                type="button"
                className="pet-mini-nav-btn"
                aria-label="next bubble"
                title="下一句（更新或回到最新）"
                onClick={(e) => {
                  e.stopPropagation();
                  historyControls.onNext();
                }}
              >
                ▶
              </button>
            )}
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
            // 容器 top:12 + maxHeight:55%；按钮浮在容器右下角内侧。
            // right 与容器一致 + 一点 inset；bottom 走 chat panel 之上。
            right: "20px",
            top: "calc(55% + 4px)",
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
