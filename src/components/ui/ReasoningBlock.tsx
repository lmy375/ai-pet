import { useState } from "react";
import { ChevronRight } from "../Icons";
import { useI18n } from "../../i18n";

interface Props {
  text: string;
  /** True while the model is still streaming its thoughts (shows a live hint). */
  streaming?: boolean;
}

/** Collapsed-by-default chain-of-thought from a reasoning model, shown above the
 *  answer it produced. The thinking is display-only (never re-sent to the model);
 *  expand to read it. Tinted with the "think" hue so it never reads as part of
 *  the answer. */
export function ReasoningBlock({ text, streaming = false }: Props) {
  const { t } = useI18n();
  const [expanded, setExpanded] = useState(false);

  return (
    <div className="mb-2 overflow-hidden rounded-field border border-think-line bg-think-soft">
      <button
        type="button"
        onClick={() => setExpanded((e) => !e)}
        className="flex w-full select-none items-center gap-1.5 px-2.5 py-1.5 text-note font-medium text-think"
      >
        <ChevronRight className={`h-3.5 w-3.5 shrink-0 transition-transform ${expanded ? "rotate-90" : ""}`} />
        <span>{t("chat.reasoning")}</span>
        {streaming && <span className="animate-pulse">…</span>}
      </button>
      {expanded && (
        <pre className="max-h-[260px] overflow-y-auto whitespace-pre-wrap break-words border-t border-think-line px-2.5 py-2 font-sans text-note leading-relaxed text-think/85">
          {text}
        </pre>
      )}
    </div>
  );
}
