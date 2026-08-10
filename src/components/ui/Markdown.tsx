import { memo } from "react";
import ReactMarkdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import { openUrl } from "@tauri-apps/plugin-opener";

/** Element styles for assistant markdown inside a chat bubble. Kept tight
 *  (bubble-sized headings, small margins) so a formatted answer still reads as
 *  a message and not a document. */
const components: Components = {
  // Single newlines inside a paragraph/list item are meaningful in chat, so keep
  // them (`pre-wrap`) even though CommonMark would collapse them. The wrapper
  // resets `white-space` so the newlines *between* blocks stay invisible.
  p: ({ children }) => <p className="mb-2 whitespace-pre-wrap last:mb-0">{children}</p>,
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
    <li className={`whitespace-pre-wrap ${className?.includes("task-list-item") ? "list-none" : ""}`}>{children}</li>
  ),
  input: ({ type, checked }) =>
    type === "checkbox" ? <input type="checkbox" checked={checked} readOnly className="mr-1.5 align-[-1px]" /> : null,
  blockquote: ({ children }) => (
    <blockquote className="mb-2 border-l-2 border-slate-400 pl-2.5 text-slate-600 last:mb-0">{children}</blockquote>
  ),
  // Inline code. Inside a fence the same element is neutralized by the `pre`
  // rules below, so no `inline` prop sniffing is needed.
  code: ({ children }) => (
    <code className="rounded bg-slate-900/8 px-1 py-px font-mono text-[12.5px]">{children}</code>
  ),
  pre: ({ children }) => (
    <pre className="mb-2 max-h-[260px] overflow-auto rounded-lg border border-slate-300/60 bg-slate-50 px-2.5 py-2 font-mono text-[12px] leading-relaxed text-slate-700 last:mb-0 [&>code]:bg-transparent [&>code]:p-0 [&>code]:text-[12px]">
      {children}
    </pre>
  ),
  table: ({ children }) => (
    <div className="mb-2 overflow-x-auto last:mb-0">
      <table className="w-full border-collapse text-[13px]">{children}</table>
    </div>
  ),
  th: ({ children, style }) => (
    <th style={style} className="border border-slate-300 bg-slate-100/70 px-2 py-1 text-left font-semibold">
      {children}
    </th>
  ),
  td: ({ children, style }) => (
    <td style={style} className="border border-slate-300 px-2 py-1 align-top">
      {children}
    </td>
  ),
  hr: () => <hr className="my-2 border-slate-300" />,
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
      className="underline decoration-slate-400 underline-offset-2 hover:decoration-current"
    >
      {children}
    </a>
  ),
};

interface Props {
  text: string;
  /** Append a blinking cursor to the last block (while the answer streams in). */
  caret?: boolean;
}

/** Renders assistant text as GitHub-flavored markdown (tables, task lists,
 *  strikethrough, autolinks). Raw HTML in the source is escaped, not rendered —
 *  react-markdown's default — so model output can't inject markup.
 *
 *  `memo`'d because streaming re-renders the whole thread on every chunk and
 *  re-parsing every past message each time is wasted work. */
export const Markdown = memo(function Markdown({ text, caret = false }: Props) {
  return (
    <div className={`whitespace-normal break-words ${caret ? "[&>:last-child]:stream-caret" : ""}`}>
      <ReactMarkdown remarkPlugins={[remarkGfm]} components={components}>
        {text}
      </ReactMarkdown>
    </div>
  );
});
