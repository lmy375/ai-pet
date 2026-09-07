import { useState, type ReactNode } from "react";
import { ImageLightbox } from "./ImageLightbox";
import { formatHm } from "../../utils/format";
import { useI18n } from "../../i18n";

interface Props {
  role: "user" | "assistant";
  error?: boolean;
  images?: string[]; // base64 data URLs, rendered above the text (user messages)
  /** Sender label for the meta line above the bubble. Omitted in the pet
   *  window, which is too small for a name/time row per message. */
  name?: string;
  /** Epoch ms shown next to `name`. Only used when `name` is set. */
  ts?: number;
  children: ReactNode;
}

/** Chat bubble: user = accent blue (right), assistant = white card (left). With
 *  `name` it grows a meta row (sender + time) — the panel layout; without it,
 *  the bubble alone (pet window). */
export function MessageBubble({ role, error = false, images, name, ts, children }: Props) {
  const { t } = useI18n();
  const isUser = role === "user";
  const tone = error
    ? "bg-red-50 text-red-600 border border-red-200 rounded-bl-md"
    : isUser
      ? "bg-accent text-white rounded-br-md"
      : "bg-surface text-ink border border-line rounded-bl-md";

  const hasImages = images && images.length > 0;
  const [zoomed, setZoomed] = useState<string | null>(null);

  return (
    <div className={`flex ${isUser ? "justify-end" : "justify-start"}`}>
      <div className={`flex min-w-0 max-w-[80%] flex-col ${isUser ? "items-end" : "items-start"}`}>
        {name && (
          <div className="mb-1 flex items-baseline gap-1.5 px-0.5 text-meta">
            <span className="font-medium text-ink-soft">{name}</span>
            {ts !== undefined && <span className="text-ink-faint">{formatHm(ts)}</span>}
          </div>
        )}
        <div
          className={`min-w-0 whitespace-pre-wrap break-words rounded-bubble px-3.5 py-2 text-chat leading-relaxed shadow-card ${tone}`}
        >
          {hasImages && (
            <div className="mb-1.5 flex flex-col gap-1.5">
              {images!.map((url, i) => (
                <img
                  key={i}
                  src={url}
                  alt=""
                  onClick={() => setZoomed(url)}
                  title={t("common.zoomImage")}
                  className="max-w-full cursor-zoom-in rounded-lg object-contain"
                />
              ))}
            </div>
          )}
          {children}
        </div>
      </div>
      {zoomed && <ImageLightbox src={zoomed} onClose={() => setZoomed(null)} />}
    </div>
  );
}
