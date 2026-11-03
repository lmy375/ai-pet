import { useState } from "react";
import { Badge } from "../../ui/Badge";
import { formatIsoTime } from "../../../utils/format";
import { useI18n } from "../../../i18n";
import { ExpandChevron, WrenchIcon } from "../../Icons";
import { EntryDetail } from "./EntryDetail";
import { KINDS, type LlmEntry, type LlmMeta, type LogKind } from "./types";

interface Props {
  metas: LlmMeta[];
  expandedId: string | null;
  /** Bodies are fetched on demand; `null` means the file is gone. */
  bodies: Record<string, LlmEntry | null>;
  onToggle: (id: string) => void;
  onZoom: (src: string) => void;
}

/** The grouped log rows under the toolbar. */
export function LogList({ metas, expandedId, bodies, onToggle, onZoom }: Props) {
  const { t } = useI18n();
  // Only chat starts open: heartbeats alone can fill a hundred rows, and
  // scrolling past them to reach the conversation you came for is the whole
  // problem grouping is here to solve.
  const [collapsed, setCollapsed] = useState<Set<LogKind>>(
    () => new Set<LogKind>(KINDS.filter((kind) => kind !== "chat")),
  );

  const toggleGroup = (kind: LogKind) => {
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (!next.delete(kind)) next.add(kind);
      return next;
    });
  };

  // Metas arrive newest-first; bucketing preserves that inside each group.
  const groups = KINDS.map((kind) => ({
    kind,
    rows: metas.filter((m) => m.kind === kind),
  })).filter((g) => g.rows.length > 0);

  return (
    <div className="flex-1 overflow-y-auto px-3 py-2">
      {groups.length === 0 ? (
        <div className="mt-10 text-center text-[13px] text-ink-faint">
          {t("llm.empty")}
        </div>
      ) : (
        groups.map(({ kind, rows }) => {
          const open = !collapsed.has(kind);
          return (
            <section key={kind} className="mb-2">
              <button
                type="button"
                onClick={() => toggleGroup(kind)}
                className="flex w-full items-center gap-2 px-1 py-1.5 text-left text-[12px] font-semibold text-ink-soft hover:text-ink"
              >
                <ExpandChevron expanded={open} />
                {t(`llm.kind.${kind}`)}
                <span className="font-normal text-ink-faint">{rows.length}</span>
              </button>
              {open &&
                rows.map((meta) => (
                  <LogRow
                    key={meta.id}
                    meta={meta}
                    expanded={expandedId === meta.id}
                    body={bodies[meta.id]}
                    loaded={meta.id in bodies}
                    onToggle={onToggle}
                    onZoom={onZoom}
                  />
                ))}
            </section>
          );
        })
      )}
    </div>
  );
}

/** One collapsed row plus, when open, its detail. The kind badge lives on the
 *  group header instead — inside a group every row would carry the same one. */
function LogRow({
  meta,
  expanded,
  body,
  loaded,
  onToggle,
  onZoom,
}: {
  meta: LlmMeta;
  expanded: boolean;
  body: LlmEntry | null | undefined;
  loaded: boolean;
  onToggle: (id: string) => void;
  onZoom: (src: string) => void;
}) {
  const last = meta.rounds[meta.rounds.length - 1];
  const tcNames = [...new Set(meta.rounds.flatMap((r) => r.tools))];
  return (
    <div className="mb-1.5 overflow-hidden rounded-xl border border-line bg-surface">
      <div
        onClick={() => onToggle(meta.id)}
        className="flex cursor-pointer select-none items-center gap-2.5 px-3.5 py-2.5"
      >
        <span className="whitespace-nowrap font-mono text-[11px] text-ink-faint">
          {formatIsoTime(meta.request_time)}
        </span>
        {meta.label && (
          <span className="max-w-[140px] truncate font-mono text-[11px] text-ink-faint">
            {meta.label}
          </span>
        )}
        <Badge color="sky">{meta.model}</Badge>
        {last && <Badge color="green">R{last.round}</Badge>}
        {last?.ttft_ms != null && <Badge color="amber">TTFT {last.ttft_ms}ms</Badge>}
        {last && <Badge color="purple">{last.total_ms}ms</Badge>}
        {tcNames.length > 0 && (
          <Badge color="orange">
            <WrenchIcon className="h-3 w-3" />
            {tcNames.join(", ")}
          </Badge>
        )}
        <span className="flex-1 truncate text-[12px] text-ink-soft">{meta.preview}</span>
        <ExpandChevron expanded={expanded} />
      </div>
      {expanded && <EntryDetail meta={meta} entry={body} loaded={loaded} onZoom={onZoom} />}
    </div>
  );
}