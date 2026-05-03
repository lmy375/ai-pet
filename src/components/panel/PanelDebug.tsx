import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

interface CacheStats {
  turns: number;
  total_hits: number;
  total_calls: number;
}

interface ProactiveDecision {
  timestamp: string;
  kind: string;
  reason: string;
}

interface MoodTagStats {
  with_tag: number;
  without_tag: number;
  no_mood: number;
}

interface LlmOutcomeStats {
  spoke: number;
  silent: number;
  error: number;
}

interface EnvToolStats {
  spoke_total: number;
  spoke_with_any: number;
  active_window: number;
  weather: number;
  upcoming_events: number;
  memory_search: number;
}

interface PendingReminder {
  time: string;
  topic: string;
  title: string;
  due_now: boolean;
}

interface ToneSnapshot {
  period: string;
  cadence: string | null;
  since_last_proactive_minutes: number | null;
  wake_seconds_ago: number | null;
  mood_text: string | null;
  mood_motion: string | null;
  pre_quiet_minutes: number | null;
  proactive_count: number;
  chatty_day_threshold: number;
  active_prompt_rules: string[];
}

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
      const [result, stats, dec, mts, speeches, toneSnap, reminderList, lifetime, today, llmOut, envT] = await Promise.all([
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
        <span style={{ flex: 1 }} />
        {cacheStats.total_calls > 0 && (
          <span
            style={{ display: "inline-flex", alignItems: "center", gap: "6px" }}
          >
            <span
              style={{
                fontSize: "12px",
                color: "#0ea5e9",
                alignSelf: "center",
                fontFamily: "'SF Mono', 'Menlo', monospace",
              }}
              title={`${cacheStats.turns} 次 LLM turn 中累计触发了 ${cacheStats.total_calls} 次环境工具调用，其中 ${cacheStats.total_hits} 次命中缓存`}
            >
              Cache {cacheStats.total_hits}/{cacheStats.total_calls} (
              {Math.round((cacheStats.total_hits / cacheStats.total_calls) * 100)}
              %) · {cacheStats.turns} turns
            </span>
            <button
              onClick={handleResetCacheStats}
              title="重置 cache 统计计数器"
              style={{
                fontSize: "10px",
                padding: "2px 6px",
                borderRadius: "4px",
                border: "1px solid #cbd5e1",
                background: "#fff",
                color: "#64748b",
                cursor: "pointer",
              }}
            >
              重置
            </button>
          </span>
        )}
        {moodTagStats.with_tag + moodTagStats.without_tag > 0 && (
          <span style={{ display: "inline-flex", alignItems: "center", gap: "6px" }}>
            <span
              style={{
                fontSize: "12px",
                color: "#a855f7",
                alignSelf: "center",
                fontFamily: "'SF Mono', 'Menlo', monospace",
              }}
              title={`${moodTagStats.with_tag} 次心情写入带 [motion: X] 前缀，${moodTagStats.without_tag} 次缺失（前端走关键词 fallback）`}
            >
              Tag {moodTagStats.with_tag}/{moodTagStats.with_tag + moodTagStats.without_tag} (
              {Math.round(
                (moodTagStats.with_tag /
                  (moodTagStats.with_tag + moodTagStats.without_tag)) *
                  100,
              )}
              %)
            </span>
            <button
              onClick={handleResetMoodTagStats}
              title="重置 [motion: X] 前缀遵守率统计"
              style={{
                fontSize: "10px",
                padding: "2px 6px",
                borderRadius: "4px",
                border: "1px solid #cbd5e1",
                background: "#fff",
                color: "#64748b",
                cursor: "pointer",
              }}
            >
              重置
            </button>
          </span>
        )}
        {llmOutcomeStats.spoke + llmOutcomeStats.silent + llmOutcomeStats.error > 0 && (
          <span style={{ display: "inline-flex", alignItems: "center", gap: "6px" }}>
            <span
              style={{
                fontSize: "12px",
                color:
                  llmOutcomeStats.silent + llmOutcomeStats.error >
                  llmOutcomeStats.spoke + llmOutcomeStats.silent + llmOutcomeStats.error
                    ? "#ea580c"
                    : "#7c3aed",
                alignSelf: "center",
                fontFamily: "'SF Mono', 'Menlo', monospace",
              }}
              title={`gate 放行后的 LLM 决策: ${llmOutcomeStats.spoke} 次开口，${llmOutcomeStats.silent} 次沉默，${llmOutcomeStats.error} 次失败。沉默率高说明 prompt 偏克制（如 chatty_day_threshold 太低），可作为调优反馈。`}
            >
              LLM沉默 {llmOutcomeStats.silent}/
              {llmOutcomeStats.spoke + llmOutcomeStats.silent + llmOutcomeStats.error} (
              {Math.round(
                (llmOutcomeStats.silent /
                  (llmOutcomeStats.spoke + llmOutcomeStats.silent + llmOutcomeStats.error)) *
                  100,
              )}
              %)
            </span>
            <button
              onClick={handleResetLlmOutcomeStats}
              title="重置 LLM 决策结果统计"
              style={{
                fontSize: "10px",
                padding: "2px 6px",
                borderRadius: "4px",
                border: "1px solid #cbd5e1",
                background: "#fff",
                color: "#64748b",
                cursor: "pointer",
              }}
            >
              重置
            </button>
          </span>
        )}
        {envToolStats.spoke_total > 0 && (
          <span style={{ display: "inline-flex", alignItems: "center", gap: "6px" }}>
            <span
              style={{
                fontSize: "12px",
                color:
                  envToolStats.spoke_with_any * 2 < envToolStats.spoke_total
                    ? "#ea580c"
                    : "#0891b2",
                alignSelf: "center",
                fontFamily: "'SF Mono', 'Menlo', monospace",
              }}
              title={`Spoke 中 ${envToolStats.spoke_with_any}/${envToolStats.spoke_total} 次至少调用过一个 env 工具。分项: window=${envToolStats.active_window} · weather=${envToolStats.weather} · events=${envToolStats.upcoming_events} · memory_search=${envToolStats.memory_search}。比例低于 50% 说明 prompt 没有有效驱动 LLM 用工具，开口贴合度可能差。`}
            >
              环境感知 {envToolStats.spoke_with_any}/{envToolStats.spoke_total} (
              {Math.round((envToolStats.spoke_with_any / envToolStats.spoke_total) * 100)}
              %)
            </span>
            <button
              onClick={handleResetEnvToolStats}
              title="重置环境工具调用统计"
              style={{
                fontSize: "10px",
                padding: "2px 6px",
                borderRadius: "4px",
                border: "1px solid #cbd5e1",
                background: "#fff",
                color: "#64748b",
                cursor: "pointer",
              }}
            >
              重置
            </button>
          </span>
        )}
        {tone && tone.active_prompt_rules.length > 0 && (
          <button
            onClick={() => setShowPromptHints((v) => !v)}
            title={`点击展开/收起规则详情。当前活跃：${tone.active_prompt_rules.join("、")}`}
            style={{
              fontSize: "11px",
              color: "#fff",
              background: showPromptHints ? "#5b21b6" : "#7c3aed",
              padding: "2px 8px",
              borderRadius: "10px",
              alignSelf: "center",
              fontFamily: "'SF Mono', 'Menlo', monospace",
              cursor: "pointer",
              border: "none",
              display: "inline-flex",
              alignItems: "center",
              gap: "4px",
            }}
          >
            prompt: {tone.active_prompt_rules.length} 条 hint
            <span style={{ fontSize: "9px", opacity: 0.85 }}>
              {showPromptHints ? "▾" : "▸"}
            </span>
          </button>
        )}
        <span style={{ fontSize: "12px", color: "#94a3b8", alignSelf: "center" }}>
          {logs.length} 条日志
        </span>
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

/**
 * Frontend-side translation of backend `active_prompt_rules` labels into a short
 * Chinese title + one-line summary of what each rule does. Lives here rather than
 * in the backend to keep all UI strings co-located with the panel; the backend's
 * label set is the contract — adding a new label means adding a row here.
 *
 * Order doesn't matter; lookup is by label string.
 */
/**
 * Each backend prompt rule label has a "nature" describing the *kind* of guidance it
 * pushes at the LLM. Lets the panel show "you've got 3 restraint hints + 2 engagement
 * hints active" as an at-a-glance prompt-tilt summary.
 *
 * - restraint: tells the pet to stay quiet, brief, or low-key.
 * - engagement: encourages the pet to open up / take initiative.
 * - corrective: addresses a past behavioral pattern (e.g., ignoring tools).
 * - instructional: prescribes a specific operation when the pet does speak.
 */
type PromptRuleNature = "restraint" | "engagement" | "corrective" | "instructional";

const PROMPT_RULE_DESCRIPTIONS: Record<
  string,
  { title: string; summary: string; nature: PromptRuleNature }
> = {
  "wake-back": {
    title: "刚回桌",
    summary: "用户的电脑刚从休眠唤醒；问候要简短克制，先轻打招呼。",
    nature: "restraint",
  },
  "first-mood": {
    title: "首次开口",
    summary: "还没有 mood 记忆条目；开口后用 memory_edit create 初始化。",
    nature: "instructional",
  },
  "pre-quiet": {
    title: "近安静时段",
    summary: "再过几分钟到夜里安静时段；语气往收尾靠，简短晚安/睡前关心。",
    nature: "restraint",
  },
  reminders: {
    title: "到期提醒",
    summary: "用户设置的 todo 到期了；自然带进开口里，并 memory_edit delete。",
    nature: "instructional",
  },
  plan: {
    title: "今日计划",
    summary: "ai_insights/daily_plan 有未完成项；优先推进一条并 update 进度。",
    nature: "instructional",
  },
  icebreaker: {
    title: "破冰阶段",
    summary: "之前主动开口 < 3 次；偏向问简短低压力的了解性问题。",
    nature: "restraint",
  },
  chatty: {
    title: "今日克制",
    summary: "今天已经聊得不少；除非有新信号否则保持安静或极简一句。",
    nature: "restraint",
  },
  "env-awareness": {
    title: "环境感知低",
    summary: "近几次开口很少看环境；本次先调 get_active_window 看用户在做啥。",
    nature: "corrective",
  },
  "engagement-window": {
    title: "积极开口",
    summary: "刚回桌 + 有今日 plan：是「先关心、再带 plan」串起来的复合时机。",
    nature: "engagement",
  },
  "long-idle-no-restraint": {
    title: "久未开口",
    summary: "≥ 60min 没主动说话 + 不在克制态：找个贴合用户当下的轻话题。",
    nature: "engagement",
  },
};

const NATURE_META: Record<PromptRuleNature, { label: string; color: string }> = {
  restraint: { label: "克制", color: "#dc2626" },
  engagement: { label: "引导", color: "#16a34a" },
  corrective: { label: "校正", color: "#ea580c" },
  instructional: { label: "操作", color: "#0891b2" },
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
