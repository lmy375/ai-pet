import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ProgressRing } from "../ui/ProgressRing";
import { useSettings } from "../../hooks/useSettings";
import { useI18n } from "../../i18n";

interface ToolInfo {
  name: string;
  description: string;
  is_mcp: boolean;
}

/**
 * The context-occupancy gauge in the chat toolbar: a ring + percentage that opens
 * a popover with the token breakdown and the live tool list. Hovering opens it,
 * clicking pins it open (so the tool list can be scrolled); Esc / outside-click
 * closes a pinned one.
 *
 * It stays mounted even with no usage numbers — a provider that reports no token
 * usage (or a session last written by the CLI) would otherwise take the whole
 * tool list away with it. The ring then reads empty and the percentage "—".
 */
export function ContextUsageRing({ usage }: { usage: { used: number; total: number } | null }) {
  const { t } = useI18n();
  const { settings } = useSettings();
  const [pinned, setPinned] = useState(false);
  const [hovered, setHovered] = useState(false);
  const [tools, setTools] = useState<ToolInfo[] | null>(null);
  const ref = useRef<HTMLDivElement>(null);
  // Hover-out is delayed so crossing into the popover doesn't close it.
  const leaveTimer = useRef<number | undefined>(undefined);

  const open = pinned || hovered;

  const onEnter = () => {
    window.clearTimeout(leaveTimer.current);
    setHovered(true);
  };
  const onLeave = () => {
    window.clearTimeout(leaveTimer.current);
    leaveTimer.current = window.setTimeout(() => setHovered(false), 150);
  };
  useEffect(() => () => window.clearTimeout(leaveTimer.current), []);

  useEffect(() => {
    if (!pinned) return;
    const onDown = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setPinned(false);
    };
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && setPinned(false);
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [pinned]);

  // Fetch the live tool list each time the popover opens (MCP tools can change).
  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    invoke<ToolInfo[]>("list_available_tools", { agentId: settings.active_agent })
      .then((list) => !cancelled && setTools(list))
      .catch(() => !cancelled && setTools([]));
    return () => { cancelled = true; };
  }, [open, settings.active_agent]);

  const known = !!usage && usage.total > 0;
  const used = usage?.used ?? 0;
  const total = usage?.total ?? 0;
  const ratio = known ? used / total : 0;
  const percent = Math.round(ratio * 100);
  const remaining = Math.max(0, total - used);
  const tip = known
    ? t("chat.context.tooltip", {
        used: used.toLocaleString(),
        total: total.toLocaleString(),
        percent,
      })
    : t("chat.context.none");

  return (
    <div className="relative" ref={ref} onMouseEnter={onEnter} onMouseLeave={onLeave}>
      <button
        type="button"
        onClick={() => setPinned((v) => !v)}
        title={tip}
        aria-label={tip}
        className={`flex items-center gap-1.5 rounded-field border px-2 py-1 transition-colors ${
          open ? "border-accent-line bg-accent-soft" : "border-line bg-surface hover:bg-hover"
        }`}
      >
        <ProgressRing value={ratio} />
        <span className={`text-note font-medium ${known ? "text-ink-soft" : "text-ink-faint"}`}>
          {known ? `${percent}%` : "—"}
        </span>
      </button>
      {open && (
        /* Anchored flush to the button so the hover path into it is unbroken. */
        <div className="absolute right-0 top-full z-30 pt-1.5">
          <div className="w-64 rounded-card border border-line bg-surface p-3 text-left shadow-pop">
            <div className="flex items-baseline justify-between">
              <span className="text-note font-semibold text-ink-soft">{t("chat.context.title")}</span>
              <span className="text-title font-semibold text-accent">{known ? `${percent}%` : "—"}</span>
            </div>
            {known ? (
              <>
                <div className="mt-1.5 text-note text-ink-soft">
                  {t("chat.context.usedTotal", { used: used.toLocaleString(), total: total.toLocaleString() })}
                </div>
                <div className="mt-0.5 text-note text-ink-faint">
                  {t("chat.context.remaining", { remaining: remaining.toLocaleString() })}
                </div>
              </>
            ) : (
              <div className="mt-1.5 text-note text-ink-faint">{t("chat.context.none")}</div>
            )}

            {/* Available tools */}
            <div className="mt-2.5 border-t border-line pt-2.5">
              <div className="mb-1.5 text-meta font-semibold uppercase tracking-wide text-ink-faint">
                {t("chat.context.tools")}{tools ? ` (${tools.length})` : ""}
              </div>
              {tools === null ? (
                <div className="text-note text-ink-faint">{t("common.loading")}</div>
              ) : tools.length === 0 ? (
                <div className="text-note text-ink-faint">{t("chat.context.toolsEmpty")}</div>
              ) : (
                <div className="flex max-h-52 flex-col gap-0.5 overflow-y-auto">
                  {tools.map((tool) => (
                    <div key={tool.name} className="rounded-md px-1.5 py-1 hover:bg-surface-soft" title={tool.description}>
                      <div className="flex items-center gap-1.5">
                        <span className="truncate font-mono text-note text-ink">{tool.name}</span>
                        {tool.is_mcp && (
                          <span className="shrink-0 rounded bg-purple-100 px-1 text-meta font-medium text-purple-600">MCP</span>
                        )}
                      </div>
                      {tool.description && (
                        <div className="truncate text-meta text-ink-faint">{tool.description}</div>
                      )}
                    </div>
                  ))}
                </div>
              )}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}