import { useEffect, useState } from "react";
import type { ToolCall } from "../../hooks/useChat";
import { SpinnerIcon } from "../Icons";
import { formatDuration, formatTokens } from "../../utils/format";
import { useI18n } from "../../i18n";

interface Props {
  /** Epoch ms the backend stamped on the turn; omitted by views that don't
   *  track one (the group tabs), which then show the phase alone. */
  startedAt?: number;
  /** Tokens the turn has burned so far; 0 until the first round reports usage. */
  tokens?: number;
  /** The round's tool calls — a still-running one names the current phase. */
  toolCalls: ToolCall[];
  streaming: string;
  reasoning: string;
}

/** A 1s ticker for the elapsed clock. The component is mounted only while a
 *  turn runs, so an idle window keeps no interval alive. */
function useSecondTick(active: boolean): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!active) return;
    setNow(Date.now());
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, [active]);
  return now;
}

/**
 * The "this turn is still running" indicator: how long it has been going, what
 * it has cost so far, and what it is doing right now — replacing the breathing
 * dots, which said only "something is happening" during turns that can run for
 * many minutes across a dozen tool rounds.
 *
 * Everything shown is derived from the turn stream the thread already has, so
 * a window that re-attaches mid-turn (reload, tab switch, focus) shows the same
 * figures as one that watched from the start.
 */
export function TurnStatus({ startedAt, tokens = 0, toolCalls, streaming, reasoning }: Props) {
  const { t } = useI18n();
  const now = useSecondTick(startedAt !== undefined);

  const running = toolCalls.find((tc) => tc.isRunning);
  const phase = running
    ? t("chat.turn.tool", { name: running.name })
    : streaming.trim()
      ? t("chat.turn.replying")
      : reasoning.trim()
        ? t("chat.turn.thinking")
        : t("chat.turn.working");

  const stats = [
    startedAt !== undefined ? formatDuration(now - startedAt) : null,
    tokens > 0 ? t("chat.turn.tokens", { n: formatTokens(tokens) }) : null,
  ].filter(Boolean) as string[];

  return (
    <div className="flex w-[min(100%,300px)] flex-col gap-2 self-start rounded-bubble border border-line bg-surface px-3.5 py-2.5 shadow-card">
      <div className="flex min-w-0 items-center gap-1.5 text-note">
        <SpinnerIcon className="h-3.5 w-3.5 shrink-0 animate-spin text-accent" />
        {stats.length > 0 && (
          <span className="shrink-0 font-medium tabular-nums text-ink-soft">{stats.join(" · ")}</span>
        )}
        {stats.length > 0 && <span className="shrink-0 text-ink-faint">·</span>}
        <span className="truncate text-ink-faint">{phase}</span>
      </div>
      {/* Indeterminate on purpose: a turn has no measurable progress, only
          "still going" — the numbers above carry the actual information. */}
      <div className="h-[3px] overflow-hidden rounded-full bg-hover">
        <div className="h-full w-1/3 animate-turn-sweep rounded-full bg-accent" />
      </div>
    </div>
  );
}
