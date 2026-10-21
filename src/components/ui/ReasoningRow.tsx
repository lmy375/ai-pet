import { useState } from "react";
import { ExpandChevron, LightbulbIcon } from "../Icons";
import { Markdown } from "./Markdown";
import { useI18n } from "../../i18n";

interface Props {
  text: string;
  /** True while the model is still streaming its thoughts (the collapsed row
   *  then tracks the latest thought instead of the first). */
  streaming?: boolean;
}

/** Adjacent bold runs (`**A****B**`) are how OpenAI-style summary parts reach us:
 *  genai hands each part over as its own chunk and the stream concatenates them
 *  with no separator, so markdown would render one glued line. Split them into
 *  separate paragraphs. Display-only — the stored reasoning is untouched. */
function sections(text: string) {
  return text.replace(/\*\*\*\*/g, "**\n\n**");
}

/** The thought shown while collapsed: the first one, or the latest while the
 *  model is still thinking. Returned as markdown source, not stripped text — the
 *  row goes through the same renderer as the expanded body, so opening the block
 *  never re-styles the line that was already on screen. */
function headline(text: string, latest: boolean) {
  const lines = sections(text)
    .split("\n")
    .map((l) => l.trim())
    .filter(Boolean);
  const line = (latest ? lines[lines.length - 1] : lines[0]) ?? "";
  return closeMarks(line || text.trim());
}

/** A line captured mid-stream can end inside an emphasis or code run
 *  (`**Formul`). Closing the dangling marker keeps the row from rendering a raw
 *  `**` for one chunk and then snapping to bold on the next. */
function closeMarks(line: string) {
  let out = line;
  if ((out.match(/\*\*/g)?.length ?? 0) % 2) out += "**";
  if ((out.match(/`/g)?.length ?? 0) % 2) out += "`";
  return out;
}

/** Markdown tone shared by both states, so toggling only ever adds or removes
 *  text — it never restyles what was already rendered. */
const THOUGHT = "[&_:is(h1,h2,h3,h4)]:text-note";

/** Collapsed the block is exactly one row, so every element is flattened inline
 *  (a `-webkit-box` clamp is only dependable over inline content) and block
 *  chrome — margins, the code-fence card, list bullets — is dropped. Whatever
 *  the thought turns out to be, heading or fence or bullet, the row keeps the
 *  same single-line height. */
const CLAMPED = [
  "line-clamp-1",
  "[&_*]:m-0 [&_*]:inline",
  "[&_:is(ul,ol)]:list-none [&_:is(ul,ol)]:pl-0",
  "[&_:is(pre,blockquote)]:border-0 [&_:is(pre,blockquote)]:bg-transparent [&_:is(pre,blockquote)]:p-0",
  // A fence keeps `white-space: pre` and its own scroll box, which would run the
  // row past the bubble instead of ellipsizing; an inline code chip's padding
  // would make the row 1px taller than a plain one.
  "[&_pre]:max-h-none [&_pre]:overflow-hidden [&_pre]:whitespace-normal",
  "[&_code]:py-0 [&_:is(pre,code)]:text-note",
].join(" ");

/** Chain-of-thought from a reasoning model, shown above the answer it produced:
 *  a compact row (chevron, accent icon + label, one-line preview) that expands
 *  in place. Same row in a merged activity card and inside a chat bubble — only
 *  `embedded` drops the card padding and paints the expanded body inline.
 *
 *  The thinking is display-only — it is never re-sent to the model.
 *
 *  Not a <button>: the rendered markdown may contain links, which must not nest
 *  inside one. */
export function ReasoningRow({ text, streaming = false, embedded = false }: Props & { embedded?: boolean }) {
  const { t } = useI18n();
  const [expanded, setExpanded] = useState(false);

  return (
    <div className={embedded ? "mb-2" : ""}>
      <div
        role="button"
        tabIndex={0}
        aria-expanded={expanded}
        onClick={(e) => {
          // A link opens (Markdown hands it to the OS browser) and a click that
          // ends a selection leaves the thought expanded, so it stays copyable.
          if ((e.target as HTMLElement).closest("a")) return;
          if (window.getSelection()?.isCollapsed === false) return;
          setExpanded((v) => !v);
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter") setExpanded((v) => !v);
          if (e.key === "Escape") setExpanded(false);
        }}
        className={`flex cursor-pointer select-none items-center gap-1.5 text-ink-soft ${
          embedded ? "py-0.5" : "px-3 py-1.5"
        }`}
      >
        <ExpandChevron expanded={expanded} className="h-3.5 w-3.5 shrink-0 text-ink-faint" />
        <LightbulbIcon className="h-4 w-4 shrink-0 text-accent" />
        <span className="shrink-0 font-semibold text-accent">{t("chat.reasoning")}</span>
        <div className={`min-w-0 flex-1 overflow-hidden ${streaming && !expanded ? "animate-pulse" : ""}`}>
          <Markdown text={headline(text, streaming)} className={`${THOUGHT} ${CLAMPED} text-note text-ink-faint`} />
        </div>
      </div>
      {expanded && (
        <div
          className={
            embedded
              ? "mt-1 max-h-[260px] overflow-y-auto border-t border-line/70 pt-1.5"
              : "max-h-[260px] overflow-y-auto border-t border-line bg-surface px-3 py-2"
          }
        >
          <Markdown text={sections(text)} className={`${THOUGHT} text-note leading-relaxed text-ink-faint`} />
        </div>
      )}
    </div>
  );
}
