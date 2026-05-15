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
  { name: "reset", description: "清掉 LLM 上下文但保留可见历史（与 TG /reset 对偶）", parametric: false },
  { name: "repeat", description: "再发一遍上一条 user 消息（IM 风便利）", parametric: false },
  { name: "tasks", description: "切到「任务」标签", parametric: false },
  { name: "stats", description: "汇总：待办 / 逾期 / 今日完成 / 出错 / 今日取消 计数", parametric: false },
  { name: "today", description: "今日叙事视图：到期 / 已完成任务标题清单", parametric: false },
  { name: "mood", description: "查看宠物当前心情", parametric: false },
  { name: "whoami", description: "宠物自我介绍：陪伴 / 心情 / 自我画像 / 近常用工具", parametric: false },
  { name: "version", description: "查看 pet 版本 / schema / 平台", parametric: false },
  { name: "clearstats", description: "清掉 slash 命令使用历史（重置 /help 与菜单排序）", parametric: false },
  { name: "title", description: "改当前会话标题：/title <新标题>", parametric: true },
  { name: "new", description: "新建会话：/new [初始标题]（留空走默认「新会话」）", parametric: true },
  { name: "pin", description: "无参 → 钉住当前会话；带参 → 钉任务：/pin [<标题>]", parametric: false },
  { name: "search", description: "打开跨会话搜索面板", parametric: false },
  { name: "sleep", description: "让宠物 mute 主动开口 N 分钟（缺省 30；输 0 解除）", parametric: true },
  { name: "done", description: "标记任务完成：/done <标题（子串模糊匹配）>", parametric: true },
  { name: "cancel", description: "取消任务：/cancel <标题（子串模糊匹配）>", parametric: true },
  { name: "retry", description: "重试 Error 任务：/retry <标题（子串模糊匹配）>", parametric: true },
  {
    name: "snooze",
    description: "暂停任务：/snooze <标题> [30m / 2h / tonight / tomorrow / monday]（缺省 30m）",
    parametric: true,
  },
  { name: "unsnooze", description: "解除任务暂停：/unsnooze <标题>", parametric: true },
  { name: "unpin", description: "取消任务钉住：/unpin <标题>", parametric: true },
  { name: "image", description: "生成图：/image <描述>（-n 多张 / -r 引用上文 / -h help）", parametric: true },
  { name: "help", description: "在当前会话展示命令清单", parametric: false },
];

const DEFAULT_SLEEP_MINUTES = 30;
/// `/snooze` 不带 preset token 时的缺省时长，分钟。与 TG `/snooze` 同。
const DEFAULT_SNOOZE_MINUTES = 30;

/// Snooze preset 的语义键。与 Rust `SnoozeSpec` 一一对应；handler 拿到后
/// 配合 `computeSnoozeUntil(spec, now)` 算绝对时刻。
export type SnoozeSpec =
  | { kind: "minutes"; n: number }
  | { kind: "hours"; n: number }
  | { kind: "tonight" }
  | { kind: "tomorrow" }
  | { kind: "monday" };

/// 把 `/snooze` 的 preset token 解析为 SnoozeSpec。大小写不敏感。
/// 空串 / 不识别 / 数字越界（> 7 天）→ null。与 Rust `parse_snooze_token`
/// 同算法 —— 让桌面与 TG 行为完全对等。
export function parseSnoozeToken(token: string): SnoozeSpec | null {
  const t = token.trim().toLowerCase();
  if (t.length === 0) return null;
  if (t === "tonight") return { kind: "tonight" };
  if (t === "tomorrow") return { kind: "tomorrow" };
  if (t === "monday") return { kind: "monday" };
  const mMatch = /^(\d+)m$/.exec(t);
  if (mMatch) {
    const n = parseInt(mMatch[1], 10);
    if (!Number.isFinite(n) || n <= 0 || n > 7 * 24 * 60) return null;
    return { kind: "minutes", n };
  }
  const hMatch = /^(\d+)h$/.exec(t);
  if (hMatch) {
    const n = parseInt(hMatch[1], 10);
    if (!Number.isFinite(n) || n <= 0 || n > 7 * 24) return null;
    return { kind: "hours", n };
  }
  return null;
}

