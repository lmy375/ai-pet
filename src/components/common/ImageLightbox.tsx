import { useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { copyImageToClipboard } from "../../utils/clipboard";

/**
 * 图片放大预览。`src` 非 null 时挂 portal 到 document.body：黑底全屏盖住
 * 当前 panel，点暗背景 / Esc 关闭，点图片本身吸收事件不关闭（避免误关）。
 *
 * 多处调用方（CopyableMessage / ChatMini / ToolCallBlock）各自管自己的
 * activeSrc state —— 由于 lightbox 一次只显一张，多实例也只会有一个 portal
 * 在 body 上有 src，互不冲突。
 *
 * 不做"上一张 / 下一张"导航：当前用户的图集都是一两张为主，方向键导航属
 * 于"图库"心智，超本组件职责。
 */
export function ImageLightbox({
  src,
  onClose,
}: {
  src: string | null;
  onClose: () => void;
}) {
  // 复制 + 下载反馈：各自 idle / done / err，1.5s 自动回 idle。
  const [copyState, setCopyState] = useState<"idle" | "done" | "err">("idle");
  const [downloadState, setDownloadState] = useState<"idle" | "done" | "err">("idle");
  useEffect(() => {
    if (!src) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [src, onClose]);
  // 切图（src 改变）时重置反馈，避免上一张的"已复制 / 已下载"飘到新图上
  useEffect(() => {
    setCopyState("idle");
    setDownloadState("idle");
  }, [src]);
  if (!src) return null;
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
  /// 触发原生浏览器 save dialog：<a download> 对 data: / blob: / 同源 http URL
  /// 都有效。Tauri WKWebView / WebView2 都支持。filename 用 timestamp 让多次
  /// 保存不重名；扩展名按 src MIME 头取（data:image/png 取 png；data:image/jpeg
  /// 取 jpg；其它 fallback png）。
  const handleDownload = async (e: React.MouseEvent) => {
    e.stopPropagation();
    try {
      const mimeMatch = src.match(/^data:(image\/[^;]+)/);
      const mime = mimeMatch?.[1] ?? "image/png";
      const ext =
        mime === "image/jpeg" ? "jpg" :
        mime === "image/webp" ? "webp" :
        mime === "image/gif" ? "gif" :
        mime === "image/svg+xml" ? "svg" :
        "png";
      const a = document.createElement("a");
      a.href = src;
      a.download = `pet-image-${Date.now()}.${ext}`;
      document.body.appendChild(a);
      a.click();
      a.remove();
      setDownloadState("done");
    } catch (err) {
      console.error("download image failed:", err);
      setDownloadState("err");
    }
    window.setTimeout(() => setDownloadState("idle"), 1500);
  };
  return createPortal(
    <div
      onClick={onClose}
      style={{
        position: "fixed",
        inset: 0,
        background: "rgba(0,0,0,0.85)",
        zIndex: 9999,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        cursor: "zoom-out",
        animation: "pet-lightbox-fade-in 140ms ease-out",
      }}
      role="dialog"
      aria-modal="true"
      aria-label="image preview"
    >
      <style>{`@keyframes pet-lightbox-fade-in {
        from { opacity: 0 }
        to { opacity: 1 }
      }`}</style>
      <img
        src={src}
        alt=""
        onClick={(e) => e.stopPropagation()}
        style={{
          maxWidth: "92vw",
          maxHeight: "92vh",
          objectFit: "contain",
          boxShadow: "0 4px 16px rgba(0,0,0,0.4)",
          borderRadius: 4,
          cursor: "default",
        }}
      />
      <div
        style={{
          position: "absolute",
          bottom: 16,
          color: "rgba(255,255,255,0.7)",
          fontSize: 12,
          pointerEvents: "none",
          userSelect: "none",
        }}
      >
        Esc 或点暗背景关闭
      </div>
      {/* 顶部右侧浮一组按钮：📋 复制（剪贴板）+ 💾 另存为（本地）。
          stopPropagation 防点了之后 backdrop 关闭。状态 1.5s 自动回 idle。 */}
      <div
        style={{
          position: "absolute",
          top: 16,
          right: 16,
          display: "flex",
          gap: 8,
        }}
      >
        <button
          type="button"
          onClick={handleCopy}
          title={
            copyState === "done"
              ? "已复制图片到剪贴板（粘贴到聊天 / 文档即得二进制图）"
              : copyState === "err"
                ? "复制失败，详情看 console"
                : "复制图片到剪贴板"
          }
          aria-label="copy image to clipboard"
          style={{
            padding: "6px 12px",
            fontSize: 13,
            borderRadius: 8,
            border: "1px solid rgba(255,255,255,0.3)",
            background:
              copyState === "done"
                ? "color-mix(in srgb, var(--pet-tint-green-fg) 85%, transparent)"
                : copyState === "err"
                  ? "color-mix(in srgb, var(--pet-tint-red-fg) 85%, transparent)"
                  : "rgba(255,255,255,0.15)",
            color: "#fff",
            cursor: "pointer",
            backdropFilter: "blur(8px)",
          }}
        >
          {copyState === "done" ? "✓ 已复制" : copyState === "err" ? "✗ 复制失败" : "📋 复制"}
        </button>
        <button
          type="button"
          onClick={handleDownload}
          title={
            downloadState === "done"
              ? "已触发下载（看浏览器 / 系统下载位置）"
              : downloadState === "err"
                ? "下载失败，详情看 console"
                : "另存为本地文件"
          }
          aria-label="download image"
          style={{
            padding: "6px 12px",
            fontSize: 13,
            borderRadius: 8,
            border: "1px solid rgba(255,255,255,0.3)",
            background:
              downloadState === "done"
                ? "color-mix(in srgb, var(--pet-tint-green-fg) 85%, transparent)"
                : downloadState === "err"
                  ? "color-mix(in srgb, var(--pet-tint-red-fg) 85%, transparent)"
                  : "rgba(255,255,255,0.15)",
            color: "#fff",
            cursor: "pointer",
            backdropFilter: "blur(8px)",
          }}
        >
          {downloadState === "done"
            ? "✓ 已下载"
            : downloadState === "err"
              ? "✗ 下载失败"
              : "💾 另存为"}
        </button>
      </div>
    </div>,
    document.body,
  );
}
