import type { ToneSnapshot } from "./panelTypes";

/**
 * Conversational tone snapshot strip (Iter 99 — extracted from PanelDebug).
 *
 * Renders one row of compact emoji-prefixed signals — the same data the proactive
 * prompt is currently feeding the LLM — so the user can answer "why did the pet
 * choose *that* register right now?" at a glance. Each chip only renders when its
 * underlying field is populated; the row collapses entirely if `tone` is null.
 */
interface PanelToneStripProps {
  tone: ToneSnapshot | null;
}

export function PanelToneStrip({ tone }: PanelToneStripProps) {
  if (!tone) return null;
  return (
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
          style={{
            flex: 1,
            minWidth: 0,
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
          }}
          title={tone.mood_text}
        >
          ☁ mood: {tone.mood_text}
        </span>
      )}
    </div>
  );
}