/// 把 SnoozeSpec + now 算成 `YYYY-MM-DD HH:MM` 字符串（task_set_snooze 接口
/// 协议格式，空格分隔不是 ISO `T`）。tonight / tomorrow / monday 边界与桌面
/// 右键 Snooze chip + Rust `compute_snooze_until` 完全一致。
export function computeSnoozeUntil(spec: SnoozeSpec, now: Date): string {
  const fmt = (d: Date) => {
    const y = d.getFullYear();
    const mo = String(d.getMonth() + 1).padStart(2, "0");
    const da = String(d.getDate()).padStart(2, "0");
    const hh = String(d.getHours()).padStart(2, "0");
    const mm = String(d.getMinutes()).padStart(2, "0");
    return `${y}-${mo}-${da} ${hh}:${mm}`;
  };
  switch (spec.kind) {
    case "minutes":
      return fmt(new Date(now.getTime() + spec.n * 60 * 1000));
    case "hours":
      return fmt(new Date(now.getTime() + spec.n * 60 * 60 * 1000));
    case "tonight": {
      const d = new Date(
        now.getFullYear(),
        now.getMonth(),
        now.getDate(),
        18,
        0,
        0,
      );
      if (d.getTime() <= now.getTime()) d.setDate(d.getDate() + 1);
      return fmt(d);
    }
    case "tomorrow":
      return fmt(
        new Date(
          now.getFullYear(),
          now.getMonth(),
          now.getDate() + 1,
          9,
          0,
          0,
        ),
      );
    case "monday": {
      // JS getDay: Sun=0..Sat=6；目标：下个周一（今日也是周一时跳下周一）。
      const today = now.getDay();
      const daysAhead = today === 0 ? 1 : 7 - today + 1;
      return fmt(
        new Date(
          now.getFullYear(),
          now.getMonth(),
          now.getDate() + daysAhead,
          9,
          0,
          0,
        ),
      );
    }
  }
}

/// `/snooze <title> [preset]` 参数串拆 `(title, token)`。取最后一个 whitespace-
/// 分隔 token；命中 `parseSnoozeToken` 时剥下作 preset，其余拼回 title；不命
/// 中 → 全 arg 当 title、token 空。与 Rust `split_trailing_snooze_token` 同算
/// 法 —— 让 `/snooze 报告 with whitespace` 这种"标题含 preset 字眼"也能正确归到
/// title 而不是被吞当 preset。
function splitTrailingSnoozeToken(arg: string): { title: string; token: string } {
  const a = arg.trim();
  if (a.length === 0) return { title: "", token: "" };
  const words = a.split(/\s+/);
  if (words.length < 2) return { title: a, token: "" };
  const last = words[words.length - 1];
  if (parseSnoozeToken(last) !== null) {
    return { title: words.slice(0, -1).join(" "), token: last };
  }
  return { title: a, token: "" };
}

/// `/image -n N` 的前端"软"上限。提供商 API（dall-e-3 只支持 1，dall-e-2 支
/// 持 10，SD/flux 通常 1-4）会自己再约束；这里只是兜底防误打 -n 100 触发
/// 大额生成。
const IMAGE_MAX_N = 8;

