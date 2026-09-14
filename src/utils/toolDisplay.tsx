/** Per-tool header descriptors for ToolCallBlock — turns a raw tool name + its
 *  arguments into a glanceable summary (icon, action label, key inline info). */
import {
  WrenchIcon,
  TerminalIcon,
  FileTextIcon,
  FilePlusIcon,
  PencilIcon,
  ClockIcon,
  AgentIcon,
  GlobeIcon,
  SendIcon,
} from "../components/Icons";
import { parseJsonish } from "./format";

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

/** `rawArgs` is a JSON string (chat stream) or an already-parsed object (LLM
 *  log) — both reach the same header. */
export function describeToolCall(name: string, rawArgs: unknown): ToolDisplay {
  // args may be missing or partial while streaming — fall through with {}
  const parsed = parseJsonish(rawArgs);
  const args: Record<string, unknown> =
    parsed && typeof parsed === "object" && !Array.isArray(parsed) ? (parsed as Record<string, unknown>) : {};

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
    case "web_search": {
      const query = str(args.query);
      return { Icon: GlobeIcon, label: "Search", summary: query, fullSummary: query };
    }
    case "spawn_subagent": {
      const prompt = str(args.prompt);
      const summary = str(args.description) ?? prompt?.split("\n")[0];
      return { Icon: AgentIcon, label: "Agent", summary, fullSummary: str(args.description) ?? prompt };
    }
    case "GroupChat": {
      const message = str(args.message);
      return { Icon: SendIcon, label: "Group", summary: message, fullSummary: message };
    }
    default:
      return { Icon: WrenchIcon, label: name };
  }
}
