import { useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Badge, type BadgeColor } from "../ui/Badge";
import { Button } from "../ui/Button";
import { codeBlockClass as preClass } from "../ui/CodeBlock";
import { ToolCallBlock } from "../ui/ToolCallBlock";
import { formatIsoTime } from "../../utils/format";
import { ImageLightbox } from "../ui/ImageLightbox";
import { useI18n } from "../../i18n";
import {
  ExpandChevron,
  RefreshIcon,
  WrenchIcon,
  ClockIcon,
  ArrowUpIcon,
  ArrowDownIcon,
} from "../Icons";
import { usePolling } from "../../hooks/usePolling";

type LogKind = "chat" | "sub" | "group" | "heartbeat";

interface RoundStat {
  round: number;
  ttft_ms: number | null;
  total_ms: number;
  tools: string[];
}

/** One line of `index.jsonl` — everything a collapsed row needs, ~300 bytes. */
interface LlmMeta {
  id: string;
  kind: LogKind;
  /** Agent id for a group run, parent session for a sub-agent, else empty. */
  label: string;
  model: string;
  request_time: string;
  rounds: RoundStat[];
  preview: string;
}

/** The body file, fetched only when a row is opened. */
interface LlmEntry {
  meta: LlmMeta;
  first_token_time: string | null;
  done_time: string;
  messages: Array<{ role: string; content: unknown }>;
  response: {
    text: string;
    reasoning?: string | null;
    tool_calls: ToolCall[];
  };
}

// Messages are logged as serialized genai `ChatMessage`s: content is an array of
// externally-tagged `ContentPart`s. A tool call and its response are content
// blocks — NOT the OpenAI `tool_calls` / `tool_call_id` message fields.
type ToolCall = { call_id?: string; fn_name?: string; fn_arguments?: unknown };
type ToolResponse = { call_id?: string; fn_name?: string; content?: unknown };

type ContentBlock = {
  Text?: string;
  Binary?: { content_type?: string; source?: { Base64?: string; Url?: string } };
  ToolCall?: ToolCall;
  ToolResponse?: ToolResponse;
  ThoughtSignature?: string;
  ReasoningContent?: string;
};

/** The displayable image URL of a content block, or null if it isn't an image. */
function blockImageUrl(b: ContentBlock): string | null {
  if (b?.Binary) {
    const { content_type, source } = b.Binary;
    if (!content_type?.startsWith("image/")) return null;
    if (source?.Url) return source.Url;
    if (source?.Base64) return `data:${content_type};base64,${source.Base64}`;
    return null;
  }
  return null;
}

/** The plain text of a content block, or null if it carries none. */
function blockText(b: ContentBlock): string | null {
  if (typeof b?.Text === "string") return b.Text;
  if (typeof b?.ReasoningContent === "string") return b.ReasoningContent;
  return null;
}

/** A message body as blocks; empty text is dropped so a message that renders
 *  nothing can be skipped entirely. */
function blocksOf(content: unknown): ContentBlock[] {
  const raw: ContentBlock[] =
    content == null ? []
    : typeof content === "string" ? [{ Text: content }]
    : Array.isArray(content) ? (content as ContentBlock[])
    : [{ Text: JSON.stringify(content, null, 2) }];
  return raw.filter((b) => blockText(b)?.trim() !== "");
}

/** One content block. Tool calls and their responses go through the same
 *  `ToolCallBlock` the chat thread uses — this view differs only in starting
 *  expanded, since it is a full replay of what went on the wire. */
function Block({ block, onZoom, zoomTitle }: { block: ContentBlock; onZoom: (src: string) => void; zoomTitle: string }) {
  const url = blockImageUrl(block);
  if (url) {
    return (
      <img
        src={url}
        alt="attachment"
        onClick={() => onZoom(url)}
        title={zoomTitle}
        className="max-h-[300px] max-w-full cursor-zoom-in rounded-lg border border-line object-contain"
      />
    );
  }
  if (block.ToolCall) {
    const { fn_name, fn_arguments, call_id } = block.ToolCall;
    return <ToolCallBlock name={fn_name ?? "unknown"} arguments={fn_arguments ?? {}} callId={call_id} defaultExpanded />;
  }
  if (block.ToolResponse) {
    const { fn_name, content, call_id } = block.ToolResponse;
    return <ToolCallBlock name={fn_name ?? "unknown"} result={content ?? ""} callId={call_id} defaultExpanded />;
  }
  const text = blockText(block);
  if (text !== null) return <pre className={preClass}>{text}</pre>;
  // Opaque provider token replayed on the next turn — long, base64-ish and
  // unreadable, so don't let it drown the message.
  if (typeof block.ThoughtSignature === "string") return <pre className={preClass}>[ThoughtSignature]</pre>;
  return <pre className={preClass}>{JSON.stringify(block, null, 2)}</pre>;
}