/// 解析后的 action。Unknown 留命令名用于错误文案。Incomplete 表示用户刚敲了
/// `/` 还没输入命令名，UI 此时应展示全部命令任选。
export type SlashAction =
  | { kind: "clear" }
  | { kind: "reset" }
  | { kind: "repeat" }
  | { kind: "tasks" }
  | { kind: "stats" }
  | { kind: "today" }
  | { kind: "mood" }
  | { kind: "whoami" }
  | { kind: "version" }
  | { kind: "clearstats" }
  | { kind: "title"; query: string }
  | { kind: "new"; query: string }
  | { kind: "pin" }
  | { kind: "pinTask"; query: string }
  | { kind: "search" }
  | { kind: "sleep"; minutes: number }
  | { kind: "done"; query: string }
  | { kind: "cancel"; query: string }
  | { kind: "retry"; query: string }
  | { kind: "snooze"; title: string; spec: SnoozeSpec }
  | { kind: "unsnooze"; query: string }
  | { kind: "unpin"; query: string }
  | {
      kind: "image";
      prompt: string;
      n: number;
      referenceLastAssistant: boolean;
      /// `-s WxH` 覆盖 settings.image_size。null = 走 settings 默认。前端只做
      /// 格式校验（`\d+x\d+`），合法性由 provider 拒绝时透传到 errors。
      sizeOverride: string | null;
    }
  | { kind: "imageHelp" }
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
    case "reset":
      return { kind: "reset" };
    case "repeat":
      return { kind: "repeat" };
    case "tasks":
      return { kind: "tasks" };
    case "stats":
      return { kind: "stats" };
    case "today":
      return { kind: "today" };
    case "mood":
      return { kind: "mood" };
    case "whoami":
      return { kind: "whoami" };
    case "version":
      return { kind: "version" };
    case "clearstats":
      return { kind: "clearstats" };
    case "title": {
      // 必带新标题；空 → unknown 让用户看用法提示
      if (arg.length === 0) return { kind: "unknown", name: "title" };
      return { kind: "title", query: arg };
    }
    case "new": {
      // 空 arg 合法 —— 等价于点 ＋ 新建按钮（默认 "新会话" 标题）
      return { kind: "new", query: arg };
    }
    case "pin": {
      // 双语义：无参 → 切换当前会话钉住（原 SessionList toggle 行为）；带参
      // → 钉指定任务（写任务 description 的 `[pinned]` marker）。两个动作发
      // 生在不同对象上，由 `/pin` 后是否跟标题消歧；与 /snooze 单义命令对照
      // 看，pin 之所以兼容双语义是因为"钉当前会话"恰好是高频且没歧义的 alias。
      if (arg.length === 0) return { kind: "pin" };
      return { kind: "pinTask", query: arg };
    }
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
    case "done": {
      // 必须带 title query；空参数 → unknown 提示用法。fuzzy 匹配在执行层
      // (PanelChat) 做，parser 只做字符串拆分。
      if (arg.length === 0) return { kind: "unknown", name: "done" };
      return { kind: "done", query: arg };
    }
    case "cancel": {
      // 与 done 同构。
      if (arg.length === 0) return { kind: "unknown", name: "cancel" };
      return { kind: "cancel", query: arg };
    }
    case "retry": {
      // 与 done / cancel 同构；执行层做 status==error 预过滤。
      if (arg.length === 0) return { kind: "unknown", name: "retry" };
      return { kind: "retry", query: arg };
    }
    case "snooze": {
      // `/snooze <title> [preset]`：剥尾随 preset token，余者当 title。空
      // title → unknown 让用户看用法。preset 缺省 30m；不识别的 token 已被
      // splitTrailingSnoozeToken 留作 title 一部分（与 TG 同语义）—— 用户
      // "/snooze 倒垃圾 with whitespace" 不会把 whitespace 误当 preset。
      if (arg.length === 0) return { kind: "unknown", name: "snooze" };
      const { title, token } = splitTrailingSnoozeToken(arg);
      if (title.length === 0) return { kind: "unknown", name: "snooze" };
      const spec: SnoozeSpec =
        token.length > 0
          ? (parseSnoozeToken(token) ?? { kind: "minutes", n: DEFAULT_SNOOZE_MINUTES })
          : { kind: "minutes", n: DEFAULT_SNOOZE_MINUTES };
      return { kind: "snooze", title, spec };
    }
    case "unsnooze": {
      // 与 done 同构：执行层 fuzzy 命中 title 后调 task_set_snooze(null)。
      if (arg.length === 0) return { kind: "unknown", name: "unsnooze" };
      return { kind: "unsnooze", query: arg };
    }
    case "unpin": {
      // 与 pin 对偶；调 task_set_pinned(false) 剥所有 `[pinned]` marker。
      if (arg.length === 0) return { kind: "unknown", name: "unpin" };
      return { kind: "unpin", query: arg };
    }
    case "image": {
      // 空 prompt → 当 unknown，弹错给用户提示用法。`/image` 默认 parametric=true，
      // 菜单选中只回填 `/image ` 等用户输；这里命中是用户已经按 Enter 提交。
      if (arg.length === 0) return { kind: "unknown", name: "image" };
      // -h / --help：用户敲 `/image -h` 显内置 help 文案。比其它 flag 优先 ——
      // help 一旦命中就不再解析其它 flag。位置必须在 arg 头（或独占整 arg）。
      if (/^(?:-h|--help)(?:\s|$)/i.test(arg)) {
        return { kind: "imageHelp" };
      }
      // 顺序解析 -n / -r / -s 三个 flag，都必须放在 prompt 之前；顺序任意但
      // 每个最多出现一次。`-n N` 是 1..=IMAGE_MAX_N；`-r` 是 boolean；
      // `-s WxH` 是 `\d+x\d+` 格式串。任一不合法整体当 unknown。
      let rest = arg;
      let n = 1;
      let referenceLastAssistant = false;
      let sizeOverride: string | null = null;
      let nSeen = false;
      let rSeen = false;
      let sSeen = false;
      // 最多剥 3 个 flag（防同 flag 重复 → unknown 报错）；之后剩余即 prompt。
      for (let i = 0; i < 3; i += 1) {
        const nMatch = rest.match(/^-n\s+(\d+)(?:\s+(.+))?$/s);
        if (nMatch) {
          if (nSeen) return { kind: "unknown", name: "image" };
          const val = parseInt(nMatch[1], 10);
          if (Number.isNaN(val) || val < 1 || val > IMAGE_MAX_N) {
            return { kind: "unknown", name: "image" };
          }
          n = val;
          nSeen = true;
          rest = (nMatch[2] ?? "").trim();
          continue;
        }
        const rMatch = rest.match(/^-r(?:\s+(.+))?$/s);
        if (rMatch) {
          if (rSeen) return { kind: "unknown", name: "image" };
          referenceLastAssistant = true;
          rSeen = true;
          rest = (rMatch[1] ?? "").trim();
          continue;
        }
        const sMatch = rest.match(/^-s\s+(\d+x\d+)(?:\s+(.+))?$/s);
        if (sMatch) {
          if (sSeen) return { kind: "unknown", name: "image" };
          sizeOverride = sMatch[1];
          sSeen = true;
          rest = (sMatch[2] ?? "").trim();
          continue;
        }
        break;
      }
      if (rest.length === 0) {
        // `-r` 不带 prompt 时仍允许（用户可能想"就拿上文画"）；这条放宽
        // 但仅在 referenceLastAssistant 时；其它情况空 prompt 当 unknown。
        if (!referenceLastAssistant) return { kind: "unknown", name: "image" };
      }
      return {
        kind: "image",
        prompt: rest,
        n,
        referenceLastAssistant,
        sizeOverride,
      };
    }
    default:
      return { kind: "unknown", name };
  }
}

