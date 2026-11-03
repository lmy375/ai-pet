import { useState } from "react";
import type { SwarmTask } from "../../../hooks/useSwarm";
import { useI18n, type TKey } from "../../../i18n";
import { IconActionButton } from "../../ui/IconButton";
import { ChatIcon, TrashIcon } from "../../Icons";

export type TaskGroup = "pending" | "running" | "error" | "completed";

export const TASK_GROUPS: TaskGroup[] = ["pending", "running", "error", "completed"];

export const TASK_DOT: Record<TaskGroup, string> = {
  pending: "bg-amber-400",
  running: "bg-sky-500",
  error: "bg-red-500",
  completed: "bg-emerald-500",
};

export const TASK_LABEL: Record<TaskGroup, TKey> = {
  pending: "swarm.task.pending",
  running: "swarm.task.running",
  error: "swarm.task.error",
  completed: "swarm.task.completed",
};

export function taskGroup(task: SwarmTask): TaskGroup {
  if (task.status === "completed" && task.outcome === "failed") return "error";
  return task.status;
}

export function TaskCard({
  task,
  workerId,
  onOpenChat,
  onDelete,
}: {
  task: SwarmTask;
  workerId?: string;
  onOpenChat: (workerId: string) => void;
  onDelete: (task: SwarmTask) => void;
}) {
  const { t } = useI18n();
  const [expanded, setExpanded] = useState(false);
  const group = taskGroup(task);
  const label = t(TASK_LABEL[group]);
  return (
    <div
      role="button"
      tabIndex={0}
      aria-expanded={expanded}
      onClick={() => setExpanded((value) => !value)}
      onKeyDown={(event) => {
        if (event.target !== event.currentTarget) return;
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          setExpanded((value) => !value);
        }
      }}
      className="cursor-pointer rounded-card border border-line bg-surface p-2.5 shadow-sm transition-colors hover:border-accent-line focus:outline-none focus:ring-1 focus:ring-accent"
    >
      <div className="flex items-center gap-1.5">
        <span
          role="img"
          aria-label={label}
          title={label}
          className={`h-2.5 w-2.5 shrink-0 rounded-full ${TASK_DOT[group]}`}
        />
        <div className="min-w-0 flex-1 truncate text-body font-medium text-ink">{task.title}</div>
        <IconActionButton
          size="sm"
          disabled={!workerId}
          aria-label={t("swarm.task.chat")}
          title={workerId ? t("swarm.task.chat") : t("swarm.task.chatUnavailable")}
          className="disabled:cursor-not-allowed disabled:opacity-35"
          onClick={(event) => {
            event.stopPropagation();
            if (workerId) onOpenChat(workerId);
          }}
        >
          <ChatIcon className="h-4 w-4" />
        </IconActionButton>
        <IconActionButton
          size="sm"
          variant="danger"
          aria-label={t("swarm.task.delete")}
          title={t("swarm.task.delete")}
          onClick={(event) => {
            event.stopPropagation();
            onDelete(task);
          }}
        >
          <TrashIcon className="h-4 w-4" />
        </IconActionButton>
      </div>
      {expanded && (
        <div className="mt-2 border-t border-line/70 pt-2">
          <div className="whitespace-pre-wrap text-note text-ink-soft">{task.detail}</div>
          <div className="mt-2 h-1.5 overflow-hidden rounded-full bg-line">
            <div className="h-full rounded-full bg-accent transition-all" style={{ width: `${task.progress}%` }} />
          </div>
          <div className="mt-1 flex justify-between gap-2 text-meta text-ink-faint">
            <span className="truncate">{task.progress}% {task.progress_summary}</span>
            <span className="shrink-0">{task.attempt > 0 ? t("swarm.task.attempt", { count: task.attempt }) : ""}</span>
          </div>
          {task.result && <div className="mt-2 max-h-48 overflow-y-auto whitespace-pre-wrap rounded-field bg-surface-soft px-2 py-1.5 text-note text-ink-soft">{task.result}</div>}
        </div>
      )}
    </div>
  );
}