import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { PanelChipStrip } from "./PanelChipStrip";
import { PanelStatsCard } from "./PanelStatsCard";
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

export function PanelDebug() {
  const [logs, setLogs] = useState<string[]>([]);
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
  // Iter R6: feedback timeline (replied / ignored / dismissed). Surfaces
  // R1's capture data so the user can see whether the pet is "learning"
  // from outcomes. R1c added "dismissed" — user clicked the bubble within
  // 5s, distinct from passive ignore.
  type FeedbackEntry = {
    timestamp: string;
    kind: "replied" | "ignored" | "dismissed";
    excerpt: string;
  };
  const [feedbackHistory, setFeedbackHistory] = useState<FeedbackEntry[]>([]);
  const [showFeedbackHistory, setShowFeedbackHistory] = useState(false);
  const [triggeringProactive, setTriggeringProactive] = useState(false);
  const [showPromptHints, setShowPromptHints] = useState(false);
  const [proactiveStatus, setProactiveStatus] = useState<string>("");
  // Iter E4: ring buffer of recent turns, newest first. Panel modal navigates
  // with « / » buttons; index 0 = newest. Replaces E1/E2/E3's three separate
  // fetches with a single Vec<TurnRecord> source.
  const [recentTurns, setRecentTurns] = useState<
    { timestamp: string; prompt: string; reply: string; tools_used: string[]; outcome?: string }[]
  >([]);
  const [turnIndex, setTurnIndex] = useState(0);
  const [showLastPrompt, setShowLastPrompt] = useState(false);
  const [copyMsg, setCopyMsg] = useState<string>("");
  const currentTurn = recentTurns[turnIndex] ?? null;
  const lastPrompt = currentTurn?.prompt ?? "";
  const lastReply = currentTurn?.reply ?? "";
  const lastTurnMeta = {
    timestamp: currentTurn?.timestamp ?? "",
    tools_used: currentTurn?.tools_used ?? [],
  };
  const scrollRef = useRef<HTMLDivElement>(null);
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

  // Auto-scroll
  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [logs]);

  const handleClear = async () => {
    await invoke("clear_logs");
    setLogs([]);
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
              background: "#fff",
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
                <div style={{ fontSize: "12px", color: "#475569", marginBottom: "6px" }}>
                  <span style={{ fontFamily: "monospace", color: "#0f172a" }}>{r.review_id}</span>
                  {" · "}
                  <span style={{ fontWeight: 600 }}>{r.tool_name}</span>
                  {" · "}
                  <span>{r.timestamp}</span>
                </div>
                <div style={{ fontSize: "12px", color: "#1e293b", marginBottom: "6px" }}>
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
                <details style={{ fontSize: "11px", color: "#475569", marginBottom: "8px" }}>
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
            <div style={{ fontSize: "10px", color: "#94a3b8", marginTop: "4px" }}>
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
              background: "#fff",
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
                borderBottom: "1px solid #e2e8f0",
                display: "flex",
                alignItems: "center",
                gap: "12px",
              }}
            >
              <span style={{ fontSize: "14px", fontWeight: 600, color: "#0f172a" }}>
                proactive 的 prompt + reply
              </span>
              {/* Iter E4: prev/next navigator across the ring buffer */}
              {recentTurns.length > 0 ? (
                <span
                  style={{ display: "inline-flex", alignItems: "center", gap: "4px" }}
                  title="« 上一条（更早）/ » 下一条（更新）。Iter E4 ring buffer 保留最近 5 次"
                >
                  <button
                    onClick={() =>
                      setTurnIndex((i) => Math.min(i + 1, recentTurns.length - 1))
                    }
                    disabled={turnIndex >= recentTurns.length - 1}
                    style={{
                      fontSize: "11px",
                      padding: "1px 6px",
                      borderRadius: "4px",
                      border: "1px solid #cbd5e1",
                      background: turnIndex >= recentTurns.length - 1 ? "#f1f5f9" : "#fff",
                      color: turnIndex >= recentTurns.length - 1 ? "#cbd5e1" : "#475569",
                      cursor: turnIndex >= recentTurns.length - 1 ? "default" : "pointer",
                    }}
                  >
                    «
                  </button>
                  <span
                    style={{
                      fontSize: "11px",
                      color: "#475569",
                      fontFamily: "'SF Mono', monospace",
                      minWidth: "32px",
                      textAlign: "center",
                    }}
                  >
                    {turnIndex + 1}/{recentTurns.length}
                  </span>
                  <button
                    onClick={() => setTurnIndex((i) => Math.max(i - 1, 0))}
                    disabled={turnIndex === 0}
                    style={{
                      fontSize: "11px",
                      padding: "1px 6px",
                      borderRadius: "4px",
                      border: "1px solid #cbd5e1",
                      background: turnIndex === 0 ? "#f1f5f9" : "#fff",
                      color: turnIndex === 0 ? "#cbd5e1" : "#475569",
                      cursor: turnIndex === 0 ? "default" : "pointer",
                    }}
                  >
                    »
                  </button>
                </span>
              ) : (
                <span style={{ fontSize: "11px", color: "#94a3b8" }}>（还没触发过）</span>
              )}
              <span style={{ fontSize: "11px", color: "#94a3b8" }}>
                {lastPrompt ? `prompt ${lastPrompt.length} / reply ${lastReply.length} chars` : ""}
              </span>
              {lastTurnMeta.timestamp && (
                <span
                  style={{
                    fontSize: "11px",
                    color: "#475569",
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
                  color: "#64748b",
                  fontSize: "16px",
                }}
              >
                ✕
              </button>
            </div>
            <div style={{ flex: 1, overflow: "auto", display: "flex", flexDirection: "column" }}>
              <div
                style={{
                  padding: "8px 16px",
                  background: "#f8fafc",
                  borderBottom: "1px solid #e2e8f0",
                  display: "flex",
                  alignItems: "center",
                  gap: "8px",
                }}
              >
                <span style={{ fontSize: "11px", fontWeight: 600, color: "#475569" }}>
                  ⇢ PROMPT (LLM input)
                </span>
                <button
                  onClick={async () => {
                    try {
                      await navigator.clipboard.writeText(lastPrompt);
                      setCopyMsg("prompt 已复制");
                      setTimeout(() => setCopyMsg(""), 2500);
                    } catch (e) {
                      setCopyMsg(`复制失败: ${e}`);
                    }
                  }}
                  disabled={!lastPrompt}
                  style={{
                    fontSize: "10px",
                    padding: "2px 8px",
                    borderRadius: "4px",
                    border: "1px solid #cbd5e1",
                    background: "#fff",
                    color: "#475569",
                    cursor: lastPrompt ? "pointer" : "default",
                  }}
                >
                  复制
                </button>
              </div>
              <pre
                style={{
                  padding: "12px 16px",
                  fontSize: "12px",
                  fontFamily: "'SF Mono', 'Menlo', monospace",
                  color: "#1e293b",
                  whiteSpace: "pre-wrap",
                  wordBreak: "break-word",
                  margin: 0,
                  borderBottom: "1px solid #e2e8f0",
                }}
              >
                {lastPrompt || "（还没有 proactive 触发过——按上面 立即开口 试一次）"}
              </pre>
              <div
                style={{
                  padding: "8px 16px",
                  background: "#f0fdf4",
                  borderBottom: "1px solid #e2e8f0",
                  display: "flex",
                  alignItems: "center",
                  gap: "8px",
                }}
              >
                <span style={{ fontSize: "11px", fontWeight: 600, color: "#166534" }}>
                  ⇠ REPLY (LLM output)
                </span>
                <button
                  onClick={async () => {
                    try {
                      await navigator.clipboard.writeText(lastReply);
                      setCopyMsg("reply 已复制");
                      setTimeout(() => setCopyMsg(""), 2500);
                    } catch (e) {
                      setCopyMsg(`复制失败: ${e}`);
                    }
                  }}
                  disabled={!lastReply}
                  style={{
                    fontSize: "10px",
                    padding: "2px 8px",
                    borderRadius: "4px",
                    border: "1px solid #cbd5e1",
                    background: "#fff",
                    color: "#475569",
                    cursor: lastReply ? "pointer" : "default",
                  }}
                >
                  复制
                </button>
              </div>
              <pre
                style={{
                  padding: "12px 16px",
                  fontSize: "12px",
                  fontFamily: "'SF Mono', 'Menlo', monospace",
                  color: "#1e293b",
                  whiteSpace: "pre-wrap",
                  wordBreak: "break-word",
                  margin: 0,
                }}
              >
                {lastReply || "（还没有 reply — 上次没触发或者 LLM 调用失败）"}
              </pre>
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
      <div style={{ display: "flex", gap: "8px", padding: "12px 16px", borderBottom: "1px solid #e2e8f0", background: "#fff" }}>
        <button onClick={fetchLogs} style={toolBtnStyle}>刷新</button>
        <button onClick={handleClear} style={toolBtnStyle}>清空</button>
        <button
          onClick={handleTriggerProactive}
          disabled={triggeringProactive}
          title="绕过 idle/cooldown/quiet/focus 等闸门，立刻让宠物跑一次主动开口检查（用于测试 prompt 或现场 demo）。"
          style={{
            ...toolBtnStyle,
            background: triggeringProactive ? "#94a3b8" : "#10b981",
            color: "#fff",
          }}
        >
          {triggeringProactive ? "开口中…" : "立即开口"}
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
            borderBottom: "1px solid #e2e8f0",
            background: "#faf5ff",
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
                <span style={{ color: "#6b21a8" }}>
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

      <PanelToneStrip tone={tone} />

      {/* Recent proactive decisions — answers "why didn't the pet say anything?" */}
      {decisions.length > 0 && (
        <div
          style={{
            padding: "8px 16px",
            borderBottom: "1px solid #e2e8f0",
            background: "#f8fafc",
            fontSize: "11px",
            fontFamily: "'SF Mono', 'Menlo', monospace",
            maxHeight: "200px",
            overflowY: "auto",
          }}
        >
          <div style={{ color: "#64748b", marginBottom: "4px", fontFamily: "inherit", fontSize: "12px" }}>
            最近 {decisions.length} 次主动开口判断（最新在底部）
          </div>
          {decisions.map((d, i) => {
            const isOutcome = d.kind === "Spoke" || d.kind === "LlmSilent" || d.kind === "LlmError";
            return (
              <div key={i} style={{ display: "flex", gap: "8px" }}>
                <span style={{ color: "#94a3b8" }}>{d.timestamp.slice(11)}</span>
                <span style={{ color: kindColor(d.kind), fontWeight: 600, minWidth: "44px" }}>
                  {/* Tree-like connector visually links an outcome row to the Run above it */}
                  {isOutcome ? "└ " : ""}{d.kind}
                </span>
                <span style={{ color: "#475569", flex: 1, wordBreak: "break-all" }}>
                  {localizeReason(d.kind, d.reason)}
                </span>
              </div>
            );
          })}
        </div>
      )}

      {/* Pet's recent proactive utterances — sourced from speech_history.log */}
      {recentSpeeches.length > 0 && (
        <div
          style={{
            padding: "8px 16px",
            borderBottom: "1px solid #e2e8f0",
            background: "#fdf4ff",
            fontSize: "12px",
            maxHeight: "120px",
            overflowY: "auto",
          }}
        >
          <div style={{ color: "#86198f", marginBottom: "4px", fontSize: "12px" }}>
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
                <span style={{ color: "#475569", flex: 1, wordBreak: "break-all" }}>{text}</span>
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
          borderBottom: "1px solid #e2e8f0",
          background: "#fefce8",
          fontSize: "12px",
        }}
      >
        <div
          onClick={() => setShowToolHistory((s) => !s)}
          style={{
            cursor: "pointer",
            color: "#854d0e",
            fontWeight: 600,
            display: "flex",
            justifyContent: "space-between",
          }}
        >
          <span>
            🔧 工具调用历史（{toolCallHistory.length}）
          </span>
          <span>{showToolHistory ? "收起 ▾" : "展开 ▸"}</span>
        </div>
        {showToolHistory && toolCallHistory.length === 0 && (
          <div style={{ color: "#92400e", paddingTop: "6px" }}>
            暂无工具调用记录。reactive chat 期间发起的工具调用会出现在这里。
          </div>
        )}
        {showToolHistory && toolCallHistory.length > 0 && (
          <div style={{ paddingTop: "6px", maxHeight: "260px", overflowY: "auto" }}>
            {toolCallHistory.map((c, i) => (
              <div
                key={i}
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
                  <pre style={preStyle}>{c.args_excerpt}</pre>
                  <pre style={preStyle}>{c.result_excerpt}</pre>
                </details>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Iter R6: feedback timeline. Surfaces R1's capture data so the user
          can audit what the pet "saw" — whether each prior proactive turn
          was replied to or ignored. Pure data view; the prompt-side hint is
          built from the same log. Default-collapsed; chip shows count + a
          summary ratio of recent replies. */}
      <div
        style={{
          padding: "8px 16px",
          borderBottom: "1px solid #e2e8f0",
          background: "#f0fdf4",
          fontSize: "12px",
        }}
      >
        <div
          onClick={() => setShowFeedbackHistory((s) => !s)}
          style={{
            cursor: "pointer",
            color: "#065f46",
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
          </span>
          <span>{showFeedbackHistory ? "收起 ▾" : "展开 ▸"}</span>
        </div>
        {showFeedbackHistory && feedbackHistory.length === 0 && (
          <div style={{ color: "#15803d", paddingTop: "6px" }}>
            暂无反馈记录。proactive 开口被回复 / 忽略后会出现在这里。
          </div>
        )}
        {showFeedbackHistory && feedbackHistory.length > 0 && (
          <div style={{ paddingTop: "6px", maxHeight: "240px", overflowY: "auto" }}>
            {feedbackHistory.map((f, i) => (
              <div
                key={i}
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
                    color: "#15803d",
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
                      : "用户回复了这次开口"
                  }
                >
                  {f.kind === "replied"
                    ? "回复"
                    : f.kind === "dismissed"
                    ? "点掉"
                    : "忽略"}
                </span>
                <span style={{ color: "#1e293b", flex: 1, wordBreak: "break-all" }}>
                  {f.excerpt}
                </span>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Pending user-set reminders — sourced from todo memory category */}
      {reminders.length > 0 && (
        <div
          style={{
            padding: "8px 16px",
            borderBottom: "1px solid #e2e8f0",
            background: "#fff7ed",
            fontSize: "12px",
            maxHeight: "120px",
            overflowY: "auto",
          }}
        >
          <div style={{ color: "#9a3412", marginBottom: "4px", fontSize: "12px" }}>
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
              <span style={{ color: "#475569", flex: 1, wordBreak: "break-all" }}>
                {r.topic}
                <span style={{ color: "#94a3b8", marginLeft: "6px", fontSize: "11px" }}>
                  ({r.title})
                </span>
              </span>
            </div>
          ))}
        </div>
      )}

      {/* Log output */}
      <div
        ref={scrollRef}
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
        {logs.length === 0 ? (
          <div style={{ color: "#64748b", textAlign: "center", marginTop: "40px" }}>
            暂无日志。聊天和操作会产生日志。
          </div>
        ) : (
          logs.map((line, i) => (
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

const toolBtnStyle: React.CSSProperties = {
  padding: "6px 14px",
  borderRadius: "6px",
  border: "1px solid #e2e8f0",
  background: "#fff",
  color: "#475569",
  fontSize: "13px",
  cursor: "pointer",
  fontWeight: 500,
};