/// 命令面板里基于当前 prefix 过滤可见命令。空 prefix 展全部；带 prefix 过滤
/// `name.startsWith(prefix)`（小写敏感）。结果会按用户最近使用频次排序：
/// 高分命令置顶，零分按 SLASH_COMMANDS 声明序兜底（新用户没用过任何命令时，
/// 排序与声明序一致 —— 体感无差别）。
export function filterCommandsByPrefix(prefix: string): SlashCommand[] {
  const p = prefix.toLowerCase();
  const matched =
    p.length === 0 ? SLASH_COMMANDS : SLASH_COMMANDS.filter((c) => c.name.startsWith(p));
  const scores = readSlashScores();
  // 稳定排序：score desc 优先，相同 score 按 SLASH_COMMANDS 中的原序（即
  // matched 数组的当前下标）。Array.sort 是稳定的（V8 Tim sort），可信赖。
  return [...matched].sort((a, b) => {
    const sa = scores[a.name] ?? 0;
    const sb = scores[b.name] ?? 0;
    if (sa !== sb) return sb - sa;
    return 0;
  });
}

/// localStorage 里 slash 命令使用频次。每次 record 时全局 ×DECAY 再 +1，让旧
/// 热点缓慢 fade、新偏好抬升。半衰期约 6.5 次（log(0.5)/log(0.9) ≈ 6.6）—— 用户
/// 切到新命令几次后排序顺位就被新偏好接管。
const SLASH_HISTORY_KEY = "pet-slash-history";
const SLASH_DECAY = 0.9;
const SLASH_PRUNE_THRESHOLD = 0.05;

type ScoreMap = Record<string, number>;

function readSlashScores(): ScoreMap {
  try {
    const raw = localStorage.getItem(SLASH_HISTORY_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw) as unknown;
    if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
      const out: ScoreMap = {};
      for (const [k, v] of Object.entries(parsed)) {
        if (typeof v === "number" && Number.isFinite(v)) out[k] = v;
      }
      return out;
    }
  } catch {
    // localStorage 禁用 / 解析失败 → 返回空 map，等价"无历史"，不影响功能。
  }
  return {};
}

