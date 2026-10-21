import { useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import type {
  InputHTMLAttributes,
  KeyboardEvent,
  TextareaHTMLAttributes,
  ReactNode,
} from "react";
import { CheckIcon, ChevronDownIcon } from "../Icons";

/** Shared field classes — the single source of truth for input styling. */
export const inputClass =
  "w-full rounded-field border border-line bg-surface px-3 py-2 text-body text-ink outline-none transition-colors focus:border-accent placeholder:text-ink-faint disabled:bg-surface-soft disabled:text-ink-faint";

export const labelClass = "mb-1 block text-note font-medium text-ink-soft";

export function Label({ children, className = "" }: { children: ReactNode; className?: string }) {
  return <label className={`${labelClass} ${className}`}>{children}</label>;
}

export function TextInput({ className = "", ...rest }: InputHTMLAttributes<HTMLInputElement>) {
  return <input className={`${inputClass} ${className}`} {...rest} />;
}

type TextAreaProps = TextareaHTMLAttributes<HTMLTextAreaElement> & {
  /** Grow to fit the whole value instead of scrolling inside a fixed box. */
  autoGrow?: boolean;
};

export function TextArea({ className = "", autoGrow = false, value, ...rest }: TextAreaProps) {
  const ref = useRef<HTMLTextAreaElement>(null);
  // Measure after every value change (and after mount, once the font is applied):
  // reset to `auto` first so the box can shrink as well as grow.
  useLayoutEffect(() => {
    const el = ref.current;
    if (!autoGrow || !el) return;
    el.style.height = "auto";
    el.style.height = `${el.scrollHeight}px`;
  }, [autoGrow, value]);
  // Only the vertical scroll is replaced by growth; a `whitespace-pre` field
  // (config.yaml) still needs to scroll sideways for long lines.
  const sizing = autoGrow ? "resize-none overflow-y-hidden overflow-x-auto" : "resize-y";
  return (
    <textarea
      ref={ref}
      value={value}
      className={`${inputClass} ${sizing} leading-relaxed ${className}`}
      {...rest}
    />
  );
}

export interface SelectOption {
  value: string;
  label: string;
  disabled?: boolean;
}

interface SelectProps {
  value: string;
  options: SelectOption[];
  onChange: (value: string) => void;
  placeholder?: string;
  disabled?: boolean;
  title?: string;
  /** Classes applied to the trigger (width, flex sizing, …). */
  className?: string;
  /** `sm` is the compact toolbar variant; `md` matches the other fields. */
  size?: "sm" | "md";
  emptyText?: string;
}

type MenuPosition = {
  top: number;
  left: number;
  width: number;
  maxHeight: number;
  /** Anchored to the trigger's top edge and flipped upward. */
  up: boolean;
};

function firstEnabled(options: SelectOption[], from: number, step: 1 | -1): number {
  for (let n = 0; n < options.length; n++) {
    const i = (from + step * n + options.length) % options.length;
    if (!options[i].disabled) return i;
  }
  return -1;
}

/**
 * The app-wide single-choice dropdown. It replaces the native `<select>` so the
 * trigger and menu follow the design tokens on every platform (native menus
 * render with OS chrome and can't be styled). The menu is portalled and fixed
 * positioned, so it is never clipped by a scrolling settings pane.
 */
export function Select({
  value,
  options,
  onChange,
  placeholder = "",
  disabled = false,
  title,
  className = "",
  size = "md",
  emptyText = "—",
}: SelectProps) {
  const [open, setOpen] = useState(false);
  const [position, setPosition] = useState<MenuPosition | null>(null);
  const [activeIndex, setActiveIndex] = useState(0);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const itemRefs = useRef<(HTMLButtonElement | null)[]>([]);
  const listId = useId();

  const selected = options.find((o) => o.value === value);
  const label = selected?.label ?? value;

  const close = (refocus = false) => {
    setOpen(false);
    if (refocus) triggerRef.current?.focus();
  };

  const placeMenu = (): MenuPosition | null => {
    const trigger = triggerRef.current;
    if (!trigger) return null;
    const rect = trigger.getBoundingClientRect();
    const margin = 8;
    const gap = 4;
    const width = Math.min(Math.max(rect.width, 180), window.innerWidth - margin * 2);
    const left = Math.min(Math.max(margin, rect.left), window.innerWidth - margin - width);
    const below = window.innerHeight - rect.bottom - gap - margin;
    const above = rect.top - gap - margin;
    const up = below < 200 && above > below;
    return {
      top: up ? rect.top - gap : rect.bottom + gap,
      left,
      width,
      maxHeight: Math.max(80, Math.min(280, up ? above : below)),
      up,
    };
  };

  const openMenu = (last = false) => {
    if (disabled) return;
    const index = options.findIndex((o) => o.value === value && !o.disabled);
    const fallback = firstEnabled(options, last ? options.length - 1 : 0, last ? -1 : 1);
    setActiveIndex(index >= 0 ? index : fallback);
    setPosition(placeMenu());
    setOpen(true);
  };

  const choose = (option: SelectOption | undefined) => {
    if (!option || option.disabled) return;
    if (option.value !== value) onChange(option.value);
    close(true);
  };

  // Dismiss on any click outside the trigger or menu. A document listener (not
  // a blur) so clicking a menu row is not treated as a dismissal first.
  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      const target = e.target as Node;
      if (triggerRef.current?.contains(target) || menuRef.current?.contains(target)) return;
      setOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [open]);

  // A fixed menu would drift away from its trigger, so dismiss on layout moves.
  useEffect(() => {
    if (!open) return;
    const onMove = (e: Event) => {
      if (menuRef.current?.contains(e.target as Node)) return;
      setOpen(false);
    };
    window.addEventListener("scroll", onMove, true);
    window.addEventListener("resize", onMove);
    return () => {
      window.removeEventListener("scroll", onMove, true);
      window.removeEventListener("resize", onMove);
    };
  }, [open]);

  useEffect(() => {
    if (open) itemRefs.current[activeIndex]?.scrollIntoView({ block: "nearest" });
  }, [open, activeIndex]);

  const move = (step: 1 | -1) => {
    if (options.length === 0) return;
    const from = activeIndex >= 0 ? activeIndex : step > 0 ? -1 : 0;
    const next = firstEnabled(options, from + step, step);
    if (next >= 0) setActiveIndex(next);
  };

  const onTriggerKeyDown = (e: KeyboardEvent<HTMLButtonElement>) => {
    if (disabled) return;
    if (!open) {
      if (e.key === "ArrowDown" || e.key === "ArrowUp" || e.key === "Enter" || e.key === " ") {
        e.preventDefault();
        openMenu(e.key === "ArrowUp");
      }
      return;
    }
    switch (e.key) {
      case "ArrowDown":
        e.preventDefault();
        move(1);
        break;
      case "ArrowUp":
        e.preventDefault();
        move(-1);
        break;
      case "Home":
        e.preventDefault();
        setActiveIndex(firstEnabled(options, 0, 1));
        break;
      case "End":
        e.preventDefault();
        setActiveIndex(firstEnabled(options, options.length - 1, -1));
        break;
      case "Enter":
      case " ":
        e.preventDefault();
        choose(options[activeIndex]);
        break;
      case "Escape":
        e.preventDefault();
        close(true);
        break;
      case "Tab":
        setOpen(false);
        break;
    }
  };

  const triggerClass =
    size === "sm"
      ? "flex items-center gap-1.5 rounded-field border border-line bg-surface px-2 py-1.5 text-note font-medium text-ink outline-none transition-colors hover:border-accent-line focus-visible:border-accent disabled:cursor-not-allowed disabled:bg-surface-soft disabled:text-ink-faint"
      : `${inputClass} flex items-center gap-2 text-left`;
  const chevronClass = size === "sm" ? "h-3.5 w-3.5" : "h-4 w-4";

  return (
    <>
      <button
        ref={triggerRef}
        type="button"
        disabled={disabled}
        title={title}
        onClick={() => (open ? close(true) : openMenu())}
        onKeyDown={onTriggerKeyDown}
        role="combobox"
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={open ? listId : undefined}
        aria-activedescendant={open && options[activeIndex] ? `${listId}-opt-${activeIndex}` : undefined}
        className={`${triggerClass} ${className}`}
      >
        <span className={`min-w-0 flex-1 truncate text-left ${label ? "text-ink" : "text-ink-faint"}`}>
          {label || placeholder}
        </span>
        <ChevronDownIcon
          className={`${chevronClass} shrink-0 text-ink-faint transition-transform ${open ? "rotate-180" : ""}`}
        />
      </button>

      {open && position &&
        createPortal(
          <div
            ref={menuRef}
            id={listId}
            role="listbox"
            style={{
              position: "fixed",
              top: position.top,
              left: position.left,
              width: position.width,
              maxHeight: position.maxHeight,
              transform: position.up ? "translateY(-100%)" : undefined,
            }}
            className="z-[60] overflow-y-auto rounded-field border border-line bg-surface py-1 shadow-pop"
          >
            {options.length === 0 ? (
              <div className="px-3 py-2 text-note text-ink-faint">{emptyText}</div>
            ) : (
              options.map((option, i) => {
                const isSelected = option.value === value;
                return (
                  <button
                    key={option.value}
                    ref={(el) => { itemRefs.current[i] = el; }}
                    id={`${listId}-opt-${i}`}
                    type="button"
                    role="option"
                    aria-selected={isSelected}
                    disabled={option.disabled}
                    onMouseEnter={() => setActiveIndex(i)}
                    onClick={() => choose(option)}
                    className={`flex w-full items-center gap-2 px-3 py-1.5 text-left text-note transition-colors disabled:opacity-50 ${
                      i === activeIndex ? "bg-hover text-ink" : "text-ink-soft"
                    }`}
                  >
                    <span className="min-w-0 flex-1 truncate">{option.label}</span>
                    {isSelected && <CheckIcon className="h-3.5 w-3.5 shrink-0 text-accent" />}
                  </button>
                );
              })
            )}
          </div>,
          document.body,
        )}
    </>
  );
}

