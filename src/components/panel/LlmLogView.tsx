import { useState, useEffect, useRef } from "react";
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

interface LlmLogEntry {
  session_id?: string;
  round: number;
  request_time: string;
  first_token_time: string | null;
  done_time: string;
  first_token_latency_ms: number | null;
  total_latency_ms: number;
  request: {
    model: string;
    messages: Array<{ role: string; content: unknown; tool_calls?: ToolCall[]; tool_call_id?: string }>;
    tools?: unknown[];
  };
  response: {
    text: string;
    tool_calls: ToolCall[];
  };
}

type ContentBlock = { type?: string; text?: string; image_url?: { url?: string } };
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
    if (b?.type === "image_url" || b?.image_url) return `[Image #${++imageCount}]`;
    if (b?.type === "text" && typeof b.text === "string") return b.text;
    return JSON.stringify(block);
  });
  return parts.join("\n");
}

// Full render of a message's `content` for the expanded detail. Text renders in
// a <pre>; an `image_url` block renders the base64 data URL as an actual <img>
// thumbnail instead of dumping the raw string.
function renderContent(content: unknown, onZoom: (src: string) => void, zoomTitle: string) {
  if (content == null) return null;
  if (typeof content === "string") return <pre className={preClass}>{content}</pre>;
  if (!Array.isArray(content)) return <pre className={preClass}>{JSON.stringify(content, null, 2)}</pre>;
  return (
    <div className="mt-0.5 flex flex-col gap-1.5">
      {content.map((block, k) => {
        const b = block as ContentBlock;
        const url = b?.image_url?.url;
        if ((b?.type === "image_url" || b?.image_url) && url) {
          return (
            <img
              key={k}
              src={url}
              alt={`Image #${k + 1}`}
              onClick={() => onZoom(url)}
              title={zoomTitle}
              className="max-h-[300px] max-w-full cursor-zoom-in rounded-lg border border-slate-200 object-contain"
            />
          );
        }
        if (b?.type === "text" && typeof b.text === "string") {
          return <pre key={k} className={preClass}>{b.text}</pre>;
        }
        return <pre key={k} className={preClass}>{JSON.stringify(block, null, 2)}</pre>;
      })}
    </div>
  );
}