/// 清掉持久化的 slash 命令使用历史 —— 让 `/help` 与 slash 菜单的排序回到
/// 声明默认序。`/clearstats` 命令的实现入口。localStorage 不可用时静默失败
/// （下次启动读到空 / 仍是空效果一致）。
export function clearSlashScores(): void {
  try {
    localStorage.removeItem(SLASH_HISTORY_KEY);
  } catch {
    // 隐私 / 配额 → 静默；下次重启读到空也是同效果
  }
}

function writeSlashScores(scores: ScoreMap): void {
  try {
    localStorage.setItem(SLASH_HISTORY_KEY, JSON.stringify(scores));
  } catch {
    // 配额满 / 私密窗口 → 静默失败，让用户至少这次使用还能正常发命令。
  }
}

/// 用户每次执行一个完整 slash 命令时调一次。全局衰减 + 当前命令 +1；衰减后
/// 低于 prune 阈值的条目从 map 删掉，防 score 表无限增长。
export function recordSlashCommandUsage(name: string): void {
  const scores = readSlashScores();
  const next: ScoreMap = {};
  for (const [k, v] of Object.entries(scores)) {
    const decayed = v * SLASH_DECAY;
    if (decayed >= SLASH_PRUNE_THRESHOLD) next[k] = decayed;
  }
  next[name] = (next[name] ?? 0) + 1;
  writeSlashScores(next);
}

/// `/image` 历史 prompt（最新在前）。每条至多 200 字（防长 prompt 把 localStorage
/// 挤爆）；同一 prompt 重复用 → dedupe 后保留最近一次位置（旧位置移除）。
///
/// 存储升级：从 `string[]` → `ImagePromptEntry[]`（带可选 thumb 缩略图）。
/// 读取时向后兼容老 `string[]` 格式：自动转成 `{ prompt }` 项无 thumb。
/// thumb 由 attachThumbToImagePrompt 在生成成功后回填。
const IMAGE_PROMPT_HISTORY_KEY = "pet-image-prompt-history";
const IMAGE_PROMPT_HISTORY_CAP = 5;
const IMAGE_PROMPT_MAX_LEN = 200;

export interface ImagePromptEntry {
  prompt: string;
  /// data URL（已被 canvas 缩到 ~64px 短边）。undefined = 未生成 / 老格式
  /// 升级 / 生成失败。
  thumb?: string;
}

/// 读出标准化 entry 列表。老格式（纯 string[]）自动转，写回时仍按新格式
/// 落盘 —— 任意一次 record 调用都会迁移到新结构。
export function readImagePrompts(): ImagePromptEntry[] {
  try {
    const raw = localStorage.getItem(IMAGE_PROMPT_HISTORY_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw) as unknown;
    if (Array.isArray(parsed)) {
      return parsed
        .map((v): ImagePromptEntry | null => {
          if (typeof v === "string") return { prompt: v };
          if (
            v &&
            typeof v === "object" &&
            typeof (v as ImagePromptEntry).prompt === "string"
          ) {
            const e = v as ImagePromptEntry;
            return e.thumb && typeof e.thumb === "string"
              ? { prompt: e.prompt, thumb: e.thumb }
              : { prompt: e.prompt };
          }
          return null;
        })
        .filter((e): e is ImagePromptEntry => e !== null);
    }
  } catch {
    // 解析失败 → 等价空历史，不影响 /image 主流程。
  }
  return [];
}

function writeImagePrompts(list: ImagePromptEntry[]): void {
  try {
    localStorage.setItem(IMAGE_PROMPT_HISTORY_KEY, JSON.stringify(list));
  } catch {
    // 配额满 / 隐私窗口 → 历史功能降级，静默失败让本次生图正常进行。
  }
}

export function recordImagePrompt(prompt: string): void {
  const trimmed = prompt.trim();
  if (!trimmed) return;
  const capped =
    trimmed.length > IMAGE_PROMPT_MAX_LEN
      ? trimmed.slice(0, IMAGE_PROMPT_MAX_LEN)
      : trimmed;
  const existing = readImagePrompts();
  // dedupe + 最新一次置顶；保持总长在 cap 内（slice 末尾掉最旧条）。
  // dedupe 时若旧条目有 thumb，回填到新条目顶上 —— 用户再点同一 prompt
  // 重发不会"画面瞬时丢"（虽然下一次 attachThumb 会更新到新生成结果）。
  const dupe = existing.find((e) => e.prompt === capped);
  const filtered = existing.filter((e) => e.prompt !== capped);
  const next: ImagePromptEntry[] = [
    dupe ? { prompt: capped, thumb: dupe.thumb } : { prompt: capped },
    ...filtered,
  ].slice(0, IMAGE_PROMPT_HISTORY_CAP);
  writeImagePrompts(next);
}

