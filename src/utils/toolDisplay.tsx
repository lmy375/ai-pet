/** Per-tool header descriptors for ToolCallBlock — turns a raw tool name + JSON
 *  arguments into a glanceable summary (icon, action label, key inline info). */
import {
  WrenchIcon,
  TerminalIcon,
  FileTextIcon,
  FilePlusIcon,
  PencilIcon,
  ClockIcon,
  AgentIcon,
} from "../components/Icons";

type IconComponent = (props: { className?: string }) => React.ReactElement;

export interface ToolDisplay {
  Icon: IconComponent;
  label: string; // action name, e.g. "Bash" | "Read" — falls back to raw name
  summary?: string; // key inline info (command / file name / task id)
  summaryMono?: boolean; // render summary in a monospace font (bash command)
  hint?: string; // muted secondary text (bash purpose)
  fullSummary?: string; // untruncated value for the hover `title`
}

/** Last path segment of an absolute/relative path; returns the input if no `/`. */
function basename(path: string): string {
  const trimmed = path.replace(/\/+$/, "");
  const i = trimmed.lastIndexOf("/");
  return i >= 0 ? trimmed.slice(i + 1) : trimmed;
}

export function describeToolCall(name: string, argsJson: string): ToolDisplay {
  let args: Record<string, unknown> = {};
  try {
    const parsed = JSON.parse(argsJson);
    if (parsed && typeof parsed === "object") args = parsed as Record<string, unknown>;
  } catch {
    // args may be empty or partial while streaming — fall through with {}
  }

  const str = (v: unknown): string | undefined => (typeof v === "string" && v ? v : undefined);

  switch (name) {
    case "bash": {
      const command = str(args.command);
      return {
        Icon: TerminalIcon,
        label: "Bash",
        summary: command,
        summaryMono: true,
        hint: str(args.description),
        fullSummary: command,
      };
    }
    case "read_file": {
      const path = str(args.file_path);
      return { Icon: FileTextIcon, label: "Read", summary: path && basename(path), fullSummary: path };
    }
    case "write_file": {
      const path = str(args.file_path);
      return { Icon: FilePlusIcon, label: "Write", summary: path && basename(path), fullSummary: path };
    }
    case "edit_file": {
      const path = str(args.file_path);
      return { Icon: PencilIcon, label: "Edit", summary: path && basename(path), fullSummary: path };
    }
    case "check_task_status": {
      const taskId = str(args.task_id);
      return { Icon: ClockIcon, label: "Status", summary: taskId, summaryMono: true, fullSummary: taskId };
    }
    case "spawn_subagent": {
      const prompt = str(args.prompt);
      const summary = str(args.description) ?? prompt?.split("\n")[0];
      return { Icon: AgentIcon, label: "Agent", summary, fullSummary: str(args.description) ?? prompt };
    }
    default:
      return { Icon: WrenchIcon, label: name };
  }
}
