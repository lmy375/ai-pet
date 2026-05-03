import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { PanelChipStrip } from "./PanelChipStrip";
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
  const [tone, setTone] = useState<ToneSnapshot | null>(null);
  const [reminders, setReminders] = useState<PendingReminder[]>([]);
  const [triggeringProactive, setTriggeringProactive] = useState(false);
  const [showPromptHints, setShowPromptHints] = useState(false);
  const [proactiveStatus, setProactiveStatus] = useState<string>("");
  const scrollRef = useRef<HTMLDivElement>(null);
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const fetchLogs = async () => {
    try {
      const [result, stats, dec, mts, speeches, toneSnap, reminderList, lifetime, today, llmOut, envT, tilt] = await Promise.all([
        invoke<string[]>("get_logs"),
        invoke<CacheStats>("get_cache_stats"),
        invoke<ProactiveDecision[]>("get_proactive_decisions"),
        invoke<MoodTagStats>("get_mood_tag_stats"),
        invoke<string[]>("get_recent_speeches", { n: 10 }),
        invoke<ToneSnapshot>("get_tone_snapshot"),
        invoke<PendingReminder[]>("get_pending_reminders"),
        invoke<number>("get_lifetime_speech_count"),
        invoke<number>("get_today_speech_count"),
        invoke<LlmOutcomeStats>("get_llm_outcome_stats"),
        invoke<EnvToolStats>("get_env_tool_stats"),
        invoke<PromptTiltStats>("get_prompt_tilt_stats"),
      ]);
      setLogs(result);
      setCacheStats(stats);
      setDecisions(dec);
      setMoodTagStats(mts);
      setRecentSpeeches(speeches);
      setTone(toneSnap);
      setReminders(reminderList);
      setLifetimeSpeechCount(lifetime);
      setTodaySpeechCount(today);
      setLlmOutcomeStats(llmOut);
      setEnvToolStats(envT);
      setPromptTiltStats(tilt);
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

      {/* Prominent lifetime stats — single big number for the "we've talked N times" feel.
          Sourced from speech_count.txt sidecar (persisted across restarts). The same value
          is also surfaced in the chip strip below; here it gets a bigger typographic moment
          for users who actually want to see the long-running total. */}
      <div
        style={{
          padding: "10px 16px",
          borderBottom: "1px solid #e2e8f0",
          background: "linear-gradient(135deg, #fdf4ff 0%, #f0f9ff 100%)",
          display: "flex",
          alignItems: "baseline",
          gap: "16px",
        }}
      >
        {(() => {
          const threshold = tone?.chatty_day_threshold ?? 0;
          const restraining = threshold > 0 && todaySpeechCount >= threshold;
          const todayColor = restraining ? "#ea580c" : "#0ea5e9";
          const todayTitle = restraining
            ? `今日 ${todaySpeechCount} 次 ≥ 阈值 ${threshold}：宠物已进入克制模式（prompt 软规则建议保持安静，除非有新信号）`
            : "今天（本机时区）记录的主动开口次数。来自 ~/.config/pet/speech_daily.json";
          return (
            <>
              <div style={{ display: "flex", alignItems: "baseline", gap: "6px" }} title={todayTitle}>
                <span style={{ fontSize: "20px", fontWeight: 600, color: todayColor, lineHeight: 1, fontFamily: "'SF Mono', 'Menlo', monospace" }}>
                  {todaySpeechCount}
                </span>
                <span style={{ fontSize: "11px", color: "#64748b" }}>今日</span>
              </div>
              <div style={{ display: "flex", alignItems: "baseline", gap: "6px" }} title="持久化在 speech_count.txt，跨重启不归零">
                <span style={{ fontSize: "28px", fontWeight: 600, color: "#7c3aed", lineHeight: 1, fontFamily: "'SF Mono', 'Menlo', monospace" }}>
                  {lifetimeSpeechCount}
                </span>
                <span style={{ fontSize: "11px", color: "#64748b" }}>累计</span>
              </div>
              <span style={{ fontSize: "12px", color: "#64748b" }}>次主动开口</span>
              {restraining && (
                <span
                  title={`已超过设置的 chatty_day_threshold (${threshold})，prompt 里加了"今天聊得不少了"的克制规则`}
                  style={{
                    fontSize: "11px",
                    color: "#ea580c",
                    marginLeft: "auto",
                    background: "#fff7ed",
                    border: "1px solid #fed7aa",
                    padding: "2px 8px",
                    borderRadius: "10px",
                  }}
                >
                  克制模式
                </span>
              )}
              {!restraining && lifetimeSpeechCount < 3 && (
                <span style={{ fontSize: "11px", color: "#d97706", marginLeft: "auto" }}>
                  破冰阶段
                </span>
              )}
            </>
          );
        })()}
      </div>

      {/* Conversational tone snapshot — what signals the proactive prompt is seeing */}
      {tone && (
        <div
          style={{
            padding: "6px 16px",
            borderBottom: "1px solid #e2e8f0",
            background: "#f1f5f9",
            fontSize: "11px",
            color: "#475569",
            fontFamily: "'SF Mono', 'Menlo', monospace",
            display: "flex",
            flexWrap: "wrap",
            gap: "12px",
          }}
        >
          <span title="period_of_day(now)">⏱ {tone.period}</span>
          {tone.cadence && tone.since_last_proactive_minutes !== null && (
            <span title="距上次宠物主动开口">
              💬 {tone.cadence}（{tone.since_last_proactive_minutes}m）
            </span>
          )}
          {tone.wake_seconds_ago !== null && tone.wake_seconds_ago <= 600 && (
            <span title="刚检测到 wake-from-sleep" style={{ color: "#0891b2" }}>
              ☀ wake {tone.wake_seconds_ago}s
            </span>
          )}
          {tone.pre_quiet_minutes !== null && (
            <span title="距离配置的 quiet hours 开始时间" style={{ color: "#dc2626" }}>
              🌙 距安静时段 {tone.pre_quiet_minutes}m
            </span>
          )}
          <span
            title={
              tone.proactive_count < 3
                ? "破冰阶段——前 3 次主动开口走探索性话题"
                : "宠物累计主动开口次数（持久化在 speech_count.txt，跨重启不归零）"
            }
            style={{ color: tone.proactive_count < 3 ? "#d97706" : "#64748b" }}
          >
            🤝 已开口 {tone.proactive_count} 次
            {tone.proactive_count < 3 ? "（破冰）" : ""}
          </span>
          {tone.mood_motion && (
            <span title="LLM 当前 motion 标签" style={{ color: "#a855f7" }}>
              ★ motion: {tone.mood_motion}
            </span>
          )}
          {tone.mood_text && (
            <span
              style={{ flex: 1, minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}
              title={tone.mood_text}
            >
              ☁ mood: {tone.mood_text}
            </span>
          )}
        </div>
      )}

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
