import { useEffect, useRef, useState } from "react";
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
  lazy = false,
}: {
  src: string;
  onOpen: () => void;
  /// 缩略图最大边，px。caller 在桌面 ChatMini 等紧凑视图里压到 96，PanelChat
  /// 历史 / give_image 工具卡片等宽屏视图用 160。
  maxSize?: number;
  /// 懒加载：IntersectionObserver 监听 wrapper，进入 viewport 前 300px 才把
  /// `<img src>` 实际挂载。专为 detail.md 长 markdown 含多张内嵌 data URL 优化
  /// —— 原生 `loading="lazy"` 对 data URL 无效（不走网络只走 decode），必须靠
  /// IO 控制 mount 才能避免一次性 decode 全部 base64 卡 paint。
  /// 默认 false 以免影响 ChatMini / 工具卡片等小集合场景。
  lazy?: boolean;
}) {
  injectStyles();
  const [copyState, setCopyState] = useState<"idle" | "done" | "err">("idle");
  // lazy=false 直接 true；lazy=true 时先 false，IO 命中后翻 true 触发 img
  // mount。命中后 disconnect 保证仅 mount 一次（即便 wrapper 后续滚出 viewport
  // 也不卸载，避免反复 decode + 闪烁）。
  const [shouldLoad, setShouldLoad] = useState<boolean>(!lazy);
  const wrapperRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!lazy || shouldLoad) return;
    const el = wrapperRef.current;
    if (!el) return;
    // Tauri WKWebView 上 IO 已 universally available；防御 typeof check 兜底
    // 老环境 / SSR：缺失时直接 fallback 到立即加载（功能正确性优先于性能）。
    if (typeof IntersectionObserver === "undefined") {
      setShouldLoad(true);
      return;
    }
    const obs = new IntersectionObserver(
      (entries) => {
        for (const e of entries) {
          if (e.isIntersecting) {
            setShouldLoad(true);
            obs.disconnect();
            break;
          }
        }
      },
      // rootMargin 300px：滚动到距 viewport 还剩 ~1 屏时已开始 decode，让用户
      // 等到真的看到位置时大概率已 ready，体感无卡顿。再大浪费、再小看得到加载。
      { rootMargin: "300px 0px" },
    );
    obs.observe(el);
    return () => obs.disconnect();
  }, [lazy, shouldLoad]);
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
  // 占位尺寸：未知图片实际比例时取 width:maxSize / height:maxSize×0.6 近似
  // 16:10 横屏截图。让 layout reservation 接近真值 → 加载完成时位移最小。
  const placeholderHeight = Math.round(maxSize * 0.6);
  return (
    <div
      ref={wrapperRef}
      className="pet-image-thumb"
      style={{ position: "relative", display: "inline-block" }}
    >
      {shouldLoad ? (
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
      ) : (
        // 占位 div：固定尺寸保 layout，点击 = 强制加载 + onOpen（让用户主动
        // 戳穿懒加载）。视觉是浅灰底 + 🖼 + "加载中"小字提示，与 ImageLightbox
        // 占位风格一致。
        <div
          onClick={(e) => {
            e.stopPropagation();
            setShouldLoad(true);
            onOpen();
          }}
          onDoubleClick={(e) => e.stopPropagation()}
          title="懒加载中 — 点击立即加载并放大"
          style={{
            width: maxSize,
            height: placeholderHeight,
            borderRadius: 6,
            background:
              "color-mix(in srgb, var(--pet-card-bg, #f3f4f6) 70%, transparent)",
            border:
              "1px dashed color-mix(in srgb, var(--pet-fg-muted, #9ca3af) 35%, transparent)",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            cursor: "zoom-in",
            color: "var(--pet-fg-muted, #6b7280)",
            fontSize: 11,
            gap: 4,
          }}
        >
          <span style={{ fontSize: 18 }}>🖼</span>
          <span>懒加载</span>
        </div>
      )}
      {shouldLoad && (
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
      )}
    </div>
  );
}
