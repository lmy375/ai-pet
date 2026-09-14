import { PlusIcon } from "../Icons";

export interface Chip {
  name: string;
  /** Optional status dot class (see `utils/tone`), e.g. connection state. */
  dotClass?: string;
}

interface Props {
  items: Chip[];
  selected: string | null;
  onSelect: (name: string) => void;
  onAdd: () => void;
  addTitle: string;
  className?: string;
}

/**
 * A row of pills that selects which entry of a collection the card below edits —
 * the in-card equivalent of the settings tab bar, used for both the model pool
 * and the MCP server pool so the two cards behave identically.
 */
export function ChipTabs({ items, selected, onSelect, onAdd, addTitle, className = "" }: Props) {
  return (
    <div className={`flex flex-wrap items-center gap-1.5 ${className}`}>
      {items.map((item) => {
        const active = item.name === selected;
        return (
          <button
            key={item.name}
            type="button"
            onClick={() => onSelect(item.name)}
            className={`flex items-center gap-1.5 rounded-full px-3 py-1 text-note font-medium transition-colors ${
              active ? "bg-accent text-white" : "bg-surface-soft text-ink-soft hover:bg-hover"
            }`}
          >
            {item.dotClass && <span className={`h-1.5 w-1.5 shrink-0 rounded-full ${item.dotClass}`} />}
            {item.name}
          </button>
        );
      })}
      <button
        type="button"
        onClick={onAdd}
        title={addTitle}
        className="flex items-center gap-1 rounded-full px-2.5 py-1 text-note font-medium text-accent transition-colors hover:bg-accent/10"
      >
        <PlusIcon className="h-4 w-4" />
      </button>
    </div>
  );
}
