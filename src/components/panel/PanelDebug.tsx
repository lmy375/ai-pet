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
  const [recentSpeeches, setRecentSpeeches] = useState<string[]>([]);
  const [tone, setTone] = useState<ToneSnapshot | null>(null);
  const [reminders, setReminders] = useState<PendingReminder[]>([]);
  const scrollRef = useRef<HTMLDivElement>(null);
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const fetchLogs = async () => {
    try {
      const [result, stats, dec, mts, speeches, toneSnap, reminderList] = await Promise.all([
        invoke<string[]>("get_logs"),
        invoke<CacheStats>("get_cache_stats"),
        invoke<ProactiveDecision[]>("get_proactive_decisions"),
        invoke<MoodTagStats>("get_mood_tag_stats"),
        invoke<string[]>("get_recent_speeches", { n: 10 }),
        invoke<ToneSnapshot>("get_tone_snapshot"),
        invoke<PendingReminder[]>("get_pending_reminders"),
      ]);
      setLogs(result);
      setCacheStats(stats);
      setDecisions(dec);
      setMoodTagStats(mts);
      setRecentSpeeches(speeches);
      setTone(toneSnap);
      setReminders(reminderList);
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
        <button onClick={handleOpenDevTools} style={{ ...toolBtnStyle, background: "#f59e0b", color: "#fff" }}>
          DevTools
        </button>
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
        <span style={{ fontSize: "12px", color: "#94a3b8", alignSelf: "center" }}>
          {logs.length} 条日志
        </span>
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
            maxHeight: "120px",
            overflowY: "auto",
          }}
        >
          <div style={{ color: "#64748b", marginBottom: "4px", fontFamily: "inherit", fontSize: "12px" }}>
            最近 {decisions.length} 次主动开口判断（最新在底部）
          </div>
          {decisions.map((d, i) => (
            <div key={i} style={{ display: "flex", gap: "8px" }}>
              <span style={{ color: "#94a3b8" }}>{d.timestamp.slice(11)}</span>
              <span style={{ color: kindColor(d.kind), fontWeight: 600, minWidth: "44px" }}>
                {d.kind}
              </span>
              <span style={{ color: "#475569", flex: 1, wordBreak: "break-all" }}>
                {localizeReason(d.kind, d.reason)}
              </span>
            </div>
          ))}
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