/**
 * A `TextInput` that commits on blur and on Enter (Enter blurs the field, which
 * triggers the blur commit). The settings panel saves on commit, so this bakes
 * in the `onBlur` + "Enter blurs" pair every saved field repeats. Supply
 * `onChange` to update local state per keystroke and `onCommit` to persist.
 */
type SavedTextInputProps = Omit<InputHTMLAttributes<HTMLInputElement>, "onBlur" | "onKeyDown"> & {
  onCommit: () => void;
};
export function SavedTextInput({ onCommit, ...rest }: SavedTextInputProps) {
  return (
    <TextInput
      {...rest}
      onBlur={() => onCommit()}
      onKeyDown={(e) => {
        if (e.key === "Enter") e.currentTarget.blur();
      }}
    />
  );
}

/**
 * A numeric `SavedTextInput`. `onChange` receives the raw entered number on each
 * keystroke; `onCommit` receives the value clamped to `[min, ∞)` (falling back
 * to `fallback` when the field is empty/0) on blur or Enter.
 */
type NumberFieldProps = Omit<
  InputHTMLAttributes<HTMLInputElement>,
  "onChange" | "onBlur" | "onKeyDown" | "value" | "type"
> & {
  value: number;
  min?: number;
  fallback: number;
  onChange: (v: number) => void;
  onCommit: (clamped: number) => void;
};
export function NumberField({ value, min = 1, fallback, onChange, onCommit, ...rest }: NumberFieldProps) {
  return (
    <TextInput
      type="number"
      min={min}
      value={value}
      onChange={(e) => onChange(Number(e.target.value) || 0)}
      onBlur={() => onCommit(Math.max(min, value || fallback))}
      onKeyDown={(e) => {
        if (e.key === "Enter") e.currentTarget.blur();
      }}
      {...rest}
    />
  );
}
