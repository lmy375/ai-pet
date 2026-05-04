import type { ToneSnapshot } from "./panelTypes";

/**
 * Prominent lifetime stats card (Iter 99 — extracted from PanelDebug; Iter 106 —
 * companionship-days indicator added; Iter 74 — weekly column added; Iter Cψ —
 * "上次开口" since-last-proactive indicator added).
 *
 * Renders five stats horizontally: today's count (20px sky blue / orange when
 * restraining), trailing 7-day count (16px muted indigo), lifetime count (28px
 * purple, the dominant number), since-last-proactive (12px muted gray — quiet
 * "is the pet alive lately" cue), and companionship days (16px muted teal —
 * quieter so it sits as identity context rather than primary data). A single
 * trailing badge shows either "克制模式" (when chatty threshold crossed) or
 * "破冰阶段" (when lifetime < 3); both states are mutually exclusive.
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

// Iter Cψ: format minutes-since-last-proactive into a compact glance value.
// Tracks the same band-by-band logic as cadence text but renders short ("8m" /
// "1h32" / "5h") so it fits the stats card row without crowding.
function formatSinceLast(mins: number): string {
  if (mins < 60) return `${mins}m`;
  const h = Math.floor(mins / 60);
  const m = mins % 60;
  return m === 0 ? `${h}h` : `${h}h${m}`;
}

export function PanelStatsCard(props: PanelStatsCardProps) {
  const { todaySpeechCount, weekSpeechCount, lifetimeSpeechCount, companionshipDays, tone } = props;
  const sinceLast = tone?.since_last_proactive_minutes ?? null;
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
      {/* Iter R50: average speeches per day — derived stat lifetime / max(1, companionshipDays).
          Indicates long-term engagement intensity ("是常聊还是少聊的伴侣关系"). Hidden
          on day 0 (no meaningful average from a single-day denominator). */}
      {companionshipDays >= 1 && (
        <span
          style={{
            display: "inline-flex",
            alignItems: "baseline",
            gap: "4px",
            marginLeft: "8px",
            paddingLeft: "12px",
            borderLeft: "1px solid #e2e8f0",
          }}
          title={`累计 ${lifetimeSpeechCount} 次 / 陪伴 ${companionshipDays} 天 = 平均每天 ${(lifetimeSpeechCount / companionshipDays).toFixed(1)} 次主动开口。`}
        >
          <span
            style={{
              fontSize: "13px",
              fontWeight: 500,
              color: "#0d9488",
              lineHeight: 1,
              fontFamily: "'SF Mono', 'Menlo', monospace",
            }}
          >
            {(() => {
              const avg = lifetimeSpeechCount / companionshipDays;
              return avg < 10 ? avg.toFixed(1) : Math.round(avg).toString();
            })()}
          </span>
          <span style={{ fontSize: "11px", color: "#94a3b8" }}>/日均</span>
        </span>
      )}
      {/* Iter R51: 7-day rolling average — distinct from lifetime avg (R50)
          because the week window reveals *recent* trend that lifetime
          smooths out. lifetime avg = "long-term character"; week avg =
          "recent state". Both visible lets user spot drift like
          "lifetime 2 / week 5" → "trending more chatty recently".
          Divisor capped at min(7, companionshipDays + 1) so first-week
          users get a fair denominator instead of always /7. */}
      {companionshipDays >= 1 && (
        <span
          style={{
            display: "inline-flex",
            alignItems: "baseline",
            gap: "4px",
            marginLeft: "8px",
            paddingLeft: "12px",
            borderLeft: "1px solid #e2e8f0",
          }}
          title={(() => {
            const denom = Math.min(7, companionshipDays + 1);
            const lifetimeAvg = lifetimeSpeechCount / companionshipDays;
            const weekAvg = weekSpeechCount / denom;
            const direction =
              weekAvg > lifetimeAvg * 1.3 ? "（最近比长期均值更健谈）"
              : weekAvg < lifetimeAvg * 0.7 ? "（最近比长期均值更安静）"
              : "";
            return `本周 ${weekSpeechCount} 次 / ${denom} 天 = ${weekAvg.toFixed(1)} 次/天${direction}。对比长期 ${lifetimeAvg.toFixed(1)} 次/天看 trend。`;
          })()}
        >
          <span
            style={{
              fontSize: "13px",
              fontWeight: 500,
              color: "#0d9488",
              lineHeight: 1,
              fontFamily: "'SF Mono', 'Menlo', monospace",
            }}
          >
            {(() => {
              const denom = Math.min(7, companionshipDays + 1);
              const avg = weekSpeechCount / denom;
              return avg < 10 ? avg.toFixed(1) : Math.round(avg).toString();
            })()}
          </span>
          <span style={{ fontSize: "11px", color: "#94a3b8" }}>/周日均</span>
        </span>
      )}
      <span
        style={{
          display: "inline-flex",
          alignItems: "baseline",
          gap: "4px",
          marginLeft: "8px",
          paddingLeft: "12px",
          borderLeft: "1px solid #e2e8f0",
        }}
        title={
          sinceLast !== null
            ? `距宠物上次主动开口 ${sinceLast} 分钟${tone?.cadence ? `（${tone.cadence}）` : ""}。来自 InteractionClock 的 since_last_proactive_seconds。`
            : "宠物今天还没主动开过口（since_last_proactive_seconds = None）。"
        }
      >
        <span
          style={{
            fontSize: "13px",
            fontWeight: 500,
            color: sinceLast !== null && sinceLast >= 60 ? "#94a3b8" : "#475569",
            lineHeight: 1,
            fontFamily: "'SF Mono', 'Menlo', monospace",
          }}
        >
          {sinceLast !== null ? formatSinceLast(sinceLast) : "—"}
        </span>
        <span style={{ fontSize: "11px", color: "#94a3b8" }}>前开口</span>
      </span>
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
      {/* Iter R68: weekly deep-focus aggregate, sits before today's column
          so user reads "本周整体 → 今日聚焦" left-to-right. Hidden when
          window empty (consistent with daily empty-state hide). days
          subtitle shows "Y 天/共 N 次/Xm" so summary self-explains depth. */}
      {tone?.weekly_block_stats && tone.weekly_block_stats.total_count > 0 && (
        <span
          title={`本周（最近 7 天）有 ${tone.weekly_block_stats.days} 天进入深度专注，共完成 ${tone.weekly_block_stats.total_count} 次 stretch，峰值时长合计 ${tone.weekly_block_stats.total_minutes} 分钟。来自 R67 持久化的 DAILY_BLOCK_HISTORY，cap=7 entries。`}
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
              fontSize: "13px",
              fontWeight: 600,
              color: "#9f1239",
              lineHeight: 1,
              fontFamily: "'SF Mono', 'Menlo', monospace",
            }}
          >
            🛑 {tone.weekly_block_stats.total_count}
          </span>
          <span style={{ fontSize: "11px", color: "#94a3b8" }}>
            本周/{tone.weekly_block_stats.total_minutes}m/{tone.weekly_block_stats.days}天
          </span>
        </span>
      )}
      {/* Iter R65: today's deep-focus stretch summary. Finalized stretches
          only — in-progress block doesn't count yet (it'll show after
          finalize on transition or recovery hint). Hidden until at least
          one stretch finalizes today; avoids "🛑 0 次" empty state being
          a permanent default for users who never hit deep focus. */}
      {tone?.daily_block_stats && tone.daily_block_stats.count > 0 && (
        <span
          title={`今日完成 ${tone.daily_block_stats.count} 次深度专注（≥${tone.effective_hard_block_minutes ?? 90}m 同 app 触发的 R62 hard-block stretch），峰值时长合计 ${tone.daily_block_stats.total_minutes} 分钟。当前进行中的不计，要等切 app 或 take recovery hint 才 finalize。日期 ${tone.daily_block_stats.date}。`}
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
              color: "#7f1d1d",
              lineHeight: 1,
              fontFamily: "'SF Mono', 'Menlo', monospace",
            }}
          >
            🛑 {tone.daily_block_stats.count}
          </span>
          <span style={{ fontSize: "11px", color: "#64748b" }}>
            次/{tone.daily_block_stats.total_minutes}m
          </span>
        </span>
      )}
      {/* Iter D2: celebration cue on milestone days (7 / 30 / 100 / 180 / 365 /
          yearly). The same signal drives the proactive prompt's
          companionship-milestone rule (Cρ). */}
      {tone?.companionship_milestone && (
        <span
          title={`今天是和宠物相处的「${tone.companionship_milestone}」 — 同样的信号会让 proactive 提示触发 companionship-milestone 规则。`}
          style={{
            fontSize: "11px",
            color: "#fff",
            background: "linear-gradient(90deg, #f59e0b 0%, #ec4899 100%)",
            padding: "2px 8px",
            borderRadius: "10px",
            fontWeight: 600,
            alignSelf: "center",
          }}
        >
          ✨ {tone.companionship_milestone}
        </span>
      )}
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
