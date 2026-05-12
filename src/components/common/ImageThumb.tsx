import { useState } from "react";
import { copyImageToClipboard } from "../../utils/clipboard";

/// 每个 webview 第一次渲染 ImageThumb 时往 <head> 注入 hover CSS。Panel / 桌面
/// 是不同 webview window，各自独立 document —— 模块级 boolean 单 window 内就
/// 够了，跨 window 不共享是好事（每个 window 第一次用时自己注入）。
let stylesInjected = false;
function injectStyles() {
  if (stylesInjected || typeof document === "undefined") return;
  stylesInjected = true;
  const tag = document.createElement("style");
  tag.dataset.petComponent = "ImageThumb";
  tag.textContent = `
    .pet-image-thumb:hover .pet-image-thumb-copy { opacity: 0.92 !important; }
    .pet-image-thumb .pet-image-thumb-copy:hover { opacity: 1 !important; }
  `;
  document.head.appendChild(tag);
}

/**
 * 聊天 / 工具卡片里的图片缩略图。两个交互：
 * - 点图本体 → 调 caller 传入的 onOpen（一般用于打开 ImageLightbox）
 * - hover 时右上角浮 📋 复制按钮 → 把图片二进制写到剪贴板
 *
 * 复制反馈走内部 state，1.5s 自清。caller 只需把 src + onOpen 传进来。
 *
 * 不在此组件内挂 ImageLightbox：避免每个 thumb 都 portal 一份；caller 在外层
 * 持单个 src state 复用 lightbox。
 */
export function ImageThumb({
  src,
  onOpen,
  maxSize = 160,
}: {
  src: string;
  onOpen: () => void;
  /// 缩略图最大边，px。caller 在桌面 ChatMini 等紧凑视图里压到 96，PanelChat
  /// 历史 / give_image 工具卡片等宽屏视图用 160。
  maxSize?: number;
}) {
  injectStyles();
  const [copyState, setCopyState] = useState<"idle" | "done" | "err">("idle");
  const handleCopy = async (e: React.MouseEvent) => {
    e.stopPropagation();
    try {
      await copyImageToClipboard(src);
      setCopyState("done");
    } catch (err) {
      console.error("copy image failed:", err);
      setCopyState("err");
    }
    window.setTimeout(() => setCopyState("idle"), 1500);
  };
  return (
    <div
      className="pet-image-thumb"
      style={{ position: "relative", display: "inline-block" }}
    >
      <img
        src={src}
        alt=""
        onClick={(e) => {
          // stopPropagation 防 ChatMini 等父级 onDoubleClick 在双击图片时被触发
          // —— 用户快速双击图片只想打开 lightbox，不想顺带打开整个 panel。
          e.stopPropagation();
          onOpen();
        }}
        onDoubleClick={(e) => e.stopPropagation()}
        title="点击放大"
        style={{
          maxWidth: maxSize,
          maxHeight: maxSize,
          borderRadius: 6,
          display: "block",
          objectFit: "cover",
          cursor: "zoom-in",
        }}
      />
      <button
        type="button"
        className="pet-image-thumb-copy"
        onClick={handleCopy}
        title={
          copyState === "done"
            ? "已复制图片到剪贴板"
            : copyState === "err"
              ? "复制失败，详情看 console"
              : "复制图片到剪贴板"
        }
        aria-label="copy image to clipboard"
        style={{
          position: "absolute",
          top: 4,
          right: 4,
          padding: "2px 6px",
          fontSize: 10,
          borderRadius: 4,
          border: "none",
          background:
            copyState === "done"
              ? "color-mix(in srgb, var(--pet-tint-green-fg) 92%, transparent)"
              : copyState === "err"
                ? "color-mix(in srgb, var(--pet-tint-red-fg) 92%, transparent)"
                : "rgba(15,23,42,0.78)",
          color: "#fff",
          cursor: "pointer",
          // 默认隐藏，hover wrapper 时浮出；copyState 非 idle 时强制可见。
          opacity: copyState === "idle" ? 0 : 1,
          transition: "opacity 120ms ease-out",
        }}
      >
        {copyState === "done" ? "✓" : copyState === "err" ? "✗" : "📋"}
      </button>
    </div>
  );
}
