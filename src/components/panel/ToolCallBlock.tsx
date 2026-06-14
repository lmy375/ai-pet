import { useState } from "react";
import { ChevronRight, WrenchIcon, CheckIcon, SpinnerIcon } from "../Icons";
import { formatJson } from "../../utils/format";

interface Props {
  name: string;
  arguments: string;
  result?: string;
  isRunning?: boolean;
}

export function ToolCallBlock({ name, arguments: args, result, isRunning }: Props) {
  const [expanded, setExpanded] = useState(false);

  const StatusIcon = isRunning ? SpinnerIcon : result ? CheckIcon : WrenchIcon;
  const statusColor = isRunning ? "text-slate-400" : result ? "text-green-600" : "text-slate-500";

  return (
    <div className="my-1 overflow-hidden rounded-xl border border-slate-200 bg-slate-50 text-[13px]">
      {/* Header — always visible */}
      <div
        onClick={() => setExpanded(!expanded)}
        className="flex cursor-pointer select-none items-center gap-1.5 px-3 py-2 text-slate-600"
      >
        <ChevronRight className={`h-3.5 w-3.5 text-slate-400 transition-transform ${expanded ? "rotate-90" : ""}`} />
        <StatusIcon className={`h-4 w-4 ${statusColor} ${isRunning ? "animate-spin" : ""}`} />
        <span className="font-semibold text-accent">{name}</span>
        {isRunning && <span className="text-[12px] text-slate-400">执行中...</span>}
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
