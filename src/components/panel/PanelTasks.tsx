import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Badge, type BadgeColor } from "../ui/Badge";
import { Button } from "../ui/Button";
import { RefreshIcon, ClockIcon, ChevronRight } from "../Icons";
import { useI18n } from "../../i18n";

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

const preClass =
  "m-0 max-h-[260px] overflow-y-auto whitespace-pre-wrap break-all rounded-lg border border-slate-100 bg-slate-50 px-2.5 py-2 font-mono text-[12px] leading-relaxed text-slate-700";

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
    <div className="rounded-xl border border-slate-200/70 bg-white">
      {/* Header — click to expand */}
      <div
        onClick={() => onToggle(task.taskId)}
        className="flex cursor-pointer select-none items-center gap-2 px-3 py-2.5"
      >
        <ChevronRight
          className={`h-3.5 w-3.5 shrink-0 text-slate-400 transition-transform ${expanded ? "rotate-90" : ""}`}
        />
        <Badge color={kindColor}>{kindText}</Badge>
        <span title={task.label} className="min-w-0 flex-1 truncate text-[13px] text-slate-700">
          {task.label || t("tasks.noLabel")}
        </span>
        <span className="flex shrink-0 items-center gap-1 text-[12px] text-slate-400">
          <ClockIcon className="h-3.5 w-3.5" />
          {fmtDuration(task.elapsedMs)}
        </span>
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

      {/* Detail — lazy-loaded on expand */}
      {expanded && (
        <div className="space-y-2 border-t border-slate-200/70 px-3 py-2.5">
          {detail ? (
            <>
              <div>
                <div className="mb-1 text-[11px] font-semibold text-slate-400">
                  {task.kind === "bash" ? t("tasks.cmd") : "Prompt"}
                </div>
                <pre className={preClass}>{detail.input || "—"}</pre>
              </div>
              <div>
                <div className="mb-1 text-[11px] font-semibold text-slate-400">{t("tasks.result")}</div>
                <pre className={preClass}>{detail.stdout || t("tasks.noOutput")}</pre>
                {detail.stderr && (
                  <>
                    <div className="mb-1 mt-2 text-[11px] font-semibold text-slate-400">stderr</div>
                    <pre className={preClass}>{detail.stderr}</pre>
                  </>
                )}
              </div>
            </>
          ) : (
            <div className="text-[12px] text-slate-400">{t("common.loading")}</div>
          )}
        </div>
      )}
    </div>
  );
}

export function PanelTasks() {
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

  useEffect(() => {
    fetchTasks();
    const timer = setInterval(() => {
      fetchTasks();
      if (expandedId) fetchDetail(expandedId); // keep a running task's output fresh
    }, 2000);
    return () => clearInterval(timer);
  }, [fetchTasks, fetchDetail, expandedId]);

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

  const renderRow = (t: TaskListItem) => (
    <TaskRow
      key={t.taskId}
      task={t}
      expanded={expandedId === t.taskId}
      detail={expandedId === t.taskId ? detail : null}
      onToggle={toggle}
      onKill={kill}
      killing={killing.has(t.taskId)}
    />
  );

  return (
    <div className="h-full overflow-y-auto px-5 py-5">
      <div className="mb-3 flex items-center justify-between">
        <h2 className="text-[15px] font-semibold text-slate-800">{t("tasks.title")}</h2>
        <button
          onClick={fetchTasks}
          title={t("common.refresh")}
          className="flex h-8 w-8 items-center justify-center rounded-lg text-slate-500 transition-colors hover:bg-slate-100 hover:text-slate-700"
        >
          <RefreshIcon className="h-[18px] w-[18px]" />
        </button>
      </div>

      {tasks.length === 0 ? (
        <div className="mt-16 text-center text-[13px] text-slate-400">{t("tasks.empty")}</div>
      ) : (
        <div className="space-y-5">
          <section>
            <div className="mb-1.5 text-[12px] font-semibold text-slate-400">{t("tasks.running", { count: running.length })}</div>
            {running.length === 0 ? (
              <div className="rounded-xl border border-dashed border-slate-200 px-3 py-3 text-[12px] text-slate-400">
                {t("tasks.runningEmpty")}
              </div>
            ) : (
              <div className="space-y-2">{running.map(renderRow)}</div>
            )}
          </section>

          {finished.length > 0 && (
            <section>
              <div className="mb-1.5 text-[12px] font-semibold text-slate-400">{t("tasks.recent", { count: finished.length })}</div>
              <div className="space-y-2">{finished.map(renderRow)}</div>
            </section>
          )}
        </div>
      )}
    </div>
  );
}
