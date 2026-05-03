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
  const [triggeringProactive, setTriggeringProactive] = useState(false);
  const [showPromptHints, setShowPromptHints] = useState(false);
  const [proactiveStatus, setProactiveStatus] = useState<string>("");
  // Iter E4: ring buffer of recent turns, newest first. Panel modal navigates
  // with « / » buttons; index 0 = newest. Replaces E1/E2/E3's three separate
  // fetches with a single Vec<TurnRecord> source.
  const [recentTurns, setRecentTurns] = useState<
    { timestamp: string; prompt: string; reply: string; tools_used: string[] }[]
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
    } catch (e) {
      console.error("Failed to fetch logs:", e);
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
                { timestamp: string; prompt: string; reply: string; tools_used: string[] }[]
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
