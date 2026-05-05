import { useState, useEffect, useMemo, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
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
  RedactionStats,
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

type LogLevel = "ERROR" | "WARN" | "INFO";

export function PanelDebug() {
  const [logs, setLogs] = useState<string[]>([]);
  // R99: 日志区按 level 多选过滤。Set 空 = "全部"（与决策日志 R83 多选语义
  // 一致）；勾选 ERROR / WARN 让 INFO 噪音不淹没问题信号。
  const [logLevels, setLogLevels] = useState<Set<LogLevel>>(() => new Set());
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
  const [lifetimeSpeechCount, setLifetimeSpeechCount] = useState<number>(0);
  const [todaySpeechCount, setTodaySpeechCount] = useState<number>(0);
  const [weekSpeechCount, setWeekSpeechCount] = useState<number>(0);
  const [companionshipDays, setCompanionshipDays] = useState<number>(0);
  const [redactionStats, setRedactionStats] = useState<RedactionStats>({ calls: 0, hits: 0 });
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

  // R99: 日志 level 计数 + 过滤。`includes("ERROR")` 是简化的检测：rust
  // env_logger 输出格式 `[YYYY-... ERROR pet::xxx]`，"ERROR" 在每行 level
  // 段唯一出现，substring 命中无歧义。WARN 同理；其它 → INFO（含 DEBUG /
  // TRACE 等更低 level）。
  const logLevelCounts = useMemo(() => {
    let err = 0;
    let warn = 0;
    let info = 0;
    for (const line of logs) {
      if (line.includes("ERROR")) err++;
      else if (line.includes("WARN")) warn++;
      else info++;
    }
    return { ERROR: err, WARN: warn, INFO: info };
  }, [logs]);
  const filteredLogs = useMemo(() => {
    if (logLevels.size === 0) return logs;
    return logs.filter((line) => {
      const lvl: LogLevel = line.includes("ERROR")
        ? "ERROR"
        : line.includes("WARN")
          ? "WARN"
          : "INFO";
      return logLevels.has(lvl);
    });
  }, [logs, logLevels]);
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
  // R128: 工具调用历史 args/result 块复制反馈。key = `${index}-args` /
  // `${index}-result`，1.5s 自清空让 ✓ 反馈短暂可见。多个按钮共用一个 state，
  // 同时只一个处于"已复制"态（用户连点两个时后者覆盖前者，符合直觉）。
  const [copiedToolKey, setCopiedToolKey] = useState<string | null>(null);
  const [showPromptHints, setShowPromptHints] = useState(false);
  const [proactiveStatus, setProactiveStatus] = useState<string>("");
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
  const currentTurn = recentTurns[turnIndex] ?? null;
  const lastPrompt = currentTurn?.prompt ?? "";
  const lastReply = currentTurn?.reply ?? "";
  const lastToolCalls = currentTurn?.tool_calls ?? [];
  const currentTurnTs = currentTurn?.timestamp ?? "";
  const expandedToolCallSet = expandedToolCallByTs.get(currentTurnTs) ?? EMPTY_INDEX_SET;
  const lastTurnMeta = {
    timestamp: currentTurn?.timestamp ?? "",
    tools_used: currentTurn?.tools_used ?? [],
  };
  const scrollRef = useRef<HTMLDivElement>(null);
  // R139: 日志区"跟随最新"模式。默认 true（与原行为一致）；用户向上滚
  // → onScroll 自动设 false；点回 true 立即滚到底；脱离时 useEffect 不
  // 再 scroll 让阅读旧 log 不被新 log 拽下。
  const [followTail, setFollowTail] = useState(true);
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
        redaction_stats: RedactionStats;
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
      setLogs(snap.logs);
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
      setRedactionStats(snap.redaction_stats);
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

  useEffect(() => {
    fetchLogs();
    intervalRef.current = setInterval(fetchLogs, 1000);
    return () => {
      if (intervalRef.current) clearInterval(intervalRef.current);
    };
  }, []);

  // Auto-scroll. R139: 仅 followTail=true 时自动滚到底；用户向上滚阅读
  // 旧 log 时 (followTail=false) 不动视口。deps 加 followTail 让用户
  // 切回 true 时立即跳到底（无需等下次 logs 更新）。
  useEffect(() => {
    if (followTail && scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [logs, followTail]);

  const handleClear = async () => {
    await invoke("clear_logs");
    setLogs([]);
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

  const handleResetRedactionStats = async () => {
    await invoke("reset_redaction_stats");
    setRedactionStats({ calls: 0, hits: 0 });
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
      setTriggeringProactive(false);
      // Auto-clear after a few seconds so the toolbar doesn't stick on a stale message.
      setTimeout(() => setProactiveStatus(""), 8000);
    }
  };

  const handleOpenDevTools = async () => {
    try {
      // Open devtools for the current webview
      const win = getCurrentWindow();
      await (win as any).emit("open-devtools");
      // Use internal API
      await invoke("plugin:webview|internal_toggle_devtools", {});
    } catch {
      // Fallback: try the webview API directly
      try {
        await (getCurrentWindow() as any).openDevtools();
      } catch (e) {
        console.error("Cannot open devtools:", e);
        alert("无法打开 DevTools。请使用右键菜单 → Inspect Element。");
      }
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
            <div style={{ fontSize: "13px", color: "#dc2626", fontWeight: 700, marginBottom: "10px" }}>
              ⚠ 高风险工具调用待审核（{pendingReviews.length}）
            </div>
            {pendingReviews.map((r) => (
              <div
                key={r.review_id}
                style={{
                  border: "1px solid #f3d7d7",
                  borderRadius: "8px",
                  padding: "12px 14px",
                  marginBottom: "10px",
                  background: "#fffafa",
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
                <div style={{ fontSize: "11px", color: "#7c2d12", marginBottom: "6px" }}>
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
                      background: "#16a34a",
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
                      background: "#dc2626",
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
              <div style={{ fontSize: "11px", color: "#dc2626", marginTop: "6px" }}>
                {reviewError}
              </div>
            )}
            <div style={{ fontSize: "10px", color: "var(--pet-color-muted)", marginTop: "4px" }}>
              超过 60 秒未响应将按默认安全策略拒绝。
            </div>
          </div>
        </div>
      )}

      {/* Iter E1: modal showing the last-built proactive prompt verbatim. Triggered
          by the "看上次 prompt" toolbar button; click backdrop to close. */}
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
              {/* Iter E4: prev/next navigator across the ring buffer */}
              {recentTurns.length > 0 ? (
                <span
                  style={{ display: "inline-flex", alignItems: "center", gap: "4px" }}
                  title="« 上一条（更早）/ » 下一条（更新）。Iter E4 ring buffer 保留最近 5 次"
                >
                  <button
                    onClick={() => {
                      // 切 turn 不再清空展开状态 —— 折叠记忆 per-turn 持久化
                      setTurnIndex((i) => Math.min(i + 1, recentTurns.length - 1));
                    }}
                    disabled={turnIndex >= recentTurns.length - 1}
                    style={{
                      fontSize: "11px",
                      padding: "1px 6px",
                      borderRadius: "4px",
                      border: "1px solid var(--pet-color-border)",
                      background: turnIndex >= recentTurns.length - 1 ? "#f1f5f9" : "var(--pet-color-card)",
                      color: turnIndex >= recentTurns.length - 1 ? "#cbd5e1" : "var(--pet-color-fg)",
                      cursor: turnIndex >= recentTurns.length - 1 ? "default" : "pointer",
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
                    {turnIndex + 1}/{recentTurns.length}
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
                      color: promptOver ? "#dc2626" : "var(--pet-color-muted)",
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
                    background: currentTurn.outcome === "spoke" ? "#16a34a" : "#94a3b8",
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
              <button
                onClick={() => setShowLastPrompt(false)}
                style={{
                  marginLeft: "auto",
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
                    <span style={{ fontSize: "10px", color: "#94a3b8" }}>
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
                            border: "1px solid #fde68a",
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
                                  borderTop: "1px solid #fde68a",
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
                                  borderTop: "1px solid #fde68a",
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
                <span style={{ fontSize: "10px", color: "#94a3b8" }}>
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
        redactionStats={redactionStats}
        tone={tone}
        showPromptHints={showPromptHints}
        setShowPromptHints={setShowPromptHints}
        onResetCache={handleResetCacheStats}
        onResetMoodTag={handleResetMoodTagStats}
        onResetLlmOutcome={handleResetLlmOutcomeStats}
        onResetEnvTool={handleResetEnvToolStats}
        onResetPromptTilt={handleResetPromptTiltStats}
        onResetRedaction={handleResetRedactionStats}
        logsCount={logs.length}
      />

      {/* Toolbar */}
      <div style={{ display: "flex", gap: "8px", padding: "12px 16px", borderBottom: "1px solid var(--pet-color-border)", background: "var(--pet-color-card)" }}>
        <button onClick={fetchLogs} style={toolBtnStyle}>刷新</button>
        <button onClick={handleClear} style={toolBtnStyle}>清空</button>
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
              ? "#94a3b8"
              : triggerArmed
                ? "#fef2f2"
                : "#10b981",
            color: triggeringProactive ? "#fff" : triggerArmed ? "#b91c1c" : "#fff",
            borderColor: triggerArmed ? "#dc2626" : undefined,
            fontWeight: triggerArmed ? 600 : undefined,
          }}
        >
          {triggeringProactive
            ? "开口中…"
            : triggerArmed
              ? "再点确认 (3s)"
              : "立即开口"}
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
        {proactiveStatus && (
          <span
            style={{
              fontSize: "12px",
              color: proactiveStatus.startsWith("触发失败") ? "#dc2626" : "#059669",
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
            const natureColor = desc ? NATURE_META[desc.nature].color : "#94a3b8";
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
                    color: "#7c2d12",
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
                { value: "Spoke", label: "开口", accent: "#16a34a", title: "LLM 选择开口的轮次" },
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
                  color: decisions.length >= 16 ? "#a16207" : "var(--pet-color-muted)",
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
                          color: isOtherDay ? "#a16207" : "var(--pet-color-muted)",
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
                        color: triggeringProactive ? "#94a3b8" : "var(--pet-color-fg)",
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

      {/* Pet's recent proactive utterances — sourced from speech_history.log */}
      {recentSpeeches.length > 0 && (
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

      {/* Iter R4: 工具调用历史 collapsible. Surfaces purpose / risk / review
          status from the tool_call_history ring buffer. Toggled via the
          summary chip; not always-on because in long sessions the list
          would dominate the panel. */}
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
              <PanelFilterButtonRow<typeof toolRiskFilter>
                active={toolRiskFilter}
                onChange={setToolRiskFilter}
                rowStyle={{ paddingTop: "6px" }}
                options={[
                  { value: "all", label: "全部", count: toolCallHistory.length, accent: "#475569", title: "显示全部工具调用" },
                  { value: "low", label: "低险", count: lowCt, accent: "#16a34a", title: "只看 low risk_level 调用（read-only / 无副作用）" },
                  { value: "medium", label: "中险", count: medCt, accent: "#d97706", title: "只看 medium risk_level 调用（写本地 / 启动外部）" },
                  { value: "high", label: "高险", count: highCt, accent: "#dc2626", title: "只看 high risk_level 调用（删数据 / 网络外发 / 走 TR3 review）" },
                ]}
              />
              <div style={{ paddingTop: "6px", maxHeight: "260px", overflowY: "auto" }}>
                {filtered.length === 0 && (
                  <div style={{ color: "#94a3b8", fontStyle: "italic", padding: "4px 0" }}>
                    当前过滤下没有匹配条目。
                  </div>
                )}
                {filtered.map((c, i) => (
              <div
                key={i}
                className="pet-tool-history-row"
                style={{
                  border: "1px solid #fde68a",
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
                  <span style={{ color: "#94a3b8", fontFamily: "monospace", fontSize: "10px" }}>
                    {c.timestamp.slice(11)}
                  </span>
                </div>
                {c.purpose && (
                  <div style={{ color: "#1e293b", marginTop: "3px" }}>
                    <strong>用途：</strong>{c.purpose}
                  </div>
                )}
                {c.reasons.length > 0 && (
                  <div style={{ color: "#7c2d12", marginTop: "2px", fontSize: "11px" }}>
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
                      color: copied ? "#16a34a" : "var(--pet-color-muted)",
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

      {/* Iter R6: feedback timeline. Surfaces R1's capture data so the user
          can audit what the pet "saw" — whether each prior proactive turn
          was replied to or ignored. Pure data view; the prompt-side hint is
          built from the same log. Default-collapsed; chip shows count + a
          summary ratio of recent replies. */}
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
                  { value: "replied", label: "回复", count: repliedCt, accent: "#16a34a", title: "只看用户回复的开口" },
                  { value: "liked", label: "👍 点赞", count: likedCt, accent: "#ec4899", title: "只看用户主动点赞的开口（高质量正向）" },
                  { value: "ignored", label: "忽略", count: ignoredCt, accent: "#94a3b8", title: "只看被动忽略的开口" },
                  { value: "dismissed", label: "点掉", count: dismissedCt, accent: "#dc2626", title: "只看 5 秒内主动点掉的开口" },
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
                      f.kind === "replied" ? "#16a34a"
                      : f.kind === "liked" ? "#ec4899"
                      : f.kind === "dismissed" ? "#dc2626"
                      : "#94a3b8",
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
                  color: r.due_now ? "#ea580c" : "#a16207",
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

      {/* R99: 日志 level chip 行。accent 配色与日志体内 ERROR 红 / WARN 黄 /
          INFO 灰对应，让"chip 颜色 ↔ 日志正文颜色"形成视觉锚定。 */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 6,
          padding: "6px 16px",
          borderBottom: "1px solid var(--pet-color-border)",
          background: "var(--pet-color-bg)",
          flexWrap: "wrap",
        }}
      >
        <span style={{ fontSize: 10, color: "var(--pet-color-muted)" }}>
          level:
        </span>
        <button
          type="button"
          onClick={() => setLogLevels(new Set())}
          style={multiSelectChipStyle(logLevels.size === 0, "#475569")}
          title="显示全部级别。点击清空多选过滤。"
        >
          全部 {logs.length}
        </button>
        {(["ERROR", "WARN", "INFO"] as const).map((lvl) => {
          const accent =
            lvl === "ERROR" ? "#dc2626" : lvl === "WARN" ? "#f59e0b" : "#475569";
          const active = logLevels.has(lvl);
          return (
            <button
              key={lvl}
              type="button"
              onClick={() => {
                setLogLevels((prev) => {
                  const next = new Set(prev);
                  if (next.has(lvl)) next.delete(lvl);
                  else next.add(lvl);
                  return next;
                });
              }}
              style={multiSelectChipStyle(active, accent)}
              title={
                active
                  ? `再次点击移出过滤集合（当前: ${lvl}）`
                  : `加入到只看的 level 集合（多选）：${lvl}`
              }
            >
              {lvl} {logLevelCounts[lvl]}
            </button>
          );
        })}
        {logLevels.size > 0 && (
          <span
            style={{
              fontSize: 10,
              color: "var(--pet-color-muted)",
              marginLeft: "auto",
              fontFamily: "'SF Mono', 'Menlo', monospace",
            }}
          >
            显示 {filteredLogs.length} / {logs.length}
          </span>
        )}
        {/* R139: follow-tail toggle。always 显（off 时让用户清楚当前不
            follow，不会被新 log 拽下视口）。点击 → 立即滚到底 + setFollowTail(true)。
            marginLeft 在 stats 显示时是 8（紧贴 stats 右），stats 不显时是
            auto（自己推到行末）。 */}
        <button
          type="button"
          onClick={() => {
            setFollowTail(true);
            const el = scrollRef.current;
            if (el) el.scrollTop = el.scrollHeight;
          }}
          title={
            followTail
              ? "当前跟随最新日志。向上滚读旧 log 时自动脱离。"
              : "已脱离最新（向上滚读旧 log 触发）。点击重新跟随 + 滚到底。"
          }
          style={{
            fontSize: "10px",
            padding: "1px 6px",
            border: "1px solid var(--pet-color-border)",
            borderRadius: 4,
            background: followTail ? "var(--pet-color-card)" : "var(--pet-color-bg)",
            color: followTail ? "var(--pet-color-fg)" : "var(--pet-color-muted)",
            cursor: "pointer",
            marginLeft: logLevels.size > 0 ? 8 : "auto",
            whiteSpace: "nowrap",
          }}
        >
          {followTail ? "📌 跟随最新" : "📌 已脱离"}
        </button>
      </div>

      {/* Log output */}
      <div
        ref={scrollRef}
        onScroll={() => {
          // R139: 检测用户离底是否 > 阈值；阈值 8px 给浮点偏差 buffer。
          // 程序设 scrollTop=scrollHeight 时也会触发本回调，distFromBottom=0
          // → setFollowTail(true) 与目标一致。
          const el = scrollRef.current;
          if (!el) return;
          const distFromBottom = el.scrollHeight - el.scrollTop - el.clientHeight;
          setFollowTail(distFromBottom <= 8);
        }}
        style={{
          flex: 1,
          overflowY: "auto",
          padding: "12px 16px",
          fontFamily: "'SF Mono', 'Menlo', 'Monaco', monospace",
          fontSize: "12px",
          lineHeight: "1.7",
          background: "#0f172a",
          color: "#e2e8f0",
        }}
      >
        {filteredLogs.length === 0 ? (
          <div style={{ color: "#64748b", textAlign: "center", marginTop: "40px" }}>
            {logs.length === 0
              ? "暂无日志。聊天和操作会产生日志。"
              : "当前 level 过滤无匹配日志"}
          </div>
        ) : (
          filteredLogs.map((line, i) => (
            <div key={i} style={{ wordBreak: "break-all" }}>
              <span style={{ color: "#94a3b8" }}>{line.slice(0, 14)}</span>
              <span style={{ color: line.includes("ERROR") ? "#f87171" : line.includes("WARN") ? "#fbbf24" : "#e2e8f0" }}>
                {line.slice(14)}
              </span>
            </div>
          ))
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
      return "#dc2626";
    case "medium":
      return "#f59e0b";
    case "low":
      return "#16a34a";
    default:
      return "#94a3b8";
  }
}

function reviewStatusBg(status: string): string {
  switch (status) {
    case "approved":
      return "#0ea5e9";
    case "denied":
      return "#dc2626";
    case "timeout":
      return "#f97316";
    case "missing_purpose":
      return "#6b21a8";
    case "not_required":
    default:
      return "#64748b";
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
      return "#16a34a";
    case "LlmSilent":
      return "#a855f7";
    case "LlmError":
      return "#dc2626";
    case "Skip":
      return "#f59e0b";
    case "Silent":
      return "#94a3b8";
    // Iter R2: tool-review outcomes share the timeline with proactive decisions.
    case "ToolReviewApprove":
      return "#0ea5e9";
    case "ToolReviewDeny":
      return "#dc2626";
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
