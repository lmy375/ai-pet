/**
 * butler_task `[reminderMin: N]` 标记纯逻辑。
 *
 * 含义：在 butler_task 描述中可加 `[reminderMin: N]`（N = 分钟数）。系统会
 * 在该任务的 fire-time（来自 [once:] / [deadline:] / [every:]）前 N 分钟
 * 浮一条 ChatMini 软提醒（不打开 Live2D proactive 主动开口）。让 owner 有
 * 「抬头 buffer」 —— 例如开会前 5 分钟提醒一下，不让 owner 等到点才被
 * 突然打断。
 *
 * 设计：
 * - 提取 `parseScheduleAndReminder` 把 description 一次过解析出 schedule
 *   （[once:] / [deadline:] / [every:]）和 reminderMin（可选）。简化调用
 *   方代码。
 * - `fireTimeAbs` 计算 schedule 的下一次 fire-time（绝对 `Date`）。`every`
 *   类型给"今日已过 → 明日" 语义，与 PanelMemory 既有 mostRecentFire 对偶
 *   但取下一次未来时刻。
 * - `findRemindersToFire(tasks, now, alreadyFired)` 返回当下 tick 应该触
 *   发的提醒列表（不操作状态，纯函数）。
 * - 防重 key 用 `${title}::${fireTimeIso}` —— 同一任务 same fire-cycle 内
 *   只触发一次；every 类型一天会产生新 key，所以每天触发一次。
 */

export type ButlerScheduleParsed =
  | { kind: "every"; hour: number; minute: number }
  | {
      kind: "once" | "deadline";
      year: number;
      month: number;
      day: number;
      hour: number;
      minute: number;
    };

export interface ParsedButlerDesc {
  schedule: ButlerScheduleParsed | null;
  reminderMin: number | null;
  topic: string;
}

/** 解析 butler_task 描述出 schedule + reminderMin。两个 marker 都可选；
 *  schedule 不命中时 schedule = null（任务可能是 free-form butler_task，
 *  不参与定时触发）。reminderMin 命中但无 schedule 也无意义 → caller 自然
 *  跳过。
 *
 *  description 形态举例：
 *  - `[every: 09:00] [reminderMin: 5] 早安播报`
 *  - `[once: 2026-05-20 18:00] [reminderMin: 30] 准备会议材料`
 *  - `[deadline: 2026-05-25 23:59] 月报提交`（无 reminderMin → null）
 *  - `空 desc` / `[xxx:] foo` 不识别 → schedule null
 *
 *  marker 顺序无关。topic = 去掉两个 marker 后 trim 的剩余字符串。 */
export function parseScheduleAndReminder(desc: string): ParsedButlerDesc {
  let working = desc;
  // 1. schedule
  let schedule: ButlerScheduleParsed | null = null;
  const schedRe = /\[(every|once|deadline):\s*([^\]]+)\]/;
  const schedMatch = working.match(schedRe);
  if (schedMatch) {
    const kind = schedMatch[1];
    const body = schedMatch[2].trim();
    if (kind === "every") {
      const hm = body.match(/^(\d{1,2}):(\d{1,2})$/);
      if (hm) {
        const hour = Number(hm[1]);
        const minute = Number(hm[2]);
        if (hour <= 23 && minute <= 59) {
          schedule = { kind: "every", hour, minute };
        }
      }
    } else {
      const dt = body.match(/^(\d{4})-(\d{2})-(\d{2})\s+(\d{1,2}):(\d{1,2})$/);
      if (dt) {
        schedule = {
          kind: kind as "once" | "deadline",
          year: Number(dt[1]),
          month: Number(dt[2]),
          day: Number(dt[3]),
          hour: Number(dt[4]),
          minute: Number(dt[5]),
        };
      }
    }
    if (schedule) working = working.replace(schedMatch[0], "");
  }
  // 2. reminderMin
  let reminderMin: number | null = null;
  const remRe = /\[reminderMin:\s*(\d+)\s*\]/;
  const remMatch = working.match(remRe);
  if (remMatch) {
    const n = Number(remMatch[1]);
    if (n > 0 && n <= 1440) {
      // [1, 1440 min = 24h]：负数 / 0 / 超过一天的"提前提醒" 不接受
      reminderMin = n;
    }
    working = working.replace(remMatch[0], "");
  }
  return {
    schedule,
    reminderMin,
    topic: working.trim(),
  };
}

/** 计算给定 schedule 相对于 now 的"下一次 fire-time"。every 类型：
 *  - 今日 HH:MM 未过 → 今日；否则 → 明日 HH:MM
 *  - once/deadline：返回 schedule 指定的固定时刻（不论过去未来）
 *
 *  返回 null 表示无法计算（不该发生，schedule 已经 parsed）。 */
export function nextFireTime(
  schedule: ButlerScheduleParsed,
  now: Date,
): Date | null {
  if (schedule.kind === "every") {
    const today = new Date(
      now.getFullYear(),
      now.getMonth(),
      now.getDate(),
      schedule.hour,
      schedule.minute,
      0,
      0,
    );
    if (today.getTime() > now.getTime()) return today;
    // 今日已过 → 明日
    return new Date(today.getTime() + 24 * 3600 * 1000);
  }
  return new Date(
    schedule.year,
    schedule.month - 1,
    schedule.day,
    schedule.hour,
    schedule.minute,
    0,
    0,
  );
}

export interface ReminderToFire {
  title: string;
  topic: string;
  reminderMin: number;
  fireTimeIso: string;
  /** 用作防重 key 与 caller 持久化的 Set 比对。 */
  dedupKey: string;
}

/** Pure: 列出当前 tick 应该触发的所有 reminder。条件：
 *  1. 任务 description parse 出 schedule + reminderMin
 *  2. nextFireTime - now 在 (0, reminderMin] 分钟内（即"还差不到 N 分钟到点"）
 *  3. dedupKey 不在 alreadyFired Set 内
 *
 *  把"将到点前 N 分钟"理解为"任意 tick 命中 `(reminderMin-Δ, reminderMin]`"
 *  会因 poll 间隔 Δ 漏 / 重 fire。所以条件是 fire-time - now ≤ reminderMin
 *  且 > 0；命中即 fire，配 dedupKey 保证同 cycle 只一次。每次 poll 都重算
 *  下一个 fire-time（every 类型 fire-time 在跨 0 点会变），不会卡死。
 *
 *  Δ tolerance：fire 到点之后 fire-time-now <= 0 不再 fire（不"延后"提醒，
 *  到点之后的事让 LLM proactive cycle 处理）。 */
export function findRemindersToFire(
  tasks: { title: string; description: string }[],
  now: Date,
  alreadyFired: Set<string>,
): ReminderToFire[] {
  const out: ReminderToFire[] = [];
  for (const t of tasks) {
    const { schedule, reminderMin, topic } = parseScheduleAndReminder(
      t.description,
    );
    if (!schedule || reminderMin === null) continue;
    const fire = nextFireTime(schedule, now);
    if (!fire) continue;
    const remainingMin = (fire.getTime() - now.getTime()) / 60_000;
    if (remainingMin <= 0 || remainingMin > reminderMin) continue;
    const fireTimeIso = fire.toISOString();
    const dedupKey = `${t.title}::${fireTimeIso}`;
    if (alreadyFired.has(dedupKey)) continue;
    out.push({
      title: t.title,
      topic: topic || t.title,
      reminderMin,
      fireTimeIso,
      dedupKey,
    });
  }
  return out;
}
