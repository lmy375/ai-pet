import type { ToneSnapshot } from "./panelTypes";

/**
 * Prominent lifetime stats card (Iter 99 — extracted from PanelDebug; Iter 106 —
 * companionship-days indicator added; Iter 74 — weekly column added).
 *
 * Renders four stats horizontally: today's count (20px sky blue / orange when
 * restraining), trailing 7-day count (16px muted indigo), lifetime count (28px
 * purple, the dominant number), and companionship days (16px muted teal — quieter
 * so it sits as identity context rather than primary data). A single trailing
 * badge shows either "克制模式" (when chatty threshold crossed) or "破冰阶段"
 * (when lifetime < 3); both states are mutually exclusive.
 *
 * Pure presentation — all state lives in PanelDebug.
 */
interface PanelStatsCardProps {
  todaySpeechCount: number;
  weekSpeechCount: number;
  lifetimeSpeechCount: number;
  companionshipDays: number;
  tone: ToneSnapshot | null;
}

export function PanelStatsCard(props: PanelStatsCardProps) {
  const { todaySpeechCount, weekSpeechCount, lifetimeSpeechCount, companionshipDays, tone } = props;
  const threshold = tone?.chatty_day_threshold ?? 0;
  const restraining = threshold > 0 && todaySpeechCount >= threshold;
  const todayColor = restraining ? "#ea580c" : "#0ea5e9";
  const todayTitle = restraining
    ? `今日 ${todaySpeechCount} 次 ≥ 阈值 ${threshold}：宠物已进入克制模式（prompt 软规则建议保持安静，除非有新信号）`
    : "今天（本机时区）记录的主动开口次数。来自 ~/.config/pet/speech_daily.json";

  return (
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
      <div style={{ display: "flex", alignItems: "baseline", gap: "6px" }} title={todayTitle}>
        <span
          style={{
            fontSize: "20px",
            fontWeight: 600,
            color: todayColor,
            lineHeight: 1,
            fontFamily: "'SF Mono', 'Menlo', monospace",
          }}
        >
          {todaySpeechCount}
        </span>
        <span style={{ fontSize: "11px", color: "#64748b" }}>今日</span>
      </div>
      <div
        style={{ display: "flex", alignItems: "baseline", gap: "6px" }}
        title="今天 + 过去 6 天 = 滚动 7 天的主动开口次数。来自 speech_daily.json 各日 bucket 求和。"
      >
        <span
          style={{
            fontSize: "16px",
            fontWeight: 600,
            color: "#6366f1",
            lineHeight: 1,
            fontFamily: "'SF Mono', 'Menlo', monospace",
          }}
        >
          {weekSpeechCount}
        </span>
        <span style={{ fontSize: "11px", color: "#64748b" }}>本周</span>
      </div>
      <div
        style={{ display: "flex", alignItems: "baseline", gap: "6px" }}
        title="持久化在 speech_count.txt，跨重启不归零"
      >
        <span
          style={{
            fontSize: "28px",
            fontWeight: 600,
            color: "#7c3aed",
            lineHeight: 1,
            fontFamily: "'SF Mono', 'Menlo', monospace",
          }}
        >
          {lifetimeSpeechCount}
        </span>
        <span style={{ fontSize: "11px", color: "#64748b" }}>累计</span>
      </div>
      <span style={{ fontSize: "12px", color: "#64748b" }}>次主动开口</span>
      <span
        title={`你和宠物已经一起走过 ${companionshipDays} 天（自首次启动起算）。来自 ~/.config/pet/install_date.txt。`}
        style={{
          display: "inline-flex",
          alignItems: "baseline",
          gap: "4px",
          marginLeft: "8px",
          paddingLeft: "12px",
          borderLeft: "1px solid #e2e8f0",
        }}
      >
        <span
          style={{
            fontSize: "16px",
            fontWeight: 600,
            color: "#0d9488",
            lineHeight: 1,
            fontFamily: "'SF Mono', 'Menlo', monospace",
          }}
        >
          {companionshipDays}
        </span>
        <span style={{ fontSize: "11px", color: "#64748b" }}>
          {companionshipDays === 0 ? "天（今天初识）" : "天陪伴"}
        </span>
      </span>
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
    </div>
  );
}
