import { useState, useRef, useEffect } from "react";
import { SendIcon } from "./Icons";
import { useI18n } from "../i18n";

interface Props {
  onSend: (message: string, images?: string[]) => void;
  isLoading: boolean;
  placeholder?: string;
}

/** Read a clipboard image File into a base64 `data:` URL. */
function readImage(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(reader.result as string);
    reader.onerror = () => reject(reader.error);
    reader.readAsDataURL(file);
  });
}

/** Shared chat input row (auto-resizing textarea + send button). Used by both
 *  the pet window and the panel — the caller provides the surrounding bar.
 *  Supports Cmd+V pasting images, sent to the model as multimodal content. */
export function ChatInput({ onSend, isLoading, placeholder }: Props) {
  const { t } = useI18n();
  const [input, setInput] = useState("");
  const [images, setImages] = useState<string[]>([]); // base64 data URLs
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  // Auto-resize textarea height
  useEffect(() => {
    const el = textareaRef.current;
    if (el) {
      el.style.height = "auto";
      el.style.height = Math.min(el.scrollHeight, 80) + "px";
    }
  }, [input]);

  const submit = () => {
    const text = input.trim();
    if ((!text && images.length === 0) || isLoading) return;
    onSend(text, images.length > 0 ? images : undefined);
    setInput("");
    setImages([]);
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      submit();
    }
  };

  // Capture pasted images; prevent the default so binary/filename text isn't
  // dumped into the textarea. Non-image pastes fall through to normal behavior.
  const handlePaste = (e: React.ClipboardEvent) => {
    const files = Array.from(e.clipboardData.items)
      .filter((it) => it.type.startsWith("image/"))
      .map((it) => it.getAsFile())
      .filter((f): f is File => f !== null);
    if (files.length === 0) return;
    e.preventDefault();
    Promise.all(files.map(readImage)).then((urls) => setImages((prev) => [...prev, ...urls]));
  };

  return (
    <div onMouseDown={(e) => e.stopPropagation()} className="flex flex-col gap-2">
      {images.length > 0 && (
        <div className="flex flex-wrap gap-2">
          {images.map((url, i) => (
            <div key={i} className="group relative">
              <img
                src={url}
                alt=""
                className="h-14 w-14 rounded-lg border border-slate-300/50 object-cover"
              />
              <button
                onClick={() => setImages((prev) => prev.filter((_, j) => j !== i))}
                title={t("chat.input.removeImage")}
                className="absolute -right-1.5 -top-1.5 flex h-5 w-5 items-center justify-center rounded-full bg-slate-700 text-white opacity-0 transition-opacity group-hover:opacity-100"
              >
                <span className="text-[12px] leading-none">×</span>
              </button>
            </div>
          ))}
        </div>
      )}
      <div className="flex items-end gap-2">
        <textarea
          ref={textareaRef}
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={handleKeyDown}
          onPaste={handlePaste}
          placeholder={placeholder ?? t("chat.input.placeholder")}
          rows={1}
          className="flex-1 resize-none overflow-hidden rounded-2xl border border-slate-300/50 bg-white/90 px-4 py-2.5 text-[14px] leading-snug text-slate-800 outline-none backdrop-blur-md transition-colors focus:border-accent placeholder:text-slate-400"
        />
        <button
          onClick={submit}
          disabled={isLoading || (!input.trim() && images.length === 0)}
          title={t("chat.input.send")}
          className="flex h-10 w-10 flex-shrink-0 items-center justify-center rounded-full bg-accent text-white transition-colors hover:bg-accent-hover disabled:cursor-not-allowed disabled:bg-slate-300"
        >
          <SendIcon className="h-5 w-5 -translate-x-px" />
        </button>
      </div>
    </div>
  );
}
