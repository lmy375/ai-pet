import { useState, useEffect, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Badge, type BadgeColor } from "../ui/Badge";
import { Button } from "../ui/Button";
import { codeBlockClass as preClass } from "../ui/CodeBlock";
import { formatIsoTime } from "../../utils/format";
import { describeToolCall } from "../../utils/toolDisplay";
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
  messages: Array<{ role: string; content: unknown; tool_calls?: ToolCall[]; tool_call_id?: string }>;
  response: {
    text: string;
    reasoning?: string | null;
    tool_calls: ToolCall[];
  };
}

// Messages are logged as serialized genai `ChatMessage`s: content is an array of
// externally-tagged `ContentPart`s.
type ContentBlock = {
  Text?: string;
  Binary?: { content_type?: string; source?: { Base64?: string; Url?: string } };
  ToolCall?: unknown;
  ToolResponse?: unknown;
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
type ToolCall = { id?: string; type?: string; function?: { name?: string; arguments?: string } };

// One-line text summary of a message's `content` for the collapsed list row.
// Image blocks collapse to `[Image #N]` — dumping the base64 data URL would be
// huge and useless here.
function contentToText(content: unknown): string {
  if (content == null) return "";
  if (typeof content === "string") return content;
  if (!Array.isArray(content)) return JSON.stringify(content, null, 2);
  let imageCount = 0;
  const parts = content.map((block) => {
    const b = block as ContentBlock;
    if (blockImageUrl(b)) return `[Image #${++imageCount}]`;
    const text = blockText(b);
    if (text !== null) return text;
    // Opaque provider token replayed on the next turn — it's long, base64-ish
    // and unreadable, so don't let it drown the row.
    if (typeof b?.ThoughtSignature === "string") return "[ThoughtSignature]";
    return JSON.stringify(block);
  });
  return parts.join("\n");
}

// Full render of a message's `content` for the expanded detail. Text renders in
// a <pre>; a Binary image part renders as an actual <img> thumbnail instead of
// dumping its base64 payload.
function renderContent(content: unknown, onZoom: (src: string) => void, zoomTitle: string) {
  if (content == null) return null;
  if (typeof content === "string") return <pre className={preClass}>{content}</pre>;
  if (!Array.isArray(content)) return <pre className={preClass}>{JSON.stringify(content, null, 2)}</pre>;
  return (
    <div className="mt-0.5 flex flex-col gap-1.5">
      {content.map((block, k) => {
        const b = block as ContentBlock;
        const url = blockImageUrl(b);
        if (url) {
          return (
            <img
              key={k}
              src={url}
              alt={`Image #${k + 1}`}
              onClick={() => onZoom(url)}
              title={zoomTitle}
              className="max-h-[300px] max-w-full cursor-zoom-in rounded-lg border border-line object-contain"
            />
          );
        }
        const text = blockText(b);
        if (text !== null) {
          return <pre key={k} className={preClass}>{text}</pre>;
        }
        if (typeof b?.ThoughtSignature === "string") {
          return <pre key={k} className={preClass}>[ThoughtSignature]</pre>;
        }
        return <pre key={k} className={preClass}>{JSON.stringify(block, null, 2)}</pre>;
      })}
    </div>
  );
}

const roleColors: Record<string, BadgeColor> = {
  System: "green",
  User: "sky",
  Assistant: "purple",
  Tool: "orange",
};

function shortId(id: string | undefined): string | null {
  if (!id) return null;
  return id.length > 14 ? `${id.slice(0, 8)}...${id.slice(-4)}` : id;
}

function parseJsonValue(value: unknown): unknown {
  if (typeof value !== "string") return value;
  const trimmed = value.trim();
  if (!trimmed) return value;
  if (!trimmed.startsWith("{") && !trimmed.startsWith("[")) return value;
  try {
    return JSON.parse(trimmed);
  } catch {
    return value;
  }
}

function valueToText(value: unknown): string {
  if (value == null) return "—";
  if (typeof value === "string") return value || "—";
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  return String(value);
}

function renderStructuredValue(value: unknown): React.ReactNode {
  const parsed = parseJsonValue(value);
  if (parsed == null || typeof parsed !== "object") {
    return <span className="break-all text-ink">{valueToText(parsed)}</span>;
  }
  if (Array.isArray(parsed)) {
    return (
      <div className="space-y-1">
        {parsed.map((item, i) => (
          <div key={i} className="rounded-md bg-surface px-2 py-1">
            {renderStructuredValue(item)}
          </div>
        ))}
      </div>
    );
  }

  const entries = Object.entries(parsed as Record<string, unknown>);
  if (entries.length === 0) return <span className="text-ink-faint">—</span>;
  return (
    <div className="grid grid-cols-[120px_minmax(0,1fr)] gap-x-3 gap-y-1.5">
      {entries.map(([key, val]) => (
        <div key={key} className="contents">
          <span className="font-mono text-[11px] text-ink-faint">{key}</span>
          <div className="min-w-0">{renderStructuredValue(val)}</div>
        </div>
      ))}
    </div>
  );
}

function ToolCallView({ call }: { call: ToolCall }) {
  const name = call.function?.name ?? "unknown";
  const argsText = call.function?.arguments ?? "{}";
  const args = parseJsonValue(argsText);
  const { Icon, label, summary, summaryMono, hint, fullSummary } = describeToolCall(name, argsText);
  return (
    <div className="mt-1.5 rounded-lg border border-orange-100 bg-orange-50/50 px-2.5 py-2">
      <div className="mb-2 flex items-center gap-1.5">
        <Badge color="orange">tool-call</Badge>
        <Icon className="h-4 w-4 shrink-0 text-orange-600" />
        <span className="shrink-0 text-[12px] font-semibold text-ink">{label}</span>
        {summary && (
          <span
            title={fullSummary}
            className={`min-w-0 flex-1 truncate text-[12px] text-ink-soft ${summaryMono ? "font-mono" : ""}`}
          >
            {summary}
          </span>
        )}
        {hint && <span className="min-w-0 shrink truncate text-[12px] text-ink-faint">{hint}</span>}
        {shortId(call.id) && <span className="font-mono text-[11px] text-ink-faint">{shortId(call.id)}</span>}
      </div>
      <div className="rounded-md bg-surface/75 px-2.5 py-2 text-[12px]">
        {renderStructuredValue(args)}
      </div>
    </div>
  );
}

function ToolResultView({ content, call }: { content: unknown; call?: ToolCall }) {
  const parsed = parseJsonValue(content);
  const obj = parsed && typeof parsed === "object" && !Array.isArray(parsed)
    ? parsed as Record<string, unknown>
    : null;
  const status = typeof obj?.status === "string" ? obj.status : null;
  const stdout = typeof obj?.stdout === "string" ? obj.stdout : null;
  const stderr = typeof obj?.stderr === "string" ? obj.stderr : null;
  const metaEntries = obj
    ? Object.entries(obj).filter(([key]) => !["stdout", "stderr", "stdout_path", "stderr_path"].includes(key))
    : [];
  const name = call?.function?.name;

  return (
    <div className="mt-1.5 rounded-lg border border-amber-100 bg-amber-50/50 px-2.5 py-2">
      <div className="mb-2 flex items-center gap-1.5">
        <Badge color="orange">tool-result</Badge>
        {name && <span className="text-[12px] font-semibold text-ink">{name}</span>}
        {status && <Badge color={status === "finished" ? "green" : "amber"}>{status}</Badge>}
        {shortId(call?.id) && <span className="font-mono text-[11px] text-ink-faint">{shortId(call?.id)}</span>}
      </div>
      {obj ? (
        <div className="space-y-2 text-[12px]">
          {metaEntries.length > 0 && (
            <div className="rounded-md bg-surface/75 px-2.5 py-2">
              {renderStructuredValue(Object.fromEntries(metaEntries))}
            </div>
          )}
          {stdout != null && (
            <div>
              <div className="mb-1 font-semibold text-ink-faint">stdout</div>
              <pre className={preClass}>{stdout || "—"}</pre>
            </div>
          )}
          {stderr && (
            <div>
              <div className="mb-1 font-semibold text-ink-faint">stderr</div>
              <pre className={preClass}>{stderr}</pre>
            </div>
          )}
        </div>
      ) : (
        <pre className={preClass}>{valueToText(parsed)}</pre>
      )}
    </div>
  );
}

const kindColors: Record<LogKind, BadgeColor> = {
  chat: "sky",
  sub: "purple",
  group: "green",
  heartbeat: "slate",
};

export function LlmLogView() {
  const { t } = useI18n();
  const [metas, setMetas] = useState<LlmMeta[]>([]);
  const [expandedId, setExpandedId] = useState<string | null>(null);
  // Bodies are fetched on demand and cached; `null` means the file is gone
  // (compaction dropped it after the index line was read).
  const [bodies, setBodies] = useState<Record<string, LlmEntry | null>>({});
  const [zoomed, setZoomed] = useState<string | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);

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

  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = 0;
    }
  }, [metas.length]);

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
      <div ref={scrollRef} className="flex-1 overflow-y-auto px-3 py-2">
        {metas.length === 0 ? (
          <div className="mt-10 text-center text-[13px] text-ink-faint">
            {t("llm.empty")}
          </div>
        ) : (
          metas.map((meta) => {
            const isExpanded = expandedId === meta.id;
            const last = meta.rounds[meta.rounds.length - 1];
            const tcNames = [...new Set(meta.rounds.flatMap((r) => r.tools))];
            return (
              <div key={meta.id} className="mb-1.5 overflow-hidden rounded-xl border border-line bg-surface">
                {/* Summary row */}
                <div
                  onClick={() => toggle(meta.id)}
                  className="flex cursor-pointer select-none items-center gap-2.5 px-3.5 py-2.5"
                >
                  <span className="whitespace-nowrap font-mono text-[11px] text-ink-faint">
                    {formatIsoTime(meta.request_time)}
                  </span>
                  <Badge color={kindColors[meta.kind] ?? "slate"}>{t(`llm.kind.${meta.kind}`)}</Badge>
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
                  <ExpandChevron expanded={isExpanded} />
                </div>

                {/* Expanded detail */}
                {isExpanded && (
                  <EntryDetail meta={meta} entry={bodies[meta.id]} loaded={meta.id in bodies} onZoom={setZoomed} />
                )}
              </div>
            );
          })
        )}
      </div>
      {zoomed && <ImageLightbox src={zoomed} onClose={() => setZoomed(null)} />}
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

  const toolCallsById = new Map<string, ToolCall>();
  for (const msg of entry.messages) {
    msg.tool_calls?.forEach((call) => {
      if (call.id) toolCallsById.set(call.id, call);
    });
  }
  entry.response.tool_calls.forEach((call) => {
    if (call.id) toolCallsById.set(call.id, call);
  });

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
          if (msg.role === "Tool") {
            return (
              <div key={j} className="mb-1.5">
                <ToolResultView content={msg.content} call={msg.tool_call_id ? toolCallsById.get(msg.tool_call_id) : undefined} />
              </div>
            );
          }
          const hasContent = msg.content != null && contentToText(msg.content).trim().length > 0;
          return (
            <div key={j} className="mb-1.5">
              {hasContent && (
                <>
                  <Badge color={roleColors[msg.role] ?? "slate"}>{msg.role}</Badge>
                  {renderContent(msg.content, onZoom, t("common.zoomImage"))}
                </>
              )}
              {msg.tool_calls?.map((call, k) => (
                <ToolCallView key={call.id ?? k} call={call} />
              ))}
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
        {entry.response.tool_calls.length > 0 && (
          <div className="space-y-1.5">
            {entry.response.tool_calls.map((call, k) => (
              <ToolCallView key={call.id ?? k} call={call} />
            ))}
          </div>
        )}
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
