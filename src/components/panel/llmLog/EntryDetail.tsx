import { Badge, type BadgeColor } from "../../ui/Badge";
import { codeBlockClass as preClass } from "../../ui/CodeBlock";
import { ToolCallBlock } from "../../ui/ToolCallBlock";
import { useI18n } from "../../../i18n";
import { ClockIcon, ArrowUpIcon, ArrowDownIcon } from "../../Icons";
import { blocksOf, Block } from "./blocks";
import type { LlmEntry, LlmMeta } from "./types";

const roleColors: Record<string, BadgeColor> = {
  System: "green",
  User: "sky",
  Assistant: "purple",
  Tool: "orange",
};

/** The opened row: timings from the index, messages from the body file. */
export function EntryDetail({
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