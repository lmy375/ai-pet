import { useState, useRef, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { SendIcon } from "./Icons";
import { useTauriEvent } from "../hooks/useTauriEvent";
import type { SkillItem, SkillsInfo } from "../hooks/useSettings";
import { useI18n } from "../i18n";

interface Props {
  onSend: (message: string, images?: string[]) => void;
  isLoading: boolean;
  placeholder?: string;
}

const SKILL_PREFIX = "/skill:";

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
 *  Supports Cmd+V pasting images, sent to the model as multimodal content.
 *
 *  Typing `/` opens a completion list of the installed skills; accepting one
 *  fills in `/skill:<slug> ` so a task can be typed after it. On submit the
 *  command is expanded by the engine (`expand_skill_command`) into the plain
 *  user message that's actually sent — the wording lives in pet-core, not here. */
export function ChatInput({ onSend, isLoading, placeholder }: Props) {
  const { t } = useI18n();
  const [input, setInput] = useState("");
  const [images, setImages] = useState<string[]>([]); // base64 data URLs
  const [skills, setSkills] = useState<SkillItem[]>([]);
  const [menuSel, setMenuSel] = useState(0);
  const [menuDismissed, setMenuDismissed] = useState(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  // Broken skills can't be invoked, so they're never offered.
  const loadSkills = () => {
    invoke<SkillsInfo>("list_skills")
      .then((info) => setSkills(info.skills.filter((s) => s.error === null)))
      .catch(() => setSkills([]));
  };
  useEffect(loadSkills, []);
  // The skills dir is a setting, so a change there must refresh the list.
  useTauriEvent("settings-changed", loadSkills);

  // Auto-resize textarea height
  useEffect(() => {
    const el = textareaRef.current;
    if (el) {
      el.style.height = "auto";
      el.style.height = Math.min(el.scrollHeight, 80) + "px";
    }
  }, [input]);

  // Candidates for the current input: only while typing a single `/…` token.
  const matches =
    menuDismissed || !input.startsWith("/") || /\s/.test(input)
      ? []
      : skills.filter((s) => `${SKILL_PREFIX}${s.slug}`.startsWith(input));
  const menuOpen = matches.length > 0;
  const sel = Math.min(menuSel, matches.length - 1);

  const accept = (skill: SkillItem) => {
    // Trailing space, no auto-send: the owner still has to type the task.
    setInput(`${SKILL_PREFIX}${skill.slug} `);
    setMenuDismissed(true);
    textareaRef.current?.focus();
  };

  const submit = async () => {
    const text = input.trim();
    if ((!text && images.length === 0) || isLoading) return;
    setInput("");
    setImages([]);
    setMenuDismissed(false);
    // An unknown slug expands to nothing — send it as typed rather than
    // swallowing the message behind an error.
    const expanded = text.startsWith(SKILL_PREFIX)
      ? await invoke<string | null>("expand_skill_command", { line: text }).catch(() => null)
      : null;
    onSend(expanded ?? text, images.length > 0 ? images : undefined);
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (menuOpen) {
      if (e.key === "ArrowDown") {
        e.preventDefault();
        return setMenuSel(Math.min(sel + 1, matches.length - 1));
      }
      if (e.key === "ArrowUp") {
        e.preventDefault();
        return setMenuSel(Math.max(sel - 1, 0));
      }
      if (e.key === "Escape") {
        e.preventDefault();
        return setMenuDismissed(true);
      }
      // While the menu is open Enter completes instead of sending — same rule
      // as the CLI palette, so Enter never fires a skill with no task.
      if (e.key === "Enter" || e.key === "Tab") {
        e.preventDefault();
        return accept(matches[sel]);
      }
    }
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
      <div className="relative flex items-end gap-2">
        {menuOpen && (
          <div className="absolute bottom-full left-0 right-12 z-20 mb-2 max-h-52 overflow-y-auto rounded-2xl border border-slate-300/50 bg-white/95 py-1 shadow-lg backdrop-blur-md">
            <div className="px-3 py-1 text-[10px] font-medium uppercase tracking-wide text-slate-400">
              {t("chat.skillMenu.title")}
            </div>
            {matches.map((s, i) => (
              <button
                key={s.slug}
                onMouseDown={(e) => {
                  e.preventDefault();
                  accept(s);
                }}
                onMouseEnter={() => setMenuSel(i)}
                className={`flex w-full items-baseline gap-2 px-3 py-1.5 text-left ${
                  i === sel ? "bg-accent/10" : ""
                }`}
              >
                <span className="shrink-0 font-mono text-[12px] text-accent">
                  {SKILL_PREFIX}
                  {s.slug}
                </span>
                <span className="truncate text-[12px] text-slate-500">{s.description}</span>
              </button>
            ))}
          </div>
        )}
        <textarea
          ref={textareaRef}
          value={input}
          onChange={(e) => {
            setInput(e.target.value);
            setMenuDismissed(false);
            setMenuSel(0);
          }}
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
