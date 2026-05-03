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
      {!tone.proactive_enabled && (
        <span
          title="settings.proactive.enabled = false — 主动开口循环不会触发任何 LLM 评估。所有其它 chip 仍按现状显示，只是 gate 不会真的放行。"
          style={{
            color: "#fff",
            background: "#475569",
            padding: "1px 8px",
            borderRadius: "10px",
            fontWeight: 600,
          }}
        >
          🔕 proactive 已关
        </span>
      )}
      {tone.feedback_summary && (() => {
        const { replied, dismissed, total } = tone.feedback_summary;
        const ratio = total === 0 ? 0 : replied / total;
        // Color logic mirrors R7's adapter bands (>0.6 high negative = back
        // off → red-orange; <0.2 high reply = great → green; else neutral).
        // The chip surfaces "is the pet currently being heard?" at a glance.
        // R1c: negative = ignored + dismissed; ratio = replied/total still
        // captures the same band semantics (replied + negative = total).
        const negative = 1 - ratio;
        const bg =
          negative > 0.6 ? "#dc2626"
          : negative < 0.2 ? "#16a34a"
          : "#64748b";
        const ignored = total - replied - dismissed;
        return (
          <span
            title={`过去 ${total} 次主动开口：用户回复 ${replied}，被动忽略 ${ignored}，主动点掉 ${dismissed}。R7 cooldown 调整阈值：负反馈率（忽略+点掉）> 60% → cooldown × 2，< 20% → cooldown × 0.7。`}
            style={{
              color: "#fff",
              background: bg,
              padding: "1px 8px",
              borderRadius: "10px",
              fontWeight: 600,
            }}
          >
            💬 {replied}/{total}
            {dismissed > 0 && (
              <span style={{ marginLeft: "4px", opacity: 0.85 }}>
                · 👋{dismissed}
              </span>
            )}
          </span>
        );
      })()}
      <span title="period_of_day(now)">⏱ {tone.period}</span>
      {tone.day_of_week && (
        <span title="weekday + 工作日/周末（Iter Cβ — proactive prompt 时间行已包含）">
          📆 {tone.day_of_week}
        </span>
      )}
      {tone.idle_register && (
        <span
          title={`用户上次互动距今 ${tone.idle_minutes}m（Iter Cμ — proactive prompt 时间行已含此 register cue）`}
        >
          👤 {tone.idle_register}
        </span>
      )}
      {tone.cadence && tone.since_last_proactive_minutes !== null && (
        <span title="距上次宠物主动开口">
          💬 {tone.cadence}（{tone.since_last_proactive_minutes}m）
        </span>
      )}
      {tone.cooldown_remaining_seconds !== null && (
        <span
          title={`cooldown gate 还有 ${tone.cooldown_remaining_seconds}s 才会放过这一轮 proactive 评估（settings.proactive.cooldown_seconds 控制窗口）`}
          style={{ color: "#0891b2" }}
        >
          ⏳ 冷却 {tone.cooldown_remaining_seconds < 60
            ? `${tone.cooldown_remaining_seconds}s`
            : `${Math.floor(tone.cooldown_remaining_seconds / 60)}m${tone.cooldown_remaining_seconds % 60 > 0 ? `${tone.cooldown_remaining_seconds % 60}s` : ""}`}
        </span>
      )}
      {tone.awaiting_user_reply && (
        <span
          title="awaiting gate (Iter 5) — 宠物上次主动说了话但你还没回，gate 让宠物先等等。给 ta 一句回应（任何聊天或交互）就会清除这个状态。"
          style={{ color: "#a855f7" }}
        >
          💭 等回应
        </span>
      )}
      {tone.wake_seconds_ago !== null && tone.wake_seconds_ago <= 600 && (
        <span title="刚检测到 wake-from-sleep" style={{ color: "#0891b2" }}>
          ☀ wake {tone.wake_seconds_ago}s
        </span>
      )}
      {tone.focus_mode && (
        <span
          title={`用户开着 macOS Focus 模式: ${tone.focus_mode}。proactive engine 默认会 gate（看 settings.respect_focus_mode），所以宠物可能会更安静。`}
          style={{ color: "#7c3aed", fontWeight: 600 }}
        >
          🎯 focus: {tone.focus_mode}
        </span>
      )}
      {tone.pre_quiet_minutes !== null && (
        <span title="距离配置的 quiet hours 开始时间" style={{ color: "#dc2626" }}>
          🌙 距安静时段 {tone.pre_quiet_minutes}m
        </span>
      )}
      {tone.in_quiet_hours && (
        <span
          title="当前时间在配置的 quiet hours 内 — proactive engine 现在会 gate 所有主动开口（看 settings.proactive.quiet_hours_start/end）。"
          style={{ color: "#475569", fontWeight: 600 }}
        >
          😴 安静时段中
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
