import { useState } from "react";
import { ChevronRight } from "../Icons";
import { useI18n } from "../../i18n";

interface Props {
  text: string;
  /** True while the model is still streaming its thoughts (shows a live hint). */
  streaming?: boolean;
}

/** Collapsed-by-default chain-of-thought from a reasoning model. The thinking is
 *  display-only (never re-sent to the model); expand to read it. Styled dark to
 *  read as "internal", distinct from the answer above/below it. */
export function ReasoningBlock({ text, streaming = false }: Props) {
  const { t } = useI18n();
  const [expanded, setExpanded] = useState(false);

  return (
    <div className="mb-1.5 text-[13px]">
      <button
        type="button"
        onClick={() => setExpanded((e) => !e)}
        className="flex select-none items-center gap-1 text-[12px] text-slate-500 hover:text-slate-700"
      >
        <ChevronRight className={`h-3.5 w-3.5 shrink-0 transition-transform ${expanded ? "rotate-90" : ""}`} />
        <span>{t("chat.reasoning")}</span>
        {streaming && <span className="animate-pulse text-slate-400">…</span>}
      </button>
      {expanded && (
        <pre className="mt-1 max-h-[260px] overflow-y-auto whitespace-pre-wrap break-words rounded-lg bg-slate-800 p-2.5 font-sans text-[12px] leading-relaxed text-slate-300">
          {text}
        </pre>
      )}
    </div>
  );
}
