import { useEffect, useId, useMemo, useRef, useState } from "react";
import type { KeyboardEvent } from "react";
import { inputClass } from "./fields";
import { CheckIcon, ChevronDownIcon, SearchIcon } from "../Icons";

interface Props {
  value: string;
  /** The complete, already-resolved set of allowed values (list-only: no free text). */
  options: string[];
  onChange: (value: string) => void;
  /** Shown in the trigger — and as the search box placeholder — when nothing is selected. */
  placeholder?: string;
  /** Shown in the list when there are no options, or the query matches none. */
  emptyText?: string;
  disabled?: boolean;
  className?: string;
}

/**
 * Best-effort fuzzy score for one option against a lowercased query.
 * `query` must match as a case-insensitive subsequence of `text`; the value is
 * ranked so concrete and contiguous hits beat scattered ones (a full substring
 * always exists as a consecutive run, so it naturally ranks first). Returns
 * `null` when there is no match at all.
 */
function fuzzyScore(text: string, query: string): number | null {
  const hay = text.toLowerCase();
  let score = hay.includes(query) ? 60 : 0;
  let from = 0;
  let prev = -1;
  for (const ch of query) {
    const at = hay.indexOf(ch, from);
    if (at === -1) return null;
    if (at === prev + 1) score += 8;
    else score -= Math.min(at - prev - 1, 4);
    if (at === 0 || !/[a-z0-9]/.test(hay[at - 1])) score += 10;
    score -= Math.floor(at / 8);
    prev = at;
    from = at + 1;
  }
  return score;
}

/**
 * A list-only searchable select. The closed trigger looks like a normal field;
 * opening it swaps in a search box that filters `options` by fuzzy,
 * case-insensitive subsequence match (so "cs46" finds "claude-sonnet-4-6"),
 * keeping a long model list usable without allowing arbitrary input.
 */
export function SearchSelect({
  value,
  options,
  onChange,
  placeholder = "",
  emptyText = "",
  disabled = false,
  className = "",
}: Props) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [activeIndex, setActiveIndex] = useState(0);
  const rootRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const itemRefs = useRef<(HTMLLIElement | null)[]>([]);
  const listId = useId();

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return options;
    return options
      .map((option, index) => ({ option, index, score: fuzzyScore(option, q) }))
      .filter((r) => r.score !== null)
      .sort((a, b) => (b.score! - a.score!) || (a.index - b.index))
      .map((r) => r.option);
  }, [options, query]);

  const close = (refocus = false) => {
    setOpen(false);
    setQuery("");
    if (refocus) triggerRef.current?.focus();
  };

  const openMenu = () => {
    if (disabled) return;
    setQuery("");
    const idx = options.indexOf(value);
    setActiveIndex(idx >= 0 ? idx : 0);
    setOpen(true);
  };

  const choose = (option: string) => {
    onChange(option);
    close(true);
  };

  // Dismiss on any click outside the field. A document listener (not a blur) so
  // clicking a list row — which is outside the input but inside the root — is
  // not treated as a dismissal before its own click lands.
  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) close();
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [open]);

  // Keep the active row visible while arrowing through a long list.
  useEffect(() => {
    if (!open) return;
    itemRefs.current[activeIndex]?.scrollIntoView({ block: "nearest" });
  }, [open, activeIndex]);

  const onKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setActiveIndex((i) => (filtered.length ? (i + 1) % filtered.length : 0));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActiveIndex((i) => (filtered.length ? (i - 1 + filtered.length) % filtered.length : 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      const active = filtered[activeIndex];
      if (active) choose(active);
    } else if (e.key === "Escape") {
      close(true);
    } else if (e.key === "Tab") {
      close();
    }
  };

  return (
    <div ref={rootRef} className={`relative ${className}`}>
      {open ? (
        <div className={`${inputClass} flex items-center gap-2 focus-within:border-accent`}>
          <SearchIcon className="h-4 w-4 shrink-0 text-ink-faint" />
          <input
            autoFocus
            value={query}
            onChange={(e) => {
              setQuery(e.target.value);
              setActiveIndex(0);
            }}
            onKeyDown={onKeyDown}
            placeholder={placeholder}
            role="combobox"
            aria-expanded="true"
            aria-autocomplete="list"
            aria-controls={listId}
            aria-activedescendant={filtered.length ? `${listId}-opt-${activeIndex}` : undefined}
            className="min-w-0 flex-1 bg-transparent font-mono text-body text-ink outline-none placeholder:text-ink-faint"
          />
        </div>
      ) : (
        <button
          ref={triggerRef}
          type="button"
          disabled={disabled}
          onClick={openMenu}
          aria-haspopup="listbox"
          aria-expanded="false"
          className={`${inputClass} flex items-center gap-2 text-left`}
        >
          <span className={`min-w-0 flex-1 truncate font-mono text-body ${value ? "text-ink" : "text-ink-faint"}`}>
            {value || placeholder}
          </span>
          <ChevronDownIcon className="h-4 w-4 shrink-0 text-ink-faint" />
        </button>
      )}

      {open && (
        <ul
          id={listId}
          role="listbox"
          className="absolute left-0 right-0 top-full z-20 mt-1 max-h-60 overflow-y-auto rounded-field border border-line bg-surface py-1 shadow-card"
        >
          {filtered.length === 0 ? (
            <li role="presentation" className="px-3 py-2 text-note text-ink-faint">{emptyText}</li>
          ) : (
            filtered.map((option, i) => (
              <li
                key={option}
                role="presentation"
                ref={(el) => { itemRefs.current[i] = el; }}
              >
                <button
                  type="button"
                  id={`${listId}-opt-${i}`}
                  role="option"
                  aria-selected={option === value}
                  onMouseEnter={() => setActiveIndex(i)}
                  onClick={() => choose(option)}
                  className={`flex w-full items-center gap-2 px-3 py-1.5 text-left font-mono text-note transition-colors ${
                    i === activeIndex ? "bg-hover text-ink" : "text-ink-soft"
                  }`}
                >
                  <span className="min-w-0 flex-1 truncate">{option}</span>
                  {option === value && <CheckIcon className="h-3.5 w-3.5 shrink-0 text-accent" />}
                </button>
              </li>
            ))
          )}
        </ul>
      )}
    </div>
  );
}