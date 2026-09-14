import { useState } from "react";
import type { ReactNode } from "react";
import { ExpandChevron } from "../Icons";
import { parseJsonish } from "../../utils/format";

/** Containers at or below this depth start expanded; deeper ones start folded. */
const AUTO_OPEN_DEPTH = 2;
/** Strings longer than this (or holding a newline) get their own toggle. */
const INLINE_STRING_MAX = 96;
/** …and past this they start folded even near the root: an 8k-char `stdout`
 *  would otherwise bury every sibling key under it. */
const TEXT_FOLD_MAX = 400;

type Container = Record<string, unknown> | unknown[];

function isContainer(v: unknown): v is Container {
  return typeof v === "object" && v !== null;
}

/** Children as `[label, value]`; array indices become the label. */
function entriesOf(v: Container): [string, unknown][] {
  return Array.isArray(v) ? v.map((item, i) => [String(i), item]) : Object.entries(v);
}

/** What a folded container shows in place of its body. */
function foldedPreview(v: Container): string {
  return Array.isArray(v) ? `[ ${v.length} ]` : `{ ${Object.keys(v).length} }`;
}

/** A string big enough to deserve its own fold rather than sitting inline.
 *  Trimmed first: a one-word `stdout` ending in "\n" is still a one-liner. */
function isTextBlock(v: unknown): v is string {
  if (typeof v !== "string") return false;
  const trimmed = v.trim();
  return trimmed.length > INLINE_STRING_MAX || trimmed.includes("\n");
}

function onelinePreview(text: string): string {
  const flat = text.replace(/\s+/g, " ").trim();
  return flat.length > INLINE_STRING_MAX ? `${flat.slice(0, INLINE_STRING_MAX)}…` : flat;
}

/** The disclosure row of a foldable node. Chevron + key, clickable full width. */
function Toggle({ open, onToggle, children }: { open: boolean; onToggle: () => void; children: ReactNode }) {
  return (
    <button
      type="button"
      onClick={onToggle}
      className="flex w-full items-start gap-1 rounded text-left hover:bg-hover"
    >
      <ExpandChevron expanded={open} className="mt-[3px] h-3 w-3 shrink-0 text-ink-faint" />
      {children}
    </button>
  );
}

function Key({ name }: { name?: string }) {
  if (name === undefined) return null;
  return <span className="shrink-0 text-json-key">{name}:</span>;
}

/** Indented child list, hanging off the parent's chevron. */
function Children({ children }: { children: ReactNode }) {
  return <div className="ml-[5px] border-l border-line pl-2">{children}</div>;
}

function Scalar({ value }: { value: unknown }) {
  if (value === null) return <span className="text-json-null">null</span>;
  switch (typeof value) {
    case "string":
      return value ? <span className="text-json-string">{value}</span> : <span className="text-json-punct">""</span>;
    case "number":
      return <span className="text-json-number">{String(value)}</span>;
    case "boolean":
      return <span className="text-json-bool">{String(value)}</span>;
    default:
      return <span className="text-ink">{String(value)}</span>;
  }
}

/** One node of the tree. Every object, array and long string folds on its own;
 *  scalars render inline next to their key. */
function Node({ name, value, depth }: { name?: string; value: unknown; depth: number }) {
  const [open, setOpen] = useState(
    depth < AUTO_OPEN_DEPTH && !(typeof value === "string" && value.length > TEXT_FOLD_MAX),
  );

  if (isContainer(value)) {
    const entries = entriesOf(value);
    // Nothing to reveal — a toggle here would just be a dead chevron.
    if (entries.length === 0) {
      return (
        <div className="flex items-start gap-1 pl-4">
          <Key name={name} />
          <span className="text-json-punct">{Array.isArray(value) ? "[ ]" : "{ }"}</span>
        </div>
      );
    }
    return (
      <div>
        <Toggle open={open} onToggle={() => setOpen(!open)}>
          <Key name={name} />
          {!open && <span className="text-json-punct">{foldedPreview(value)}</span>}
        </Toggle>
        {open && (
          <Children>
            {entries.map(([key, child]) => (
              <Node key={key} name={key} value={child} depth={depth + 1} />
            ))}
          </Children>
        )}
      </div>
    );
  }

  if (isTextBlock(value)) {
    return (
      <div>
        <Toggle open={open} onToggle={() => setOpen(!open)}>
          <Key name={name} />
          {!open && <span className="min-w-0 truncate text-json-string">{onelinePreview(value)}</span>}
        </Toggle>
        {open && (
          <Children>
            <pre className="m-0 whitespace-pre-wrap break-all font-mono text-json-string">
              {value}
            </pre>
          </Children>
        )}
      </div>
    );
  }

  return (
    <div className="flex items-start gap-1 pl-4">
      <Key name={name} />
      <span className="min-w-0 break-all">
        <Scalar value={value} />
      </span>
    </div>
  );
}

/** Collapsible JSON tree — the single renderer for every tool payload (chat
 *  tool calls, group agents, the LLM log). Takes a raw JSON string or an
 *  already-parsed value; text that isn't JSON renders as plain text. */
export function JsonView({ value, className = "" }: { value: unknown; className?: string }) {
  const parsed = parseJsonish(value);

  if (!isContainer(parsed)) {
    const text = parsed == null ? "" : String(parsed);
    return (
      <pre className={`m-0 whitespace-pre-wrap break-all font-mono text-note leading-relaxed text-ink ${className}`}>
        {text || "—"}
      </pre>
    );
  }

  const entries = entriesOf(parsed);
  if (entries.length === 0) {
    return <div className={`font-mono text-note text-ink-faint ${className}`}>—</div>;
  }
  return (
    <div className={`font-mono text-note leading-relaxed ${className}`}>
      {entries.map(([key, child]) => (
        <Node key={key} name={key} value={child} depth={0} />
      ))}
    </div>
  );
}
