import { useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Badge, type BadgeColor } from "../ui/Badge";
import { Button } from "../ui/Button";
import { CodeBlock } from "../ui/CodeBlock";
import { RefreshIcon, ClockIcon, ChevronRight, CloseIcon } from "../Icons";
import { IconActionButton } from "../ui/IconButton";
import { useI18n } from "../../i18n";
import { formatIsoTime } from "../../utils/format";
import { usePolling } from "../../hooks/usePolling";

interface TaskListItem {
  taskId: string;
  kind: string; // "bash" | "subagent"
  label: string;
  status: string; // "running" | "finished"
  returnCode: number | null;
  elapsedMs: number;
  startedAt: string;
  sessionId: string;
}

interface TaskDetail {
  taskId: string;
  input: string;
  stdout: string;
  stderr: string;
  status: string;
  returnCode: number | null;
}

const KIND_COLOR: Record<string, BadgeColor> = {
  bash: "orange",
  subagent: "purple",
  heartbeat: "sky",
};

function fmtDuration(ms: number): string {
  const s = Math.floor(ms / 1000);
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m${s % 60}s`;
  return `${Math.floor(m / 60)}h${m % 60}m`;
}

function TaskRow({
  task,
  expanded,
  detail,
  onToggle,
  onKill,
  killing,
}: {
  task: TaskListItem;
  expanded: boolean;
  detail: TaskDetail | null;
  onToggle: (id: string) => void;
  onKill: (id: string) => void;
  killing: boolean;
}) {
  const { t } = useI18n();
  const kindColor: BadgeColor = KIND_COLOR[task.kind] ?? "slate";
  const kindText =
    task.kind === "subagent" ? t("tasks.kind.subagent")
    : task.kind === "heartbeat" ? t("tasks.kind.heartbeat")
    : task.kind === "bash" ? "Bash"
    : task.kind;
  const running = task.status === "running";
  const killed = task.returnCode === -1;

  return (
    <div className="rounded-xl border border-line/70 bg-surface">
      {/* Header — click to expand */}
      <div
        onClick={() => onToggle(task.taskId)}
        className="flex cursor-pointer select-none items-start gap-2 px-3 py-2.5"
      >
        <ChevronRight
          className={`mt-0.5 h-3.5 w-3.5 shrink-0 text-ink-faint transition-transform ${expanded ? "rotate-90" : ""}`}
        />
        <div className="min-w-0 flex-1">
          <div className="flex min-w-0 items-center gap-1.5">
            <Badge color={kindColor}>{kindText}</Badge>
            <span title={task.label} className="min-w-0 flex-1 truncate text-[13px] text-ink">
              {task.label || t("tasks.noLabel")}
            </span>
          </div>
          <div className="mt-1 flex items-center gap-1 text-meta text-ink-faint">
            <ClockIcon className="h-3 w-3" />
            {fmtDuration(task.elapsedMs)}
            <span>·</span>
            {formatIsoTime(task.startedAt)}
          </div>
        </div>
        <div className="mt-0.5 shrink-0">
          {running ? (
            <Button
              variant="danger"
              size="sm"
              disabled={killing}
              onClick={(e) => {
                e.stopPropagation();
                onKill(task.taskId);
              }}
            >
              {killing ? t("tasks.killing") : t("tasks.kill")}
            </Button>
          ) : killed ? (
            <Badge color="amber">{t("tasks.killed")}</Badge>
          ) : task.returnCode === 0 ? (
            <Badge color="green">{t("tasks.done")}</Badge>
          ) : (
            <Badge color="slate">{t("tasks.ended")}{task.returnCode != null ? ` (${task.returnCode})` : ""}</Badge>
          )}
        </div>
      </div>

      {/* Detail — lazy-loaded on expand */}
      {expanded && (
        <div className="space-y-2 border-t border-line/70 px-3 py-2.5">
          {detail ? (
            <>
              <div className="grid grid-cols-[88px_minmax(0,1fr)] gap-x-2 gap-y-1 text-[12px]">
                <span className="font-semibold text-ink-faint">{t("tasks.triggerTime")}</span>
                <span className="min-w-0 break-all text-ink-soft">{task.startedAt}</span>
              </div>
              <div>
                <div className="mb-1 text-[11px] font-semibold text-ink-faint">
                  {task.kind === "bash" ? t("tasks.cmd") : "Prompt"}
                </div>
                <CodeBlock>{detail.input || "—"}</CodeBlock>
              </div>
              <div>
                <div className="mb-1 text-[11px] font-semibold text-ink-faint">{t("tasks.result")}</div>
                <CodeBlock>{detail.stdout || t("tasks.noOutput")}</CodeBlock>
                {detail.stderr && (
                  <>
                    <div className="mb-1 mt-2 text-[11px] font-semibold text-ink-faint">stderr</div>
                    <CodeBlock>{detail.stderr}</CodeBlock>
                  </>
                )}
              </div>
            </>
          ) : (
            <div className="text-[12px] text-ink-faint">{t("common.loading")}</div>
          )}
        </div>
      )}
    </div>
  );
}

export function TaskDrawer({ onClose }: { onClose: () => void }) {
  const { t } = useI18n();
  const [tasks, setTasks] = useState<TaskListItem[]>([]);
  const [killing, setKilling] = useState<Set<string>>(new Set());
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [detail, setDetail] = useState<TaskDetail | null>(null);

  const fetchTasks = useCallback(async () => {
    try {
      setTasks(await invoke<TaskListItem[]>("list_tasks"));
    } catch (e) {
      console.error("Failed to list tasks:", e);
    }
  }, []);

  const fetchDetail = useCallback(async (taskId: string) => {
    try {
      setDetail(await invoke<TaskDetail>("check_task_status", { taskId }));
    } catch (e) {
      console.error("Failed to load task detail:", e);
    }
  }, []);

  usePolling(() => {
    fetchTasks();
    if (expandedId) fetchDetail(expandedId); // keep a running task's output fresh
  }, 2000);

  const toggle = useCallback(
    (taskId: string) => {
      if (expandedId === taskId) {
        setExpandedId(null);
        setDetail(null);
      } else {
        setExpandedId(taskId);
        setDetail(null);
        fetchDetail(taskId);
      }
    },
    [expandedId, fetchDetail],
  );

  const kill = useCallback(
    async (taskId: string) => {
      if (!window.confirm(t("tasks.confirmKill"))) return;
      setKilling((s) => new Set(s).add(taskId));
      try {
        await invoke("kill_task", { taskId });
        await fetchTasks();
        if (expandedId === taskId) await fetchDetail(taskId);
      } catch (e) {
        console.error("Failed to kill task:", e);
      } finally {
        setKilling((s) => {
          const n = new Set(s);
          n.delete(taskId);
          return n;
        });
      }
    },
    [fetchTasks, fetchDetail, expandedId, t],
  );

  // Running first (newest started first), then finished (most recent first).
  const running = tasks
    .filter((t) => t.status === "running")
    .sort((a, b) => b.startedAt.localeCompare(a.startedAt));
  const finished = tasks
    .filter((t) => t.status !== "running")
    .sort((a, b) => b.startedAt.localeCompare(a.startedAt));

  const renderRow = (task: TaskListItem) => (
    <TaskRow
      key={task.taskId}
      task={task}
      expanded={expandedId === task.taskId}
      detail={expandedId === task.taskId ? detail : null}
      onToggle={toggle}
      onKill={kill}
      killing={killing.has(task.taskId)}
    />
  );

  return (
    <aside
      id="tasks-drawer"
      aria-label={t("tasks.title")}
      className="task-drawer-in absolute inset-y-0 right-0 z-20 flex w-[360px] max-w-[calc(100%-1rem)] flex-col border-l border-line bg-surface shadow-pop"
    >
      <div className="flex shrink-0 items-center justify-between border-b border-line px-4 py-3">
        <h2 className="text-title font-semibold text-ink">{t("tasks.title")}</h2>
        <div className="flex items-center gap-1">
          <IconActionButton onClick={fetchTasks} title={t("common.refresh")}>
            <RefreshIcon className="h-4 w-4" />
          </IconActionButton>
          <IconActionButton onClick={onClose} title={t("tasks.close")}>
            <CloseIcon className="h-4 w-4" />
          </IconActionButton>
        </div>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-4 py-4">
        {tasks.length === 0 ? (
          <div className="mt-16 text-center text-[13px] text-ink-faint">{t("tasks.empty")}</div>
        ) : (
          <div className="space-y-5">
            <section>
              <div className="mb-1.5 text-[12px] font-semibold text-ink-faint">{t("tasks.running", { count: running.length })}</div>
              {running.length === 0 ? (
                <div className="rounded-xl border border-dashed border-line px-3 py-3 text-[12px] text-ink-faint">
                  {t("tasks.runningEmpty")}
                </div>
              ) : (
                <div className="space-y-2">{running.map(renderRow)}</div>
              )}
            </section>

            {finished.length > 0 && (
              <section>
                <div className="mb-1.5 text-[12px] font-semibold text-ink-faint">{t("tasks.recent", { count: finished.length })}</div>
                <div className="space-y-2">{finished.map(renderRow)}</div>
              </section>
            )}
          </div>
        )}
      </div>
    </aside>
  );
}
