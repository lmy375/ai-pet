import { useState } from "react";
import { ChevronRight, CheckIcon, SpinnerIcon } from "../Icons";
import { formatJson } from "../../utils/format";
import { describeToolCall } from "../../utils/toolDisplay";

interface Props {
  name: string;
  arguments: string;
  result?: string;
  isRunning?: boolean;
}

export function ToolCallBlock({ name, arguments: args, result, isRunning }: Props) {
  const [expanded, setExpanded] = useState(false);

  const { Icon, label, summary, summaryMono, hint, fullSummary } = describeToolCall(name, args);

  return (
    <div className="my-1 overflow-hidden rounded-xl border border-slate-200 bg-slate-50 text-[13px]">
      {/* Header — always visible */}
      <div
        onClick={() => setExpanded(!expanded)}
        className="flex cursor-pointer select-none items-center gap-1.5 px-3 py-2 text-slate-600"
      >
        <ChevronRight className={`h-3.5 w-3.5 shrink-0 text-slate-400 transition-transform ${expanded ? "rotate-90" : ""}`} />
        <Icon className="h-4 w-4 shrink-0 text-accent" />
        <span className="shrink-0 font-semibold text-accent">{label}</span>
        {summary && (
          <span
            title={fullSummary}
            className={`min-w-0 flex-1 truncate text-slate-500 ${summaryMono ? "font-mono text-[12px]" : ""}`}
          >
            {summary}
          </span>
        )}
        {hint && <span className="min-w-0 shrink truncate text-[12px] text-slate-400">{hint}</span>}
        {isRunning ? (
          <span className="flex shrink-0 items-center gap-1 text-[12px] text-slate-400">
            <SpinnerIcon className="h-4 w-4 animate-spin" />
            执行中...
          </span>
        ) : result ? (
          <CheckIcon className="h-4 w-4 shrink-0 text-green-600" />
        ) : null}
      </div>

      {/* Details — collapsible */}
      {expanded && (
        <div className="border-t border-slate-200">
          <div className="px-3 py-2">
            <div className="mb-1 text-[11px] font-semibold text-slate-400">参数</div>
            <pre className="m-0 max-h-[200px] overflow-y-auto whitespace-pre-wrap break-all rounded-lg bg-slate-800 p-2 font-mono text-[12px] leading-normal text-slate-200">
              {formatJson(args)}
            </pre>
          </div>

          {result && (
            <div className="px-3 pb-2">
              <div className="mb-1 text-[11px] font-semibold text-slate-400">返回值</div>
              <pre className="m-0 max-h-[300px] overflow-y-auto whitespace-pre-wrap break-all rounded-lg bg-slate-800 p-2 font-mono text-[12px] leading-normal text-emerald-300">
                {formatJson(result)}
              </pre>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