const roleColors: Record<string, BadgeColor> = {
  system: "green",
  user: "sky",
  assistant: "purple",
  tool: "orange",
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
    return <span className="break-all text-slate-700">{valueToText(parsed)}</span>;
  }
  if (Array.isArray(parsed)) {
    return (
      <div className="space-y-1">
        {parsed.map((item, i) => (
          <div key={i} className="rounded-md bg-white px-2 py-1">
            {renderStructuredValue(item)}
          </div>
        ))}
      </div>
    );
  }

  const entries = Object.entries(parsed as Record<string, unknown>);
  if (entries.length === 0) return <span className="text-slate-400">—</span>;
  return (
    <div className="grid grid-cols-[120px_minmax(0,1fr)] gap-x-3 gap-y-1.5">
      {entries.map(([key, val]) => (
        <div key={key} className="contents">
          <span className="font-mono text-[11px] text-slate-400">{key}</span>
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
        <span className="shrink-0 text-[12px] font-semibold text-slate-700">{label}</span>
        {summary && (
          <span
            title={fullSummary}
            className={`min-w-0 flex-1 truncate text-[12px] text-slate-600 ${summaryMono ? "font-mono" : ""}`}
          >
            {summary}
          </span>
        )}
        {hint && <span className="min-w-0 shrink truncate text-[12px] text-slate-400">{hint}</span>}
        {shortId(call.id) && <span className="font-mono text-[11px] text-slate-400">{shortId(call.id)}</span>}
      </div>
      <div className="rounded-md bg-white/75 px-2.5 py-2 text-[12px]">
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
        {name && <span className="text-[12px] font-semibold text-slate-700">{name}</span>}
        {status && <Badge color={status === "finished" ? "green" : "amber"}>{status}</Badge>}
        {shortId(call?.id) && <span className="font-mono text-[11px] text-slate-400">{shortId(call?.id)}</span>}
      </div>
      {obj ? (
        <div className="space-y-2 text-[12px]">
          {metaEntries.length > 0 && (
            <div className="rounded-md bg-white/75 px-2.5 py-2">
              {renderStructuredValue(Object.fromEntries(metaEntries))}
            </div>
          )}
          {stdout != null && (
            <div>
              <div className="mb-1 font-semibold text-slate-400">stdout</div>
              <pre className={preClass}>{stdout || "—"}</pre>
            </div>
          )}
          {stderr && (
            <div>
              <div className="mb-1 font-semibold text-slate-400">stderr</div>
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

export function LlmLogView() {
  const { t } = useI18n();
  const [entries, setEntries] = useState<LlmLogEntry[]>([]);
  const [expandedIdx, setExpandedIdx] = useState<number | null>(null);
  const [zoomed, setZoomed] = useState<string | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);

  const fetchLogs = async () => {
    try {
      const lines = await invoke<string[]>("get_llm_logs", { limit: 200 });
      const parsed = lines
        .map((l) => {
          try {
            const raw = JSON.parse(l);
            // Normalize old format: "ts" -> "request_time"
            if (!raw.request_time && raw.ts) raw.request_time = raw.ts;
            if (!raw.done_time) raw.done_time = raw.request_time ?? "";
            if (raw.first_token_latency_ms == null) raw.first_token_latency_ms = null;
            if (raw.total_latency_ms == null) raw.total_latency_ms = 0;
            return raw as LlmLogEntry;
          } catch { return null; }
        })
        .filter((e): e is LlmLogEntry => e !== null && !!e.request_time)
        .reverse();
      // Within one session each LLM request carries the full prior history, so
      // the newest entry of a session is a superset of every earlier one. Keep
      // only that newest entry per session (legacy entries lacking a session_id
      // are kept individually).
      const seen = new Set<string>();
      const deduped = parsed.filter((e) => {
        if (!e.session_id) return true;
        if (seen.has(e.session_id)) return false;
        seen.add(e.session_id);
        return true;
      });
      setEntries(deduped);
    } catch (e) {
      console.error("Failed to fetch LLM logs:", e);
    }
  };

  usePolling(fetchLogs, 2000);

  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = 0;
    }
  }, [entries.length]);

  const lastUserMsg = (entry: LlmLogEntry): string => {
    const msgs = entry.request.messages;
    for (let i = msgs.length - 1; i >= 0; i--) {
      if (msgs[i].role === "user") {
        const text = contentToText(msgs[i].content);
        return text.length > 80 ? text.slice(0, 80) + "..." : text;
      }
    }
    return "(no user message)";
  };

  const toolCallNames = (entry: LlmLogEntry): string[] =>
    entry.response.tool_calls.map((tc) => tc.function?.name).filter((name): name is string => !!name);

  const toggle = (idx: number) => {
    setExpandedIdx(expandedIdx === idx ? null : idx);
  };

  return (
    <div className="flex h-full flex-col bg-slate-100">
      {/* Toolbar */}
      <div className="flex shrink-0 items-center gap-2 border-b border-slate-200/70 bg-white px-4 py-2.5">
        <Button variant="ghost" size="sm" onClick={fetchLogs}>
          <RefreshIcon className="h-4 w-4" />
          {t("common.refresh")}
        </Button>
        <span className="flex-1" />
        <span className="text-[12px] text-slate-400">{t("llm.recordCount", { count: entries.length })}</span>
      </div>

      {/* Log entries */}
      <div ref={scrollRef} className="flex-1 overflow-y-auto px-3 py-2">
        {entries.length === 0 ? (
          <div className="mt-10 text-center text-[13px] text-slate-400">
            {t("llm.empty")}
          </div>
        ) : (
          entries.map((entry, i) => {
            const isExpanded = expandedIdx === i;
            const tcNames = toolCallNames(entry);
            const toolCallsById = new Map<string, ToolCall>();
            for (const msg of entry.request.messages) {
              msg.tool_calls?.forEach((call) => {
                if (call.id) toolCallsById.set(call.id, call);
              });
            }
            entry.response.tool_calls.forEach((call) => {
              if (call.id) toolCallsById.set(call.id, call);
            });
            return (
              <div key={i} className="mb-1.5 overflow-hidden rounded-xl border border-slate-200 bg-white">
                {/* Summary row */}
                <div
                  onClick={() => toggle(i)}
                  className="flex cursor-pointer select-none items-center gap-2.5 px-3.5 py-2.5"
                >
                  <span className="whitespace-nowrap font-mono text-[11px] text-slate-400">
                    {formatIsoTime(entry.request_time)}
                  </span>
                  <Badge color="sky">{entry.request.model}</Badge>
                  <Badge color="green">R{entry.round}</Badge>
                  {entry.first_token_latency_ms != null && (
                    <Badge color="amber">TTFT {entry.first_token_latency_ms}ms</Badge>
                  )}
                  <Badge color="purple">{entry.total_latency_ms}ms</Badge>
                  {tcNames.length > 0 && (
                    <Badge color="orange">
                      <WrenchIcon className="h-3 w-3" />
                      {tcNames.join(", ")}
                    </Badge>
                  )}
                  <span className="flex-1 truncate text-[12px] text-slate-600">{lastUserMsg(entry)}</span>
                  <ExpandChevron expanded={isExpanded} />
                </div>

                {/* Expanded detail */}
                {isExpanded && (
                  <div className="border-t border-slate-100 px-3.5 py-3">
                    <DetailSection icon={<ClockIcon className="h-3.5 w-3.5" />} title={t("llm.section.time")}>
                      <Row label={t("llm.row.requestTime")} value={entry.request_time} />
                      <Row label={t("llm.row.firstToken")} value={entry.first_token_time ?? "—"} />
                      <Row label={t("llm.row.doneTime")} value={entry.done_time} />
                      <Row label={t("llm.row.firstTokenLatency")} value={entry.first_token_latency_ms != null ? `${entry.first_token_latency_ms} ms` : "—"} />
                      <Row label={t("llm.row.totalLatency")} value={`${entry.total_latency_ms} ms`} />
                    </DetailSection>

                    <DetailSection icon={<ArrowUpIcon className="h-3.5 w-3.5" />} title={t("llm.section.request")}>
                      {entry.request.messages.map((msg, j) => {
                        if (msg.role === "tool") {
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
                                {renderContent(msg.content, setZoomed, t("common.zoomImage"))}
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

function DetailSection({ icon, title, children }: { icon: React.ReactNode; title: string; children: React.ReactNode }) {
  return (
    <div className="mb-3">
      <div className="mb-1.5 flex items-center gap-1.5 text-[12px] font-semibold text-slate-700">
        <span className="text-slate-400">{icon}</span>
        {title}
      </div>
      {children}
    </div>
  );
}

function Row({ label, value }: { label: string; value: string }) {
  return (
    <div className="mb-0.5 flex gap-2 font-mono text-[12px]">
      <span className="min-w-[100px] text-slate-400">{label}</span>
      <span className="text-slate-800">{value}</span>
    </div>
  );
}
