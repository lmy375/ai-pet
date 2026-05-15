import { useEffect } from "react";

/// 跨面板共享 modal 容器：fixed 全屏 backdrop（带轻微 fade-in）+ 居中卡片
/// （pop-in 微动画 + shadow-lg）。点 backdrop 或按 Esc 关闭；按 card 自身
/// stopPropagation 避免穿透。
///
/// 替代散落各处的 `position:fixed inset:0 background:rgba(15,23,42,0.55)
/// + card` 组合 —— overlay alpha 0.4/0.45/0.55/0.6 多种、shadow 各不相同，
/// 视觉割裂。本组件用 shadow-lg token + 一个固定 overlay 配方。
///
/// 用法：
///   <Modal open={dialogOpen} onClose={...} maxWidth={460}>
///     <h3>...</h3>
///     ...
///   </Modal>
///
/// 不做的事：
/// - 不渲染默认 header / close-✕ 按钮 —— 让 caller 自己排（不同 dialog 标题
///   长度、附带 sub-action 不一）。
/// - 不带 portal —— 现 Tauri 单 root，DOM 顺序没问题；引 portal 会让 Esc /
///   click-outside 多写一段。
export function Modal({
  open,
  onClose,
  maxWidth = 460,
  children,
  /// 默认值 100：与既有 dialog 一致。如果与 marks modal (300+) 等共存时
  /// caller 可调高，让多层 modal 互不遮挡。
  zIndex = 100,
}: {
  open: boolean;
  onClose: () => void;
  maxWidth?: number;
  zIndex?: number;
  children: React.ReactNode;
}) {
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, onClose]);

  if (!open) return null;
  return (
    <div
      onClick={onClose}
      style={{
        position: "fixed",
        inset: 0,
        // backdrop：fg 半透混 + 轻量 blur 让背景内容隐去；blur 在 light / dark
        // 都柔和，且让 modal 卡片"浮"出来。
        background: "color-mix(in srgb, var(--pet-color-fg) 45%, transparent)",
        backdropFilter: "blur(6px) saturate(120%)",
        WebkitBackdropFilter: "blur(6px) saturate(120%)",
        zIndex,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        padding: 24,
        animation: "pet-modal-fade-in 140ms ease-out",
      }}
    >
      {/* keyframes 已迁到 src/styles/app.css 全局；这里不再重复 inject。 */}
      <div
        className="pet-modal-card"
        onClick={(e) => e.stopPropagation()}
        style={{
          position: "relative",
          width: "100%",
          maxWidth,
          // 顶端 accent 极淡渐变，与 .pet-card-elev 同语言；让 modal 卡片有
          // 一点 "this is special" 的视觉信号。
          background:
            "linear-gradient(180deg, color-mix(in srgb, var(--pet-color-accent) 4%, var(--pet-color-card)) 0%, var(--pet-color-card) 35%)",
          border:
            "1px solid color-mix(in srgb, var(--pet-color-accent) 8%, var(--pet-color-border))",
          borderRadius: 14,
          boxShadow: "var(--pet-shadow-lg)",
          padding: "20px 24px",
          maxHeight: "85vh",
          overflowY: "auto",
          animation: "pet-modal-pop 180ms ease-out",
        }}
      >
        {children}
      </div>
    </div>
  );
}
