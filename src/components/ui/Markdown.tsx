import { memo } from "react";
import ReactMarkdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import remarkBreaks from "remark-breaks";
import { openUrl } from "@tauri-apps/plugin-opener";

/** Element styles for assistant markdown inside a chat bubble. Kept tight
 *  (bubble-sized headings, small margins) so a formatted answer still reads as
 *  a message and not a document. */
const components: Components = {
  p: ({ children }) => <p className="mb-2 last:mb-0">{children}</p>,
  h1: ({ children }) => <h1 className="mb-1.5 mt-1 text-[15px] font-semibold first:mt-0">{children}</h1>,
  h2: ({ children }) => <h2 className="mb-1.5 mt-1 text-[15px] font-semibold first:mt-0">{children}</h2>,
  h3: ({ children }) => <h3 className="mb-1 mt-1 text-[14px] font-semibold first:mt-0">{children}</h3>,
  h4: ({ children }) => <h4 className="mb-1 mt-1 text-[14px] font-semibold first:mt-0">{children}</h4>,
  ul: ({ children, className }) => (
    <ul className={`mb-2 space-y-0.5 last:mb-0 ${className?.includes("contains-task-list") ? "pl-1" : "list-disc pl-5"}`}>
      {children}
    </ul>
  ),
  ol: ({ children, start }) => (
    <ol start={start} className="mb-2 list-decimal space-y-0.5 pl-5 last:mb-0">
      {children}
    </ol>
  ),
  li: ({ children, className }) => (
    <li className={className?.includes("task-list-item") ? "list-none" : undefined}>{children}</li>
  ),
  input: ({ type, checked }) =>
    type === "checkbox" ? <input type="checkbox" checked={checked} readOnly className="mr-1.5 align-[-1px]" /> : null,
  blockquote: ({ children }) => (
    <blockquote className="mb-2 border-l-2 border-accent-line pl-2.5 opacity-75 last:mb-0">{children}</blockquote>
  ),
  // Inline code. Inside a fence the same element is neutralized by the `pre`
  // rules below, so no `inline` prop sniffing is needed.
  code: ({ children }) => (
    <code className="rounded bg-code/8 px-1 py-px font-mono text-[12.5px]">{children}</code>
  ),
  pre: ({ children }) => (
    <pre className="mb-2 max-h-[260px] overflow-auto rounded-lg border border-line/60 bg-surface-soft px-2.5 py-2 font-mono text-[12px] leading-relaxed text-inherit last:mb-0 [&>code]:bg-transparent [&>code]:p-0 [&>code]:text-[12px]">
      {children}
    </pre>
  ),
  table: ({ children }) => (
    <div className="mb-2 overflow-x-auto last:mb-0">
      <table className="w-full border-collapse text-[13px]">{children}</table>
    </div>
  ),
  th: ({ children, style }) => (
    <th style={style} className="border border-line bg-surface-soft px-2 py-1 text-left font-semibold">
      {children}
    </th>
  ),
  td: ({ children, style }) => (
    <td style={style} className="border border-line px-2 py-1 align-top">
      {children}
    </td>
  ),
  hr: () => <hr className="my-2 border-line" />,
  img: ({ src, alt }) => <img src={src as string} alt={alt} className="max-w-full rounded-lg" />,
  // Links must never navigate the webview (that would replace the app UI) —
  // hand them to the OS default browser instead.
  a: ({ href, children }) => (
    <a
      href={href}
      onClick={(e) => {
        e.preventDefault();
        if (href) void openUrl(href).catch(() => {});
      }}
      className="underline decoration-ink-faint underline-offset-2 hover:decoration-current"
    >
      {children}
    </a>
  ),
};

interface Props {
  text: string;
  /** Extra wrapper classes — how a caller retones the whole block (e.g. the
   *  faint, note-sized reasoning aside). Element colors inherit, so setting a
   *  color here is enough. */
  className?: string;
}

/** Renders assistant text as GitHub-flavored markdown (tables, task lists,
 *  strikethrough, autolinks). Raw HTML in the source is escaped, not rendered —
 *  react-markdown's default — so model output can't inject markup.
 *
 *  Single newlines inside a paragraph are meaningful in chat, so `remark-breaks`
 *  turns them into real <br> elements. Do NOT go back to `white-space: pre-wrap`
 *  for this: the tree carries structural "\n" text nodes between block children,
 *  and `white-space` inherits, so a `pre-wrap` on <li> rendered every one of them
 *  — inside nested lists too — blowing a list up to 2.7x its height.
 *
 *  `memo`'d because streaming re-renders the whole thread on every chunk and
 *  re-parsing every past message each time is wasted work. */
export const Markdown = memo(function Markdown({ text, className = "" }: Props) {
  return (
    <div className={`whitespace-normal break-words ${className}`}>
      <ReactMarkdown remarkPlugins={[remarkGfm, remarkBreaks]} components={components}>
        {text}
      </ReactMarkdown>
    </div>
  );
});
