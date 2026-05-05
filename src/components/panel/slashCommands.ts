// PanelChat 输入框 `/` 快捷命令的纯解析层。
//
// `parseSlashCommand` 把用户输入（要求以 `/` 起头）转换成 SlashAction，
// 或返回 `Unknown`（让 UI 提示"输入 /help 查看"）。所有命令都在这里登记，
// 新增命令只需更新 SLASH_COMMANDS 列表 + Match 一个分支。
//
// 边界：
// - `/` 单独 → `{ kind: "incomplete" }`（菜单显示全部命令，无错误提示）
// - `/cmd extra` 命令名后允许任意尾参；带参数的命令（如 `/sleep`）解析参数，
//   不带参数的命令（`/clear` `/tasks`）忽略尾随。
// - 大小写：命令名一律小写化匹配（`/Tasks` ≡ `/tasks`）。
// - `/sleep` 无参数视为默认 30 分钟（与设置滑块默认对齐）。

export interface SlashCommand {
  /// 不带 `/` 的名字
  name: string;
  /// 命令面板里展示的一行描述
  description: string;
  /// 是否带参数（带参数时菜单选中后只回填到输入框，等用户输完按 Enter 才执行）
  parametric: boolean;
}

export const SLASH_COMMANDS: SlashCommand[] = [
  { name: "clear", description: "清空当前会话的消息（不删 session 文件）", parametric: false },
  { name: "tasks", description: "切到「任务」标签", parametric: false },
  { name: "search", description: "打开跨会话搜索面板", parametric: false },
  { name: "sleep", description: "让宠物 mute 主动开口 N 分钟（缺省 30；输 0 解除）", parametric: true },
  { name: "help", description: "在当前会话展示命令清单", parametric: false },
];

const DEFAULT_SLEEP_MINUTES = 30;

/// 解析后的 action。Unknown 留命令名用于错误文案。Incomplete 表示用户刚敲了
/// `/` 还没输入命令名，UI 此时应展示全部命令任选。
export type SlashAction =
  | { kind: "clear" }
  | { kind: "tasks" }
  | { kind: "search" }
  | { kind: "sleep"; minutes: number }
  | { kind: "help" }
  | { kind: "incomplete" }
  | { kind: "unknown"; name: string };

/// 输入字符串解析。仅当首字符是 `/` 时返回非 null（调用方在调用前自行判断）。
/// trim 头部空白防御 IME 偶发首空格；命令尾参数 trim 处理。
export function parseSlashCommand(input: string): SlashAction | null {
  const trimmed = input.trimStart();
  if (!trimmed.startsWith("/")) return null;
  const after = trimmed.slice(1);
  if (after.length === 0) return { kind: "incomplete" };
  const spaceIdx = after.search(/\s/);
  const rawName = spaceIdx < 0 ? after : after.slice(0, spaceIdx);
  const arg = spaceIdx < 0 ? "" : after.slice(spaceIdx + 1).trim();
  const name = rawName.toLowerCase();
  switch (name) {
    case "clear":
      return { kind: "clear" };
    case "tasks":
      return { kind: "tasks" };
    case "search":
      return { kind: "search" };
    case "help":
      return { kind: "help" };
    case "sleep": {
      // 空参数 → DEFAULT_SLEEP_MINUTES；非整数 → 仍当 unknown，让用户看到错误反馈
      if (arg.length === 0) return { kind: "sleep", minutes: DEFAULT_SLEEP_MINUTES };
      const n = parseInt(arg, 10);
      if (Number.isNaN(n) || n < 0) return { kind: "unknown", name: "sleep" };
      return { kind: "sleep", minutes: n };
    }
    default:
      return { kind: "unknown", name };
  }
}

/// 命令面板里基于当前 prefix 过滤可见命令。空 prefix 展全部；带 prefix 过滤
/// `name.startsWith(prefix)`（小写敏感）。
export function filterCommandsByPrefix(prefix: string): SlashCommand[] {
  const p = prefix.toLowerCase();
  if (p.length === 0) return SLASH_COMMANDS;
  return SLASH_COMMANDS.filter((c) => c.name.startsWith(p));
}

/// 当前输入提取出 slash 命令的"名字 prefix" —— 用来给 UI 决定过滤集。
/// 输入不以 `/` 起头 → null（说明不在 slash 模式）。
/// 输入是 `/` 单独 → ""（展全部）。
/// 输入是 `/abc` → "abc"。
/// 输入是 `/abc def` → null（已经在敲参数了，菜单不再过滤命令名）。
export function extractCommandPrefix(input: string): string | null {
  const trimmed = input.trimStart();
  if (!trimmed.startsWith("/")) return null;
  const after = trimmed.slice(1);
  const spaceIdx = after.search(/\s/);
  if (spaceIdx >= 0) return null;
  return after;
}

/// `/help` 在会话气泡里显示的文案。每行 `/{name}  {description}`，开头
/// 一行总说明。pure 让测试 / 调用方都能直接复用。
export function formatHelpText(): string {
  const lines = SLASH_COMMANDS.map((c) => {
    const arg = c.parametric ? " <参数>" : "";
    return `/${c.name}${arg}  —  ${c.description}`;
  });
  return ["可用命令：", ...lines].join("\n");
}
