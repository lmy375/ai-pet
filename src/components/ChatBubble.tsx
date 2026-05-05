import { parseMarkdown } from "../utils/inlineMarkdown";

interface HistoryControls {
  canPrev: boolean;
  canNext: boolean;
  onPrev: () => void;
  onNext: () => void;
  /// 形如 "2/10"，仅 history 模式有；live 模式为 null（不渲染指示器）。
  indicator: string | null;
}

interface Props {
  message: string;
  visible: boolean;
  onClick?: () => void;
  /// 可选的"主动点赞"回调。传入则在气泡右上角 ✕ 左侧渲染一个 👍。点击
  /// 时不应同时触发 onClick（dismiss 与 R1b 反馈），按钮内 stopPropagation。
  /// 不传则按钮不渲染（如历史模式下 onLike 故意 undefined，避免给历史快照
  /// 写新反馈）。
  onLike?: () => void;
  /// 可选的历史导航控件。传入则在气泡右下角渲染 `◀ 指示器 ▶`，让用户翻
  /// 看最近 N 条 proactive 发言；不传则气泡保持纯展示形态。
  historyControls?: HistoryControls;
}

// Iter R40: CSS keyframes for the bubble's mount-time fade-in. Runs once
// per visible→mounted transition (parent toggles `visible` so each new
// utterance gets its own fade-in). Inline <style> keeps the animation
// scoped to this file without needing a global stylesheet.
//
// translateY(4px) → 0 gives a faint "settle" feel — bubble doesn't pop
// in flat. 220ms is short enough to feel responsive; longer lags the
// utterance vs reading the text.
//
// Iter R41: also adds a `:active` press feedback so the click that
// triggers dismiss has a tactile reaction (scale 0.97 for 80ms). Without
// it, click → bubble disappears with no transition felt → user wonders
// "did the click register?". The press scale is the universal "I felt
// your tap" affordance from native UI.
//
// Iter R42: hover lift completes the interaction state machine
// (mount fadeIn / hover lift / active press). Hover deepens border
// color + lifts 1px so cursor entering signals "you can interact".
// :active overrides :hover transform via CSS source order.
const BUBBLE_STYLES = `
@keyframes pet-bubble-fade-in {
  from { opacity: 0; transform: translateY(4px); }
  to   { opacity: 1; transform: translateY(0); }
}
.pet-bubble:hover {
  border-color: #7dd3fc;
  transform: translateY(-1px);
}
.pet-bubble:active {
  transform: scale(0.97);
}
.pet-bubble-nav-btn {
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
.pet-bubble-nav-btn:hover:not(:disabled) {
  background: #e0f2fe;
  border-color: #7dd3fc;
}
.pet-bubble-nav-btn:disabled {
  opacity: 0.35;
  cursor: not-allowed;
}
.pet-bubble-like-btn {
  border: none;
  background: transparent;
  color: #94a3b8;
  font-size: 12px;
  line-height: 1;
  padding: 0 2px;
  cursor: pointer;
  opacity: 0.55;
  transition: opacity 120ms ease-out, color 120ms ease-out, transform 120ms ease-out;
}
.pet-bubble-like-btn:hover {
  opacity: 1;
  color: #ec4899;
  transform: scale(1.15);
}
`;

export function ChatBubble({ message, visible, onClick, onLike, historyControls }: Props) {
  if (!visible || !message) return null;

  return (
    <>
      <style>{BUBBLE_STYLES}</style>
      <div
        className="pet-bubble"
        onClick={onClick}
        style={{
          position: "absolute",
          bottom: "100px",
          left: "12px",
          right: "12px",
          maxHeight: "80px",
          overflowY: "auto",
          padding: "10px 14px",
          background: "#ffffff",
          borderRadius: "16px",
          boxShadow: "none",
          border: "1px solid #bae6fd",
          fontSize: "13px",
          lineHeight: "1.5",
          color: "#333",
          zIndex: 10,
          wordBreak: "break-word",
          cursor: onClick ? "pointer" : "default",
          animation: "pet-bubble-fade-in 220ms ease-out",
          transition: "transform 80ms ease-out, border-color 120ms ease-out",
        }}
      >
        {/* Iter R24: subtle dismiss affordance — the bubble was already
            click-to-dismiss in R1b, but with no visual hint that clicking
            would do anything. The ✕ corner icon makes it discoverable.
            Click bubbles up to the parent div's onClick, so clicking either
            the ✕ or the bubble body is equivalent (no separate handler).
            Tooltip on the ✕ explains the strength-of-signal nuance.
            👍 (onLike) 紧贴 ✕ 左侧；点 👍 写 Liked 信号、不冒泡触发 dismiss/
            R1b 负反馈。两图标布局：右起 ✕ → 👍。 */}
        <div
          style={{
            position: "absolute",
            top: "4px",
            right: "8px",
            display: "flex",
            alignItems: "center",
            gap: "4px",
            userSelect: "none",
          }}
        >
          {onLike && (
            <button
              type="button"
              className="pet-bubble-like-btn"
              aria-label="like this bubble"
              title="给宠物点个赞（写 Liked 进 feedback_history，作为正向信号告诉它「这条说得对」）"
              onClick={(e) => {
                e.stopPropagation();
                onLike();
              }}
            >
              👍
            </button>
          )}
          {onClick && (
            <span
              aria-label="dismiss bubble"
              title="点掉气泡（5 秒内点 = 给宠物 '别这条' 信号；R1b dismissed feedback）"
              style={{
                fontSize: "11px",
                color: "#94a3b8",
                opacity: 0.55,
              }}
            >
              ✕
            </span>
          )}
        </div>
        {parseMarkdown(message)}
        {historyControls && (
          <div
            // 阻止冒泡：点 nav 按钮不该触发 bubble 主体的 onClick（dismiss）。
            onClick={(e) => e.stopPropagation()}
            style={{
              position: "absolute",
              bottom: "4px",
              right: "8px",
              display: "flex",
              alignItems: "center",
              gap: "4px",
              fontSize: "10px",
              color: "#64748b",
              userSelect: "none",
            }}
          >
            <button
              type="button"
              className="pet-bubble-nav-btn"
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
                className="pet-bubble-nav-btn"
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
    </>
  );
}
