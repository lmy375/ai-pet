import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Badge, type BadgeColor } from "../ui/Badge";
import { Button } from "../ui/Button";
import { formatIsoTime } from "../../utils/format";
import { ImageLightbox } from "../ui/ImageLightbox";
import { useI18n } from "../../i18n";
import {
  ChevronRight,
  ChevronDown,
  RefreshIcon,
  WrenchIcon,
  ClockIcon,
  ArrowUpIcon,
  ArrowDownIcon,
} from "../Icons";

interface LlmLogEntry {
  round: number;
  request_time: string;
  first_token_time: string | null;
  done_time: string;
  first_token_latency_ms: number | null;
  total_latency_ms: number;
  request: {
    model: string;
    messages: Array<{ role: string; content: unknown }>;
    tools?: unknown[];
  };
  response: {
    text: string;
    tool_calls: Array<{ function: { name: string; arguments: string } }>;
  };
}

const preClass =
  "mt-0.5 max-h-[300px] overflow-y-auto whitespace-pre-wrap break-all rounded-lg border border-slate-100 bg-slate-50 px-2.5 py-2 font-mono text-[12px] leading-relaxed text-slate-700";

type ContentBlock = { type?: string; text?: string; image_url?: { url?: string } };

// One-line text summary of a message's `content` for the collapsed list row.
// Image blocks collapse to `[Image #N]` — dumping the base64 data URL would be
// huge and useless here.
function contentToText(content: unknown): string {
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
      setEntries(parsed);
    } catch (e) {
      console.error("Failed to fetch LLM logs:", e);
    }
  };

  useEffect(() => {
    fetchLogs();
    const timer = setInterval(fetchLogs, 2000);
    return () => clearInterval(timer);
  }, []);

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
    entry.response.tool_calls.map((tc) => tc.function?.name).filter(Boolean);

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
                  {isExpanded ? (
                    <ChevronDown className="h-4 w-4 shrink-0 text-slate-400" />
                  ) : (
                    <ChevronRight className="h-4 w-4 shrink-0 text-slate-400" />
                  )}
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
                      {entry.request.messages.map((msg, j) => (
                        <div key={j} className="mb-1.5">
                          <Badge color={roleColors[msg.role] ?? "slate"}>{msg.role}</Badge>
                          {renderContent(msg.content, setZoomed, t("common.zoomImage"))}
                        </div>
                      ))}
                    </DetailSection>

                    <DetailSection icon={<ArrowDownIcon className="h-3.5 w-3.5" />} title={t("llm.section.response")}>
                      {entry.response.text && (
                        <div className="mb-1.5">
                          <Badge color="purple">assistant</Badge>
                          <pre className={preClass}>{entry.response.text}</pre>
                        </div>
                      )}
                      {entry.response.tool_calls.length > 0 && (
                        <div>
                          <div className="mb-1 text-[12px] text-slate-500">Tool Calls:</div>
                          <pre className={preClass}>{JSON.stringify(entry.response.tool_calls, null, 2)}</pre>
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
