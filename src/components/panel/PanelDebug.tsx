import { useState, useEffect, useMemo, useRef, useCallback } from "react";
import { usePollingState } from "../../hooks/usePollingState";
import { invoke } from "@tauri-apps/api/core";
import { PanelChipStrip } from "./PanelChipStrip";
import { PanelStatsCard } from "./PanelStatsCard";
import { PanelToolsTopK } from "./PanelToolsTopK";
import { PanelFilterButtonRow } from "../common/PanelFilterButtonRow";
import { PanelToneStrip } from "./PanelToneStrip";
import {
  CacheStats,
  EnvToolStats,
  LlmOutcomeStats,
  MoodTagStats,
  NATURE_META,
  PROMPT_RULE_DESCRIPTIONS,
  PendingReminder,
  ProactiveDecision,
  PromptRuleNature,
  PromptTiltStats,
  ToneSnapshot,
} from "./panelTypes";

/// R99: 多选 chip 通用样式。active 走 accent 填充 + 白字；inactive 走 accent
/// 40% alpha 边框 + card 底 + fg 字（与 R84 决策日志同款）。模块级让多个 chip
/// 行共享：决策日志 kinds + 时间窗 / 日志 level。
const multiSelectChipStyle = (
  isActive: boolean,
  accent: string,
): React.CSSProperties => ({
  padding: "2px 8px",
  fontSize: "10px",
  borderRadius: "10px",
  border: `1px solid ${isActive ? accent : `${accent}66`}`,
  background: isActive ? accent : "var(--pet-color-card)",
  color: isActive ? "#fff" : "var(--pet-color-fg)",
  cursor: "pointer",
  fontWeight: 600,
  fontFamily: "inherit",
});

