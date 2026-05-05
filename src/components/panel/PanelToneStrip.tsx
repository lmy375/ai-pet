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
            title={`过去 ${total} 次主动开口：正向 ${replied}（含回复 + 主动点赞 👍），被动忽略 ${ignored}，主动点掉 ${dismissed}。R7 cooldown 调整阈值：负反馈率（忽略+点掉）> 60% → cooldown × 2，< 20% → cooldown × 0.7。`}
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
      {tone.speech_register && (() => {
        const { kind, mean_chars, samples } = tone.speech_register;
        // R20: long/short = monotone register → warning amber; mixed =
        // already varying → calm green. Mirrors the "feedback chip"
        // pattern (band → color) so the strip's color language stays
        // consistent.
        const label =
          kind === "long" ? "长" : kind === "short" ? "短" : "混合";
        const isMonotone = kind === "long" || kind === "short";
        const bg = isMonotone ? "#d97706" : "#16a34a";
        const titleText = isMonotone
          ? `最近 ${samples} 句开口都偏${label}（平均 ${mean_chars} 字）— R19 给 LLM 提示换 register。`
          : `最近 ${samples} 句开口长短交替（平均 ${mean_chars} 字）— register 在自然变化，pet 没卡在单一长度。`;
        return (
          <span
            title={titleText}
            style={{
              color: "#fff",
              background: bg,
              padding: "1px 8px",
              borderRadius: "10px",
              fontWeight: 600,
            }}
          >
            📏 {label}（{mean_chars}）
          </span>
        );
      })()}
      {tone.repeated_topic && (
        <span
          title={`R11 检测到最近 5 句开口里反复出现「${tone.repeated_topic}」（4-char ngram，跨 ≥3 句）— prompt 已要求 LLM 换话题。`}
          style={{
            color: "#fff",
            background: "#d97706",
            padding: "1px 8px",
            borderRadius: "10px",
            fontWeight: 600,
          }}
        >
          🔁 {tone.repeated_topic}
        </span>
      )}
      {tone.active_app && (() => {
        const { app, minutes } = tone.active_app;
        // R22 + R27 + R62 + R64: four bands matching the prompt-side / gate-side gates —
        //   < 15m: gray (panel only, no prompt)
        //   15m to soft: orange (R15 informational hint fires)
        //   soft to hard: red (R27 deep-focus directive fires — "极简或选择沉默")
        //   ≥ hard: deep red (R62 HARD-BLOCK — gate skips proactive turn entirely)
        // R64: hard threshold = tone.effective_hard_block_minutes (companion_mode
        // adjusted: chatty 135 / balanced 90 / quiet 60). soft = R27 const 60.
        const hardThreshold = tone.effective_hard_block_minutes ?? 90;
        const softThreshold = 60; // R27 directive boundary, mode-invariant
        let bg: string;
        let titleText: string;
        let suffix = "";
        if (minutes >= hardThreshold) {
          bg = "#7f1d1d"; // deep red — hard block, gate skips entirely
          titleText = `用户已经在「${app}」里专注 ${minutes} 分钟（≥${hardThreshold}m 硬阻塞）— R62 gate 直接 skip 这次 proactive turn，不发 LLM 调用。下次开口要等 app 切换或 mute / awaiting 等其他 gate 状态变化。R64 阈值随 companion_mode 调整：chatty=135 / balanced=90 / quiet=60。`;
          suffix = " 🔒🛑";
        } else if (minutes >= softThreshold) {
          bg = "#b91c1c"; // red — deep focus, explicit silence directive
          titleText = `用户已经在「${app}」里专注 ${minutes} 分钟（深度专注期 ≥${softThreshold}m）— R27 directive 已 fire，prompt 显式要求 LLM 极简或沉默。再过 ${hardThreshold - minutes} 分钟 R62 会升级为硬阻塞（当前 mode 阈值 ${hardThreshold}m）。`;
          suffix = " 🔒";
        } else if (minutes >= 15) {
          bg = "#d97706"; // orange — R15 informational hint fired
          titleText = `用户已经在「${app}」里待了 ${minutes} 分钟 — R15 prompt hint 已 fire（≥15m 阈值，描述性）。`;
        } else {
          bg = "#94a3b8"; // gray — panel only
          titleText = `当前前台 app 是「${app}」，停留 ${minutes} 分钟（< 15m，prompt 未 nudge）。`;
        }
        return (
          <span
            title={titleText}
            style={{
              color: "#fff",
              background: bg,
              padding: "1px 8px",
              borderRadius: "10px",
              fontWeight: 600,
            }}
          >
            🪟 {app}（{minutes}m{suffix}）
          </span>
        );
      })()}
      {tone.consecutive_silent_streak >= 3 && (
        <span
          title={`pet 已经连续 ${tone.consecutive_silent_streak} 次选择沉默（trailing-silent streak ≥ 3）— R33 prompt nudge 已 fire 提醒它考虑开口。spoke 一次自动清零。`}
          style={{
            color: "#fff",
            background: "#d97706",
            padding: "1px 8px",
            borderRadius: "10px",
            fontWeight: 600,
          }}
        >
          🤐 沉默 ×{tone.consecutive_silent_streak}
        </span>
      )}
      {tone.transient_note && (() => {
        // R56: append remaining minutes to chip when known. Helps user
        // see "how much longer this note is in effect" without opening
        // the popover. Hover title gets full text + precise duration.
        const remaining = tone.transient_note_remaining_seconds;
        const minutesLeft = remaining !== null ? Math.max(1, Math.round(remaining / 60)) : null;
        const suffix = minutesLeft !== null ? ` · 剩 ${minutesLeft}m` : "";
        return (
          <span
            title={
              minutesLeft !== null
                ? `用户当前留下的状态/指令：「${tone.transient_note}」（剩 ${minutesLeft} 分钟）— pet 开口时会读到这条 [临时指示]，应当尊重。R55 transient note，到期自动清除。`
                : `用户当前留下的状态/指令：「${tone.transient_note}」 — pet 开口时会读到这条 [临时指示]，应当尊重。R55 transient note。`
            }
            style={{
              color: "#fff",
              background: "#0891b2",
              padding: "1px 8px",
              borderRadius: "10px",
              fontWeight: 600,
              maxWidth: "260px",
              overflow: "hidden",
              textOverflow: "ellipsis",
              whiteSpace: "nowrap",
            }}
          >
            📝 {tone.transient_note}{suffix}
          </span>
        );
      })()}
      {tone.mute_remaining_seconds !== null && tone.mute_remaining_seconds > 0 && (() => {
        // Iter R52: transient mute chip. User explicitly muted pet for a
        // session via the 🔇 button in ChatPanel; chip surfaces "still
        // muted, M minutes left" so user doesn't forget.
        const secs = tone.mute_remaining_seconds;
        const mins = Math.floor(secs / 60);
        const remainder = secs % 60;
        const display = mins > 0
          ? (remainder > 30 ? `${mins + 1}m` : `${mins}m`)
          : `${secs}s`;
        return (
          <span
            title={`用户主动 mute 了 pet（剩 ${secs} 秒）— 期间所有 proactive turn 都被 R52 gate 跳过。点 chat 区域的 🔇 按钮可解除。`}
            style={{
              color: "#fff",
              background: "#7c3aed",
              padding: "1px 8px",
              borderRadius: "10px",
              fontWeight: 600,
            }}
          >
            🔇 静音 {display}
          </span>
        );
      })()}
      {tone.consecutive_negative_streak >= 3 && (
        <span
          title={`用户连续 ${tone.consecutive_negative_streak} 次没回应或主动点掉 pet 的开口（trailing-negative streak ≥ 3）— R35 prompt nudge 已 fire 提醒换角度或沉默。下次 Replied 自动清零。`}
          style={{
            color: "#fff",
            background: "#dc2626",
            padding: "1px 8px",
            borderRadius: "10px",
            fontWeight: 600,
          }}
        >
          🙉 拒绝 ×{tone.consecutive_negative_streak}
        </span>
      )}
      {tone.urgent_deadline_count > 0 && (
        <span
          title={`butler_tasks 里有 ${tone.urgent_deadline_count} 条 [deadline:] 任务正在 imminent (<1h) 或 overdue。pet proactive prompt 已自动 inject [逼近的 deadline] 段，imminent / overdue 时会 override deep-focus 静默原则提醒用户。`}
          style={{
            color: "#fff",
            background: "#b91c1c",
            padding: "1px 8px",
            borderRadius: "10px",
            fontWeight: 600,
          }}
        >
          ⏳ deadline {tone.urgent_deadline_count}
        </span>
      )}
      {tone.last_prompt_chars !== null && (() => {
        // R31 / R36: prompt size budget chip. Bands retuned in R36 based
        // on R-series accumulated reality — R32→R35 added 4 hints,
        // baseline shifted up. Original R31 thresholds (1500/3000) were
        // calibrated before that and started flagging "normal" turns as
        // orange. New bands:
        //   < 2000  green  (lean — fewer hints fired this turn)
        //   2000-3999 gray (normal — most hints firing)
        //   ≥4000   orange (heavy — many composite signals + long
        //           speech_hint bullets, audit which to drop next iter)
        const n = tone.last_prompt_chars;
        const bg =
          n < 2000 ? "#16a34a"   // green: lean
          : n < 4000 ? "#94a3b8" // gray: normal
          : "#d97706";           // orange: heavy
        return (
          <span
            title={`上一次 proactive prompt 长度（chars，CJK-friendly）。绿 < 2000 / 灰 2000-3999 / 橙 ≥4000。当前 ${n} 字。R36 retuned: R-series 累积 hint 后 baseline 上移，原 1500/3000 阈值过严让常态显橙。新阈值给"hint 全 fire 时" 留 normal 空间，仅在异常胖时告警。`}
            style={{
              color: "#fff",
              background: bg,
              padding: "1px 8px",
              borderRadius: "10px",
              fontWeight: 600,
            }}
          >
            📝 {n}字
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
      {tone.cooldown_remaining_seconds !== null && (() => {
        // R23: hover shows the derivation when breakdown is available
        // ("configured × mode × feedback = effective"); falls back to the
        // legacy short hover when breakdown is null (e.g. proactive
        // disabled — but then remaining is also null so this branch
        // shouldn't really hit).
        //
        // R28: color the chip by feedback band so user sees the R7
        // adapter's current verdict at a glance, not just on hover —
        //   high_negative (cooldown ×2) → amber: pet is backing off
        //   low_negative (cooldown ×0.7) → green: user engaged, pet free
        //   mid / insufficient → cyan: neutral / not enough data
        const remaining = tone.cooldown_remaining_seconds;
        const bd = tone.cooldown_breakdown;
        // R81: include the deadline factor in the derivation when there's
        // an urgent (Imminent / Overdue) butler deadline. 0.5× shrink
        // shows up as "× 0.5 (deadline 紧迫 N)" so the user sees why the
        // pet is suddenly speaking up more often.
        const deadlineSegment =
          bd && bd.urgent_deadline_count > 0
            ? ` × ${bd.deadline_factor.toFixed(1)} (deadline 紧迫 ${bd.urgent_deadline_count})`
            : "";
        // R82: surface deadline-driven cadence shift on the chip itself,
        // not only in hover. ⚡ marker + summary hover line make it obvious
        // that the pet is currently in "accelerated" mode without forcing
        // the user to mouse-over and parse the multiplier math.
        const cadenceShifted = !!bd && bd.deadline_factor < 1.0;
        const cadenceSummary = cadenceShifted
          ? `\n\ncadence ×${(1 / bd.deadline_factor).toFixed(0)} 加速：deadline 紧迫，pet 正以 ${Math.round(bd.deadline_factor * 100)}% 的冷却时长跑——更快开口。`
          : "";
        const titleText = bd
          ? `cooldown gate 还有 ${remaining}s。\n` +
            `derivation: configured ${bd.configured_seconds}s × ` +
            `${bd.mode_factor.toFixed(1)} (${bd.mode}) × ` +
            `${bd.feedback_factor.toFixed(1)} (${bd.feedback_band})` +
            `${deadlineSegment} = ` +
            `effective ${bd.effective_seconds}s。` +
            `${cadenceSummary}`
          : `cooldown gate 还有 ${remaining}s 才会放过这一轮 proactive 评估。`;
        const band = bd?.feedback_band;
        const color =
          band === "high_negative" ? "#d97706"
          : band === "low_negative" ? "#16a34a"
          : "#0891b2";
        return (
          <span title={titleText} style={{ color, fontWeight: band && band !== "mid" && band !== "insufficient_samples" ? 600 : "normal" }}>
            ⏳ 冷却 {remaining < 60
              ? `${remaining}s`
              : `${Math.floor(remaining / 60)}m${remaining % 60 > 0 ? `${remaining % 60}s` : ""}`}
            {cadenceShifted && <span style={{ marginLeft: 2, color: "#dc2626" }}>⚡</span>}
          </span>
        );
      })()}
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
