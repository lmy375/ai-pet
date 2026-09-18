import { useState, type ReactNode } from "react";
import { CopyButton } from "./CopyButton";
import { IconActionButton } from "./IconButton";
import { ImageLightbox } from "./ImageLightbox";
import { PencilIcon } from "../Icons";
import { formatWhen } from "../../utils/format";
import { useI18n } from "../../i18n";

interface Props {
  role: "user" | "assistant";
  error?: boolean;
  images?: string[]; // base64 data URLs, rendered above the text (user messages)
  /** Sender label for the meta line above the bubble. Omitted in the pet
   *  window, which is too small for a name/time row per message. */
  name?: string;
  /** Epoch ms. With `name` it becomes the "7 分钟前" label in the action row. */
  ts?: number;
  /** Raw text of the message. When set, a copy button joins the actions. */
  copyText?: string;
  /** Turns the message into an editable one (user messages): a pencil that
   *  reopens it in the composer. */
  onEdit?: () => void;
  children: ReactNode;
}

/** Chat bubble: user = accent blue (right), assistant = white card (left). With
 *  `name` it grows a meta row (sender) plus an action row (time + buttons) —
 *  the panel layout; without it, the bubble with the buttons alone in the
 *  gutter beside it (pet window, too small for a row per message). */
export function MessageBubble({ role, error = false, images, name, ts, copyText, onEdit, children }: Props) {
  const { t } = useI18n();
  const isUser = role === "user";
  const tone = error
    ? "bg-red-50 text-red-600 border border-red-200 rounded-bl-md"
    : isUser
      ? "bg-accent text-white rounded-br-md"
      : "bg-surface text-ink border border-line rounded-bl-md";

  const hasImages = images && images.length > 0;
  const [zoomed, setZoomed] = useState<string | null>(null);

  // Buttons stay invisible until the message is hovered; they always occupy
  // their space, so revealing them never shifts anything.
  const fade = "opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100";
  const buttons = (
    <>
      {copyText?.trim() && <CopyButton text={copyText} className={fade} />}
      {onEdit && (
        <IconActionButton type="button" size="sm" title={t("chat.edit")} onClick={onEdit} className={fade}>
          <PencilIcon className="h-3.5 w-3.5" />
        </IconActionButton>
      )}
    </>
  );
  const hasActions = !!copyText?.trim() || !!onEdit;
  // Compact mode has no room for a row under every bubble, so the buttons sit
  // in the free gutter on the bubble's inner side instead (the thread keeps its
  // periodic time separators there).
  const gutter = hasActions && !name ? <div className="mb-0.5 flex items-center">{buttons}</div> : null;

  return (
    <div className={`group flex items-end gap-1 ${isUser ? "justify-end" : "justify-start"}`}>
      {isUser && gutter}
      <div className={`flex min-w-0 max-w-[80%] flex-col ${isUser ? "items-end" : "items-start"}`}>
        {name && <div className="mb-1 px-0.5 text-meta font-medium text-ink-soft">{name}</div>}
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
        {name && (hasActions || ts !== undefined) && (
          <div className="mt-0.5 flex items-center gap-0.5 px-0.5 text-meta text-ink-faint">
            {ts !== undefined && <span className="mr-1">{formatWhen(ts, t)}</span>}
            {buttons}
          </div>
        )}
      </div>
      {!isUser && gutter}
      {zoomed && <ImageLightbox src={zoomed} onClose={() => setZoomed(null)} />}
    </div>
  );
}