export function PanelDebug() {
  const [cacheStats, setCacheStats] = useState<CacheStats>({
    turns: 0,
    total_hits: 0,
    total_calls: 0,
  });
  const [decisions, setDecisions] = useState<ProactiveDecision[]>([]);
  const [moodTagStats, setMoodTagStats] = useState<MoodTagStats>({
    with_tag: 0,
    without_tag: 0,
    no_mood: 0,
  });
  const [llmOutcomeStats, setLlmOutcomeStats] = useState<LlmOutcomeStats>({
    spoke: 0,
    silent: 0,
    error: 0,
  });
  const [envToolStats, setEnvToolStats] = useState<EnvToolStats>({
    spoke_total: 0,
    spoke_with_any: 0,
    active_window: 0,
    weather: 0,
    upcoming_events: 0,
    memory_search: 0,
  });
  const [promptTiltStats, setPromptTiltStats] = useState<PromptTiltStats>({
    restraint_dominant: 0,
    engagement_dominant: 0,
    balanced: 0,
    neutral: 0,
  });
  const [recentSpeeches, setRecentSpeeches] = useState<string[]>([]);
  // R142: 三 timeline 切换 tab。三卡（speech / tool / feedback）原本堆叠占
  // 垂直空间多；改成单选 tab 让用户聚焦其中一种。default 选 speech（用户
  // 最关心宠物刚说了什么）。session 内有效，关 panel 重置回 speech。
  type TimelineTab = "speech" | "tool" | "feedback";
  const [activeTimeline, setActiveTimeline] = useState<TimelineTab>("speech");
  const [lifetimeSpeechCount, setLifetimeSpeechCount] = useState<number>(0);
  // 既有"今日 / 本周"固定窗口仍保留 —— markdown 导出 / 既有 stats card
  // 仍用 today / week。Iter Pω: 加一个可调窗口 N 日 stat 在 stats card 同行。
  const [todaySpeechCount, setTodaySpeechCount] = useState<number>(0);
  const [weekSpeechCount, setWeekSpeechCount] = useState<number>(0);
  // 可调窗口 N 日的主动开口次数。选项 1/3/7/14/30；localStorage 持久 ——
  // 用户偏好稳定（看长尺度的人会一直留 30 天）。
  const [speechWindowDays, setSpeechWindowDays] = useState<number>(() => {
    try {
      const raw = window.localStorage.getItem("pet-debug-speech-window-days");
      if (raw) {
        const n = parseInt(raw, 10);
        if ([1, 3, 7, 14, 30].includes(n)) return n;
      }
    } catch {
      // ignore
    }
    return 3;
  });
  // speechWindowCount：跟随 speechWindowDays 状态 — 切 3/7/30 天 chip 时立
  // 即重 fetch。usePollingState 的 deps 参数承载这语义；30s 周期 polling 在
  // 各档下都自动跑。catch 走 hook 静默吞。
  const { data: speechWindowCount } = usePollingState(
    () =>
      invoke<number>("get_speech_count_days", { days: speechWindowDays }),
    30_000,
    0,
    [speechWindowDays],
  );
  // 今日 24 小时主动开口分桶。get_today_speech_hourly 返长度 24 数组；
  // index 0 = 00:00。每 60s 刷新一次（hour 粒度，更高频意义不大）。形状
  // 验证（必须 length === 24）走 fetcher 内 throw → 由 hook 静默吞掉保留
  // 上一份 buckets，避免渲染时撞 length 不对的 array crash。
  const { data: hourlyBuckets } = usePollingState(
    async () => {
      const arr = await invoke<number[]>("get_today_speech_hourly");
      if (!Array.isArray(arr) || arr.length !== 24) {
        throw new Error("invalid hourly buckets shape");
      }
      return arr;
    },
    60_000,
    new Array(24).fill(0) as number[],
  );
  const [companionshipDays, setCompanionshipDays] = useState<number>(0);
  // TG bot 启动期非 fatal 失败列表（set_my_commands / bot_start 等）。
  // 进程内 in-memory，重启清空；用于让用户知道为啥 bot 自动补全 / 整体
  // 没起来。空 Vec 时 banner 不渲染。
  const [tgStartupWarnings, setTgStartupWarnings] = useState<
    { timestamp: string; kind: string; message: string }[]
  >([]);
  // 用户已 dismiss 的 TG 告警指纹集合（仅前端阅读态偏好，不动后端 store）。
  // 指纹 = `timestamp|kind|message`：timestamp 含 ms 单进程内不撞。重启后
  // 后端 store 自动清空，dismissed 自然失效，无需持久化。
  const [tgDismissed, setTgDismissed] = useState<Set<string>>(new Set());
  const [tone, setTone] = useState<ToneSnapshot | null>(null);
  const [reminders, setReminders] = useState<PendingReminder[]>([]);
  // Iter TR3: pending high-risk tool reviews. Surfaces a modal asking
  // approve / deny when non-empty. Backend default-denies after 60s,
  // so reviews evaporate from the queue if the user is away.
  const [pendingReviews, setPendingReviews] = useState<
    {
      review_id: string;
      tool_name: string;
      args_json: string;
      purpose: string;
      reasons: string[];
      safe_alternative: string | null;
      timestamp: string;
    }[]
  >([]);
  const [reviewError, setReviewError] = useState<string>("");
  // 工具风险概览：PanelDebug 底部展示每个内置工具的 nominal risk + 当前
  // 用户偏好 (auto / always_review / always_approve)。3-chip 行内 toggle
  // 直接 set_tool_review_mode 写盘 → 下次 chat 调 get_settings 即生效。
  // 整个表 default 折叠，避免长列表撑 panel；点 header 展开。
  const [toolRiskRows, setToolRiskRows] = useState<
    { name: string; level: string; note: string; mode: string }[]
  >([]);
  const [toolRiskExpanded, setToolRiskExpanded] = useState(false);
  const [toolRiskBusyName, setToolRiskBusyName] = useState<string | null>(null);
  const [toolRiskMsg, setToolRiskMsg] = useState("");
  const fetchToolRiskOverview = useCallback(async () => {
    try {
      const rows = await invoke<
        { name: string; level: string; note: string; mode: string }[]
      >("get_tool_risk_overview");
      setToolRiskRows(rows);
    } catch (e) {
      console.error("get_tool_risk_overview failed:", e);
    }
  }, []);
  useEffect(() => {
    void fetchToolRiskOverview();
  }, [fetchToolRiskOverview]);
  const handleSetToolReviewMode = async (name: string, mode: string) => {
    setToolRiskBusyName(name);
    try {
      await invoke("set_tool_review_mode", { name, mode });
      await fetchToolRiskOverview();
      setToolRiskMsg(`${name} → ${mode}`);
      window.setTimeout(() => setToolRiskMsg(""), 2000);
    } catch (e) {
      setToolRiskMsg(`改失败：${e}`);
      window.setTimeout(() => setToolRiskMsg(""), 4000);
    } finally {
      setToolRiskBusyName(null);
    }
  };
  // Iter R4: structured tool-call history (newest first) from the backend
  // ring buffer. PanelDebug renders a collapsible "工具调用历史" card so
  // prompt-tuning can see purpose / risk / review status at a glance.
  type ToolCallRecord = {
    timestamp: string;
    name: string;
    args_excerpt: string;
    purpose: string;
    risk_level: string;
    reasons: string[];
    safe_alternative: string | null;
    review_status: string;
    result_excerpt: string;
  };
  const [toolCallHistory, setToolCallHistory] = useState<ToolCallRecord[]>([]);
  const [showToolHistory, setShowToolHistory] = useState(false);
  // 专用工具调用占比（v11/v12 SQLite 重构副产物）：butler_task_edit /
  // todo_edit 是新加的专用接口，memory_edit 仍接受 butler_tasks/todo 作
  // fallback。本块统计最近 N 条工具调用里两者的比例，让 owner 看到 prompt
  // 引导效果。30s polling 跟随 stats 节奏。
  type DedicatedToolStats = {
    butler_task_edit_count: number;
    memory_edit_butler_count: number;
    todo_edit_count: number;
    memory_edit_todo_count: number;
    total_records: number;
  };
  // 🛠 dedicated_tool_stats：30s 轮询 + 点击立刻 refresh。逻辑全在 usePollingState
  // 里（与下方 task_stats strip 共享同一份 polling 三件套）。
  const {
    data: dedicatedToolStats,
    refresh: refreshDedicatedToolStats,
    refreshing: dedicatedToolStatsRefreshing,
  } = usePollingState<DedicatedToolStats>(
    () => invoke<DedicatedToolStats>("get_dedicated_tool_stats"),
    30_000,
  );
  /// app 版本 + SQLite schema version —— 拼到调试 snapshot 顶部"环境"段。
  /// 三元素全可独立 fail（旧 backend 缺 app_version / get_db_stats）；缺失时
  /// snapshot 该段相应字段省略，不挡其它字段。
  const [envInfo, setEnvInfo] = useState<{
    appVersion: string;
    schemaVersion: number;
  } | null>(null);
  useEffect(() => {
    let cancelled = false;
    (async () => {
      const [v, s] = await Promise.all([
        invoke<string>("app_version").catch(() => ""),
        invoke<{ schema_version: number }>("get_db_stats")
          .then((d) => d.schema_version)
          .catch(() => 0),
      ]);
      if (!cancelled) setEnvInfo({ appVersion: v, schemaVersion: s });
    })();
    return () => {
      cancelled = true;
    };
  }, []);
  // dedicatedToolStats 的 30s 自动轮询由 usePollingState 内部 effect 接管，
  // 这里不再保留旧的 effect。
  /// 任务状态汇总 —— 30s 轮询，与 🛠 dedicated tool stats strip 并排显在
  /// PanelDebug 顶部。后端 task_stats 单 SoT（桌面 /stats / pet 窗 pill 共用
  /// 同一函数）。命令未注册 / 失败 → null，strip 不渲染。
  type TaskStats = {
    pending: number;
    overdue: number;
    done_today: number;
    error: number;
    cancelled_today: number;
  };
  // task_stats：30s 轮询 + 点击立刻 refresh。usePollingState 接管 polling +
  // 手动 refresh + refreshing flag 三件套（与上方 🛠 strip 同 hook）。
  const {
    data: taskStats,
    refresh: refreshTaskStats,
    refreshing: taskStatsRefreshing,
  } = usePollingState<TaskStats>(
    () => invoke<TaskStats>("task_stats"),
    30_000,
  );
  // 工具调用历史按 tool name 折叠分组：让"哪个 tool 用得最多"一眼看到。
  // session 内 toggle，默认关（保留原 timeline 顺序视图）。
  const [toolHistoryGroupByName, setToolHistoryGroupByName] = useState(false);
  const [toolGroupExpanded, setToolGroupExpanded] = useState<Set<string>>(new Set());
  // R146: 决策日志 collapse；default true（展开）—— 决策日志是 debug 主信号，
  // 而非 tool/feedback 那种次要 buffer，所以默认展开方向相反。
  const [showDecisions, setShowDecisions] = useState(true);
  // Iter R6: feedback timeline (replied / ignored / dismissed / liked).
  // Surfaces R1's capture data so用户能看到宠物是否在"学习"反馈。R1c 加 dismissed
  // （5 秒内点掉，与被动忽略区分）；本轮加 liked（👍 显式正向，比 replied 更高
  // 质量的"喜欢"信号）。
  type FeedbackEntry = {
    timestamp: string;
    kind: "replied" | "ignored" | "dismissed" | "liked";
    excerpt: string;
  };
  const [feedbackHistory, setFeedbackHistory] = useState<FeedbackEntry[]>([]);
  const [showFeedbackHistory, setShowFeedbackHistory] = useState(false);
  // Iter R37: filter buttons for feedback timeline. "all" by default;
  // toggling to one kind isolates retrospection (e.g., "show only the
  // dismissals to see what got rejected").
  const [feedbackFilter, setFeedbackFilter] = useState<
    "all" | "replied" | "ignored" | "dismissed" | "liked"
  >("all");
  // Iter R38: same pattern applied to decision_log timeline. Four common
  // kinds for filter (all / Spoke / LlmSilent / Skip); rare kinds (Silent
  // pre-LLM / LlmError / Run wrapper / ToolReview*) appear under "all".
  // R83: 升级到 multi-select —— Set<kind>，empty Set = "全部"。让"看
  // Spoke + LlmSilent 同时屏蔽 Skip" 这种对比场景一次过滤搞定，无需
  // 来回切。点 chip 加入 / 再点取消；点 "全部" 清空 Set。
  const [decisionKinds, setDecisionKinds] = useState<Set<string>>(
    () => new Set(),
  );
  // 决策日志 reason / kind 搜索词。空 = 不过滤；非空 = 在 kind/raw reason/
  // 本地化 reason 三域里 case-insensitive 子串匹配，让 "cooldown" / "冷却" /
  // "Skip" 都能定位同一组条目。临时 debug 视角，不持久化到 localStorage。
  const [decisionReasonSearch, setDecisionReasonSearch] = useState("");
  // Iter R86: 时间窗快捷过滤。三档（10m / 30m / 1h）覆盖 90% debug 场景，
  // 单选互斥，再点同 chip 关闭回 "all"。与 kind / reason 三层 AND 叠加。
  // 不持久化（临时 debug 视角，与 kind / reason 同语义）。
  const [decisionTimeWindow, setDecisionTimeWindow] = useState<
    "all" | "10m" | "30m" | "1h"
  >("all");
  // 决策日志渲染顺序：默认 false = 最新在底（保留 ring-buffer 自然语义），
  // true = 最新在顶（与多数 dashboard 直觉对齐）。
  const [decisionsNewestFirst, setDecisionsNewestFirst] = useState(false);
  // "清空决策日志" 二次确认：第一次点击 armed → 3s 内再点才真清。防误触
  // 把 in-memory ring buffer 抹掉。3s 后自动 revert 到 idle。
  const [clearDecisionsArmed, setClearDecisionsArmed] = useState(false);
  // 过滤结果提到 useMemo：让 header 的 N/M 统计与渲染主体共享同一份
  // 计算，避免 IIFE 内重复实现造成 drift。reverse 仅渲染层关心，不影响
  // 统计 → 不放进 memo。
  const filteredDecisions = useMemo(() => {
    let f =
      decisionKinds.size === 0
        ? decisions
        : decisions.filter((d) => decisionKinds.has(d.kind));
    // R86: 时间窗。Date.now() 在 useMemo 内调用 → 仅在依赖变化时重算
    // （新决策入队 decisions 引用变化触发，时间窗自动滑动）；静默盯着时
    // 不剔除恰好越界条目，minor staleness 不影响 debug 用例。
    if (decisionTimeWindow !== "all") {
      const windowMs =
        decisionTimeWindow === "10m"
          ? 10 * 60_000
          : decisionTimeWindow === "30m"
            ? 30 * 60_000
            : 60 * 60_000;
      const cutoff = Date.now() - windowMs;
      f = f.filter((d) => {
        const ts = Date.parse(d.timestamp);
        return !isNaN(ts) && ts >= cutoff;
      });
    }
    const q = decisionReasonSearch.trim().toLowerCase();
    if (q === "") return f;
    return f.filter((d) => {
      const haystack = `${d.kind} ${d.reason} ${localizeReason(d.kind, d.reason)}`.toLowerCase();
      return haystack.includes(q);
    });
  }, [decisions, decisionKinds, decisionTimeWindow, decisionReasonSearch]);

  // R108: 决策日志"今日累计"计数。从 decisions 全集（不受 filter 影响 ——
  // 今日累计是绝对值）算 timestamp 落本地今日的条数。受 ring buffer cap 16
  // 限制：今日实际触发可能 > 16 但被淘汰；buffer 满时 UI 加 + 后缀暗示。
  const todayDecisionCount = useMemo(() => {
    const todayStart = new Date();
    todayStart.setHours(0, 0, 0, 0);
    const todayMs = todayStart.getTime();
    let count = 0;
    for (const d of decisions) {
      const ts = Date.parse(d.timestamp);
      if (!Number.isNaN(ts) && ts >= todayMs) count++;
    }
    return count;
  }, [decisions]);
  // R111: 工具调用历史"今日累计"计数。与 R108 同款逻辑，源数据换成
  // toolCallHistory；不受 risk filter 影响（今日 count 是绝对值）。
  const todayToolCallCount = useMemo(() => {
    const todayStart = new Date();
    todayStart.setHours(0, 0, 0, 0);
    const todayMs = todayStart.getTime();
    let count = 0;
    for (const c of toolCallHistory) {
      const ts = Date.parse(c.timestamp);
      if (!Number.isNaN(ts) && ts >= todayMs) count++;
    }
    return count;
  }, [toolCallHistory]);
  // R114: 反馈记录"今日累计"计数。与 R108/R111 同款逻辑，源数据换成
  // feedbackHistory；不受 kind filter 影响。
  const todayFeedbackCount = useMemo(() => {
    const todayStart = new Date();
    todayStart.setHours(0, 0, 0, 0);
    const todayMs = todayStart.getTime();
    let count = 0;
    for (const f of feedbackHistory) {
      const ts = Date.parse(f.timestamp);
      if (!Number.isNaN(ts) && ts >= todayMs) count++;
    }
    return count;
  }, [feedbackHistory]);

  // 日志 level 计数 / 过滤 / followTail 都已搬到 PanelDebugLogs。
  // Iter R39: third application of the filter pattern — tool_call history
  // risk_level filter. Triggers PanelFilterButtonRow extraction (R32 IDEA's
  // "wait until use-3+ before extraction" threshold).
  const [toolRiskFilter, setToolRiskFilter] = useState<
    "all" | "low" | "medium" | "high"
  >("all");
  const [triggeringProactive, setTriggeringProactive] = useState(false);
  // R125: "立即开口" 顶部按钮的二次确认 armed 态。第 1 击 armed + 3s 自动
  // revert；第 2 击真触发。决策日志行 "重跑" 仍直触不走门控（power-user
  // iterate prompt 工作流）。
  const [triggerArmed, setTriggerArmed] = useState(false);
  /// "✏️ 临时 prompt fire" modal：用户改 SOUL 想 fire 测一次但不写盘。
  /// open=true 时 modal 可见；draft 缓存 textarea 当前值（默认从 get_soul
  /// 预填，让用户在原 prompt 基础上改）。busy 期 disable 按钮防双触。
  const [tempPromptOpen, setTempPromptOpen] = useState(false);
  const [tempPromptDraft, setTempPromptDraft] = useState("");
  const [tempPromptBusy, setTempPromptBusy] = useState(false);
  const openTempPromptModal = useCallback(async () => {
    setTempPromptOpen(true);
    setTempPromptDraft("");
    try {
      const soul = await invoke<string>("get_soul");
      setTempPromptDraft(soul);
    } catch {
      // get_soul 失败 → 空 draft；用户从零写
    }
  }, []);
  /// "重置 in-process stash" 按钮的二次确认 armed 态。与 triggerArmed 同模
  /// 式 —— 首点变红 + 3s 内再点确认；超时自动 revert。LAST_PROACTIVE_*
  /// 等内存 stash wipe 不可逆（虽不动磁盘），二次确认防误触。
  const [resetStashArmed, setResetStashArmed] = useState(false);
  const [resetStashBusy, setResetStashBusy] = useState(false);
  // R128: 工具调用历史 args/result 块复制反馈。key = `${index}-args` /
  // `${index}-result`，1.5s 自清空让 ✓ 反馈短暂可见。多个按钮共用一个 state，
  // 同时只一个处于"已复制"态（用户连点两个时后者覆盖前者，符合直觉）。
  const [copiedToolKey, setCopiedToolKey] = useState<string | null>(null);
  const [showPromptHints, setShowPromptHints] = useState(false);
  const [proactiveStatus, setProactiveStatus] = useState<string>("");
  // ⚙️ mute 15min 快捷按钮：调 prompt / 测 SOUL 时不想被 proactive 打扰，
  // 绕开 PanelChat /sleep 路径。muteUntil 空 → 显示 mute；非空 → 显示剩
  // 余分钟 + 允许再点解除。30s polling 跟随后端 MUTE_UNTIL。
  const [muteBusy, setMuteBusy] = useState(false);
  // "上次 manual fire" audit info：进程内 only。trigger_proactive_turn /
  // trigger_proactive_turn_for_task 完成后后端 stash；这里挂载 / fire 后
  // poll 一次。title=null 表示全局 manual fire；title=string 表示从
  // PanelMemory 的 ▶️ 现在跑 触发。
  type ManualFireRecord = {
    timestamp: string;
    title: string | null;
    result: string;
  };
  const [lastManualFire, setLastManualFire] = useState<ManualFireRecord | null>(null);
  // 近 5 条 manual fire 历史 ring（最新在前）。"上次 manual fire" 行展
  // 开为 collapsible list；默认只显最新一条，点击 ▾ 展开看全部。
  const [manualFireHistory, setManualFireHistory] = useState<
    ManualFireRecord[]
  >([]);
  const [manualFireHistoryExpanded, setManualFireHistoryExpanded] =
    useState(false);
  const refreshLastManualFire = useCallback(async () => {
    try {
      const [latest, history] = await Promise.all([
        invoke<ManualFireRecord | null>("get_last_manual_fire"),
        invoke<ManualFireRecord[]>("get_manual_fire_history").catch(
          // 老 backend 没此命令时退到 []
          () => [] as ManualFireRecord[],
        ),
      ]);
      setLastManualFire(latest);
      setManualFireHistory(history);
    } catch {
      // 命令不可用 / 早期版本 backend → 静默忽略
    }
  }, []);
  useEffect(() => {
    void refreshLastManualFire();
  }, [refreshLastManualFire]);
  // Iter D7: MUTE_UNTIL polling for the ⚙️ mute 15min button label。30s 节奏
  // 与 stats strip 同源；剩余分钟在 render 时用本地时间差算（不依赖后端持
  // 续 fetch），所以一次 muteUntil 字符串足够。muteUntil 为 "" 时按钮显
  // "mute 15min"；非空显剩余分钟。
  //
  // 失败时静默走 hook 默认（保留上一份 muteUntil；不闪到空）。button 点击
  // set_mute_minutes 之后调 refresh 拿最新 until 值。
  const { data: muteUntil, refresh: refreshMute } = usePollingState(
    () => invoke<string>("get_mute_until"),
    30_000,
    "",
  );
  // Iter E4: ring buffer of recent turns, newest first. Panel modal navigates
  // with « / » buttons; index 0 = newest. Replaces E1/E2/E3's three separate
  // fetches with a single Vec<TurnRecord> source.
  // 调试器：tool_calls 是 LLM 在该 turn 内的全部工具调用记录（name+args+result，
  // 按调用顺序）。老 ring-buffer 项（升级前持久化的）缺该字段时按空数组处理。
  const [recentTurns, setRecentTurns] = useState<
    {
      timestamp: string;
      prompt: string;
      reply: string;
      tools_used: string[];
      tool_calls?: { name: string; arguments: string; result: string }[];
      outcome?: string;
    }[]
  >([]);
  // 工具调用折叠状态：per-turn 持久化（按 turn.timestamp 索引），让用户在
  // 翻 prev/next turn 时各自维持展开布局，不必重新点开。键用 timestamp 而
  // 非 turnIndex —— ring buffer 滚动时索引会位移，但 timestamp 是稳定标识。
  const [expandedToolCallByTs, setExpandedToolCallByTs] = useState<Map<string, Set<number>>>(
    () => new Map(),
  );
  const [turnIndex, setTurnIndex] = useState(0);
  /// recent turns ring buffer 的 outcome 过滤：调 prompt 时常想"只看刚才
  /// silent 的 turn"或"只看 spoke"，全局列表里翻翻太多杂音。三档：all /
  /// spoke / silent。filter 切换时 turnIndex 重置 0，避免 stale index 指
  /// 到空 / 越界。
  const [turnOutcomeFilter, setTurnOutcomeFilter] = useState<
    "all" | "spoke" | "silent"
  >("all");
  const [showLastPrompt, setShowLastPrompt] = useState(false);
  const [copyMsg, setCopyMsg] = useState<string>("");
  // 上次 prompt modal 内 PROMPT / REPLY 两段的折叠态。默认展开（保留首次
  // 打开"一眼看全文"的既有习惯）；用户可点 header 折起来给 turn-nav 腾空间。
  // 跨 turn / 关 modal 不重置 —— 折叠是阅读姿态，不该频繁丢失。
  const [promptCollapsed, setPromptCollapsed] = useState(false);
  const [replyCollapsed, setReplyCollapsed] = useState(false);
  // 行数：空字符串 → 0（贴近"啥也没有"的视觉直觉，而非 split 默认的 1）。
  const countLines = (text: string): number =>
    text.length === 0 ? 0 : text.split("\n").length;
  // outcome filter 应用到 ring buffer。`outcome === undefined` 视作 spoke
  // （非 silent 的老 ring 项升级前没记录字段时按 spoke 兜底 — 与 R25 既
  // 有 spoke / silent 二态对齐）。"all" 不过滤。
  const filteredTurns =
    turnOutcomeFilter === "all"
      ? recentTurns
      : recentTurns.filter((t) =>
          turnOutcomeFilter === "silent"
            ? t.outcome === "silent"
            : t.outcome === undefined || t.outcome === "spoke",
        );
  const currentTurn = filteredTurns[turnIndex] ?? null;
  const lastPrompt = currentTurn?.prompt ?? "";
  const lastReply = currentTurn?.reply ?? "";
  const lastToolCalls = currentTurn?.tool_calls ?? [];
  const currentTurnTs = currentTurn?.timestamp ?? "";
  const expandedToolCallSet = expandedToolCallByTs.get(currentTurnTs) ?? EMPTY_INDEX_SET;
  const lastTurnMeta = {
    timestamp: currentTurn?.timestamp ?? "",
    tools_used: currentTurn?.tools_used ?? [],
  };
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null);

  // Iter QG6: collapsed 15 independent invokes-per-second into one bundled
  // get_debug_snapshot call. Keeps the same shape on the frontend (one
  // setState per field) but cuts IPC overhead by ~14× per refresh.
  const fetchLogs = async () => {
    try {
      const snap = await invoke<{
        logs: string[];
        cache_stats: CacheStats;
        decisions: ProactiveDecision[];
        mood_tag_stats: MoodTagStats;
        recent_speeches: string[];
        tone: ToneSnapshot;
        reminders: PendingReminder[];
        lifetime_speech_count: number;
        today_speech_count: number;
        week_speech_count: number;
        llm_outcome_stats: LlmOutcomeStats;
        env_tool_stats: EnvToolStats;
        prompt_tilt_stats: PromptTiltStats;
        companionship_days: number;
        pending_tool_reviews: {
          review_id: string;
          tool_name: string;
          args_json: string;
          purpose: string;
          reasons: string[];
          safe_alternative: string | null;
          timestamp: string;
        }[];
        recent_tool_calls: ToolCallRecord[];
        recent_feedback: FeedbackEntry[];
      }>("get_debug_snapshot");
      // logs 已搬到「日志」tab (PanelDebugLogs) 自轮询；本 tab 不再消费。
      setCacheStats(snap.cache_stats);
      setDecisions(snap.decisions);
      setMoodTagStats(snap.mood_tag_stats);
      setRecentSpeeches(snap.recent_speeches);
      setTone(snap.tone);
      setReminders(snap.reminders);
      setLifetimeSpeechCount(snap.lifetime_speech_count);
      setTodaySpeechCount(snap.today_speech_count);
      setWeekSpeechCount(snap.week_speech_count);
      setLlmOutcomeStats(snap.llm_outcome_stats);
      setEnvToolStats(snap.env_tool_stats);
      setPromptTiltStats(snap.prompt_tilt_stats);
      setCompanionshipDays(snap.companionship_days);
      setPendingReviews(snap.pending_tool_reviews ?? []);
      setToolCallHistory(snap.recent_tool_calls ?? []);
      setFeedbackHistory(snap.recent_feedback ?? []);
    } catch (e) {
      console.error("Failed to fetch logs:", e);
    }
    // 单独 fetch tg 启动告警 — 不挤进 get_debug_snapshot 包，避免 backend
    // bundle 签名 ripple；列表通常为空（命中 fast path），开销可忽略。
    try {
      const ws = await invoke<
        { timestamp: string; kind: string; message: string }[]
      >("get_tg_startup_warnings");
      setTgStartupWarnings(ws);
    } catch (e) {
      console.error("get_tg_startup_warnings failed:", e);
    }
  };

  const handleToolReviewDecision = async (
    reviewId: string,
    decision: "approve" | "deny",
  ) => {
    setReviewError("");
    try {
      await invoke("submit_tool_review", { reviewId, decision });
      setPendingReviews((prev) => prev.filter((r) => r.review_id !== reviewId));
    } catch (e) {
      // Race: backend may have already timed out. Refresh shortly to clear.
      setReviewError(String(e));
      fetchLogs();
    }
  };

  // Esc = 拒绝最上面那条待审核：和 60s 默认拒绝同语义，但用户能立刻按
  // 一下抢在超时前否决，省去把鼠标移到 modal 按按钮的来回。仅在
  // pendingReviews 非空时挂监听，避免无业务时占全局快捷键位。处理后
  // pendingReviews 数组缩短，下一次 Esc 自动作用到新的"最上面那条"，
  // 形成「连按 Esc 全部拒绝」的快捷路径。
  useEffect(() => {
    if (pendingReviews.length === 0) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      void handleToolReviewDecision(pendingReviews[0].review_id, "deny");
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
    // handleToolReviewDecision 依赖闭包外 invoke / setState（稳定）；列表
    // 引用变化才需要重订阅，这是预期行为。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pendingReviews]);

  useEffect(() => {
    fetchLogs();
    intervalRef.current = setInterval(fetchLogs, 1000);
    return () => {
      if (intervalRef.current) clearInterval(intervalRef.current);
    };
  }, []);

  const handleClear = async () => {
    await invoke("clear_logs");
  };

  // R128: 工具调用 args / result 块复制到剪贴板。key 唯一标识每个按钮 ——
  // 同时只一个处于"已复制"态（连点多个时后者覆盖前者）。失败时 console
  // 而非 toast；剪贴板失败极少见，不值得占视觉空间报错。
  const copyExcerpt = async (key: string, text: string) => {
    try {
      await navigator.clipboard.writeText(text);
      setCopiedToolKey(key);
      window.setTimeout(() => setCopiedToolKey(null), 1500);
    } catch (e) {
      console.error("clipboard write failed:", e);
    }
  };

  const handleResetCacheStats = async () => {
    await invoke("reset_cache_stats");
    setCacheStats({ turns: 0, total_hits: 0, total_calls: 0 });
  };

  const handleResetMoodTagStats = async () => {
    await invoke("reset_mood_tag_stats");
    setMoodTagStats({ with_tag: 0, without_tag: 0, no_mood: 0 });
  };

  const handleResetLlmOutcomeStats = async () => {
    await invoke("reset_llm_outcome_stats");
    setLlmOutcomeStats({ spoke: 0, silent: 0, error: 0 });
  };

  const handleResetEnvToolStats = async () => {
    await invoke("reset_env_tool_stats");
    setEnvToolStats({
      spoke_total: 0,
      spoke_with_any: 0,
      active_window: 0,
      weather: 0,
      upcoming_events: 0,
      memory_search: 0,
    });
  };

  const handleResetPromptTiltStats = async () => {
    await invoke("reset_prompt_tilt_stats");
    setPromptTiltStats({
      restraint_dominant: 0,
      engagement_dominant: 0,
      balanced: 0,
      neutral: 0,
    });
  };

  /// 把当前 PanelDebug 各 chip / stats / timeline / 风险表 拼成 markdown
  /// 写到剪贴板，方便用户贴 issue / 排查时一次性导出诊断快照。
  const buildDebugMarkdownSnapshot = useCallback((): string => {
    const ts = new Date().toLocaleString();
    const lines: string[] = [
      `# Pet 调试快照（${ts}）`,
      "",
    ];
    // 环境段：app + schema + 平台。给 triage 提供"是哪个版本 / 哪个 schema"
    // 关键上下文。envInfo === null（还在 fetch 或 backend 缺命令）时整段跳。
    if (envInfo) {
      lines.push("## 环境");
      if (envInfo.appVersion) lines.push(`- app: pet v${envInfo.appVersion}`);
      if (envInfo.schemaVersion > 0) lines.push(`- schema: v${envInfo.schemaVersion}`);
      // navigator.platform 在 Tauri webview 仍返回（Mac / Win / Linux），粗粒度
      // OS 分类够用；空 / undefined 时跳过该行。
      const plat = typeof navigator !== "undefined" ? navigator.platform : "";
      if (plat) lines.push(`- 平台: ${plat}`);
      lines.push(`- 时间: ${ts}`);
      lines.push("");
    }
    lines.push(
      `- 陪伴 ${companionshipDays} 天`,
      `- 主动开口 · 今日 ${todaySpeechCount} · 本周 ${weekSpeechCount} · 累计 ${lifetimeSpeechCount}`,
      "",
    );
    // 任务状态：与 PanelDebug 顶部 "📊 任务状态" strip 同 SoT（task_stats 命令）。
    // null 时整段跳；非 null 含全 0 也输出，让"任务清零"本身在 snapshot 可见。
    if (taskStats) {
      lines.push(
        `## 任务状态`,
        `- 待办: ${taskStats.pending}`,
        `- 逾期: ${taskStats.overdue}`,
        `- 今日完成: ${taskStats.done_today}`,
        `- 出错: ${taskStats.error}`,
        `- 今日取消: ${taskStats.cancelled_today}`,
        "",
      );
    }
    lines.push(
      `## 工具缓存`,
      `- turns: ${cacheStats.turns}`,
      `- hits / calls: ${cacheStats.total_hits} / ${cacheStats.total_calls}`,
      "",
      `## 心情 motion 命中`,
      `- with_tag: ${moodTagStats.with_tag}`,
      `- without_tag: ${moodTagStats.without_tag}`,
      `- no_mood: ${moodTagStats.no_mood}`,
      "",
      `## proactive 出口分布`,
      `- spoke: ${llmOutcomeStats.spoke}`,
      `- silent: ${llmOutcomeStats.silent}`,
      `- error: ${llmOutcomeStats.error}`,
      "",
      `## env 工具被引用`,
      `- spoke_total: ${envToolStats.spoke_total} · spoke_with_any: ${envToolStats.spoke_with_any}`,
      `- active_window: ${envToolStats.active_window}`,
      `- weather: ${envToolStats.weather}`,
      `- upcoming_events: ${envToolStats.upcoming_events}`,
      `- memory_search: ${envToolStats.memory_search}`,
      "",
      `## prompt tilt 分布`,
      `- restraint_dominant: ${promptTiltStats.restraint_dominant}`,
      `- engagement_dominant: ${promptTiltStats.engagement_dominant}`,
      `- balanced: ${promptTiltStats.balanced}`,
      `- neutral: ${promptTiltStats.neutral}`,
    );
    if (tone) {
      lines.push("", `## tone snapshot`, "```json", JSON.stringify(tone, null, 2), "```");
    }
    if (pendingReviews.length > 0) {
      lines.push("", `## 待审核工具调用（${pendingReviews.length}）`);
      for (const r of pendingReviews) {
        lines.push(
          `- ${r.timestamp} · **${r.tool_name}** · 用途: ${r.purpose || "-"} · 原因: ${r.reasons.join(" / ") || "-"}`,
        );
      }
    }
    if (reminders.length > 0) {
      lines.push("", `## 待提醒 (${reminders.length})`);
      for (const r of reminders.slice(0, 10)) {
        lines.push(`- ${r.title}`);
      }
      if (reminders.length > 10) lines.push(`- ... 还有 ${reminders.length - 10} 条`);
    }
    const overrideRows = toolRiskRows.filter((r) => r.mode !== "auto");
    if (overrideRows.length > 0) {
      lines.push("", `## 工具风险偏好覆盖（${overrideRows.length}）`);
      for (const r of overrideRows) {
        lines.push(`- ${r.name} (${r.level}): \`${r.mode}\` — ${r.note}`);
      }
    }
    if (recentSpeeches.length > 0) {
      lines.push("", `## 宠物最近说（${recentSpeeches.length}）`);
      for (const s of recentSpeeches.slice(-5)) {
        lines.push(`- ${s}`);
      }
    }
    return lines.join("\n");
  }, [
    envInfo,
    taskStats,
    companionshipDays,
    todaySpeechCount,
    weekSpeechCount,
    lifetimeSpeechCount,
    cacheStats,
    moodTagStats,
    llmOutcomeStats,
    envToolStats,
    promptTiltStats,
    tone,
    pendingReviews,
    reminders,
    toolRiskRows,
    recentSpeeches,
  ]);
  const [debugExportMsg, setDebugExportMsg] = useState("");
  const handleExportDebugMd = async () => {
    try {
      await navigator.clipboard.writeText(buildDebugMarkdownSnapshot());
      setDebugExportMsg("已复制调试快照 markdown 到剪贴板");
    } catch (e) {
      setDebugExportMsg(`复制失败：${e}`);
    }
    window.setTimeout(() => setDebugExportMsg(""), 3500);
  };

  /// 快照对比：抓两个时间点的 markdown 快照，简单 set-diff 显增 / 删 / 同。
  /// 不用 jsdiff —— 内部 snapshot 格式是稳定的 key:value 行，set-diff 已能
  /// 答"哪些值变了"的核心问题，省一个 npm 依赖。
  const [snapshotA, setSnapshotA] = useState<string | null>(null);
  const [snapshotATs, setSnapshotATs] = useState<string>("");
  const [compareDiff, setCompareDiff] = useState<string | null>(null);
  const handleCaptureSnapshotA = () => {
    setSnapshotA(buildDebugMarkdownSnapshot());
    setSnapshotATs(new Date().toLocaleString());
    setCompareDiff(null);
    setDebugExportMsg("已抓 A 快照（点 🔀 对比 现在的状态）");
    window.setTimeout(() => setDebugExportMsg(""), 3500);
  };
  const handleCompareSnapshot = () => {
    if (snapshotA === null) return;
    const b = buildDebugMarkdownSnapshot();
    const bTs = new Date().toLocaleString();
    const aLines = snapshotA.split("\n");
    const bLines = b.split("\n");
    const aSet = new Set(aLines);
    const bSet = new Set(bLines);
    const removed: string[] = [];
    const added: string[] = [];
    let common = 0;
    for (const line of aLines) {
      if (!bSet.has(line)) removed.push(line);
      else common += 1;
    }
    for (const line of bLines) {
      if (!aSet.has(line)) added.push(line);
    }
    const out: string[] = [
      `# 调试快照对比`,
      `- A: ${snapshotATs}`,
      `- B: ${bTs}`,
      `- 共有行: ${common} · 仅 A: ${removed.length} · 仅 B: ${added.length}`,
      "",
    ];
    if (removed.length > 0) {
      out.push("## 仅 A 出现（被移除 / 已变化）");
      out.push("```diff");
      for (const l of removed) out.push(`- ${l}`);
      out.push("```");
      out.push("");
    }
    if (added.length > 0) {
      out.push("## 仅 B 出现（新增 / 已变化）");
      out.push("```diff");
      for (const l of added) out.push(`+ ${l}`);
      out.push("```");
      out.push("");
    }
    if (removed.length === 0 && added.length === 0) {
      out.push("> 两次快照完全一致 —— 这段时间没有可观测变化");
    }
    setCompareDiff(out.join("\n"));
  };
  const handleCopyCompareDiff = async () => {
    if (!compareDiff) return;
    try {
      await navigator.clipboard.writeText(compareDiff);
      setDebugExportMsg("已复制 diff markdown 到剪贴板");
    } catch (e) {
      setDebugExportMsg(`复制失败：${e}`);
    }
    window.setTimeout(() => setDebugExportMsg(""), 3500);
  };
  const handleClearSnapshotCompare = () => {
    setSnapshotA(null);
    setSnapshotATs("");
    setCompareDiff(null);
  };

  const handleTriggerProactive = async () => {
    setTriggeringProactive(true);
    setProactiveStatus("");
    try {
      const status = await invoke<string>("trigger_proactive_turn");
      setProactiveStatus(status);
    } catch (e) {
      console.error("trigger_proactive_turn failed:", e);
      setProactiveStatus(`触发失败: ${e}`);
    } finally {
      // 拉一次 last manual fire 让审计行同步更新（用户自己点的 fire 才
      // 跟得上自己的动作；其它窗口 / 路径触发的也会在下次挂载 / refresh
      // 时同步）。
      void refreshLastManualFire();
      setTriggeringProactive(false);
      // Auto-clear after a few seconds so the toolbar doesn't stick on a stale message.
      setTimeout(() => setProactiveStatus(""), 8000);
    }
  };

  // 计算 mute 剩余分钟（向上取整）。muteUntil 空 / 解析失败 / 已过期 → 0。
  // 后端 get_mute_until 已对过期返回空串，这里再用本地 now 兜底渲染层不会
  // 显示"-1m"等异常值。
  const muteRemainingMins = useMemo(() => {
    if (!muteUntil) return 0;
    const t = Date.parse(muteUntil);
    if (!Number.isFinite(t)) return 0;
    const diff = t - Date.now();
    if (diff <= 0) return 0;
    return Math.ceil(diff / 60_000);
  }, [muteUntil]);
  const handleMuteToggle = async () => {
    if (muteBusy) return;
    setMuteBusy(true);
    const isMuted = muteRemainingMins > 0;
    try {
      const until = await invoke<string>("set_mute_minutes", {
        minutes: isMuted ? 0 : 15,
      });
      // set_mute_minutes 返回最新 until；调 hook 的 refresh 重 fetch 让
      // polling 数据源单一（hook 状态 = 真值，避免双 set 路径漂移）。多 1 次
      // IPC 在 mute toggle 这种罕见动作上代价可忽略。
      void refreshMute();
      setProactiveStatus(
        isMuted
          ? "✓ mute 已解除"
          : until
            ? `✓ 已 mute 15 分钟（至 ${until.replace("T", " ").slice(11, 16)}）`
            : "✓ 已 mute 15 分钟",
      );
    } catch (e) {
      setProactiveStatus(`mute 失败：${e}`);
    } finally {
      setMuteBusy(false);
      window.setTimeout(() => setProactiveStatus(""), 4000);
    }
  };
  const handleOpenDevTools = async () => {
    // 由 Rust 端 open_devtools 命令直接调 webview.open_devtools() —— 旧
    // 前端 fallback 链（plugin:webview|internal_toggle_devtools / 或
    // win.openDevtools 走 webview）在 Tauri 2 里都不稳定，命令式后端调
    // 用是当前可靠路径。Release 构建无 devtools 特性时返回 Err，banner 显信息。
    try {
      await invoke("open_devtools");
    } catch (e) {
      console.error("open_devtools failed:", e);
      alert(`无法打开 DevTools：${e}`);
    }
  };

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%" }}>
      {/* Iter TR3: high-risk tool-call review modal. Top-of-stack: blocks the
          panel until user picks approve/deny so accidental click-through is hard.
          Backend default-denies after 60s. */}
      {pendingReviews.length > 0 && (
        <div
          style={{
            position: "fixed",
            inset: 0,
            background: "rgba(0,0,0,0.55)",
            zIndex: 2000,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            padding: "32px",
          }}
        >
          <div
            style={{
              background: "var(--pet-color-card)",
              borderRadius: "10px",
              maxWidth: "640px",
              width: "100%",
              maxHeight: "80vh",
              overflowY: "auto",
              padding: "20px 22px",
              boxShadow: "0 10px 40px rgba(0,0,0,0.25)",
            }}
          >
            <div style={{ fontSize: "13px", color: "var(--pet-tint-red-fg)", fontWeight: 700, marginBottom: "10px" }}>
              ⚠ 高风险工具调用待审核（{pendingReviews.length}）
            </div>
            {pendingReviews.map((r) => (
              <div
                key={r.review_id}
                style={{
                  border: "1px solid color-mix(in srgb, var(--pet-tint-red-fg) 30%, transparent)",
                  borderRadius: "8px",
                  padding: "12px 14px",
                  marginBottom: "10px",
                  background: "var(--pet-tint-red-bg)",
                }}
              >
                <div style={{ fontSize: "12px", color: "var(--pet-color-fg)", marginBottom: "6px" }}>
                  <span style={{ fontFamily: "monospace", color: "var(--pet-color-fg)" }}>{r.review_id}</span>
                  {" · "}
                  <span style={{ fontWeight: 600 }}>{r.tool_name}</span>
                  {" · "}
                  <span>{r.timestamp}</span>
                </div>
                <div style={{ fontSize: "12px", color: "var(--pet-color-fg)", marginBottom: "6px" }}>
                  <strong>用途：</strong>{r.purpose || "(未提供)"}
                </div>
                <div style={{ fontSize: "11px", color: "var(--pet-tint-red-fg)", marginBottom: "6px" }}>
                  <strong>风险：</strong>{r.reasons.join(" / ") || "-"}
                </div>
                {r.safe_alternative && (
                  <div style={{ fontSize: "11px", color: "#1e3a8a", marginBottom: "6px" }}>
                    <strong>建议替代：</strong>{r.safe_alternative}
                  </div>
                )}
                <details style={{ fontSize: "11px", color: "var(--pet-color-fg)", marginBottom: "8px" }}>
                  <summary style={{ cursor: "pointer" }}>参数（{r.args_json.length} chars）</summary>
                  <pre
                    style={{
                      whiteSpace: "pre-wrap",
                      wordBreak: "break-all",
                      background: "#f8fafc",
                      padding: "6px 8px",
                      borderRadius: "4px",
                      marginTop: "4px",
                      fontFamily: "monospace",
                      fontSize: "10.5px",
                    }}
                  >
                    {r.args_json}
                  </pre>
                </details>
                <div style={{ display: "flex", gap: "8px" }}>
                  <button
                    onClick={() => handleToolReviewDecision(r.review_id, "approve")}
                    style={{
                      flex: 1,
                      padding: "6px 10px",
                      background: "var(--pet-tint-green-fg)",
                      color: "#fff",
                      border: "none",
                      borderRadius: "5px",
                      cursor: "pointer",
                      fontSize: "12px",
                      fontWeight: 600,
                    }}
                  >
                    允许
                  </button>
                  <button
                    onClick={() => handleToolReviewDecision(r.review_id, "deny")}
                    style={{
                      flex: 1,
                      padding: "6px 10px",
                      background: "var(--pet-tint-red-fg)",
                      color: "#fff",
                      border: "none",
                      borderRadius: "5px",
                      cursor: "pointer",
                      fontSize: "12px",
                      fontWeight: 600,
                    }}
                  >
                    拒绝
                  </button>
                </div>
              </div>
            ))}
            {reviewError && (
              <div style={{ fontSize: "11px", color: "var(--pet-tint-red-fg)", marginTop: "6px" }}>
                {reviewError}
              </div>
            )}
            <div style={{ fontSize: "10px", color: "var(--pet-color-muted)", marginTop: "4px" }}>
              超过 60 秒未响应将按默认安全策略拒绝；按 Esc 立刻拒绝最上面这条。
            </div>
          </div>
        </div>
      )}

      {/* Iter E1: modal showing the last-built proactive prompt verbatim. Triggered
          by the "看上次 prompt" toolbar button; click backdrop to close. */}
      {/* "✏️ 临时 prompt fire" modal：用户编辑 SOUL 后 fire 一次（不写盘）。
          backdrop click / Esc 关；busy 期间整 modal disable 防双触。 */}
      {tempPromptOpen && (
        <div
          onClick={() => !tempPromptBusy && setTempPromptOpen(false)}
          onKeyDown={(e) => {
            if (e.key === "Escape" && !tempPromptBusy) setTempPromptOpen(false);
          }}
          style={{
            position: "fixed",
            inset: 0,
            background: "rgba(0,0,0,0.4)",
            zIndex: 1000,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            padding: "40px",
          }}
        >
          <div
            onClick={(e) => e.stopPropagation()}
            style={{
              background: "var(--pet-color-card)",
              borderRadius: "8px",
              maxWidth: "640px",
              width: "100%",
              maxHeight: "80vh",
              display: "flex",
              flexDirection: "column",
              boxShadow: "0 20px 60px rgba(0,0,0,0.3)",
            }}
          >
            <div
              style={{
                padding: "12px 16px",
                borderBottom: "1px solid var(--pet-color-border)",
                display: "flex",
                alignItems: "center",
                gap: 8,
              }}
            >
              <span style={{ fontSize: 14, fontWeight: 600, color: "var(--pet-color-fg)" }}>
                ✏️ 临时 prompt fire（仅本轮生效，不写盘）
              </span>
              <span style={{ flex: 1 }} />
              <button
                onClick={() => setTempPromptOpen(false)}
                disabled={tempPromptBusy}
                style={{
                  border: "none",
                  background: "transparent",
                  cursor: tempPromptBusy ? "default" : "pointer",
                  color: "var(--pet-color-muted)",
                  fontSize: 16,
                }}
              >
                ✕
              </button>
            </div>
            <textarea
              value={tempPromptDraft}
              onChange={(e) => setTempPromptDraft(e.target.value)}
              disabled={tempPromptBusy}
              style={{
                flex: 1,
                padding: "10px 14px",
                fontSize: 12,
                lineHeight: 1.6,
                fontFamily: "'SF Mono', 'Menlo', monospace",
                border: "none",
                outline: "none",
                resize: "none",
                background: "var(--pet-color-bg)",
                color: "var(--pet-color-fg)",
                minHeight: 240,
              }}
              placeholder="编辑 SOUL prompt（已预填当前 SOUL.md 内容）..."
            />
            <div
              style={{
                padding: "8px 16px",
                borderTop: "1px solid var(--pet-color-border)",
                display: "flex",
                gap: 8,
                alignItems: "center",
                fontSize: 11,
                color: "var(--pet-color-muted)",
              }}
            >
              <span>{tempPromptDraft.length} 字</span>
              <span style={{ flex: 1 }} />
              {/* 📥 加载上次 prompt：把 LAST_PROACTIVE_PROMPT 全文塞进 textarea，
                  让 prompt 调优有起点（不必从 SOUL 默认开始改）。失败时
                  silently 留原 draft。 */}
              <button
                onClick={async () => {
                  try {
                    const last = await invoke<string>("get_last_proactive_prompt");
                    if (last && last.trim().length > 0) {
                      setTempPromptDraft(last);
                    } else {
                      setProactiveStatus("上次 prompt 为空（还没 fire 过）");
                      window.setTimeout(() => setProactiveStatus(""), 4000);
                    }
                  } catch {
                    // ignore
                  }
                }}
                disabled={tempPromptBusy}
                title="把 LAST_PROACTIVE_PROMPT（上一次 fire 时构造的完整 prompt）覆盖到 textarea，作为本轮 prompt 调优的起点"
                style={{
                  padding: "6px 12px",
                  fontSize: 12,
                  border: "1px solid var(--pet-color-border)",
                  borderRadius: 6,
                  background: "var(--pet-color-card)",
                  color: "var(--pet-color-muted)",
                  cursor: tempPromptBusy ? "default" : "pointer",
                }}
              >
                📥 上次 prompt
              </button>
              <button
                onClick={() => setTempPromptOpen(false)}
                disabled={tempPromptBusy}
                style={{
                  padding: "6px 12px",
                  fontSize: 12,
                  border: "1px solid var(--pet-color-border)",
                  borderRadius: 6,
                  background: "var(--pet-color-card)",
                  color: "var(--pet-color-fg)",
                  cursor: tempPromptBusy ? "default" : "pointer",
                }}
              >
                取消
              </button>
              <button
                onClick={async () => {
                  if (!tempPromptDraft.trim()) return;
                  setTempPromptBusy(true);
                  setProactiveStatus("");
                  try {
                    const status = await invoke<string>(
                      "trigger_proactive_turn_with_prompt",
                      { soulOverride: tempPromptDraft },
                    );
                    setProactiveStatus(status);
                    setTempPromptOpen(false);
                  } catch (e) {
                    setProactiveStatus(`触发失败: ${e}`);
                  } finally {
                    setTempPromptBusy(false);
                    void refreshLastManualFire();
                    window.setTimeout(() => setProactiveStatus(""), 8000);
                  }
                }}
                disabled={tempPromptBusy || !tempPromptDraft.trim()}
                style={{
                  padding: "6px 12px",
                  fontSize: 12,
                  border: "none",
                  borderRadius: 6,
                  background: tempPromptBusy ? "var(--pet-color-muted)" : "#10b981",
                  color: "#fff",
                  fontWeight: 600,
                  cursor:
                    tempPromptBusy || !tempPromptDraft.trim()
                      ? "default"
                      : "pointer",
                  opacity: !tempPromptDraft.trim() ? 0.5 : 1,
                }}
              >
                {tempPromptBusy ? "开口中…" : "🚀 fire 一次"}
              </button>
            </div>
          </div>
        </div>
      )}
      {showLastPrompt && (
        <div
          onClick={() => setShowLastPrompt(false)}
          style={{
            position: "fixed",
            inset: 0,
            background: "rgba(0,0,0,0.4)",
            zIndex: 1000,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            padding: "40px",
          }}
        >
          <div
            onClick={(e) => e.stopPropagation()}
            style={{
              background: "var(--pet-color-card)",
              borderRadius: "8px",
              maxWidth: "780px",
              width: "100%",
              maxHeight: "80vh",
              display: "flex",
              flexDirection: "column",
              boxShadow: "0 20px 60px rgba(0,0,0,0.3)",
            }}
          >
            <div
              style={{
                padding: "12px 16px",
                borderBottom: "1px solid var(--pet-color-border)",
                display: "flex",
                alignItems: "center",
                gap: "12px",
              }}
            >
              <span style={{ fontSize: "14px", fontWeight: 600, color: "var(--pet-color-fg)" }}>
                proactive 的 prompt + reply
              </span>
              {/* outcome filter chips：调 prompt 时聚焦 silent / spoke 子集。
                  filter 切换时 turnIndex 归零防越界。仅 recentTurns.length > 0
                  时浮（无数据时 noop）。 */}
              {recentTurns.length > 0 && (
                <span style={{ display: "inline-flex", gap: 3 }}>
                  {(["all", "spoke", "silent"] as const).map((kind) => {
                    const active = turnOutcomeFilter === kind;
                    const label =
                      kind === "all"
                        ? "全部"
                        : kind === "spoke"
                          ? "开口"
                          : "沉默";
                    const cnt =
                      kind === "all"
                        ? recentTurns.length
                        : kind === "silent"
                          ? recentTurns.filter((t) => t.outcome === "silent").length
                          : recentTurns.filter(
                              (t) =>
                                t.outcome === undefined || t.outcome === "spoke",
                            ).length;
                    return (
                      <button
                        key={kind}
                        onClick={() => {
                          setTurnOutcomeFilter(kind);
                          setTurnIndex(0);
                        }}
                        style={{
                          fontSize: 10,
                          padding: "1px 6px",
                          borderRadius: 8,
                          border: `1px solid ${active ? "var(--pet-color-accent)" : "var(--pet-color-border)"}`,
                          background: active ? "var(--pet-color-bg)" : "var(--pet-color-card)",
                          color: active ? "var(--pet-color-accent)" : "var(--pet-color-muted)",
                          cursor: "pointer",
                          fontWeight: active ? 600 : 400,
                        }}
                        title={
                          active
                            ? `当前过滤：${label}（${cnt} 条）`
                            : `切到「${label}」过滤（${cnt} 条）`
                        }
                      >
                        {active ? "✓ " : ""}
                        {label} ({cnt})
                      </button>
                    );
                  })}
                </span>
              )}
              {/* Iter E4: prev/next navigator across the ring buffer */}
              {filteredTurns.length > 0 ? (
                <span
                  style={{ display: "inline-flex", alignItems: "center", gap: "4px" }}
                  title="« 上一条（更早）/ » 下一条（更新）。Iter E4 ring buffer 保留最近 5 次"
                >
                  <button
                    onClick={() => {
                      // 切 turn 不再清空展开状态 —— 折叠记忆 per-turn 持久化
                      setTurnIndex((i) => Math.min(i + 1, filteredTurns.length - 1));
                    }}
                    disabled={turnIndex >= filteredTurns.length - 1}
                    style={{
                      fontSize: "11px",
                      padding: "1px 6px",
                      borderRadius: "4px",
                      border: "1px solid var(--pet-color-border)",
                      background: turnIndex >= filteredTurns.length - 1 ? "#f1f5f9" : "var(--pet-color-card)",
                      color: turnIndex >= filteredTurns.length - 1 ? "#cbd5e1" : "var(--pet-color-fg)",
                      cursor: turnIndex >= filteredTurns.length - 1 ? "default" : "pointer",
                    }}
                  >
                    «
                  </button>
                  <span
                    style={{
                      fontSize: "11px",
                      color: "var(--pet-color-fg)",
                      fontFamily: "'SF Mono', monospace",
                      minWidth: "32px",
                      textAlign: "center",
                    }}
                  >
                    {turnIndex + 1}/{filteredTurns.length}
                  </span>
                  <button
                    onClick={() => {
                      // 切 turn 不再清空展开状态 —— 折叠记忆 per-turn 持久化
                      setTurnIndex((i) => Math.max(i - 1, 0));
                    }}
                    disabled={turnIndex === 0}
                    style={{
                      fontSize: "11px",
                      padding: "1px 6px",
                      borderRadius: "4px",
                      border: "1px solid var(--pet-color-border)",
                      background: turnIndex === 0 ? "#f1f5f9" : "var(--pet-color-card)",
                      color: turnIndex === 0 ? "#cbd5e1" : "var(--pet-color-fg)",
                      cursor: turnIndex === 0 ? "default" : "pointer",
                    }}
                  >
                    »
                  </button>
                </span>
              ) : (
                <span style={{ fontSize: "11px", color: "var(--pet-color-muted)" }}>（还没触发过）</span>
              )}
              {(() => {
                // prompt char count token-pressure 提醒：> 8000 char 时标红 +
                // hover tooltip 解释如何收紧。中文 ~3 char/token，8000 char
                // ≈ 2700 tokens，~half of 16K context 已紧。
                const promptOver = lastPrompt.length > PROMPT_PRESSURE_CHARS;
                if (!lastPrompt) {
                  return <span style={{ fontSize: "11px", color: "var(--pet-color-muted)" }}></span>;
                }
                return (
                  <span
                    style={{
                      fontSize: "11px",
                      color: promptOver ? "var(--pet-tint-red-fg)" : "var(--pet-color-muted)",
                      fontWeight: promptOver ? 600 : 400,
                    }}
                    title={
                      promptOver
                        ? `prompt 超过 ${PROMPT_PRESSURE_CHARS} char（约 ${Math.round(lastPrompt.length / 3)} tokens），离 context 上限不远。考虑收紧 system soul / 减少 tools / 调小 max_context_messages。`
                        : undefined
                    }
                  >
                    prompt {lastPrompt.length} / reply {lastReply.length} chars
                  </span>
                );
              })()}
              {lastTurnMeta.timestamp && (
                <span
                  style={{
                    fontSize: "11px",
                    color: "var(--pet-color-fg)",
                    fontFamily: "'SF Mono', 'Menlo', monospace",
                  }}
                  title="prompt 构造时刻（Iter E3）"
                >
                  ⏱ {lastTurnMeta.timestamp}
                </span>
              )}
              {lastTurnMeta.tools_used.length > 0 && (
                <span
                  style={{
                    fontSize: "11px",
                    color: "#0891b2",
                    fontWeight: 600,
                  }}
                  title="LLM 这一轮调用过的去重工具列表（Iter E3）"
                >
                  🔧 {lastTurnMeta.tools_used.join(" · ")}
                </span>
              )}
              {currentTurn?.outcome && (
                <span
                  style={{
                    fontSize: "10px",
                    padding: "1px 8px",
                    borderRadius: "10px",
                    background: currentTurn.outcome === "spoke" ? "var(--pet-tint-green-fg)" : "var(--pet-color-muted)",
                    color: "#fff",
                    fontWeight: 600,
                  }}
                  title={
                    currentTurn.outcome === "spoke"
                      ? "LLM 这一轮选择开口（Iter R25）"
                      : "LLM 这一轮选择沉默（reply 为空或含 <silent>，Iter R25）"
                  }
                >
                  {currentTurn.outcome === "spoke" ? "开口" : "沉默"}
                </span>
              )}
              {copyMsg && (
                <span style={{ fontSize: "11px", color: "#0d9488" }}>{copyMsg}</span>
              )}
              {/* 全文复制：把这一 turn 的 timestamp / outcome / tools / prompt /
                  reply 拼成一段 markdown 写到剪贴板 —— 复盘 prompt / 提 issue /
                  把当下 turn 发给 LLM 让它自查时一次粘贴全部上下文，省得 4 次
                  分段复制再手动拼接。 */}
              <button
                onClick={async (e) => {
                  e.stopPropagation();
                  const meta: string[] = [];
                  if (lastTurnMeta.timestamp) {
                    meta.push(`**Timestamp:** ${lastTurnMeta.timestamp}`);
                  }
                  if (currentTurn?.outcome) {
                    meta.push(`**Outcome:** ${currentTurn.outcome}`);
                  }
                  if (lastTurnMeta.tools_used.length > 0) {
                    meta.push(`**Tools used:** ${lastTurnMeta.tools_used.join(", ")}`);
                  }
                  const sections: string[] = [
                    "# Proactive turn",
                    meta.join("\n"),
                    "## PROMPT",
                    "```",
                    lastPrompt || "（空）",
                    "```",
                    "## REPLY",
                    "```",
                    lastReply || "（空 / <silent>）",
                    "```",
                  ];
                  if (lastToolCalls.length > 0) {
                    sections.push("## TOOL CALLS");
                    for (const tc of lastToolCalls) {
                      sections.push(`### ${tc.name}`);
                      sections.push("**arguments**");
                      sections.push("```json");
                      sections.push(tc.arguments || "（空）");
                      sections.push("```");
                      sections.push("**result**");
                      sections.push("```");
                      sections.push(tc.result || "（空）");
                      sections.push("```");
                    }
                  }
                  try {
                    await navigator.clipboard.writeText(sections.join("\n\n"));
                    setCopyMsg("全文已复制");
                    setTimeout(() => setCopyMsg(""), 2500);
                  } catch (err) {
                    setCopyMsg(`复制失败：${err}`);
                    setTimeout(() => setCopyMsg(""), 4000);
                  }
                }}
                style={{
                  marginLeft: "auto",
                  border: "1px solid var(--pet-color-border)",
                  background: "var(--pet-color-card)",
                  cursor: "pointer",
                  color: "var(--pet-color-fg)",
                  fontSize: "11px",
                  padding: "3px 10px",
                  borderRadius: "4px",
                }}
                title="把 timestamp / outcome / tools / prompt / reply / tool calls 一次拼成 markdown 写剪贴板（复盘 / 提 issue / 二次问 LLM 用）。"
              >
                📋 全文复制
              </button>
              {/* issue 模板复制：在"全文复制"基础上再拼 buildDebugMarkdownSnapshot
                  （含陪伴天数 / proactive 出口分布 / env 工具使用 / 工具风险偏
                  好 overrides / tone snapshot 等），一次复制后贴到 GitHub
                  issue / 私聊 maintainer 即足够上下文复现问题。 */}
              <button
                onClick={async (e) => {
                  e.stopPropagation();
                  const meta: string[] = [];
                  if (lastTurnMeta.timestamp) {
                    meta.push(`**Timestamp:** ${lastTurnMeta.timestamp}`);
                  }
                  if (currentTurn?.outcome) {
                    meta.push(`**Outcome:** ${currentTurn.outcome}`);
                  }
                  if (lastTurnMeta.tools_used.length > 0) {
                    meta.push(`**Tools used:** ${lastTurnMeta.tools_used.join(", ")}`);
                  }
                  const sections: string[] = [
                    "# Issue 模板 — Proactive turn 复盘",
                    "",
                    "## TURN",
                    meta.join("\n"),
                    "### PROMPT",
                    "```",
                    lastPrompt || "（空）",
                    "```",
                    "### REPLY",
                    "```",
                    lastReply || "（空 / <silent>）",
                    "```",
                  ];
                  if (lastToolCalls.length > 0) {
                    sections.push("### TOOL CALLS");
                    for (const tc of lastToolCalls) {
                      sections.push(`#### ${tc.name}`);
                      sections.push("**arguments**");
                      sections.push("```json");
                      sections.push(tc.arguments || "（空）");
                      sections.push("```");
                      sections.push("**result**");
                      sections.push("```");
                      sections.push(tc.result || "（空）");
                      sections.push("```");
                    }
                  }
                  sections.push("");
                  sections.push("---");
                  sections.push("");
                  sections.push(buildDebugMarkdownSnapshot());
                  try {
                    await navigator.clipboard.writeText(sections.join("\n\n"));
                    setCopyMsg("issue 模板已复制");
                    setTimeout(() => setCopyMsg(""), 2500);
                  } catch (err) {
                    setCopyMsg(`复制失败：${err}`);
                    setTimeout(() => setCopyMsg(""), 4000);
                  }
                }}
                style={{
                  border: "1px solid var(--pet-color-accent)",
                  background: "var(--pet-color-card)",
                  cursor: "pointer",
                  color: "var(--pet-color-accent)",
                  fontSize: "11px",
                  padding: "3px 10px",
                  borderRadius: "4px",
                  fontWeight: 600,
                }}
                title="在'全文复制'基础上拼陪伴 / proactive 出口分布 / env 工具使用 / 工具偏好 overrides 等调试快照。一次复制粘到 issue / 私聊就足够复现上下文。"
              >
                📋+ issue 模板
              </button>
              <button
                onClick={() => setShowLastPrompt(false)}
                style={{
                  border: "none",
                  background: "transparent",
                  cursor: "pointer",
                  color: "var(--pet-color-muted)",
                  fontSize: "16px",
                }}
              >
                ✕
              </button>
            </div>
            <div style={{ flex: 1, overflow: "auto", display: "flex", flexDirection: "column" }}>
              <div
                onClick={() => setPromptCollapsed((v) => !v)}
                style={{
                  padding: "8px 16px",
                  background: "var(--pet-color-bg)",
                  borderBottom: "1px solid var(--pet-color-border)",
                  display: "flex",
                  alignItems: "center",
                  gap: "8px",
                  cursor: "pointer",
                  userSelect: "none",
                }}
                title={
                  promptCollapsed
                    ? "点击展开 PROMPT 全文"
                    : "点击折叠 PROMPT 段，给 turn-nav 腾视觉空间"
                }
              >
                <span style={{ width: 10, fontFamily: "monospace", color: "var(--pet-color-fg)" }}>
                  {promptCollapsed ? "▸" : "▾"}
                </span>
                <span style={{ fontSize: "11px", fontWeight: 600, color: "var(--pet-color-fg)" }}>
                  ⇢ PROMPT (LLM input)
                </span>
                <span style={{ fontSize: "10px", color: "var(--pet-color-muted)" }}>
                  {lastPrompt.length} 字符 · {countLines(lastPrompt)} 行
                </span>
                <button
                  onClick={async (e) => {
                    e.stopPropagation();
                    try {
                      await navigator.clipboard.writeText(lastPrompt);
                      setCopyMsg("prompt 已复制");
                      setTimeout(() => setCopyMsg(""), 2500);
                    } catch (err) {
                      setCopyMsg(`复制失败: ${err}`);
                    }
                  }}
                  disabled={!lastPrompt}
                  style={{
                    fontSize: "10px",
                    padding: "2px 8px",
                    borderRadius: "4px",
                    border: "1px solid var(--pet-color-border)",
                    background: "var(--pet-color-card)",
                    color: "var(--pet-color-fg)",
                    cursor: lastPrompt ? "pointer" : "default",
                    marginLeft: "auto",
                  }}
                >
                  复制
                </button>
              </div>
              {!promptCollapsed && (
                <pre
                  style={{
                    padding: "12px 16px",
                    fontSize: "12px",
                    fontFamily: "'SF Mono', 'Menlo', monospace",
                    color: "var(--pet-color-fg)",
                    whiteSpace: "pre-wrap",
                    wordBreak: "break-word",
                    margin: 0,
                    borderBottom: "1px solid var(--pet-color-border)",
                  }}
                >
                  {lastPrompt || "（还没有 proactive 触发过——按上面 立即开口 试一次）"}
                </pre>
              )}
              {lastToolCalls.length > 0 && (
                <>
                  <div
                    style={{
                      padding: "8px 16px",
                      background: "#fffbeb",
                      borderBottom: "1px solid var(--pet-color-border)",
                      display: "flex",
                      alignItems: "center",
                      gap: "8px",
                    }}
                  >
                    <span style={{ fontSize: "11px", fontWeight: 600, color: "#92400e" }}>
                      🔧 TOOL CALLS ({lastToolCalls.length} 个)
                    </span>
                    <span style={{ fontSize: "10px", color: "var(--pet-color-muted)" }}>
                      LLM 在本 turn 实际调用的工具，按调用顺序；点击展开看 args / result。
                    </span>
                  </div>
                  <div
                    style={{
                      padding: "8px 12px",
                      background: "var(--pet-color-card)",
                      borderBottom: "1px solid var(--pet-color-border)",
                      display: "flex",
                      flexDirection: "column",
                      gap: "6px",
                    }}
                  >
                    {lastToolCalls.map((tc, j) => {
                      const expanded = expandedToolCallSet.has(j);
                      return (
                        <div
                          key={j}
                          style={{
                            border: "1px solid var(--pet-tint-yellow-fg)",
                            borderRadius: "4px",
                            overflow: "hidden",
                          }}
                        >
                          <div
                            onClick={() => {
                              setExpandedToolCallByTs((prev) => {
                                const next = new Map(prev);
                                const cur = new Set(next.get(currentTurnTs) ?? []);
                                if (cur.has(j)) cur.delete(j);
                                else cur.add(j);
                                next.set(currentTurnTs, cur);
                                return next;
                              });
                            }}
                            style={{
                              padding: "6px 10px",
                              background: "#fef3c7",
                              cursor: "pointer",
                              display: "flex",
                              alignItems: "center",
                              gap: "8px",
                              fontSize: "12px",
                              color: "#92400e",
                              fontWeight: 600,
                            }}
                            title={expanded ? "点击折叠" : "点击展开 args 与 result"}
                          >
                            <span style={{ width: 10, fontFamily: "monospace" }}>
                              {expanded ? "▾" : "▸"}
                            </span>
                            <span>#{j + 1}</span>
                            <span style={{ fontFamily: "'SF Mono', 'Menlo', monospace" }}>
                              {tc.name}
                            </span>
                            {!expanded && (
                              <span
                                style={{
                                  fontSize: "10px",
                                  color: "#92400e",
                                  fontFamily: "'SF Mono', 'Menlo', monospace",
                                  opacity: 0.7,
                                  marginLeft: "auto",
                                  whiteSpace: "nowrap",
                                  overflow: "hidden",
                                  textOverflow: "ellipsis",
                                  maxWidth: "300px",
                                }}
                                title={tc.arguments}
                              >
                                {tc.arguments.length > 60
                                  ? `${tc.arguments.slice(0, 60)}…`
                                  : tc.arguments}
                              </span>
                            )}
                          </div>
                          {expanded && (
                            <div style={{ display: "flex", flexDirection: "column" }}>
                              <div
                                style={{
                                  padding: "4px 10px",
                                  background: "#eff6ff",
                                  fontSize: "10px",
                                  color: "#1e40af",
                                  fontWeight: 600,
                                  borderTop: "1px solid var(--pet-tint-yellow-fg)",
                                }}
                              >
                                arguments
                              </div>
                              <pre
                                style={{
                                  padding: "8px 10px",
                                  fontSize: "11px",
                                  fontFamily: "'SF Mono', 'Menlo', monospace",
                                  color: "#1e293b",
                                  whiteSpace: "pre-wrap",
                                  wordBreak: "break-word",
                                  margin: 0,
                                  background: "#f8fafc",
                                  maxHeight: "200px",
                                  overflow: "auto",
                                }}
                              >
                                {prettyPrintIfJson(tc.arguments)}
                              </pre>
                              <div
                                style={{
                                  padding: "4px 10px",
                                  background: "#f0fdf4",
                                  fontSize: "10px",
                                  color: "#166534",
                                  fontWeight: 600,
                                  borderTop: "1px solid var(--pet-tint-yellow-fg)",
                                }}
                              >
                                result
                              </div>
                              <pre
                                style={{
                                  padding: "8px 10px",
                                  fontSize: "11px",
                                  fontFamily: "'SF Mono', 'Menlo', monospace",
                                  color: "#1e293b",
                                  whiteSpace: "pre-wrap",
                                  wordBreak: "break-word",
                                  margin: 0,
                                  background: "#f8fafc",
                                  maxHeight: "240px",
                                  overflow: "auto",
                                }}
                              >
                                {prettyPrintIfJson(tc.result)}
                              </pre>
                            </div>
                          )}
                        </div>
                      );
                    })}
                  </div>
                </>
              )}
              <div
                onClick={() => setReplyCollapsed((v) => !v)}
                style={{
                  padding: "8px 16px",
                  background: "#f0fdf4",
                  borderBottom: "1px solid var(--pet-color-border)",
                  display: "flex",
                  alignItems: "center",
                  gap: "8px",
                  cursor: "pointer",
                  userSelect: "none",
                }}
                title={
                  replyCollapsed
                    ? "点击展开 REPLY 全文"
                    : "点击折叠 REPLY 段，给 turn-nav 腾视觉空间"
                }
              >
                <span style={{ width: 10, fontFamily: "monospace", color: "#166534" }}>
                  {replyCollapsed ? "▸" : "▾"}
                </span>
                <span style={{ fontSize: "11px", fontWeight: 600, color: "#166534" }}>
                  ⇠ REPLY (LLM output)
                </span>
                <span style={{ fontSize: "10px", color: "var(--pet-color-muted)" }}>
                  {lastReply.length} 字符 · {countLines(lastReply)} 行
                </span>
                <button
                  onClick={async (e) => {
                    e.stopPropagation();
                    try {
                      await navigator.clipboard.writeText(lastReply);
                      setCopyMsg("reply 已复制");
                      setTimeout(() => setCopyMsg(""), 2500);
                    } catch (err) {
                      setCopyMsg(`复制失败: ${err}`);
                    }
                  }}
                  disabled={!lastReply}
                  style={{
                    fontSize: "10px",
                    padding: "2px 8px",
                    borderRadius: "4px",
                    border: "1px solid var(--pet-color-border)",
                    background: "var(--pet-color-card)",
                    color: "var(--pet-color-fg)",
                    cursor: lastReply ? "pointer" : "default",
                    marginLeft: "auto",
                  }}
                >
                  复制
                </button>
              </div>
              {!replyCollapsed && (
                <pre
                  style={{
                    padding: "12px 16px",
                    fontSize: "12px",
                    fontFamily: "'SF Mono', 'Menlo', monospace",
                    color: "var(--pet-color-fg)",
                    whiteSpace: "pre-wrap",
                    wordBreak: "break-word",
                    margin: 0,
                  }}
                >
                  {lastReply || "（还没有 reply — 上次没触发或者 LLM 调用失败）"}
                </pre>
              )}
            </div>
          </div>
        </div>
      )}
      {/* Iter 97: data chips on their own row above the action toolbar so chips and
          buttons each get full horizontal space. The chip strip's prompt-hint badge
          still triggers `showPromptHints`; the expansion card stays just below this
          row so it visually attaches to its trigger. */}
      <PanelChipStrip
        cacheStats={cacheStats}
        moodTagStats={moodTagStats}
        llmOutcomeStats={llmOutcomeStats}
        envToolStats={envToolStats}
        promptTiltStats={promptTiltStats}
        tone={tone}
        showPromptHints={showPromptHints}
        setShowPromptHints={setShowPromptHints}
        onResetCache={handleResetCacheStats}
        onResetMoodTag={handleResetMoodTagStats}
        onResetLlmOutcome={handleResetLlmOutcomeStats}
        onResetEnvTool={handleResetEnvToolStats}
        onResetPromptTilt={handleResetPromptTiltStats}
      />

      {/* 专用工具调用占比（SQLite v11/v12 引入的新工具 vs memory_edit
          fallback）。窗口是 ring buffer 最近 30 条工具调用；total 为 0 时
          不渲染（新启动 / 没调用过任何工具）。 */}
      {dedicatedToolStats && dedicatedToolStats.total_records > 0 && (() => {
        const s = dedicatedToolStats;
        const butlerTotal = s.butler_task_edit_count + s.memory_edit_butler_count;
        const todoTotal = s.todo_edit_count + s.memory_edit_todo_count;
        const butlerRatio = butlerTotal === 0 ? null : s.butler_task_edit_count / butlerTotal;
        const todoRatio = todoTotal === 0 ? null : s.todo_edit_count / todoTotal;
        const fmt = (r: number | null) => (r === null ? "—" : `${Math.round(r * 100)}%`);
        return (
          <div
            role="button"
            onClick={() => void refreshDedicatedToolStats()}
            style={{
              padding: "6px 16px",
              fontSize: 11,
              display: "flex",
              gap: 16,
              alignItems: "center",
              borderBottom: "1px solid var(--pet-color-border)",
              background: "var(--pet-color-bg)",
              color: "var(--pet-color-muted)",
              fontFamily: "'SF Mono', 'Menlo', monospace",
              cursor: dedicatedToolStatsRefreshing ? "default" : "pointer",
              opacity: dedicatedToolStatsRefreshing ? 0.6 : 1,
              transition: "opacity 120ms ease-out",
              userSelect: "none",
            }}
            title={`最近 ${s.total_records} 条工具调用里专用工具的占比。owner 用来判断 prompt 引导效果 —— 比例高代表 LLM 在用 butler_task_edit / todo_edit 而非旧 memory_edit fallback。点击立即刷新。`}
          >
            <span>
              {dedicatedToolStatsRefreshing
                ? "🔄 刷新中"
                : `🛠 专用工具占比（窗口 ${s.total_records}）：`}
            </span>
            <span>
              butler_task_edit{" "}
              <strong style={{ color: "var(--pet-color-fg)" }}>{fmt(butlerRatio)}</strong>{" "}
              <span style={{ color: "var(--pet-color-muted)" }}>
                ({s.butler_task_edit_count} / {butlerTotal || 0})
              </span>
            </span>
            <span>
              todo_edit{" "}
              <strong style={{ color: "var(--pet-color-fg)" }}>{fmt(todoRatio)}</strong>{" "}
              <span style={{ color: "var(--pet-color-muted)" }}>
                ({s.todo_edit_count} / {todoTotal || 0})
              </span>
            </span>
          </div>
        );
      })()}

      {/* 任务状态横条：与 🛠 dedicated tool stats strip 同款。consume 后端
          task_stats（单 SoT）。逾期段在 N>0 时染红，其它段保持 muted 让逾期
          视觉优先。taskStats === null（fetch 失败 / 旧 backend）→ 整条不渲染。 */}
      {taskStats && (
        <div
          role="button"
          onClick={() => void refreshTaskStats()}
          style={{
            padding: "6px 16px",
            fontSize: 11,
            display: "flex",
            gap: 14,
            alignItems: "center",
            borderBottom: "1px solid var(--pet-color-border)",
            background: "var(--pet-color-bg)",
            color: "var(--pet-color-muted)",
            fontFamily: "'SF Mono', 'Menlo', monospace",
            flexWrap: "wrap",
            cursor: taskStatsRefreshing ? "default" : "pointer",
            opacity: taskStatsRefreshing ? 0.6 : 1,
            transition: "opacity 120ms ease-out",
            userSelect: "none",
          }}
          title="butler_tasks 状态汇总（与桌面 /stats / pet 窗 pill 同后端命令）· 点击立即刷新"
        >
          <span style={{ color: "var(--pet-color-fg)", fontWeight: 600 }}>
            {taskStatsRefreshing ? "🔄 刷新中" : "📊 任务状态"}
          </span>
          <span>待办 {taskStats.pending}</span>
          <span
            style={
              taskStats.overdue > 0
                ? { color: "var(--pet-tint-red-fg)", fontWeight: 600 }
                : undefined
            }
          >
            🔴 逾期 {taskStats.overdue}
          </span>
          <span>✓ 今日完成 {taskStats.done_today}</span>
          <span
            style={
              taskStats.error > 0
                ? { color: "var(--pet-tint-red-fg)" }
                : undefined
            }
          >
            ⚠️ 出错 {taskStats.error}
          </span>
          <span>🗑 今日取消 {taskStats.cancelled_today}</span>
        </div>
      )}

      {/* Toolbar */}
      <div style={{ display: "flex", gap: "8px", padding: "12px 16px", borderBottom: "1px solid var(--pet-color-border)", background: "var(--pet-color-card)", alignItems: "center" }}>
        <button onClick={fetchLogs} style={toolBtnStyle}>刷新</button>
        <button onClick={handleClear} style={toolBtnStyle}>清空</button>
        <button
          onClick={() => {
            invoke("open_logs_dir").catch((e) =>
              console.error("open_logs_dir failed:", e),
            );
          }}
          style={toolBtnStyle}
          title="在系统文件管理器里打开 logs 目录（~/.config/pet/logs/）。owner 想 grep / tail / 拖到第三方 viewer 时一键到位。"
        >
          📂 logs 目录
        </button>
        <button
          onClick={() => void handleExportDebugMd()}
          style={toolBtnStyle}
          title="把当前 PanelDebug 的 stats / chip / tone / pending review / 工具风险偏好覆盖 / 最近 5 句宠物语 拼成一份 markdown 写到剪贴板，方便贴 issue 或排查时给同事看"
        >
          📋 导出快照 MD
        </button>
        <button
          onClick={handleCaptureSnapshotA}
          style={toolBtnStyle}
          title="保存当前调试快照为 A（一个时间点）。稍后再点「🔀 对比」会用当前状态作 B，做 set-based 行 diff。"
        >
          {snapshotA ? "📸 重抓 A" : "📸 抓快照 A"}
        </button>
        {snapshotA && (
          <>
            <button
              onClick={handleCompareSnapshot}
              style={{
                ...toolBtnStyle,
                background: "var(--pet-tint-blue-bg)",
                color: "var(--pet-tint-blue-fg)",
                border: "1px solid var(--pet-tint-blue-fg)",
              }}
              title={`对比 A (${snapshotATs}) 与现在的状态，按行做 set-diff，列出仅 A / 仅 B / 共有数。`}
            >
              🔀 对比 A → 现在
            </button>
            <button
              onClick={handleClearSnapshotCompare}
              style={toolBtnStyle}
              title="清掉已抓的 A 与上次 diff 结果"
            >
              清 A
            </button>
          </>
        )}
        {debugExportMsg && (
          <span
            style={{
              fontSize: 11,
              color: debugExportMsg.startsWith("复制失败")
                ? "var(--pet-tint-red-fg)"
                : "var(--pet-tint-green-fg)",
            }}
          >
            {debugExportMsg}
          </span>
        )}
        <button
          onClick={() => {
            if (!triggerArmed) {
              setTriggerArmed(true);
              window.setTimeout(() => setTriggerArmed(false), 3000);
              return;
            }
            setTriggerArmed(false);
            void handleTriggerProactive();
          }}
          disabled={triggeringProactive}
          title={
            triggeringProactive
              ? "正在调 trigger_proactive_turn…"
              : triggerArmed
                ? "再次点击立即触发主动开口（3s 内有效）"
                : "绕过 idle/cooldown/quiet/focus 等闸门，立刻让宠物跑一次主动开口检查（用于测试 prompt 或现场 demo）。点击后 3s 内需再点确认，防误触。"
          }
          style={{
            ...toolBtnStyle,
            background: triggeringProactive
              ? "var(--pet-color-muted)"
              : triggerArmed
                ? "var(--pet-tint-red-bg)"
                : "#10b981",
            color: triggeringProactive ? "#fff" : triggerArmed ? "var(--pet-tint-red-fg)" : "#fff",
            borderColor: triggerArmed ? "var(--pet-tint-red-fg)" : undefined,
            fontWeight: triggerArmed ? 600 : undefined,
          }}
        >
          {triggeringProactive
            ? "开口中…"
            : triggerArmed
              ? "再点确认 (3s)"
              : "立即开口"}
        </button>
        {/* "✏️ 临时 prompt" 按钮：打开 modal 让用户编辑 SOUL 后 fire 一次
            （不写盘）。比"改 SOUL → fire → 改回"省两步，调 prompt 时高频
            用。 */}
        <button
          onClick={() => void openTempPromptModal()}
          disabled={triggeringProactive || tempPromptBusy}
          title="打开 modal 编辑 SOUL.md 临时 fire 一次（仅本轮生效，不写盘）。调 prompt / 测 system 变种时不必去 PanelSettings 改完再改回。"
          style={{
            ...toolBtnStyle,
            background: "#6366f1",
            color: "#fff",
          }}
        >
          ✏️ 临时 prompt
        </button>
        <button
          onClick={async () => {
            try {
              const turns = await invoke<
                { timestamp: string; prompt: string; reply: string; tools_used: string[]; outcome?: string }[]
              >("get_recent_proactive_turns");
              setRecentTurns(turns);
              setTurnIndex(0);
              setShowLastPrompt(true);
            } catch (e) {
              console.error("get_recent_proactive_turns failed:", e);
            }
          }}
          title="查看上次构造的 proactive prompt + LLM reply 全文（process 重启后清空）— 一眼看到 in/out。"
          style={{ ...toolBtnStyle, background: "#6366f1", color: "#fff" }}
        >
          看上次 prompt
        </button>
        <button onClick={handleOpenDevTools} style={{ ...toolBtnStyle, background: "#f59e0b", color: "#fff" }}>
          DevTools
        </button>
        {/* 重置 in-process stash：清 proactive 相关 in-memory 静态（prompt /
            reply / timestamp / turns ring / manual fire / forced focus / tools）。
            不动磁盘文件。两次点击确认（与"立即开口"同模式）；clear 完调
            refreshLastManualFire 让 audit 行同步消失。 */}
        <button
          onClick={async () => {
            if (resetStashBusy) return;
            if (!resetStashArmed) {
              setResetStashArmed(true);
              window.setTimeout(() => setResetStashArmed(false), 3000);
              return;
            }
            setResetStashArmed(false);
            setResetStashBusy(true);
            try {
              await invoke("reset_proactive_stash");
              setProactiveStatus("✓ in-process stash 已清空");
              // 也清前端镜像
              setLastManualFire(null);
              setRecentTurns([]);
              setTurnIndex(0);
            } catch (e) {
              setProactiveStatus(`重置失败: ${e}`);
            } finally {
              setResetStashBusy(false);
              window.setTimeout(() => setProactiveStatus(""), 4000);
            }
          }}
          disabled={resetStashBusy}
          title={
            resetStashBusy
              ? "重置中…"
              : resetStashArmed
                ? "再次点击确认重置 (3s 内有效)"
                : "清 proactive 相关 in-process stash（prompt / reply / turns ring / manual fire / forced focus 等内存 stash）。不动磁盘 —— butler_history / memory / sessions / decision log 都不受影响。两次点击确认。"
          }
          style={{
            ...toolBtnStyle,
            background: resetStashBusy
              ? "var(--pet-color-muted)"
              : resetStashArmed
                ? "var(--pet-tint-red-bg)"
                : "var(--pet-color-muted)",
            color: resetStashBusy ? "#fff" : resetStashArmed ? "var(--pet-tint-red-fg)" : "#fff",
            borderColor: resetStashArmed ? "var(--pet-tint-red-fg)" : undefined,
            fontWeight: resetStashArmed ? 600 : undefined,
          }}
        >
          {resetStashBusy
            ? "清理中…"
            : resetStashArmed
              ? "再点确认 (3s)"
              : "🧹 重置 stash"}
        </button>
        {/* ⚙️ mute 15min：proactive 静音 15 分钟的快捷按钮。muted 时显剩余
            分钟 + 再点解除，与 PanelChat /sleep 同后端命令（set_mute_minutes）。
            放在 proactive 工具组末尾 —— 与"立即开口 / 临时 prompt"语义相邻。 */}
        <button
          onClick={() => void handleMuteToggle()}
          disabled={muteBusy}
          title={
            muteBusy
              ? "处理中…"
              : muteRemainingMins > 0
                ? `当前已 mute（剩 ${muteRemainingMins} 分钟）— 点击解除`
                : "mute proactive 15 分钟，调 prompt / 测 SOUL 时绕开 PanelChat /sleep 路径"
          }
          style={{
            ...toolBtnStyle,
            background: muteBusy
              ? "var(--pet-color-muted)"
              : muteRemainingMins > 0
                ? "var(--pet-tint-blue-fg)"
                : "var(--pet-color-accent)",
            color: "#fff",
          }}
        >
          {muteBusy
            ? "处理中…"
            : muteRemainingMins > 0
              ? `🔕 mute ${muteRemainingMins}m`
              : "⚙️ mute 15min"}
        </button>
        {/* 强制 reload 整个 DebugApp window：用户改外部 config / memory /
            sessions 文件后想 panel 立刻 sync 时用。write current tab 到
            sessionStorage 让 reload 后 DebugApp useState initializer 读回，
            用户落回当前 tab 而非"应用"默认。reload 全清前端 state（也清
            in-process stash 镜像，与 🧹 重置 stash 是不同 axis：那条仅清
            backend 内存 stash 不动 frontend）。 */}
        <button
          onClick={() => {
            try {
              const cur = (() => {
                // 尝试从顶部 tab bar 找当前 active tab（class 含 borderBottom
                // 0ea5e9 的）—— 但 PanelDebug 没拿到 DebugApp 的 activeTab。
                // 退而求其次：从 document.title 不行，干脆默认写"应用"（用
                // 户在 PanelDebug 内点 reload 必然处于"应用" tab）。
                return "应用";
              })();
              sessionStorage.setItem("pet-debug-reload-tab", cur);
            } catch {
              // 失败 → reload 后落"应用"默认，无副作用
            }
            window.location.reload();
          }}
          title="强制 reload 当前 DebugApp 窗口 —— 用户改外部 config / memory / sessions 文件后想 panel 立刻 sync 时用。reload 会清掉所有前端 state 与 cache；当前 tab 通过 sessionStorage 保留。"
          style={{ ...toolBtnStyle, background: "var(--pet-color-accent)", color: "#fff" }}
        >
          🔄 reload
        </button>
        {/* "📥 stash JSON" 按钮：把所有 in-process stash（last prompt /
            reply / turns ring / manual fire latest + history）一次拉到一段
            JSON 写剪贴板。issue 复现 / reproduce 用最稳 —— 比单独 4 个命
            令 round-trip 快。 */}
        <button
          onClick={async () => {
            try {
              const [prompt, reply, meta, turns, lastFire, fireHistory] =
                await Promise.all([
                  invoke<string>("get_last_proactive_prompt"),
                  invoke<string>("get_last_proactive_reply"),
                  invoke<unknown>("get_last_proactive_meta"),
                  invoke<unknown[]>("get_recent_proactive_turns"),
                  invoke<unknown>("get_last_manual_fire"),
                  invoke<unknown[]>("get_manual_fire_history"),
                ]);
              const stash = {
                exported_at: new Date().toISOString(),
                last_proactive_prompt: prompt,
                last_proactive_reply: reply,
                last_proactive_meta: meta,
                recent_proactive_turns: turns,
                last_manual_fire: lastFire,
                manual_fire_history: fireHistory,
              };
              const json = JSON.stringify(stash, null, 2);
              await navigator.clipboard.writeText(json);
              setProactiveStatus(`已复制 stash JSON（${json.length} 字符）`);
            } catch (e) {
              setProactiveStatus(`stash 抓取失败: ${e}`);
            }
            window.setTimeout(() => setProactiveStatus(""), 4000);
          }}
          title="把所有 in-process stash（prompt / reply / turns ring / manual fire latest + history）一次拉成 JSON 复制到剪贴板。issue 复现 / 跨进程粘贴 reproduce 用。"
          style={{ ...toolBtnStyle, background: "#a855f7", color: "#fff" }}
        >
          📥 stash JSON
        </button>
        {proactiveStatus && (
          <span
            style={{
              fontSize: "12px",
              color: proactiveStatus.startsWith("触发失败") ? "var(--pet-tint-red-fg)" : "var(--pet-tint-green-fg)",
              alignSelf: "center",
              maxWidth: "260px",
              overflow: "hidden",
              textOverflow: "ellipsis",
              whiteSpace: "nowrap",
            }}
            title={proactiveStatus}
          >
            {proactiveStatus}
          </span>
        )}
      </div>

      {/* 上次 manual fire 审计行 + 历史 ring（近 5 条）：让用户回顾"我本
          次进程内手动触发过哪几条任务、什么时候、结果如何"。进程重启清空；
          自然 tick / 后台 loop 不进此 stash —— 只有 PanelDebug "立即开口"
          与 PanelMemory "▶️ 现在跑" 两个入口才记录。点击 🔄 立即从后端
          refetch（用户在别的窗口 fire 后切回时手动同步用）；▾ 展开看历
          史 ≤ 5 条。 */}
      {lastManualFire && (
        <div
          style={{
            borderBottom: "1px solid var(--pet-color-border)",
            background: "var(--pet-color-bg)",
          }}
        >
          <div
            style={{
              padding: "6px 16px",
              display: "flex",
              alignItems: "center",
              gap: 8,
              fontSize: 11,
              color: "var(--pet-color-muted)",
            }}
          >
            <span style={{ fontWeight: 600, color: "var(--pet-color-fg)" }}>
              🕒 上次 manual fire
            </span>
            <span style={{ fontFamily: "'SF Mono', 'Menlo', monospace" }}>
              {lastManualFire.timestamp}
            </span>
            <span>·</span>
            <span>
              {lastManualFire.title === null
                ? "全局 fire"
                : `▶️ 「${lastManualFire.title}」`}
            </span>
            <span style={{ color: "var(--pet-color-border)" }}>|</span>
            <span
              style={{
                flex: 1,
                overflow: "hidden",
                textOverflow: "ellipsis",
                whiteSpace: "nowrap",
                color: lastManualFire.result.startsWith("触发失败")
                  ? "var(--pet-tint-red-fg)"
                  : "var(--pet-color-fg)",
              }}
              title={lastManualFire.result}
            >
              {lastManualFire.result}
            </span>
            {manualFireHistory.length > 1 && (
              <button
                type="button"
                onClick={() => setManualFireHistoryExpanded((v) => !v)}
                style={{
                  padding: "2px 6px",
                  fontSize: 10,
                  border: "1px solid var(--pet-color-border)",
                  borderRadius: 3,
                  background: "var(--pet-color-card)",
                  color: "var(--pet-color-muted)",
                  cursor: "pointer",
                  flexShrink: 0,
                }}
                title={
                  manualFireHistoryExpanded
                    ? "折叠历史 ring"
                    : `展开历史 ring（共 ${manualFireHistory.length} 条）`
                }
              >
                {manualFireHistoryExpanded ? "▾" : "▸"} {manualFireHistory.length}
              </button>
            )}
            {/* 📋 复制全部 manual fire history 为 markdown：拼时间 / 类型
                / result 拼一段 markdown table-like 写剪贴板。issue / 调
                prompt 时一次性带走全部 fire 记录。仅 history 非空时浮。 */}
            {manualFireHistory.length > 0 && (
              <button
                type="button"
                onClick={async () => {
                  const lines: string[] = [
                    `# Manual fire history (${manualFireHistory.length} 条)`,
                    "",
                    "| 时间 | 类型 | 结果 |",
                    "| --- | --- | --- |",
                  ];
                  for (const rec of manualFireHistory) {
                    const scope =
                      rec.title === null ? "全局" : `▶️ 「${rec.title}」`;
                    // markdown table cell escape：把 | 改 \| 防 cell 错位
                    const safeResult = rec.result.replace(/\|/g, "\\|");
                    lines.push(`| ${rec.timestamp} | ${scope} | ${safeResult} |`);
                  }
                  try {
                    await navigator.clipboard.writeText(lines.join("\n"));
                    setProactiveStatus(
                      `已复制 history markdown（${manualFireHistory.length} 条）`,
                    );
                  } catch (e) {
                    setProactiveStatus(`复制失败: ${e}`);
                  }
                  window.setTimeout(() => setProactiveStatus(""), 4000);
                }}
                style={{
                  padding: "2px 6px",
                  fontSize: 10,
                  border: "1px solid var(--pet-color-border)",
                  borderRadius: 3,
                  background: "var(--pet-color-card)",
                  color: "var(--pet-color-muted)",
                  cursor: "pointer",
                  flexShrink: 0,
                }}
                title="把整个 history ring（含当前最新条）拼成 markdown table 复制到剪贴板。issue / prompt 调试分享用。"
              >
                📋
              </button>
            )}
            <button
              type="button"
              onClick={() => void refreshLastManualFire()}
              style={{
                padding: "2px 6px",
                fontSize: 10,
                border: "1px solid var(--pet-color-border)",
                borderRadius: 3,
                background: "var(--pet-color-card)",
                color: "var(--pet-color-muted)",
                cursor: "pointer",
                flexShrink: 0,
              }}
              title="重新从后端拉取（在别的窗口 fire 过后用此同步）"
            >
              🔄
            </button>
          </div>
          {manualFireHistoryExpanded && manualFireHistory.length > 1 && (
            <div
              style={{
                padding: "0 16px 8px 16px",
                fontSize: 11,
                color: "var(--pet-color-muted)",
              }}
            >
              {manualFireHistory.slice(1).map((rec, i) => (
                <div
                  key={`${rec.timestamp}-${i}`}
                  style={{
                    display: "flex",
                    gap: 6,
                    alignItems: "baseline",
                    padding: "3px 0",
                    borderTop: "1px dashed var(--pet-color-border)",
                  }}
                >
                  <span style={{ fontFamily: "'SF Mono', 'Menlo', monospace", flexShrink: 0 }}>
                    {rec.timestamp}
                  </span>
                  <span style={{ flexShrink: 0 }}>
                    {rec.title === null ? "全局" : `▶️ 「${rec.title}」`}
                  </span>
                  <span style={{ color: "var(--pet-color-border)" }}>|</span>
                  <span
                    style={{
                      flex: 1,
                      overflow: "hidden",
                      textOverflow: "ellipsis",
                      whiteSpace: "nowrap",
                      color: rec.result.startsWith("触发失败")
                        ? "var(--pet-tint-red-fg)"
                        : "var(--pet-color-fg)",
                    }}
                    title={rec.result}
                  >
                    {rec.result}
                  </span>
                </div>
              ))}
            </div>
          )}
        </div>
      )}

      {/* 快照对比 diff 展示：仅 compareDiff 非空时显。pre code 风格 + 复制
          按钮 + 关闭。set-diff 不保留位置信息，对结构稳定的 snapshot 已够用。 */}
      {compareDiff && (
        <div
          style={{
            padding: "10px 16px",
            borderBottom: "1px solid var(--pet-color-border)",
            background: "var(--pet-color-bg)",
          }}
        >
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: 8,
              marginBottom: 6,
            }}
          >
            <span style={{ fontSize: 12, fontWeight: 600, color: "var(--pet-color-fg)" }}>
              🔀 快照对比
            </span>
            <button
              onClick={() => void handleCopyCompareDiff()}
              style={{ ...toolBtnStyle, fontSize: 11, padding: "3px 9px" }}
              title="复制 diff markdown 到剪贴板"
            >
              📋 复制 diff
            </button>
            <button
              onClick={() => setCompareDiff(null)}
              style={{ ...toolBtnStyle, fontSize: 11, padding: "3px 9px" }}
              title="关闭对比结果"
            >
              ✕ 关
            </button>
          </div>
          <pre
            style={{
              margin: 0,
              padding: "10px 12px",
              background: "var(--pet-color-card)",
              border: "1px solid var(--pet-color-border)",
              borderRadius: 6,
              fontSize: 11,
              lineHeight: 1.5,
              fontFamily: "'SF Mono', 'Menlo', monospace",
              maxHeight: 320,
              overflowY: "auto",
              whiteSpace: "pre-wrap",
              wordBreak: "break-word",
              color: "var(--pet-color-fg)",
            }}
          >
            {compareDiff}
          </pre>
        </div>
      )}

      {/* Inline expansion of the active prompt hints — only renders when the user has
          clicked the badge. Each hint shows its nature badge + title + a one-line summary,
          plus an aggregate "克制 × N / 引导 × N / ..." line so the user gets an at-a-glance
          read on whether the prompt is currently shaping the pet toward quiet or active
          behavior. */}
      {showPromptHints && tone && tone.active_prompt_rules.length > 0 && (
        <div
          style={{
            padding: "8px 16px",
            borderBottom: "1px solid var(--pet-color-border)",
            background: "var(--pet-tint-lavender-bg)",
            fontSize: "12px",
          }}
        >
          {(() => {
            const counts: Record<PromptRuleNature, number> = {
              restraint: 0,
              engagement: 0,
              corrective: 0,
              instructional: 0,
            };
            for (const label of tone.active_prompt_rules) {
              const n = PROMPT_RULE_DESCRIPTIONS[label]?.nature;
              if (n) counts[n] += 1;
            }
            const segments = (Object.keys(counts) as PromptRuleNature[])
              .filter((n) => counts[n] > 0)
              .map((n) => (
                <span key={n} style={{ color: NATURE_META[n].color, fontWeight: 600 }}>
                  {NATURE_META[n].label} × {counts[n]}
                </span>
              ));
            return (
              <div
                style={{
                  display: "flex",
                  gap: "10px",
                  marginBottom: "6px",
                  fontSize: "11px",
                  alignItems: "baseline",
                }}
              >
                <span style={{ color: "var(--pet-tint-lavender-fg)" }}>
                  当前 prompt 软规则 ({tone.active_prompt_rules.length})：
                </span>
                {segments}
              </div>
            );
          })()}
          {tone.active_prompt_rules.map((label) => {
            const desc = PROMPT_RULE_DESCRIPTIONS[label];
            const natureColor = desc ? NATURE_META[desc.nature].color : "var(--pet-color-muted)";
            const natureLabel = desc ? NATURE_META[desc.nature].label : "?";
            return (
              <div key={label} style={{ display: "flex", gap: "8px", lineHeight: "1.6" }}>
                <span
                  title={desc ? `nature: ${desc.nature}` : undefined}
                  style={{
                    fontSize: "10px",
                    color: "#fff",
                    background: natureColor,
                    padding: "1px 5px",
                    borderRadius: "4px",
                    minWidth: "26px",
                    textAlign: "center",
                    alignSelf: "center",
                  }}
                >
                  {natureLabel}
                </span>
                <span
                  style={{
                    color: "#7c3aed",
                    fontWeight: 600,
                    minWidth: "84px",
                    fontFamily: "'SF Mono', 'Menlo', monospace",
                  }}
                >
                  {desc?.title ?? label}
                </span>
                <span style={{ color: "#475569", flex: 1 }}>
                  {desc?.summary ?? `(label "${label}" 暂无中文描述)`}
                </span>
              </div>
            );
          })}
        </div>
      )}

      <PanelStatsCard
        todaySpeechCount={todaySpeechCount}
        weekSpeechCount={weekSpeechCount}
        lifetimeSpeechCount={lifetimeSpeechCount}
        companionshipDays={companionshipDays}
        tone={tone}
      />
      {/* 可调窗口主动开口次数：与上方固定的"今日 / 本周 / 累计"互补，让用
          户看任意窗口（1d / 3d / 7d / 14d / 30d）的趋势。值通过 get_speech_count_days
          后端命令计算；切窗口立即重抓 + 30s 轮询同步 daily bucket 增量。 */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 8,
          padding: "8px 16px",
          borderBottom: "1px solid var(--pet-color-border)",
          background: "var(--pet-color-bg)",
          fontSize: 12,
        }}
      >
        <span style={{ color: "var(--pet-color-muted)" }}>近</span>
        {[1, 3, 7, 14, 30].map((d) => {
          const active = d === speechWindowDays;
          return (
            <button
              key={d}
              type="button"
              onClick={() => {
                setSpeechWindowDays(d);
                try {
                  window.localStorage.setItem(
                    "pet-debug-speech-window-days",
                    String(d),
                  );
                } catch {
                  // ignore
                }
              }}
              style={{
                fontSize: 11,
                padding: "2px 10px",
                border: "1px solid",
                borderColor: active ? "var(--pet-color-accent)" : "var(--pet-color-border)",
                borderRadius: 4,
                background: active ? "var(--pet-color-accent)" : "var(--pet-color-card)",
                color: active ? "#fff" : "var(--pet-color-muted)",
                cursor: active ? "default" : "pointer",
                fontWeight: active ? 600 : 400,
                fontFamily: "inherit",
              }}
            >
              {d}d
            </button>
          );
        })}
        <span
          style={{
            color: "var(--pet-color-fg)",
            fontFamily: "'SF Mono', 'Menlo', monospace",
            fontSize: 14,
            fontWeight: 600,
            marginLeft: 8,
          }}
          title={`过去 ${speechWindowDays} 天累计主动开口次数（含今天）。来自 speech_daily.json 各日 bucket 求和。`}
        >
          {speechWindowCount}
        </span>
        <span style={{ color: "var(--pet-color-muted)", fontSize: 11 }}>
          次 ·{" "}
          {speechWindowDays > 0 ? (speechWindowCount / speechWindowDays).toFixed(1) : "—"}/日均
        </span>
      </div>
      {/* 今日 24 小时分布 mini bar：每柱代表一小时。最高柱满高，其余按比例。
          零桶仍占位（保 24 列等宽对齐时间轴感）。bar 配色与 PanelStatsCard
          今日颜色 sky 一致。每 60s 自动 refresh；hover 显 "HH:00 · N 次"。 */}
      {(() => {
        const max = Math.max(1, ...hourlyBuckets);
        const todayTotal = hourlyBuckets.reduce((a, b) => a + b, 0);
        return (
          <div
            style={{
              display: "flex",
              alignItems: "flex-end",
              gap: 8,
              padding: "8px 16px",
              borderBottom: "1px solid var(--pet-color-border)",
              background: "var(--pet-color-bg)",
            }}
          >
            <span
              style={{
                fontSize: 11,
                color: "var(--pet-color-muted)",
                whiteSpace: "nowrap",
                paddingBottom: 2,
              }}
            >
              今日 24h
            </span>
            <div
              style={{
                display: "grid",
                gridTemplateColumns: "repeat(24, 1fr)",
                gap: 2,
                flex: 1,
                height: 28,
                alignItems: "end",
              }}
              title={`今日总 ${todayTotal} 次主动开口；hover 任一柱看小时数`}
            >
              {hourlyBuckets.map((n, h) => {
                const ratio = max > 0 ? n / max : 0;
                const heightPct = Math.max(n > 0 ? 8 : 2, ratio * 100);
                return (
                  <div
                    key={h}
                    title={`${h.toString().padStart(2, "0")}:00 · ${n} 次`}
                    style={{
                      height: `${heightPct}%`,
                      background:
                        n > 0
                          ? "var(--pet-color-accent)"
                          : "var(--pet-color-border)",
                      borderRadius: "2px 2px 0 0",
                      opacity: n > 0 ? 1 : 0.4,
                      transition: "height 240ms ease-out",
                      cursor: "default",
                    }}
                  />
                );
              })}
            </div>
            <span
              style={{
                fontSize: 11,
                color: "var(--pet-color-muted)",
                fontVariantNumeric: "tabular-nums",
                paddingBottom: 2,
              }}
            >
              Σ {todayTotal}
            </span>
          </div>
        );
      })()}

      <PanelToolsTopK history={toolCallHistory} />

      <PanelToneStrip tone={tone} />

      {/* TG bot 启动期 non-fatal 告警 banner —— set_my_commands 失败、bot 起
          不来等情况此前只 eprintln，用户看不到。空 / 全 dismiss 时不渲染。 */}
      {(() => {
        const visibleTgWarnings = tgStartupWarnings.filter(
          (w) => !tgDismissed.has(`${w.timestamp}|${w.kind}|${w.message}`),
        );
        if (visibleTgWarnings.length === 0) return null;
        return (
          <div
            style={{
              padding: "8px 16px",
              borderBottom: "1px solid #fed7aa",
              background: "#fff7ed",
              fontSize: "12px",
              color: "#9a3412",
            }}
          >
            <div style={{ fontWeight: 600, marginBottom: 4 }}>
              ⚠ Telegram 启动告警 ({visibleTgWarnings.length})
            </div>
            {visibleTgWarnings.map((w) => {
              const fp = `${w.timestamp}|${w.kind}|${w.message}`;
              return (
                <div
                  key={fp}
                  style={{
                    display: "flex",
                    alignItems: "flex-start",
                    gap: 6,
                    fontSize: 11,
                    fontFamily: "'SF Mono', 'Menlo', monospace",
                    color: "var(--pet-tint-red-fg)",
                  }}
                  title={w.timestamp}
                >
                  <span style={{ flex: 1, wordBreak: "break-all" }}>
                    <span style={{ color: "#9a3412", fontWeight: 600 }}>{w.kind}</span>: {w.message}
                  </span>
                  <button
                    type="button"
                    onClick={() =>
                      setTgDismissed((prev) => {
                        const next = new Set(prev);
                        next.add(fp);
                        return next;
                      })
                    }
                    title="知道了；隐藏这条警告（不删后端记录，进程重启自动清）"
                    style={{
                      fontSize: 10,
                      padding: "0 6px",
                      lineHeight: 1.4,
                      border: "1px solid #fed7aa",
                      borderRadius: 4,
                      background: "#fff",
                      color: "#9a3412",
                      cursor: "pointer",
                      flexShrink: 0,
                    }}
                  >
                    ✕
                  </button>
                </div>
              );
            })}
          </div>
        );
      })()}

      {/* Recent proactive decisions — answers "why didn't the pet say anything?" */}
      {/* CSS hover-only 显隐：决策行 hover 时单行复制按钮显出，平时透明
          不打扰阅读。同 PanelTasks/Chat 的 .pet-*-copy-btn 同模式。 */}
      <style>
        {`
          .pet-decision-row .pet-decision-copy-btn {
            opacity: 0;
            transition: opacity 0.12s ease;
          }
          /* R133: 决策行整体 hover bg overlay（与 R130/R131 同款 rgba），让
             密集列表里光标位置可见。容器 bg 是 var(--pet-color-bg) 灰底，
             rgba 叠加得微暗反差。 */
          .pet-decision-row {
            transition: background-color 0.12s ease;
          }
          .pet-decision-row:hover {
            background: rgba(0, 0, 0, 0.04);
          }
          .pet-decision-row:hover .pet-decision-copy-btn {
            opacity: 1;
          }
          .pet-decision-row .pet-decision-copy-btn:hover {
            background: #f1f5f9;
          }
          /* R130: 反馈记录行 hover bg 高亮，与 R122/R123 同款。rgba 而非
             token var —— feedback section 本身有绿 tint bg；alpha overlay
             跨主题都呈 subtle hover 不破坏 section 配色。 */
          .pet-feedback-row {
            transition: background-color 0.12s ease;
          }
          .pet-feedback-row:hover {
            background: rgba(0, 0, 0, 0.04);
          }
          /* R135: 工具调用历史行 hover bg overlay。inline 黄底 #fffbeb，
             用 !important 反压让 hover 时变浅 muddy 反差；移开恢复黄。 */
          .pet-tool-history-row {
            transition: background-color 0.12s ease;
          }
          .pet-tool-history-row:hover {
            background: rgba(0, 0, 0, 0.04) !important;
          }
          /* R148: 决策行重跑按钮 hover 反馈。inline bg = card token，
             rgba 0.04 overlay 叠出 subtle 灰；!important 反压 inline。
             :not(:disabled) 让 triggering 中的 button 不响应 hover。 */
          .pet-rerun-btn {
            transition: background-color 0.12s ease;
          }
          .pet-rerun-btn:not(:disabled):hover {
            background: rgba(0, 0, 0, 0.04) !important;
          }
        `}
      </style>
      {decisions.length > 0 && (
        <div
          style={{
            padding: "8px 16px",
            borderBottom: "1px solid var(--pet-color-border)",
            background: "var(--pet-color-bg)",
            fontSize: "11px",
            fontFamily: "'SF Mono', 'Menlo', monospace",
            maxHeight: "200px",
            overflowY: "auto",
          }}
        >
          <div style={{ color: "var(--pet-color-muted)", marginBottom: "4px", fontFamily: "inherit", fontSize: "12px", display: "flex", alignItems: "center", gap: "8px" }}>
            {/* R146: 标题 span 点击切换 collapse；与 tool history (line 1976)
                / feedback history (line 2185) 的 ▾/▸ 折叠交互对齐。
                folded 时 status / 清空仍可见（在 header 同行外侧）。 */}
            <span
              onClick={() => setShowDecisions((s) => !s)}
              style={{ cursor: "pointer", userSelect: "none" }}
              title={showDecisions ? "点击折叠决策日志" : "点击展开决策日志"}
            >
              最近 {decisions.length} 次主动开口判断（最新在
              {decisionsNewestFirst ? "顶部" : "底部"}）
              {" "}
              {showDecisions ? "▾" : "▸"}
            </span>
            {/* 镜像顶部「立即开口」状态文案，避免用户在 toolbar 与决策日志
                之间来回扫视。复用现有 proactiveStatus 状态 + 8s 自清空策略。 */}
            {proactiveStatus && (
              <span
                style={{
                  fontSize: "11px",
                  // R149: 失败走 orange tint（theme.ts 无 red），成功走
                  //   green tint；与 R147 / R125 "orange = 警示"语义对齐。
                  color: proactiveStatus.startsWith("触发失败")
                    ? "var(--pet-tint-orange-fg)"
                    : "var(--pet-tint-green-fg)",
                  maxWidth: "260px",
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                  whiteSpace: "nowrap",
                }}
                title={proactiveStatus}
              >
                {proactiveStatus}
              </span>
            )}
            {decisions.length > 0 && (
              <button
                onClick={async () => {
                  if (!clearDecisionsArmed) {
                    // 第一次点：armed → 3s 后自动 revert 防误触
                    setClearDecisionsArmed(true);
                    window.setTimeout(() => setClearDecisionsArmed(false), 3000);
                    return;
                  }
                  // 二次确认：真清
                  try {
                    await invoke("clear_proactive_decisions");
                    setDecisions([]);
                  } catch (e) {
                    console.error("clear_proactive_decisions failed:", e);
                  } finally {
                    setClearDecisionsArmed(false);
                  }
                }}
                title={
                  clearDecisionsArmed
                    ? "再次点击立即清空 in-memory 决策日志（3s 内有效）。"
                    : "清空 in-memory 决策日志（不影响 LogStore）。点击后 3s 内需再点确认。"
                }
                style={{
                  marginLeft: "auto",
                  padding: "1px 8px",
                  fontSize: "11px",
                  // R147: armed 用 orange tint（warning 语义；theme.ts 无 red
                  //   tint，复用 R125 立即点燃同款橙警示）；non-armed 走
                  //   framework token 跟主题切换。
                  border: `1px solid ${clearDecisionsArmed ? "var(--pet-tint-orange-fg)" : "var(--pet-color-border)"}`,
                  borderRadius: "4px",
                  background: clearDecisionsArmed ? "var(--pet-tint-orange-bg)" : "var(--pet-color-card)",
                  color: clearDecisionsArmed ? "var(--pet-tint-orange-fg)" : "var(--pet-color-muted)",
                  cursor: "pointer",
                  fontWeight: clearDecisionsArmed ? 600 : 400,
                }}
              >
                {clearDecisionsArmed ? "确认清空 (3s 内)" : "清空"}
              </button>
            )}
          </div>
          {showDecisions && (
          <>
          {/* R83: 内联 multi-select chip 行（脱离单选 PanelFilterButtonRow）。
              "全部" 在 Set 空时 active，点击清空；其它 chip 点击 toggle in/out。
              视觉规格抄自 PanelFilterButtonRow 保持视觉一致。 */}
          <div style={{ display: "flex", alignItems: "center", gap: 6, marginBottom: "6px", flexWrap: "wrap" }}>
            {(() => {
              const kindOptions: { value: string; label: string; accent: string; title: string }[] = [
                { value: "Spoke", label: "开口", accent: "var(--pet-tint-green-fg)", title: "LLM 选择开口的轮次" },
                { value: "LlmSilent", label: "沉默", accent: "#a855f7", title: "LLM 选择沉默的轮次" },
                { value: "Skip", label: "跳过", accent: "#f59e0b", title: "gate 阻止 LLM 跑的轮次" },
              ];
              const allActive = decisionKinds.size === 0;
              // R84: inactive border = accent 40% alpha；R99 hoist 到 module 级
              // `multiSelectChipStyle` 复用日志 level chip。文字仍走 fg —— amber /
              // red 等低对比度色直接当 body text 难达 WCAG AA，让边框承担色信号。
              const chipStyle = multiSelectChipStyle;
              const toggle = (k: string) => {
                setDecisionKinds((prev) => {
                  const next = new Set(prev);
                  if (next.has(k)) next.delete(k);
                  else next.add(k);
                  return next;
                });
              };
              return (
                <>
                  <button
                    type="button"
                    onClick={() => setDecisionKinds(new Set())}
                    style={chipStyle(allActive, "#475569")}
                    title="显示全部决策（含 Run / Silent / LlmError / ToolReview*）。点击清空多选过滤。"
                  >
                    全部 {decisions.length}
                  </button>
                  {kindOptions.map((opt) => {
                    const isActive = decisionKinds.has(opt.value);
                    const cnt = decisions.filter((d) => d.kind === opt.value).length;
                    return (
                      <button
                        key={opt.value}
                        type="button"
                        onClick={() => toggle(opt.value)}
                        style={chipStyle(isActive, opt.accent)}
                        title={
                          isActive
                            ? `再次点击移出过滤集合（当前: ${opt.title}）`
                            : `加入到只看的 kind 集合（多选）：${opt.title}`
                        }
                      >
                        {opt.label} {cnt}
                      </button>
                    );
                  })}
                  {/* R86: 时间窗快捷过滤。单选互斥，accent 用统一灰（与"全部"chip 同色族），
                      表示这是"非 kind 的过滤维度"，与 kind 多选 + reason 搜索三层 AND。 */}
                  {(
                    [
                      { value: "10m" as const, label: "近 10m", title: "只看最近 10 分钟内的决策" },
                      { value: "30m" as const, label: "近 30m", title: "只看最近 30 分钟内的决策" },
                      { value: "1h" as const, label: "近 1h", title: "只看最近 60 分钟内的决策" },
                    ]
                  ).map((opt) => {
                    const isActive = decisionTimeWindow === opt.value;
                    return (
                      <button
                        key={opt.value}
                        type="button"
                        onClick={() =>
                          setDecisionTimeWindow(isActive ? "all" : opt.value)
                        }
                        style={chipStyle(isActive, "#475569")}
                        title={
                          isActive
                            ? `再次点击关闭时间窗（${opt.title}）`
                            : opt.title
                        }
                      >
                        {opt.label}
                      </button>
                    );
                  })}
                </>
              );
            })()}
            {/* reason / kind 子串搜索：与 chip 同行，省垂直空间。匹配三域：
                d.kind / d.reason 原始串 / localizeReason 本地化串 ——
                "cooldown" / "冷却" / "Skip" 都能定位同一组条目。 */}
            <input
              type="search"
              value={decisionReasonSearch}
              onChange={(e) => setDecisionReasonSearch(e.target.value)}
              placeholder="搜 reason / kind"
              title="子串过滤决策日志：匹配 kind、原始 reason、本地化 reason 三域。区分大小写无关。"
              style={{
                fontFamily: "inherit",
                fontSize: "11px",
                padding: "1px 6px",
                border: "1px solid var(--pet-color-border)",
                borderRadius: 4,
                background: "var(--pet-color-card)",
                color: "var(--pet-color-fg)",
                width: 140,
                lineHeight: 1.4,
              }}
            />
            {decisionReasonSearch.trim() !== "" && (
              <button
                type="button"
                onClick={() => setDecisionReasonSearch("")}
                title="清空 reason 搜索"
                style={{
                  fontSize: "10px",
                  padding: "1px 6px",
                  border: "1px solid var(--pet-color-border)",
                  borderRadius: 4,
                  background: "var(--pet-color-card)",
                  color: "var(--pet-color-muted)",
                  cursor: "pointer",
                  lineHeight: 1.4,
                }}
              >
                ✕
              </button>
            )}
            {/* 渲染顺序开关：默认最新在底（保留 ring-buffer 自然时序），可
                切到最新在顶（与浏览器 devtools / dashboard 直觉对齐）。 */}
            <button
              type="button"
              onClick={() => setDecisionsNewestFirst((v) => !v)}
              title={
                decisionsNewestFirst
                  ? "当前最新在顶。点击切回最新在底（ring-buffer 自然时序）"
                  : "当前最新在底。点击切到最新在顶（与多数 dashboard 直觉对齐）"
              }
              style={{
                marginLeft: "auto",
                fontSize: "10px",
                padding: "1px 6px",
                border: "1px solid var(--pet-color-border)",
                borderRadius: 4,
                background: "var(--pet-color-card)",
                color: "var(--pet-color-fg)",
                cursor: "pointer",
                lineHeight: 1.4,
                whiteSpace: "nowrap",
              }}
            >
              {decisionsNewestFirst ? "↑ 最新在顶" : "↓ 最新在底"}
            </button>
            {/* R90: 批量复制当前过滤后的决策。format 与单行复制一致 `[ts] kind reason\n`，
                顺序跟随 decisionsNewestFirst（让粘贴出去的列表与屏幕一致）。空过滤时
                disabled 防止意外复制空串。 */}
            <button
              type="button"
              disabled={filteredDecisions.length === 0}
              onClick={async () => {
                if (filteredDecisions.length === 0) return;
                const ordered = decisionsNewestFirst
                  ? [...filteredDecisions].reverse()
                  : filteredDecisions;
                const text = ordered
                  .map((d) => `[${d.timestamp}] ${d.kind} ${d.reason}`)
                  .join("\n");
                try {
                  await navigator.clipboard.writeText(text);
                  setCopyMsg(`已复制 ${ordered.length} 条`);
                  setTimeout(() => setCopyMsg(""), 2000);
                } catch (err) {
                  setCopyMsg(`复制失败: ${err}`);
                }
              }}
              title={
                filteredDecisions.length === 0
                  ? "当前过滤无命中，无可复制内容"
                  : `把当前过滤后的 ${filteredDecisions.length} 条决策按 [ts] kind reason 多行格式复制到剪贴板`
              }
              style={{
                fontSize: "10px",
                padding: "1px 6px",
                border: "1px solid var(--pet-color-border)",
                borderRadius: 4,
                background:
                  filteredDecisions.length === 0
                    ? "var(--pet-color-bg)"
                    : "var(--pet-color-card)",
                color:
                  filteredDecisions.length === 0
                    ? "var(--pet-color-muted)"
                    : "var(--pet-color-fg)",
                cursor: filteredDecisions.length === 0 ? "default" : "pointer",
                lineHeight: 1.4,
                whiteSpace: "nowrap",
              }}
            >
              📋 复制 {filteredDecisions.length}
            </button>
            <span
              title="当前过滤命中条数 / 决策总数 · ring buffer 容量。后端 CAPACITY=16（src-tauri/src/decision_log.rs）；超出会从最旧丢弃。"
              style={{
                fontSize: 11,
                color: "var(--pet-color-muted)",
                whiteSpace: "nowrap",
                fontFamily: "'SF Mono', 'Menlo', monospace",
              }}
            >
              {filteredDecisions.length} / {decisions.length}
              {/* ring buffer 容量：后端 decision_log::CAPACITY = 16。
                  hardcode 16 同步要保持，drift 时改这里 + 后端常量。 */}
              <span
                style={{
                  marginLeft: 4,
                  color: decisions.length >= 16 ? "var(--pet-tint-yellow-fg)" : "var(--pet-color-muted)",
                }}
              >
                · buffer {decisions.length}/16
              </span>
              {/* R108: 今日累计。buffer 满时附 + 暗示"实际可能更多但被淘汰"。 */}
              <span style={{ marginLeft: 4 }}>
                · 今日 {todayDecisionCount}
                {decisions.length >= 16 && (
                  <span
                    title="ring buffer 已满 16 条，更早的同日决策可能已被淘汰；今日实际触发数可能更大"
                    style={{ marginLeft: 1 }}
                  >
                    +
                  </span>
                )}
              </span>
            </span>
          </div>
          {(() => {
            // filteredDecisions 已 useMemo 计算（含 kind + reason 双过滤），
            // 这里只关心 reverse 渲染序。
            const displayed = decisionsNewestFirst
              ? [...filteredDecisions].reverse()
              : filteredDecisions;
            if (displayed.length === 0) {
              return (
                <div style={{ color: "var(--pet-color-muted)", fontStyle: "italic", fontFamily: "inherit" }}>
                  当前过滤下没有匹配条目。
                </div>
              );
            }
            return displayed.map((d, i) => {
              const isOutcome = d.kind === "Spoke" || d.kind === "LlmSilent" || d.kind === "LlmError";
              const spokeRules = d.kind === "Spoke" ? parseSpokeRules(d.reason) : [];
              return (
                <div
                  key={i}
                  className="pet-decision-row"
                  style={{ display: "flex", gap: "8px", alignItems: "baseline" }}
                >
                  {/* 左侧 3px 色条贯穿整行 — 用同款 kindColor，让纵向
                      skim 时颜色成主信息通道。alignSelf stretch 把它拉到
                      整行高（即便其它 children 是 baseline 对齐）。 */}
                  <span
                    aria-hidden="true"
                    style={{
                      width: 3,
                      flexShrink: 0,
                      background: kindColor(d.kind),
                      borderRadius: 1,
                      alignSelf: "stretch",
                    }}
                  />
                  {(() => {
                    // 跨日识别：ts 是 RFC3339 (`YYYY-MM-DDThh:mm:ss+zz`)，
                    // 取前 10 字符比对 `now.toLocaleDateString('en-CA')`
                    // (输出 `YYYY-MM-DD`)。不同日期 → 在 HH:MM:SS 前加
                    // `M/D ` 提示用户这条不在今天，避免 "为啥 03:14 决策没在
                    // 今早" 的认知偏差。
                    const dPrefix = d.timestamp.slice(0, 10);
                    const today = new Date();
                    const todayPrefix = `${today.getFullYear()}-${String(
                      today.getMonth() + 1,
                    ).padStart(2, "0")}-${String(today.getDate()).padStart(2, "0")}`;
                    const isOtherDay = dPrefix !== todayPrefix;
                    const dayPart = isOtherDay
                      ? `${parseInt(d.timestamp.slice(5, 7), 10)}/${parseInt(
                          d.timestamp.slice(8, 10),
                          10,
                        )} `
                      : "";
                    return (
                      <span
                        onClick={async () => {
                          try {
                            await navigator.clipboard.writeText(d.timestamp);
                            setCopyMsg("已复制 ts");
                            setTimeout(() => setCopyMsg(""), 1500);
                          } catch (err) {
                            setCopyMsg(`复制失败: ${err}`);
                          }
                        }}
                        title={`点击复制完整 timestamp ${d.timestamp} 到剪贴板`}
                        style={{
                          color: isOtherDay ? "var(--pet-tint-yellow-fg)" : "var(--pet-color-muted)",
                          cursor: "pointer",
                        }}
                      >
                        {dayPart}{d.timestamp.slice(11)}
                      </span>
                    );
                  })()}
                  <span style={{ color: kindColor(d.kind), fontWeight: 600, minWidth: "44px" }}>
                    {/* Tree-like connector visually links an outcome row to the Run above it.
                        When filtering to a single kind, the wrapping Run is hidden — └ may
                        still appear which is fine (same kind across rows looks consistent). */}
                    {isOutcome ? "└ " : ""}{d.kind}
                  </span>
                  <span style={{ color: "var(--pet-color-fg)", flex: 1, wordBreak: "break-all" }}>
                    {localizeReason(d.kind, d.reason)}
                    {spokeRules.length > 0 && (
                      <span style={{ display: "inline-flex", gap: 4, marginLeft: 6, flexWrap: "wrap" }}>
                        {spokeRules.map((label) => (
                          <span
                            key={label}
                            title={`prompt 软规则命中：${label}（详细含义见「设置」/「调试」面板的 prompt rules 段）`}
                            style={ruleChipStyle}
                          >
                            {label}
                          </span>
                        ))}
                      </span>
                    )}
                  </span>
                  {/* 单行复制：每条决策都有此入口（与 Spoke/LlmSilent
                      独占的"重跑"互补）；格式 `[ts] kind reason`，原始
                      reason 比 localized 更适合贴 issue / debug 笔记。 */}
                  <button
                    className="pet-decision-copy-btn"
                    onClick={async () => {
                      const text = `[${d.timestamp}] ${d.kind} ${d.reason}`;
                      try {
                        await navigator.clipboard.writeText(text);
                        setCopyMsg("已复制");
                        setTimeout(() => setCopyMsg(""), 1500);
                      } catch (err) {
                        setCopyMsg(`复制失败: ${err}`);
                      }
                    }}
                    title={`复制 \`[${d.timestamp}] ${d.kind} ${d.reason}\` 到剪贴板`}
                    style={{
                      fontSize: 10,
                      padding: "1px 6px",
                      borderRadius: 4,
                      border: "1px solid var(--pet-color-border)",
                      background: "var(--pet-color-card)",
                      color: "var(--pet-color-fg)",
                      cursor: "pointer",
                      flexShrink: 0,
                    }}
                  >
                    复制
                  </button>
                  {(d.kind === "Spoke" || d.kind === "LlmSilent") && (
                    <button
                      onClick={handleTriggerProactive}
                      disabled={triggeringProactive}
                      className="pet-rerun-btn"
                      title="立即用最新 prompt 重跑一次主动开口（与顶部「立即开口」共用 trigger_proactive_turn）"
                      style={{
                        fontSize: 10,
                        padding: "1px 6px",
                        borderRadius: 4,
                        border: "1px solid var(--pet-color-border)",
                        background: triggeringProactive ? "#f1f5f9" : "var(--pet-color-card)",
                        color: triggeringProactive ? "var(--pet-color-muted)" : "var(--pet-color-fg)",
                        cursor: triggeringProactive ? "not-allowed" : "pointer",
                        flexShrink: 0,
                      }}
                    >
                      {triggeringProactive ? "…" : "重跑"}
                    </button>
                  )}
                </div>
              );
            });
          })()}
          </>
          )}
        </div>
      )}

      {/* R142: 三 timeline 切换 tab。speech / tool / feedback 共用一槽，
          点 tab 切换聚焦其中一种。tab 上显条数让用户在切换前预判内容。
          accent 边表 active，灰 border + muted 字表 inactive。 */}
      <div
        style={{
          display: "flex",
          gap: 2,
          padding: "6px 16px 0",
          background: "var(--pet-color-card)",
          borderBottom: "1px solid var(--pet-color-border)",
          flexShrink: 0,
        }}
      >
        {(
          [
            { key: "speech" as const, glyph: "🗯", label: "宠物说", count: recentSpeeches.length },
            { key: "tool" as const, glyph: "🔧", label: "工具调用", count: toolCallHistory.length },
            { key: "feedback" as const, glyph: "💬", label: "反馈记录", count: feedbackHistory.length },
          ]
        ).map(({ key, glyph, label, count }) => {
          const active = activeTimeline === key;
          return (
            <button
              key={key}
              type="button"
              onClick={() => setActiveTimeline(key)}
              style={{
                fontSize: 11.5,
                padding: "5px 10px 6px",
                border: "none",
                borderBottom: active
                  ? "2px solid var(--pet-color-accent)"
                  : "2px solid transparent",
                background: "transparent",
                color: active ? "var(--pet-color-accent)" : "var(--pet-color-muted)",
                fontWeight: active ? 600 : 500,
                cursor: active ? "default" : "pointer",
                fontFamily: "inherit",
              }}
              title={`切到「${label}」timeline`}
            >
              {glyph} {label}
              <span
                style={{
                  fontSize: 10,
                  marginLeft: 4,
                  fontWeight: 400,
                  opacity: 0.75,
                  fontVariantNumeric: "tabular-nums",
                }}
              >
                {count}
              </span>
            </button>
          );
        })}
      </div>

      {/* Pet's recent proactive utterances — sourced from speech_history.log */}
      {activeTimeline === "speech" && recentSpeeches.length > 0 && (
        <div
          style={{
            padding: "8px 16px",
            borderBottom: "1px solid var(--pet-color-border)",
            background: "var(--pet-tint-purple-bg)",
            fontSize: "12px",
            maxHeight: "120px",
            overflowY: "auto",
          }}
        >
          <div style={{ color: "var(--pet-tint-purple-fg)", marginBottom: "4px", fontSize: "12px" }}>
            宠物最近主动说过的 {recentSpeeches.length} 句（最新在底部）
          </div>
          {recentSpeeches.map((line, i) => {
            const idx = line.indexOf(" ");
            const ts = idx > 0 ? line.slice(0, idx) : "";
            const text = idx > 0 ? line.slice(idx + 1) : line;
            const tShort = ts.length >= 16 ? ts.slice(11, 16) : ts;
            return (
              <div key={i} style={{ display: "flex", gap: "8px" }}>
                <span style={{ color: "#a78bfa", fontFamily: "'SF Mono', 'Menlo', monospace", minWidth: "44px" }}>
                  {tShort}
                </span>
                <span style={{ color: "var(--pet-color-fg)", flex: 1, wordBreak: "break-all" }}>{text}</span>
              </div>
            );
          })}
        </div>
      )}

      {/* 空 speech 占位（active tab=speech 但 list 为空时，避免 tab 切过去
          看不到内容像 bug）。 */}
      {activeTimeline === "speech" && recentSpeeches.length === 0 && (
        <div
          style={{
            padding: "12px 16px",
            borderBottom: "1px solid var(--pet-color-border)",
            background: "var(--pet-tint-purple-bg)",
            fontSize: 12,
            color: "var(--pet-tint-purple-fg)",
            fontStyle: "italic",
          }}
        >
          还没有宠物主动开口记录。下一次 proactive 触发后会写入 speech_history.log。
        </div>
      )}

      {/* Iter R4: 工具调用历史 collapsible. Surfaces purpose / risk / review
          status from the tool_call_history ring buffer. Toggled via the
          summary chip; not always-on because in long sessions the list
          would dominate the panel. */}
      {activeTimeline === "tool" && (
      <div
        style={{
          padding: "8px 16px",
          borderBottom: "1px solid var(--pet-color-border)",
          background: "var(--pet-tint-yellow-bg)",
          fontSize: "12px",
        }}
      >
        <div
          onClick={() => setShowToolHistory((s) => !s)}
          style={{
            cursor: "pointer",
            color: "var(--pet-tint-yellow-fg)",
            fontWeight: 600,
            display: "flex",
            justifyContent: "space-between",
          }}
        >
          <span>
            🔧 工具调用历史（{toolCallHistory.length}）
            {toolCallHistory.length > 0 && (
              <span
                style={{
                  fontWeight: 400,
                  marginLeft: 6,
                  fontSize: 11,
                  opacity: 0.75,
                }}
                title="按 timestamp 落本地今日（00:00 起）的工具调用条数"
              >
                · 今日 {todayToolCallCount}
              </span>
            )}
          </span>
          <span>{showToolHistory ? "收起 ▾" : "展开 ▸"}</span>
        </div>
        {showToolHistory && toolCallHistory.length === 0 && (
          <div style={{ color: "var(--pet-tint-yellow-fg)", paddingTop: "6px" }}>
            暂无工具调用记录。reactive chat 期间发起的工具调用会出现在这里。
          </div>
        )}
        {showToolHistory && toolCallHistory.length > 0 && (() => {
          // Iter R39: risk-level filter for tool_call timeline. Third use
          // of the PanelFilterButtonRow pattern; together with R37/R38 it
          // triggered the component extraction.
          const lowCt = toolCallHistory.filter((c) => c.risk_level === "low").length;
          const medCt = toolCallHistory.filter((c) => c.risk_level === "medium").length;
          const highCt = toolCallHistory.filter((c) => c.risk_level === "high").length;
          const filtered =
            toolRiskFilter === "all"
              ? toolCallHistory
              : toolCallHistory.filter((c) => c.risk_level === toolRiskFilter);
          return (
            <>
              <div style={{ display: "flex", alignItems: "center", gap: 8, flexWrap: "wrap" }}>
                <PanelFilterButtonRow<typeof toolRiskFilter>
                  active={toolRiskFilter}
                  onChange={setToolRiskFilter}
                  rowStyle={{ paddingTop: "6px" }}
                  options={[
                    { value: "all", label: "全部", count: toolCallHistory.length, accent: "#475569", title: "显示全部工具调用" },
                    { value: "low", label: "低险", count: lowCt, accent: "var(--pet-tint-green-fg)", title: "只看 low risk_level 调用（read-only / 无副作用）" },
                    { value: "medium", label: "中险", count: medCt, accent: "#d97706", title: "只看 medium risk_level 调用（写本地 / 启动外部）" },
                    { value: "high", label: "高险", count: highCt, accent: "var(--pet-tint-red-fg)", title: "只看 high risk_level 调用（删数据 / 网络外发 / 走 TR3 review）" },
                  ]}
                />
                <button
                  type="button"
                  onClick={() => setToolHistoryGroupByName((v) => !v)}
                  title={
                    toolHistoryGroupByName
                      ? "切回时间线视图（最新在前）"
                      : "按 tool name 分组，看哪个工具调最多"
                  }
                  style={{
                    fontSize: 11,
                    padding: "2px 8px",
                    border: "1px solid",
                    borderColor: toolHistoryGroupByName ? "var(--pet-color-accent)" : "var(--pet-color-border)",
                    borderRadius: 4,
                    background: toolHistoryGroupByName ? "#e0f2fe" : "#fff",
                    color: toolHistoryGroupByName ? "var(--pet-tint-blue-fg)" : "var(--pet-color-muted)",
                    cursor: "pointer",
                    fontWeight: toolHistoryGroupByName ? 600 : 400,
                    marginTop: 6,
                    fontFamily: "inherit",
                  }}
                >
                  {toolHistoryGroupByName ? "📊 按工具分组" : "📜 时间线"}
                </button>
              </div>
              <div style={{ paddingTop: "6px", maxHeight: "260px", overflowY: "auto" }}>
                {filtered.length === 0 && (
                  <div style={{ color: "var(--pet-color-muted)", fontStyle: "italic", padding: "4px 0" }}>
                    当前过滤下没有匹配条目。
                  </div>
                )}
                {toolHistoryGroupByName && filtered.length > 0 && (() => {
                  // 派生 Map<name, calls[]>。BUILTIN_TOOL_NAMES 不一定全在
                  // history 里出现，只对 history 实际见过的 name 分组。按
                  // call count 降序 → 排前是"最高频"，符合"哪个 tool 调
                  // 最多一眼看到"的需求。
                  const groups = new Map<string, ToolCallRecord[]>();
                  for (const c of filtered) {
                    const arr = groups.get(c.name) ?? [];
                    arr.push(c);
                    groups.set(c.name, arr);
                  }
                  const sortedGroups = [...groups.entries()].sort(
                    (a, b) => b[1].length - a[1].length || a[0].localeCompare(b[0]),
                  );
                  return sortedGroups.map(([name, calls]) => {
                    const expanded = toolGroupExpanded.has(name);
                    // 一组的最高风险等级：决定 header 红/橙/绿 chip
                    const highest = calls.some((c) => c.risk_level === "high")
                      ? "high"
                      : calls.some((c) => c.risk_level === "medium")
                        ? "medium"
                        : "low";
                    return (
                      <div
                        key={name}
                        style={{
                          marginBottom: 6,
                          border: "1px solid var(--pet-tint-yellow-fg)",
                          borderRadius: 6,
                          background: "#fffbeb",
                          overflow: "hidden",
                        }}
                      >
                        <div
                          onClick={() => {
                            setToolGroupExpanded((prev) => {
                              const next = new Set(prev);
                              if (next.has(name)) next.delete(name);
                              else next.add(name);
                              return next;
                            });
                          }}
                          style={{
                            display: "flex",
                            alignItems: "center",
                            gap: 6,
                            padding: "6px 10px",
                            cursor: "pointer",
                            userSelect: "none",
                            borderBottom: expanded ? "1px solid var(--pet-tint-yellow-fg)" : "none",
                          }}
                        >
                          <span style={{ width: 10, fontFamily: "monospace", color: "#475569" }}>
                            {expanded ? "▾" : "▸"}
                          </span>
                          <span style={{ fontFamily: "monospace", color: "#1e293b", fontWeight: 600 }}>
                            {name}
                          </span>
                          <span
                            style={{
                              fontSize: 10,
                              padding: "1px 6px",
                              borderRadius: 10,
                              background: riskBadgeBg(highest),
                              color: "#fff",
                              fontWeight: 600,
                            }}
                          >
                            {highest}
                          </span>
                          <span style={{ color: "#475569", fontWeight: 500 }}>
                            × {calls.length}
                          </span>
                          <span
                            style={{
                              marginLeft: "auto",
                              color: "var(--pet-color-muted)",
                              fontFamily: "monospace",
                              fontSize: 10,
                            }}
                            title={`最近一次调用: ${calls[0]?.timestamp ?? "-"}`}
                          >
                            最近 {calls[0]?.timestamp?.slice(11, 16) ?? "—"}
                          </span>
                        </div>
                        {expanded && (
                          <div style={{ padding: "6px 10px" }}>
                            {calls.map((c, j) => (
                              <div
                                key={`${name}-${j}`}
                                style={{
                                  padding: "4px 0",
                                  borderTop: j === 0 ? "none" : "1px dashed var(--pet-tint-yellow-fg)",
                                  fontSize: 12,
                                }}
                              >
                                <div style={{ display: "flex", gap: 6, alignItems: "center", flexWrap: "wrap" }}>
                                  <span
                                    style={{
                                      fontSize: 10,
                                      padding: "1px 6px",
                                      borderRadius: 10,
                                      background: riskBadgeBg(c.risk_level),
                                      color: "#fff",
                                      fontWeight: 600,
                                    }}
                                  >
                                    {c.risk_level}
                                  </span>
                                  <span
                                    style={{
                                      fontSize: 10,
                                      padding: "1px 6px",
                                      borderRadius: 10,
                                      background: reviewStatusBg(c.review_status),
                                      color: "#fff",
                                      fontWeight: 600,
                                    }}
                                  >
                                    {reviewStatusLabel(c.review_status)}
                                  </span>
                                  <span style={{ color: "var(--pet-color-muted)", fontFamily: "monospace", fontSize: 10 }}>
                                    {c.timestamp.slice(11)}
                                  </span>
                                  {c.purpose && (
                                    <span style={{ color: "#1e293b" }}>
                                      {c.purpose.length > 60 ? c.purpose.slice(0, 60) + "…" : c.purpose}
                                    </span>
                                  )}
                                </div>
                              </div>
                            ))}
                          </div>
                        )}
                      </div>
                    );
                  });
                })()}
                {!toolHistoryGroupByName && filtered.map((c, i) => (
              <div
                key={i}
                className="pet-tool-history-row"
                style={{
                  border: "1px solid var(--pet-tint-yellow-fg)",
                  borderRadius: "6px",
                  padding: "6px 10px",
                  marginBottom: "6px",
                  background: "#fffbeb",
                }}
              >
                <div style={{ display: "flex", gap: "6px", flexWrap: "wrap", alignItems: "center" }}>
                  <span style={{ fontFamily: "monospace", color: "#1e293b", fontWeight: 600 }}>
                    {c.name}
                  </span>
                  <span
                    style={{
                      fontSize: "10px",
                      padding: "1px 6px",
                      borderRadius: "10px",
                      background: riskBadgeBg(c.risk_level),
                      color: "#fff",
                      fontWeight: 600,
                    }}
                  >
                    {c.risk_level}
                  </span>
                  <span
                    style={{
                      fontSize: "10px",
                      padding: "1px 6px",
                      borderRadius: "10px",
                      background: reviewStatusBg(c.review_status),
                      color: "#fff",
                      fontWeight: 600,
                    }}
                  >
                    {reviewStatusLabel(c.review_status)}
                  </span>
                  <span style={{ color: "var(--pet-color-muted)", fontFamily: "monospace", fontSize: "10px" }}>
                    {c.timestamp.slice(11)}
                  </span>
                </div>
                {c.purpose && (
                  <div style={{ color: "#1e293b", marginTop: "3px" }}>
                    <strong>用途：</strong>{c.purpose}
                  </div>
                )}
                {c.reasons.length > 0 && (
                  <div style={{ color: "var(--pet-tint-red-fg)", marginTop: "2px", fontSize: "11px" }}>
                    <strong>风险：</strong>{c.reasons.join(" / ")}
                  </div>
                )}
                {c.safe_alternative && (
                  <div style={{ color: "#1e3a8a", marginTop: "2px", fontSize: "11px" }}>
                    <strong>建议替代：</strong>{c.safe_alternative}
                  </div>
                )}
                <details style={{ fontSize: "11px", color: "#475569", marginTop: "3px" }}>
                  <summary style={{ cursor: "pointer" }}>
                    args ({c.args_excerpt.length}) · result ({c.result_excerpt.length})
                  </summary>
                  {/* R128: args / result 各自带小复制按钮，方便贴 LLM 调试上下文 / issue。 */}
                  {(() => {
                    const argsKey = `${i}-args`;
                    const resultKey = `${i}-result`;
                    const smallCopyBtnStyle = (copied: boolean): React.CSSProperties => ({
                      fontSize: 10,
                      padding: "1px 6px",
                      border: "1px solid var(--pet-color-border)",
                      borderRadius: 4,
                      background: "var(--pet-color-card)",
                      color: copied ? "var(--pet-tint-green-fg)" : "var(--pet-color-muted)",
                      cursor: "pointer",
                    });
                    return (
                      <>
                        <div style={{ display: "flex", alignItems: "center", gap: 6, marginTop: 4 }}>
                          <span style={{ fontSize: 10, color: "var(--pet-color-muted)" }}>
                            args
                          </span>
                          <button
                            type="button"
                            onClick={() => void copyExcerpt(argsKey, c.args_excerpt)}
                            style={smallCopyBtnStyle(copiedToolKey === argsKey)}
                            title={
                              copiedToolKey === argsKey
                                ? "已复制 args"
                                : "复制 args 全文到剪贴板"
                            }
                          >
                            {copiedToolKey === argsKey ? "✓ 已复制" : "📋 复制"}
                          </button>
                        </div>
                        <pre style={preStyle}>{c.args_excerpt}</pre>
                        <div style={{ display: "flex", alignItems: "center", gap: 6, marginTop: 4 }}>
                          <span style={{ fontSize: 10, color: "var(--pet-color-muted)" }}>
                            result
                          </span>
                          <button
                            type="button"
                            onClick={() => void copyExcerpt(resultKey, c.result_excerpt)}
                            style={smallCopyBtnStyle(copiedToolKey === resultKey)}
                            title={
                              copiedToolKey === resultKey
                                ? "已复制 result"
                                : "复制 result 全文到剪贴板"
                            }
                          >
                            {copiedToolKey === resultKey ? "✓ 已复制" : "📋 复制"}
                          </button>
                        </div>
                        <pre style={preStyle}>{c.result_excerpt}</pre>
                      </>
                    );
                  })()}
                </details>
              </div>
                ))}
              </div>
            </>
          );
        })()}
      </div>
      )}

      {/* Iter R6: feedback timeline. Surfaces R1's capture data so the user
          can audit what the pet "saw" — whether each prior proactive turn
          was replied to or ignored. Pure data view; the prompt-side hint is
          built from the same log. Default-collapsed; chip shows count + a
          summary ratio of recent replies. */}
      {activeTimeline === "feedback" && (
      <div
        style={{
          padding: "8px 16px",
          borderBottom: "1px solid var(--pet-color-border)",
          background: "var(--pet-tint-green-bg)",
          fontSize: "12px",
        }}
      >
        <div
          onClick={() => setShowFeedbackHistory((s) => !s)}
          style={{
            cursor: "pointer",
            color: "var(--pet-tint-green-fg)",
            fontWeight: 600,
            display: "flex",
            justifyContent: "space-between",
          }}
        >
          <span>
            💬 宠物反馈记录（{feedbackHistory.length}{
              feedbackHistory.length > 0 ? (() => {
                const replied = feedbackHistory.filter((f) => f.kind === "replied").length;
                const dismissed = feedbackHistory.filter((f) => f.kind === "dismissed").length;
                const dismissedSuffix = dismissed > 0 ? ` · 👋${dismissed} 点掉` : "";
                return ` · ${replied}/${feedbackHistory.length} 回复${dismissedSuffix}`;
              })() : ""
            }）
            {feedbackHistory.length > 0 && (
              <span
                style={{
                  fontWeight: 400,
                  marginLeft: 6,
                  fontSize: 11,
                  opacity: 0.75,
                }}
                title="按 timestamp 落本地今日（00:00 起）的反馈条数"
              >
                · 今日 {todayFeedbackCount}
              </span>
            )}
          </span>
          <span>{showFeedbackHistory ? "收起 ▾" : "展开 ▸"}</span>
        </div>
        {showFeedbackHistory && feedbackHistory.length === 0 && (
          <div style={{ color: "var(--pet-tint-green-fg)", paddingTop: "6px" }}>
            暂无反馈记录。proactive 开口被回复 / 忽略后会出现在这里。
          </div>
        )}
        {showFeedbackHistory && feedbackHistory.length > 0 && (() => {
          // R37/R39: filter row uses shared PanelFilterButtonRow component.
          const repliedCt = feedbackHistory.filter((f) => f.kind === "replied").length;
          const likedCt = feedbackHistory.filter((f) => f.kind === "liked").length;
          const ignoredCt = feedbackHistory.filter((f) => f.kind === "ignored").length;
          const dismissedCt = feedbackHistory.filter((f) => f.kind === "dismissed").length;
          const filtered =
            feedbackFilter === "all"
              ? feedbackHistory
              : feedbackHistory.filter((f) => f.kind === feedbackFilter);
          return (
            <>
              <PanelFilterButtonRow<typeof feedbackFilter>
                active={feedbackFilter}
                onChange={setFeedbackFilter}
                rowStyle={{ paddingTop: "6px" }}
                options={[
                  { value: "all", label: "全部", count: feedbackHistory.length, accent: "#475569", title: "显示全部反馈" },
                  { value: "replied", label: "回复", count: repliedCt, accent: "var(--pet-tint-green-fg)", title: "只看用户回复的开口" },
                  { value: "liked", label: "👍 点赞", count: likedCt, accent: "#ec4899", title: "只看用户主动点赞的开口（高质量正向）" },
                  { value: "ignored", label: "忽略", count: ignoredCt, accent: "var(--pet-color-muted)", title: "只看被动忽略的开口" },
                  { value: "dismissed", label: "点掉", count: dismissedCt, accent: "var(--pet-tint-red-fg)", title: "只看 5 秒内主动点掉的开口" },
                ]}
              />
              <div style={{ paddingTop: "6px", maxHeight: "240px", overflowY: "auto" }}>
            {filtered.map((f, i) => (
              <div
                key={i}
                className="pet-feedback-row"
                style={{
                  display: "flex",
                  gap: "8px",
                  alignItems: "center",
                  padding: "4px 0",
                  borderBottom: i === feedbackHistory.length - 1 ? "none" : "1px dashed #d1fae5",
                }}
              >
                <span
                  style={{
                    fontFamily: "'SF Mono', 'Menlo', monospace",
                    color: "var(--pet-tint-green-fg)",
                    fontSize: "10px",
                    minWidth: "44px",
                  }}
                >
                  {f.timestamp.length >= 16 ? f.timestamp.slice(11, 16) : f.timestamp}
                </span>
                <span
                  style={{
                    fontSize: "10px",
                    padding: "1px 8px",
                    borderRadius: "10px",
                    background:
                      f.kind === "replied" ? "var(--pet-tint-green-fg)"
                      : f.kind === "liked" ? "#ec4899"
                      : f.kind === "dismissed" ? "var(--pet-tint-red-fg)"
                      : "var(--pet-color-muted)",
                    color: "#fff",
                    fontWeight: 600,
                    minWidth: "44px",
                    textAlign: "center",
                  }}
                  title={
                    f.kind === "dismissed"
                      ? "用户在 5 秒内主动点掉了气泡（active rejection — 比被动忽略信号更强）"
                      : f.kind === "ignored"
                      ? "用户没有回应，气泡 60 秒自动消失（passive ignore）"
                      : f.kind === "liked"
                      ? "用户给气泡点了 👍（显式正向反馈，比 replied 更高质量的「我喜欢」信号）"
                      : "用户回复了这次开口"
                  }
                >
                  {f.kind === "replied"
                    ? "回复"
                    : f.kind === "liked"
                    ? "👍 点赞"
                    : f.kind === "dismissed"
                    ? "点掉"
                    : "忽略"}
                </span>
                <span style={{ color: "var(--pet-color-fg)", flex: 1, wordBreak: "break-all" }}>
                  {f.excerpt}
                </span>
              </div>
            ))}
            {filtered.length === 0 && (
              <div style={{ color: "var(--pet-color-muted)", fontStyle: "italic", padding: "4px 0" }}>
                当前过滤下没有匹配条目。
              </div>
            )}
              </div>
            </>
          );
        })()}
      </div>
      )}

      {/* Pending user-set reminders — sourced from todo memory category */}
      {reminders.length > 0 && (
        <div
          style={{
            padding: "8px 16px",
            borderBottom: "1px solid var(--pet-color-border)",
            background: "var(--pet-tint-orange-bg)",
            fontSize: "12px",
            maxHeight: "120px",
            overflowY: "auto",
          }}
        >
          <div style={{ color: "var(--pet-tint-orange-fg)", marginBottom: "4px", fontSize: "12px" }}>
            待提醒事项 {reminders.length} 条（橙色 = 已到时间窗口）
          </div>
          {reminders.map((r, i) => (
            <div key={i} style={{ display: "flex", gap: "8px" }}>
              <span
                style={{
                  color: r.due_now ? "#ea580c" : "var(--pet-tint-yellow-fg)",
                  fontFamily: "'SF Mono', 'Menlo', monospace",
                  fontWeight: r.due_now ? 600 : 400,
                  minWidth: "44px",
                }}
              >
                {r.time}
              </span>
              <span style={{ color: "var(--pet-color-fg)", flex: 1, wordBreak: "break-all" }}>
                {r.topic}
                <span style={{ color: "var(--pet-color-muted)", marginLeft: "6px", fontSize: "11px" }}>
                  ({r.title})
                </span>
              </span>
            </div>
          ))}
        </div>
      )}

      {/* 日志窗已抽到独立「日志」tab（PanelDebugLogs）。这里只留 stats /
          chips / 模态层 / 工具栏，让"应用"tab 不再既显状态又显大段黑底
          日志，分流后两个 tab 各司其职。 */}

      {/* 工具风险表 inline 调整：每行 3-chip toggle 直接 set_tool_review_mode
          写盘 → 下次 chat 调 get_settings 自动读到新值，不必去 PanelSettings
          整表保存。default 折叠避免长列表撑 panel。 */}
      <div style={{ marginTop: 20, padding: "14px 16px", background: "var(--pet-color-card)", border: "1px solid var(--pet-color-border)", borderRadius: 10 }}>
        <div
          onClick={() => setToolRiskExpanded((v) => !v)}
          style={{
            display: "flex",
            alignItems: "center",
            gap: 8,
            cursor: "pointer",
            userSelect: "none",
          }}
          title="每个内置工具的基线风险 + 当前用户偏好（auto / 强制审核 / 强制放行）。点 chip 直改 settings 立即生效；不必去 PanelSettings 改整表。"
        >
          <span style={{ width: 10, fontFamily: "monospace", color: "var(--pet-color-muted)" }}>
            {toolRiskExpanded ? "▾" : "▸"}
          </span>
          <span style={{ fontSize: 13.5, fontWeight: 600, color: "var(--pet-color-fg)", letterSpacing: 0.2 }}>
            🛡 工具风险表
          </span>
          <span style={{ fontSize: 11, color: "var(--pet-color-muted)" }}>
            ({toolRiskRows.length} 个工具 · 点 chip 改完立刻生效)
          </span>
          {toolRiskMsg && (
            <span style={{ marginLeft: "auto", fontSize: 11, color: toolRiskMsg.startsWith("改失败") ? "var(--pet-tint-red-fg)" : "var(--pet-tint-green-fg)" }}>
              {toolRiskMsg}
            </span>
          )}
        </div>
        {toolRiskExpanded && (
          <div style={{ marginTop: 12, display: "flex", flexDirection: "column", gap: 6 }}>
            {toolRiskRows.map((row) => {
              const busy = toolRiskBusyName === row.name;
              const levelBg =
                row.level === "high" ? "var(--pet-tint-red-fg)" : row.level === "medium" ? "#f59e0b" : "var(--pet-color-muted)";
              return (
                <div
                  key={row.name}
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: 8,
                    padding: "8px 10px",
                    border: "1px solid var(--pet-color-border)",
                    borderRadius: 8,
                    background: "var(--pet-color-bg)",
                    opacity: busy ? 0.6 : 1,
                  }}
                >
                  <span
                    style={{
                      fontSize: 10,
                      fontWeight: 700,
                      color: "#fff",
                      background: levelBg,
                      padding: "1px 6px",
                      borderRadius: 4,
                      textTransform: "uppercase",
                      flexShrink: 0,
                      minWidth: 50,
                      textAlign: "center",
                    }}
                  >
                    {row.level}
                  </span>
                  <div style={{ flex: 1, minWidth: 0 }}>
                    <div style={{ fontSize: 12, fontWeight: 600, color: "var(--pet-color-fg)", fontFamily: "'SF Mono', 'Menlo', monospace" }}>
                      {row.name}
                    </div>
                    <div style={{ fontSize: 11, color: "var(--pet-color-muted)", marginTop: 2 }}>
                      {row.note}
                    </div>
                  </div>
                  <div style={{ display: "flex", gap: 2, flexShrink: 0 }}>
                    {(
                      [
                        { mode: "auto", label: "自动", title: "跟着分类器走（默认）" },
                        { mode: "always_review", label: "审核", title: "强制走 panel 审核（保险）" },
                        { mode: "always_approve", label: "放行", title: "直接放行（关掉打扰）" },
                      ] as const
                    ).map(({ mode, label, title }) => {
                      const active = row.mode === mode;
                      return (
                        <button
                          key={mode}
                          type="button"
                          disabled={busy || active}
                          onClick={() => void handleSetToolReviewMode(row.name, mode)}
                          title={title}
                          style={{
                            fontSize: 11,
                            padding: "3px 9px",
                            border: "1px solid",
                            borderColor: active ? "var(--pet-color-accent)" : "var(--pet-color-border)",
                            borderRadius: 4,
                            background: active ? "var(--pet-color-accent)" : "var(--pet-color-card)",
                            color: active ? "#fff" : "var(--pet-color-fg)",
                            cursor: active || busy ? "default" : "pointer",
                            fontWeight: active ? 600 : 400,
                          }}
                        >
                          {label}
                        </button>
                      );
                    })}
                  </div>
                </div>
              );
            })}
            {toolRiskRows.length === 0 && (
              <div style={{ padding: "12px", textAlign: "center", color: "var(--pet-color-muted)", fontSize: 12 }}>
                加载中…
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}


// Iter R4: tool-call history badge palette. Match the backend's risk levels
// (`low` / `medium` / `high`) and review status enum strings.
function riskBadgeBg(level: string): string {
  switch (level) {
    case "high":
      return "var(--pet-tint-red-fg)";
    case "medium":
      return "#f59e0b";
    case "low":
      return "var(--pet-tint-green-fg)";
    default:
      return "var(--pet-color-muted)";
  }
}

function reviewStatusBg(status: string): string {
  switch (status) {
    case "approved":
      return "var(--pet-color-accent)";
    case "denied":
      return "var(--pet-tint-red-fg)";
    case "timeout":
      return "#f97316";
    case "missing_purpose":
      return "#6b21a8";
    case "not_required":
    default:
      return "var(--pet-color-muted)";
  }
}

function reviewStatusLabel(status: string): string {
  switch (status) {
    case "approved":
      return "已允许";
    case "denied":
      return "被拒绝";
    case "timeout":
      return "超时拒绝";
    case "missing_purpose":
      return "缺 purpose";
    case "not_required":
    default:
      return "无需审核";
  }
}

const preStyle: React.CSSProperties = {
  whiteSpace: "pre-wrap",
  wordBreak: "break-all",
  background: "#f8fafc",
  padding: "4px 6px",
  borderRadius: "3px",
  marginTop: "3px",
  fontFamily: "monospace",
  fontSize: "10px",
  maxHeight: "120px",
  overflowY: "auto",
};

function kindColor(kind: string): string {
  switch (kind) {
    case "Run":
      return "#22c55e";
    case "Spoke":
      return "var(--pet-tint-green-fg)";
    case "LlmSilent":
      return "#a855f7";
    case "LlmError":
      return "var(--pet-tint-red-fg)";
    case "Skip":
      return "#f59e0b";
    case "Silent":
      return "var(--pet-color-muted)";
    // Iter R2: tool-review outcomes share the timeline with proactive decisions.
    case "ToolReviewApprove":
      return "var(--pet-color-accent)";
    case "ToolReviewDeny":
      return "var(--pet-tint-red-fg)";
    case "ToolReviewTimeout":
      return "#f97316";
    default:
      return "#475569";
  }
}

/**
 * Translate the backend's reason string to user-friendly Chinese for the panel.
 *
 * - Silent reasons are stable enum keys, mapped one-to-one.
 * - Skip reasons start with "Proactive: skip — " plumbing noise; we strip it and
 *   translate a few known phrasings while preserving any dynamic numbers.
 * - Run reasons are already structured (e.g. "idle=900s, input_idle=120") — pass through.
 *
 * Falls back to the original string for anything we don't recognize, so a future backend
 * change degrades to English-passthrough rather than blanking the row.
 */
/// 从 Spoke 决策的 reason csv 里提取 `rules=A+B+C` 的标签数组。空 / 缺失返回
/// `[]`。后端 `record_proactive_outcome` 只在 active_prompt_rules 非空时 push
/// `rules=…`，标签名按约定不含 `+`，所以 split("+") 在内部不冲突。
function parseSpokeRules(reason: string): string[] {
  const parts = reason.split(", ");
  const rulesPart = parts.find((p) => p.startsWith("rules="));
  if (!rulesPart) return [];
  const value = rulesPart.slice("rules=".length).trim();
  if (value.length === 0) return [];
  return value
    .split("+")
    .map((r) => r.trim())
    .filter((r) => r.length > 0);
}

function localizeReason(kind: string, reason: string): string {
  if (kind === "Silent") {
    switch (reason) {
      case "disabled":
        return "已禁用 (proactive.enabled = false)";
      case "quiet_hours":
        return "安静时段内";
      case "idle_below_threshold":
        return "用户活跃时间未到阈值";
      default:
        return reason;
    }
  }
  if (kind === "LlmSilent") {
    // "-" means no soft tags applied; the LLM chose silence on its own judgement.
    return reason === "-" ? "LLM 自主选择沉默" : `LLM 沉默（${reason}）`;
  }
  if (kind === "Spoke") {
    // reason is a comma-separated tag bundle; "-" alone means "no tags". Strip a leading
    // "-, " left over from chatty_part so the displayed body starts with real content.
    if (reason === "-") return "宠物开口";
    const cleaned = reason.replace(/^-, /, "");
    return `宠物开口（${cleaned}）`;
  }
  if (kind === "LlmError") {
    return `LLM 调用失败：${reason}`;
  }
  if (kind === "Skip") {
    const stripped = reason.replace(/^Proactive: skip\s*—\s*/, "");
    if (stripped.startsWith("awaiting user reply")) {
      return "等待用户回复上一条主动消息";
    }
    if (stripped.startsWith("cooldown")) {
      // "cooldown (60s < 1800s)" → "冷却中 (60s < 1800s)"
      return stripped.replace(/^cooldown/, "冷却中");
    }
    if (stripped.startsWith("user active")) {
      return stripped.replace(/^user active/, "用户活跃中");
    }
    if (stripped.startsWith("macOS Focus")) {
      return "macOS Focus / 勿扰已开启";
    }
    return stripped;
  }
  // Iter R2: tool-review timeline entries — reason is "{review_id} {tool_name}".
  if (kind === "ToolReviewApprove") {
    return `用户允许了高风险工具调用（${reason}）`;
  }
  if (kind === "ToolReviewDeny") {
    return `用户拒绝了高风险工具调用（${reason}）`;
  }
  if (kind === "ToolReviewTimeout") {
    return `60秒未审核，按默认策略拒绝（${reason}）`;
  }
  return reason;
}

/// 调试器：args 与 result 通常是 JSON 字符串（LLM 给的 args 一定是 JSON；
/// 工具返回多数也 JSON-like）。能 parse 就 pretty-print 两空格缩进；不能就
/// 原样返回（保留所有空白与控制字符的可视性）。
function prettyPrintIfJson(s: string): string {
  if (!s) return "";
  try {
    const parsed = JSON.parse(s);
    return JSON.stringify(parsed, null, 2);
  } catch {
    return s;
  }
}

/// proactive 调试器 modal header 的 prompt 字数 token-pressure 阈值。中文
/// ~3 char/token，8000 char ≈ 2700 tokens，约 16K context 的 half；超过即标
/// 红 + tooltip 提示如何收紧 prompt。粗 proxy 不引入 tokenizer 依赖。
const PROMPT_PRESSURE_CHARS = 8000;

/// 共享的空 Set 字面量 —— 给 `expandedToolCallByTs.get(...) ?? EMPTY_INDEX_SET`
/// 用，避免每次渲染都 new Set() 让 .has(j) 路径产生不必要的对象。
const EMPTY_INDEX_SET: ReadonlySet<number> = new Set();

/// 决策日志 Spoke 行的 prompt-rule chip。紫色与既有 mood-tag / pri-badge 配色
/// 错开（这里是"软规则命中"性质，独立色族让用户一眼辨别）。padding 与圆角
/// 都偏小，行内 chip 不应主导视觉。
const ruleChipStyle: React.CSSProperties = {
  display: "inline-block",
  background: "#ddd6fe",
  color: "#5b21b6",
  fontSize: "10px",
  fontWeight: 600,
  padding: "0 6px",
  borderRadius: "8px",
  lineHeight: "16px",
  whiteSpace: "nowrap",
  fontFamily: "'SF Mono', 'Menlo', monospace",
};

const toolBtnStyle: React.CSSProperties = {
  padding: "6px 14px",
  borderRadius: "6px",
  border: "1px solid var(--pet-color-border)",
  background: "var(--pet-color-card)",
  color: "var(--pet-color-fg)",
  fontSize: "13px",
  cursor: "pointer",
  fontWeight: 500,
};
