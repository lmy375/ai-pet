import { useRef, useState } from "react";
import type { SessionMeta } from "../../hooks/useChat";
import { DEFAULT_SESSION_TITLE } from "../../hooks/useChat";
import { Button } from "../ui/Button";
import { IconActionButton } from "../ui/IconButton";
import { PlusIcon, PencilIcon, TrashIcon, SearchIcon } from "../Icons";
import { useI18n, type Lang } from "../../i18n";

interface Props {
  sessions: SessionMeta[];
  activeId: string;
  onSelect: (id: string) => void;
  onNew: () => void;
  onRename: (id: string, title: string) => void;
  onDelete: (id: string) => void;
}

type Bucket = "today" | "week" | "earlier";

const DAY = 24 * 60 * 60 * 1000;

/** Midnight today, in local time — the reference point for every bucket. */
function startOfToday(): number {
  const d = new Date();
  d.setHours(0, 0, 0, 0);
  return d.getTime();
}

function bucketOf(ts: number, today: number): Bucket {
  if (ts >= today) return "today";
  if (ts >= today - 6 * DAY) return "week";
  return "earlier";
}

/** Compact recency label: time today, "昨天", the weekday this week, else M/D. */
function whenLabel(ts: number, today: number, lang: Lang, yesterday: string): string {
  const d = new Date(ts);
  if (ts >= today) {
    return `${d.getHours().toString().padStart(2, "0")}:${d.getMinutes().toString().padStart(2, "0")}`;
  }
  if (ts >= today - DAY) return yesterday;
  if (ts >= today - 6 * DAY) {
    return new Intl.DateTimeFormat(lang === "zh" ? "zh-CN" : "en-US", { weekday: "short" }).format(d);
  }
  return `${d.getMonth() + 1}/${d.getDate()}`;
}

/**
 * The conversation rail: search, sessions grouped by recency, and the
 * new-conversation action. It owns the inline rename (the only place a title is
 * edited) so the chat column stays about the current conversation.
 */
export function SessionSidebar({ sessions, activeId, onSelect, onNew, onRename, onDelete }: Props) {
  const { t, lang } = useI18n();
  const [query, setQuery] = useState("");
  // Inline rename: id of the row being edited + its draft.
  const [editingId, setEditingId] = useState<string | null>(null);
  const [titleDraft, setTitleDraft] = useState("");
  // Set on Escape so the input's blur cancels instead of saving.
  const cancelRenameRef = useRef(false);

  // Untitled sessions are stored with the sentinel title; show a localized label.
  const displayTitle = (title: string) => (title === DEFAULT_SESSION_TITLE ? t("chat.newSession") : title);

  const startRename = (id: string, title: string) => {
    setTitleDraft(title === DEFAULT_SESSION_TITLE ? "" : title);
    setEditingId(id);
  };
  const commitRename = () => {
    const id = editingId;
    setEditingId(null);
    if (cancelRenameRef.current) {
      cancelRenameRef.current = false;
      return;
    }
    if (id) onRename(id, titleDraft);
  };

  const today = startOfToday();
  const q = query.trim().toLowerCase();
  const visible = sessions
    .filter((s) => !q || displayTitle(s.title).toLowerCase().includes(q))
    .map((s) => ({ ...s, ts: new Date(s.updated_at).getTime() }))
    .sort((a, b) => b.ts - a.ts);

  const groups: { bucket: Bucket; label: string; rows: typeof visible }[] = (
    [
      ["today", t("chat.session.today")],
      ["week", t("chat.session.week")],
      ["earlier", t("chat.session.earlier")],
    ] as [Bucket, string][]
  )
    .map(([bucket, label]) => ({ bucket, label, rows: visible.filter((s) => bucketOf(s.ts, today) === bucket) }))
    .filter((g) => g.rows.length > 0);

  return (
    <aside className="flex w-[236px] shrink-0 flex-col border-r border-line bg-surface">
      {/* Search */}
      <div className="shrink-0 px-3 pb-2 pt-3">
        <div className="flex items-center gap-1.5 rounded-field border border-line bg-surface-soft px-2.5 py-1.5 focus-within:border-accent">
          <SearchIcon className="h-3.5 w-3.5 shrink-0 text-ink-faint" />
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={t("chat.session.search")}
            className="min-w-0 flex-1 bg-transparent text-body text-ink outline-none placeholder:text-ink-faint"
          />
        </div>
      </div>

      {/* Grouped session list */}
      <div className="min-h-0 flex-1 overflow-y-auto px-2 pb-2">
        {visible.length === 0 ? (
          <div className="mt-6 text-center text-note text-ink-faint">
            {sessions.length === 0 ? t("chat.session.empty") : t("chat.session.searchEmpty")}
          </div>
        ) : (
          groups.map((g) => (
            <div key={g.bucket} className="mb-1">
              <div className="px-2 pb-1 pt-2 text-meta font-medium text-ink-faint">{g.label}</div>
              {g.rows.map((s) => {
                const active = s.id === activeId;
                return editingId === s.id ? (
                  <input
                    key={s.id}
                    autoFocus
                    value={titleDraft}
                    onChange={(e) => setTitleDraft(e.target.value)}
                    onFocus={(e) => e.currentTarget.select()}
                    onBlur={commitRename}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") e.currentTarget.blur();
                      else if (e.key === "Escape") {
                        cancelRenameRef.current = true;
                        e.currentTarget.blur();
                      }
                    }}
                    placeholder={t("chat.session.titlePlaceholder")}
                    className="w-full rounded-field border border-accent bg-surface px-2 py-1.5 text-body text-ink outline-none"
                  />
                ) : (
                  <div
                    key={s.id}
                    className={`group flex items-center gap-1 rounded-field px-2 py-1.5 transition-colors ${
                      active ? "bg-accent-soft" : "hover:bg-hover"
                    }`}
                  >
                    <button className="min-w-0 flex-1 text-left" onClick={() => onSelect(s.id)}>
                      <div
                        className={`truncate text-body ${active ? "font-semibold text-accent" : "text-ink"}`}
                      >
                        {displayTitle(s.title)}
                      </div>
                    </button>
                    {/* Row actions replace the timestamp on hover — the rail is
                        too narrow to hold both. */}
                    <span className="shrink-0 text-meta text-ink-faint group-hover:hidden">
                      {whenLabel(s.ts, today, lang, t("chat.session.yesterday"))}
                    </span>
                    <span className="hidden shrink-0 items-center group-hover:flex">
                      <IconActionButton
                        size="sm"
                        onClick={(e) => {
                          e.stopPropagation();
                          startRename(s.id, s.title);
                        }}
                        title={t("chat.session.rename")}
                      >
                        <PencilIcon className="h-3.5 w-3.5" />
                      </IconActionButton>
                      <IconActionButton
                        size="sm"
                        variant="danger"
                        onClick={(e) => {
                          e.stopPropagation();
                          onDelete(s.id);
                        }}
                        title={t("chat.session.delete")}
                      >
                        <TrashIcon className="h-3.5 w-3.5" />
                      </IconActionButton>
                    </span>
                  </div>
                );
              })}
            </div>
          ))
        )}
      </div>

      {/* New conversation */}
      <div className="shrink-0 border-t border-line p-3">
        <Button className="w-full" onClick={onNew} title={t("chat.session.newTitle")}>
          <PlusIcon className="h-4 w-4" />
          {t("chat.newSession")}
        </Button>
      </div>
    </aside>
  );
}