const roleColors: Record<string, BadgeColor> = {
  System: "green",
  User: "sky",
  Assistant: "purple",
  Tool: "orange",
};

// Display order, most useful first. Retention is per kind too (sub shares the
// chat bucket), so a noisy heartbeat run can never push chat logs off the list —
// but it can still fill a hundred rows, hence the collapsed-by-default groups.
const KIND_ORDER: LogKind[] = ["chat", "sub", "group", "heartbeat"];

export function LlmLogView() {
  const { t } = useI18n();
  const [metas, setMetas] = useState<LlmMeta[]>([]);
  const [expandedId, setExpandedId] = useState<string | null>(null);
  // Only chat starts open: heartbeats alone can fill a hundred rows, and
  // scrolling past them to reach the conversation you came for is the whole
  // problem grouping is here to solve.
  const [collapsed, setCollapsed] = useState<Set<LogKind>>(
    () => new Set<LogKind>(["sub", "group", "heartbeat"]),
  );
  // Bodies are fetched on demand and cached; `null` means the file is gone
  // (compaction dropped it after the index line was read).
  const [bodies, setBodies] = useState<Record<string, LlmEntry | null>>({});
  const [zoomed, setZoomed] = useState<string | null>(null);

  // The index is one small record per conversation, so polling it is cheap —
  // the messages stay on disk until a row is actually opened.
  const fetchIndex = async () => {
    try {
      setMetas(await invoke<LlmMeta[]>("get_llm_index"));
    } catch (e) {
      console.error("Failed to fetch LLM log index:", e);
    }
  };

  usePolling(fetchIndex, 2000);

  const toggle = useCallback(
    async (id: string) => {
      if (expandedId === id) {
        setExpandedId(null);
        return;
      }
      setExpandedId(id);
      if (id in bodies) return;
      try {
        const entry = await invoke<LlmEntry | null>("get_llm_entry", { id });
        setBodies((prev) => ({ ...prev, [id]: entry }));
      } catch (e) {
        console.error("Failed to fetch LLM log entry:", e);
        setBodies((prev) => ({ ...prev, [id]: null }));
      }
    },
    [expandedId, bodies],
  );

  const toggleGroup = (kind: LogKind) => {
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (!next.delete(kind)) next.add(kind);
      return next;
    });
  };

  // Metas arrive newest-first; bucketing preserves that inside each group.
  const groups = KIND_ORDER.map((kind) => ({
    kind,
    rows: metas.filter((m) => m.kind === kind),
  })).filter((g) => g.rows.length > 0);

  return (
    <div className="flex h-full flex-col bg-surface-soft">
      {/* Toolbar */}
      <div className="flex shrink-0 items-center gap-2 border-b border-line/70 bg-surface px-4 py-2.5">
        <Button variant="ghost" size="sm" onClick={fetchIndex}>
          <RefreshIcon className="h-4 w-4" />
          {t("common.refresh")}
        </Button>
        <span className="flex-1" />
        <span className="text-[12px] text-ink-faint">{t("llm.recordCount", { count: metas.length })}</span>
      </div>

      {/* Log entries */}
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
                      onToggle={toggle}
                      onZoom={setZoomed}
                    />
                  ))}
              </section>
            );
          })
        )}
      </div>
      {zoomed && <ImageLightbox src={zoomed} onClose={() => setZoomed(null)} />}
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

