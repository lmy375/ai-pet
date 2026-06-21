import { useState, type ReactNode } from "react";
import { ImageLightbox } from "./ImageLightbox";

interface Props {
  role: "user" | "assistant";
  error?: boolean;
  images?: string[]; // base64 data URLs, rendered above the text (user messages)
  children: ReactNode;
}

/** iOS Messages-style bubble: user = accent blue (right), assistant = gray (left). */
export function MessageBubble({ role, error = false, images, children }: Props) {
  const isUser = role === "user";
  const tone = error
    ? "bg-red-50 text-red-600 rounded-bl-md"
    : isUser
      ? "bg-accent text-white rounded-br-md"
      : "bg-slate-200 text-slate-900 rounded-bl-md";

  const hasImages = images && images.length > 0;
  const [zoomed, setZoomed] = useState<string | null>(null);

  return (
    <div className={`flex ${isUser ? "justify-end" : "justify-start"}`}>
      <div
        className={`max-w-[80%] whitespace-pre-wrap break-words rounded-2xl px-3.5 py-2 text-[14px] leading-relaxed ${tone}`}
      >
        {hasImages && (
          <div className="mb-1.5 flex flex-col gap-1.5">
            {images!.map((url, i) => (
              <img
                key={i}
                src={url}
                alt=""
                onClick={() => setZoomed(url)}
                title="点击查看大图"
                className="max-w-full cursor-zoom-in rounded-lg object-contain"
              />
            ))}
          </div>
        )}
        {children}
      </div>
      {zoomed && <ImageLightbox src={zoomed} onClose={() => setZoomed(null)} />}
    </div>
  );
}
