import { useEffect } from "react";
import { createPortal } from "react-dom";

// Full-screen overlay that shows one image at its natural size. Used by chat
// bubbles and the LLM log to "double-click to enlarge". Click the backdrop or
// press Esc to close.
export function ImageLightbox({ src, onClose }: { src: string; onClose: () => void }) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  return createPortal(
    <div
      onClick={onClose}
      className="fixed inset-0 z-[9999] flex items-center justify-center bg-black/70 p-6 backdrop-blur-sm"
    >
      <img
        src={src}
        alt=""
        onClick={(e) => e.stopPropagation()}
        className="max-h-full max-w-full rounded-lg object-contain shadow-2xl"
      />
    </div>,
    document.body,
  );
}
