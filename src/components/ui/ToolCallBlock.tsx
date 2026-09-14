import { useState } from "react";
import { ExpandChevron, CheckIcon, SpinnerIcon } from "../Icons";
import { JsonView } from "./JsonView";
import { parseJsonish } from "../../utils/format";
import { describeToolCall } from "../../utils/toolDisplay";
import { useI18n } from "../../i18n";

interface Props {
  name: string;
  /** Raw JSON string or an already-parsed value; omit for a result-only block. */
  arguments?: unknown;
  /** The tool's return payload; omit while the call is still running. */
  result?: unknown;
  isRunning?: boolean;
  /** Provider call id, shortened into the header (LLM log only). */
  callId?: string;
  /** Open on first render — the LLM log shows a full replay, chat starts folded. */
  defaultExpanded?: boolean;
}

/** A backgrounded task returns `{status:"running", task_id}` — still running
 *  elsewhere, not a finished call, so it must not get the completed checkmark. */
function isBackgrounded(result: unknown): boolean {
  const parsed = parseJsonish(result);
  return !!parsed && typeof parsed === "object" && (parsed as Record<string, unknown>).status === "running";
}

function shortId(id: string): string {
  return id.length > 14 ? `${id.slice(0, 8)}…${id.slice(-4)}` : id;
}

/** The one tool-call renderer: chat (pet + panel), group agents and the LLM
 *  log all go through this. A folded header line with the glanceable summary,
 *  and a collapsible JSON tree for arguments and result. */
export function ToolCallBlock({
  name,
  arguments: args,
  result,
  isRunning,
  callId,
  defaultExpanded = false,
}: Props) {
  const { t } = useI18n();
  const [expanded, setExpanded] = useState(defaultExpanded);

  const { Icon, label, summary, summaryMono, hint, fullSummary } = describeToolCall(name, args);
  const hasArgs = args !== undefined && args !== null;
  const hasResult = result !== undefined && result !== null;

  return (
    <div className="my-1 overflow-hidden rounded-field border border-line bg-surface-soft text-body">
      {/* Header — always visible */}
      <div
        onClick={() => setExpanded(!expanded)}
        className="flex cursor-pointer select-none items-center gap-1.5 px-3 py-2 text-ink-soft"
      >
        <ExpandChevron expanded={expanded} className="h-3.5 w-3.5 shrink-0 text-ink-faint" />
        <Icon className="h-4 w-4 shrink-0 text-accent" />
        <span className="shrink-0 font-semibold text-accent">{label}</span>
        {summary && (
          <span
            title={fullSummary}
            className={`min-w-0 flex-1 truncate text-ink-soft ${summaryMono ? "font-mono text-note" : ""}`}
          >
            {summary}
          </span>
        )}
        {hint && <span className="min-w-0 shrink truncate text-note text-ink-faint">{hint}</span>}
        <span className="ml-auto flex shrink-0 items-center gap-1.5">
          {callId && <span className="font-mono text-meta text-ink-faint">{shortId(callId)}</span>}
          {isRunning ? (
            <span className="flex items-center gap-1 text-note text-ink-faint">
              <SpinnerIcon className="h-4 w-4 animate-spin" />
              {t("tool.running")}
            </span>
          ) : isBackgrounded(result) ? (
            <span className="rounded-full bg-amber-100 px-2 py-0.5 text-meta text-amber-700">
              {t("tool.background")}
            </span>
          ) : hasResult ? (
            <CheckIcon className="h-4 w-4 text-green-600" />
          ) : null}
        </span>
      </div>

      {/* Payloads — collapsible, one JSON tree each */}
      {expanded && (hasArgs || hasResult) && (
        <div className="space-y-2 border-t border-line bg-surface px-3 py-2">
          {hasArgs && (
            <Payload title={t("tool.args")}>
              <JsonView value={args} />
            </Payload>
          )}
          {hasResult && (
            <Payload title={t("tool.result")}>
              <JsonView value={result} />
            </Payload>
          )}
        </div>
      )}
    </div>
  );
}

function Payload({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div>
      <div className="mb-1 text-meta font-semibold text-ink-faint">{title}</div>
      <div className="max-h-[280px] overflow-y-auto rounded-lg border border-line bg-surface-soft px-2.5 py-2">
        {children}
      </div>
    </div>
  );
}
