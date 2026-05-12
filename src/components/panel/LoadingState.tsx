import { text, fontWeight, lineHeight } from "../../text";

/// 跨面板共享的"加载中"提示：与 EmptyState 同一种视觉语言（居中、有锚点
/// glyph、可选 hint），但 glyph 用 CSS 脉冲动画区分"正在工作"语义。
///
/// 之前各面板 `padding: 20 加载中...` / `s.detailHint 加载中…` 三种 padding /
/// 字号都不一致；这里收敛。
///
/// 用法：
///   <LoadingState message="加载中…" />
///   <LoadingState message="正在加载归档" hint="第一次拉取较慢" compact />
///   <LoadingState compact inline />  // 行内细节态，无外层 padding
///
/// `inline`：去掉外层 padding，给"行内细节加载"用（如 detail.md 编辑器）。
/// `compact`：padding 减半，适合 modal / 内嵌区域。
export function LoadingState({
  message = "加载中…",
  hint,
  compact,
  inline,
}: {
  message?: string;
  hint?: string;
  compact?: boolean;
  inline?: boolean;
}) {
  const padding = inline
    ? "4px 8px"
    : compact
      ? "20px 12px"
      : "36px 16px";
  return (
    <div
      style={{
        display: "flex",
        flexDirection: inline ? "row" : "column",
        alignItems: "center",
        justifyContent: "center",
        gap: inline ? 6 : 8,
        padding,
        textAlign: "center",
        color: "var(--pet-color-muted)",
        userSelect: "none",
      }}
    >
      {/* CSS 脉冲圆点：3 个 dot 错相位 0/0.2s/0.4s。reduced-motion 退化成
          静态点（与 PanelChat thinking glyph 同思路）。keyframes 直接 inject
          一次到 head；多组件共用同一动画名，不重复定义。 */}
      <style>{`
        @keyframes pet-loading-pulse {
          0%, 70%, 100% { opacity: 0.25; transform: scale(0.85); }
          35%           { opacity: 1;    transform: scale(1); }
        }
        @media (prefers-reduced-motion: reduce) {
          .pet-loading-dot { animation: none !important; opacity: 0.6 !important; }
        }
      `}</style>
      <div
        aria-hidden
        style={{
          display: "inline-flex",
          gap: 4,
          alignItems: "center",
        }}
      >
        {[0, 1, 2].map((i) => (
          <span
            key={i}
            className="pet-loading-dot"
            style={{
              display: "inline-block",
              width: inline ? 5 : 7,
              height: inline ? 5 : 7,
              borderRadius: "50%",
              background: "var(--pet-color-accent)",
              animation: `pet-loading-pulse 1.2s ${i * 0.15}s ease-in-out infinite`,
            }}
          />
        ))}
      </div>
      <div
        style={{
          fontSize: inline ? text.sm : compact ? text.base : text.md,
          fontWeight: fontWeight.medium,
          color: "var(--pet-color-fg)",
          opacity: 0.7,
        }}
      >
        {message}
      </div>
      {hint && !inline && (
        <div
          style={{
            fontSize: compact ? text.sm : text.base,
            color: "var(--pet-color-muted)",
            maxWidth: 260,
            lineHeight: lineHeight.base,
          }}
        >
          {hint}
        </div>
      )}
    </div>
  );
}