/// 生成成功后回填指定 prompt 的 thumb。thumbDataUrl 应已被压缩到 ≤ 64px
/// 短边（由 PanelChat 端 canvas 处理）以控制 localStorage 占用：5 条 × ~6KB
/// PNG ≈ 30KB。匹配按 prompt 字符串相等（与 recordImagePrompt 的 dedupe key
/// 一致）；找不到则 noop（用户可能在生成期间又敲了新 prompt 把老的挤掉）。
export function attachThumbToImagePrompt(prompt: string, thumbDataUrl: string): void {
  const trimmed = prompt.trim();
  if (!trimmed) return;
  const capped =
    trimmed.length > IMAGE_PROMPT_MAX_LEN
      ? trimmed.slice(0, IMAGE_PROMPT_MAX_LEN)
      : trimmed;
  const existing = readImagePrompts();
  const idx = existing.findIndex((e) => e.prompt === capped);
  if (idx < 0) return;
  if (existing[idx].thumb === thumbDataUrl) return; // 已是同图，避免无谓写
  const next = [...existing];
  next[idx] = { prompt: capped, thumb: thumbDataUrl };
  writeImagePrompts(next);
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

/// `/image -h` 在会话气泡里显示的文案。列所有 flag + 例子；与 IMAGE_MAX_N
/// 同步，常用例子覆盖单图 / 多图 / 引用上文 / 组合 flag。
export function formatImageHelpText(): string {
  return [
    "🎨 /image 命令用法：",
    "",
    "/image <描述>                生成 1 张图（走 settings.image_model / image_size）",
    `/image -n <N> <描述>         一次生成 N 张（前端 cap ${IMAGE_MAX_N}；后端再 clamp）`,
    "/image -r <描述>             把最近一条 assistant 文本拼到 prompt 前作上下文",
    "/image -r                    用最近 assistant 文本作 prompt（不补充）",
    "/image -s 1024x1792 <描述>   单次覆盖 size（不改 settings）",
    "/image -n 4 -s 1792x1024 -r <描述>   flag 顺序任意",
    "",
    "敲 `/image ` 后弹历史菜单 ↑↓ 选 · Enter 直发 · Tab 填回继续编辑。",
  ].join("\n");
}

/// `/help` 在会话气泡里显示的文案。按使用频次分桶 → 与 slash 菜单（同 score
/// 排序）的视觉顺序一致，顺便把"没试过"的命令独立段做发现引导。
///
/// 分桶规则：
/// - 用过（score > 0）：按 score 倒序。
/// - 没用过（score = 0）：保留 SLASH_COMMANDS 声明序（隐式约定 = 推荐优先级）。
///
/// 三种边界：全新用户（无 used）单段无 header；老用户（全 used）单段标注按
/// 频次；混合态二段 "常用 / 未试过" header。
export function formatHelpText(): string {
  const scores = readSlashScores();
  const used: SlashCommand[] = [];
  const unused: SlashCommand[] = [];
  for (const c of SLASH_COMMANDS) {
    if ((scores[c.name] ?? 0) > 0) used.push(c);
    else unused.push(c);
  }
  used.sort((a, b) => (scores[b.name] ?? 0) - (scores[a.name] ?? 0));
  const fmt = (c: SlashCommand): string => {
    const arg = c.parametric ? " <参数>" : "";
    return `/${c.name}${arg}  —  ${c.description}`;
  };
  if (used.length === 0) {
    return ["可用命令：", ...SLASH_COMMANDS.map(fmt)].join("\n");
  }
  if (unused.length === 0) {
    return ["可用命令（按近期使用频次）：", ...used.map(fmt)].join("\n");
  }
  return [
    "常用：",
    ...used.map(fmt),
    "",
    "未试过：",
    ...unused.map(fmt),
  ].join("\n");
}
