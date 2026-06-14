import { useState, useRef, useEffect } from "react";
import { SendIcon } from "./Icons";

interface Props {
  onSend: (message: string) => void;
  isLoading: boolean;
  placeholder?: string;
}

/** Shared chat input row (auto-resizing textarea + send button). Used by both
 *  the pet window and the panel — the caller provides the surrounding bar. */
export function ChatInput({ onSend, isLoading, placeholder = "输入消息..." }: Props) {
  const [input, setInput] = useState("");
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
    if (!text || isLoading) return;
    onSend(text);
    setInput("");
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      submit();
    }
  };

  return (
    <div onMouseDown={(e) => e.stopPropagation()} className="flex items-end gap-2">
      <textarea
        ref={textareaRef}
        value={input}
        onChange={(e) => setInput(e.target.value)}
        onKeyDown={handleKeyDown}
        placeholder={placeholder}
        rows={1}
        className="flex-1 resize-none overflow-hidden rounded-2xl border border-slate-300/50 bg-white/90 px-4 py-2.5 text-[14px] leading-snug text-slate-800 outline-none backdrop-blur-md transition-colors focus:border-accent placeholder:text-slate-400"
      />
      <button
        onClick={submit}
        disabled={isLoading || !input.trim()}
        title="发送"
        className="flex h-10 w-10 flex-shrink-0 items-center justify-center rounded-full bg-accent text-white transition-colors hover:bg-accent-hover disabled:cursor-not-allowed disabled:bg-slate-300"
      >
        <SendIcon className="h-5 w-5 -translate-x-px" />
      </button>
    </div>
  );
}
