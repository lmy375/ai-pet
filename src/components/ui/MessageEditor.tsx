import { useEffect, useRef, useState } from "react";
import { Button } from "./Button";
import { useI18n } from "../../i18n";

interface Props {
  /** The message's current text, the draft's starting point. */
  text: string;
  /** How many messages sit after this one — they go away on resend. */
  dropCount: number;
  onCancel: () => void;
  onSubmit: (text: string) => void;
}

/** In-place composer that replaces a sent user message while it is being
 *  rewritten. Confirming resends it, which drops everything that followed —
 *  hence the count, stated before the button rather than in a dialog after it. */
export function MessageEditor({ text, dropCount, onCancel, onSubmit }: Props) {
  const { t } = useI18n();
  const [draft, setDraft] = useState(text);
  const areaRef = useRef<HTMLTextAreaElement>(null);

  // Grow with the draft (same rule as the composer, with more room: this one
  // starts out holding a whole message).
  useEffect(() => {
    const el = areaRef.current;
    if (el) {
      el.style.height = "auto";
      el.style.height = Math.min(el.scrollHeight, 200) + "px";
    }
  }, [draft]);
  const submit = () => {
    if (draft.trim()) onSubmit(draft.trim());
  };

  return (
    <div className="flex justify-end">
      <div className="flex w-full max-w-[85%] flex-col gap-2 rounded-card border border-accent bg-surface p-2 shadow-card">
        <textarea
          ref={areaRef}
          autoFocus
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onFocus={(e) => e.currentTarget.setSelectionRange(draft.length, draft.length)}
          onKeyDown={(e) => {
            if (e.key === "Escape") onCancel();
            else if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
              e.preventDefault();
              submit();
            }
          }}
          className="w-full resize-none bg-transparent px-1 text-chat leading-relaxed text-ink outline-none"
        />
        <div className="flex flex-wrap items-center justify-end gap-2">
          {dropCount > 0 && (
            <span className="mr-auto text-meta text-ink-faint">{t("chat.edit.dropHint", { count: dropCount })}</span>
          )}
          <Button variant="ghost" size="sm" onClick={onCancel}>
            {t("common.cancel")}
          </Button>
          <Button size="sm" disabled={!draft.trim()} onClick={submit}>
            {t("chat.edit.resend")}
          </Button>
        </div>
      </div>
    </div>
  );
}