/** The opened row: timings from the index, messages from the body file. */
function EntryDetail({
  meta,
  entry,
  loaded,
  onZoom,
}: {
  meta: LlmMeta;
  entry: LlmEntry | null | undefined;
  loaded: boolean;
  onZoom: (src: string) => void;
}) {
  const { t } = useI18n();

  if (!loaded) {
    return <div className="border-t border-line px-3.5 py-3 text-[12px] text-ink-faint">{t("llm.loading")}</div>;
  }
  if (!entry) {
    return <div className="border-t border-line px-3.5 py-3 text-[12px] text-ink-faint">{t("llm.gone")}</div>;
  }

  return (
    <div className="border-t border-line px-3.5 py-3">
      <DetailSection icon={<ClockIcon className="h-3.5 w-3.5" />} title={t("llm.section.time")}>
        <Row label={t("llm.row.requestTime")} value={meta.request_time} />
        <Row label={t("llm.row.firstToken")} value={entry.first_token_time ?? "—"} />
        <Row label={t("llm.row.doneTime")} value={entry.done_time} />
      </DetailSection>

      {/* The body only holds the final round — every round's request is a
          superset of the one before — so this table is where the earlier
          rounds' latency survives. */}
      <DetailSection icon={<ClockIcon className="h-3.5 w-3.5" />} title={t("llm.section.rounds")}>
        <div className="grid grid-cols-[auto_auto_auto_minmax(0,1fr)] gap-x-4 gap-y-1 font-mono text-[12px]">
          <span className="text-ink-faint">{t("llm.round")}</span>
          <span className="text-ink-faint">{t("llm.row.firstTokenLatency")}</span>
          <span className="text-ink-faint">{t("llm.row.totalLatency")}</span>
          <span className="text-ink-faint">{t("llm.tools")}</span>
          {meta.rounds.map((r) => (
            <div key={r.round} className="contents">
              <span className="text-ink">R{r.round}</span>
              <span className="text-ink">{r.ttft_ms != null ? `${r.ttft_ms} ms` : "—"}</span>
              <span className="text-ink">{r.total_ms} ms</span>
              <span className="truncate text-ink-soft">{r.tools.join(", ") || "—"}</span>
            </div>
          ))}
        </div>
      </DetailSection>

      <DetailSection icon={<ArrowUpIcon className="h-3.5 w-3.5" />} title={t("llm.section.request")}>
        {entry.messages.map((msg, j) => {
          const blocks = blocksOf(msg.content);
          if (blocks.length === 0) return null;
          return (
            <div key={j} className="mb-1.5">
              <Badge color={roleColors[msg.role] ?? "slate"}>{msg.role}</Badge>
              <div className="mt-0.5 flex flex-col gap-1.5">
                {blocks.map((block, k) => (
                  <Block key={k} block={block} onZoom={onZoom} zoomTitle={t("common.zoomImage")} />
                ))}
              </div>
            </div>
          );
        })}
      </DetailSection>

      <DetailSection icon={<ArrowDownIcon className="h-3.5 w-3.5" />} title={t("llm.section.response")}>
        {entry.response.reasoning?.trim() && (
          <div className="mb-1.5">
            <Badge color="slate">{t("chat.reasoning")}</Badge>
            <pre className={preClass}>{entry.response.reasoning}</pre>
          </div>
        )}
        {entry.response.text && (
          <div className="mb-1.5">
            <Badge color="purple">assistant</Badge>
            <pre className={preClass}>{entry.response.text}</pre>
          </div>
        )}
        {entry.response.tool_calls.map((call, k) => (
          <ToolCallBlock
            key={call.call_id ?? k}
            name={call.fn_name ?? "unknown"}
            arguments={call.fn_arguments ?? {}}
            callId={call.call_id}
            defaultExpanded
          />
        ))}
      </DetailSection>
    </div>
  );
}

function DetailSection({ icon, title, children }: { icon: React.ReactNode; title: string; children: React.ReactNode }) {
  return (
    <div className="mb-3">
      <div className="mb-1.5 flex items-center gap-1.5 text-[12px] font-semibold text-ink">
        <span className="text-ink-faint">{icon}</span>
        {title}
      </div>
      {children}
    </div>
  );
}

function Row({ label, value }: { label: string; value: string }) {
  return (
    <div className="mb-0.5 flex gap-2 font-mono text-[12px]">
      <span className="min-w-[100px] text-ink-faint">{label}</span>
      <span className="text-ink">{value}</span>
    </div>
  );
}
