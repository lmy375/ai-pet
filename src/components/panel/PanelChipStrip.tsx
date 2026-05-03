import type {
  CacheStats,
  EnvToolStats,
  LlmOutcomeStats,
  MoodTagStats,
  PromptTiltStats,
  ToneSnapshot,
} from "./panelTypes";
import { PROMPT_RULE_DESCRIPTIONS } from "./panelTypes";

/**
 * Props for the data-chip strip extracted from PanelDebug's toolbar (Iter 97).
 *
 * Each chip is independently visible — only renders when its underlying counter has
 * accumulated at least one observation. The strip itself is rendered on its own row
 * above the action toolbar so the chips and the buttons each get full horizontal space.
 */
interface PanelChipStripProps {
  cacheStats: CacheStats;
  moodTagStats: MoodTagStats;
  llmOutcomeStats: LlmOutcomeStats;
  envToolStats: EnvToolStats;
  promptTiltStats: PromptTiltStats;
  tone: ToneSnapshot | null;
  showPromptHints: boolean;
  setShowPromptHints: (next: boolean | ((prev: boolean) => boolean)) => void;
  onResetCache: () => void;
  onResetMoodTag: () => void;
  onResetLlmOutcome: () => void;
  onResetEnvTool: () => void;
  onResetPromptTilt: () => void;
  logsCount: number;
}

const resetBtnStyle: React.CSSProperties = {
  fontSize: "10px",
  padding: "2px 6px",
  borderRadius: "4px",
  border: "1px solid #cbd5e1",
  background: "#fff",
  color: "#64748b",
  cursor: "pointer",
};

export function PanelChipStrip(props: PanelChipStripProps) {
  const {
    cacheStats,
    moodTagStats,
    llmOutcomeStats,
    envToolStats,
    promptTiltStats,
    tone,
    showPromptHints,
    setShowPromptHints,
    onResetCache,
    onResetMoodTag,
    onResetLlmOutcome,
    onResetEnvTool,
    onResetPromptTilt,
    logsCount,
  } = props;

  return (
    <div
      style={{
        display: "flex",
        flexWrap: "wrap",
        gap: "12px",
        padding: "8px 16px",
        borderBottom: "1px solid #e2e8f0",
        background: "#f8fafc",
        alignItems: "center",
      }}
    >
      {cacheStats.total_calls > 0 && (
        <span style={{ display: "inline-flex", alignItems: "center", gap: "6px" }}>
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
          <button onClick={onResetCache} title="重置 cache 统计计数器" style={resetBtnStyle}>
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
              (moodTagStats.with_tag / (moodTagStats.with_tag + moodTagStats.without_tag)) * 100,
            )}
            %)
          </span>
          <button onClick={onResetMoodTag} title="重置 [motion: X] 前缀遵守率统计" style={resetBtnStyle}>
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
          <button onClick={onResetLlmOutcome} title="重置 LLM 决策结果统计" style={resetBtnStyle}>
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
          <button onClick={onResetEnvTool} title="重置环境工具调用统计" style={resetBtnStyle}>
            重置
          </button>
        </span>
      )}
      {(() => {
        const t = promptTiltStats;
        const total = t.restraint_dominant + t.engagement_dominant + t.balanced + t.neutral;
        if (total === 0) return null;
        const buckets: { key: keyof PromptTiltStats; label: string; color: string }[] = [
          { key: "restraint_dominant", label: "克制", color: "#dc2626" },
          { key: "engagement_dominant", label: "引导", color: "#16a34a" },
          { key: "balanced", label: "平衡", color: "#7c3aed" },
          { key: "neutral", label: "中性", color: "#94a3b8" },
        ];
        const dominant = buckets.reduce((best, b) => (t[b.key] > t[best.key] ? b : best));
        const pct = Math.round((t[dominant.key] / total) * 100);
        return (
          <span style={{ display: "inline-flex", alignItems: "center", gap: "6px" }}>
            <span
              style={{
                fontSize: "12px",
                color: dominant.color,
                alignSelf: "center",
                fontFamily: "'SF Mono', 'Menlo', monospace",
              }}
              title={`累计 ${total} 次 Run 派发的 prompt 倾向分布: 克制 ${t.restraint_dominant} · 引导 ${t.engagement_dominant} · 平衡 ${t.balanced} · 中性 ${t.neutral}。重置后从零开始累计。`}
            >
              倾向 {dominant.label} {pct}% ({t[dominant.key]}/{total})
            </span>
            <button
              onClick={onResetPromptTilt}
              title="重置 prompt 倾向累计统计"
              style={resetBtnStyle}
            >
              重置
            </button>
          </span>
        );
      })()}
      {tone &&
        tone.active_prompt_rules.length > 0 &&
        (() => {
          let restraint = 0;
          let engagement = 0;
          for (const label of tone.active_prompt_rules) {
            const n = PROMPT_RULE_DESCRIPTIONS[label]?.nature;
            if (n === "restraint") restraint += 1;
            else if (n === "engagement") engagement += 1;
          }
          let bg: string;
          let bgOpen: string;
          let tilt: string;
          if (restraint > engagement) {
            bg = "#dc2626";
            bgOpen = "#991b1b";
            tilt = `偏克制（克制 × ${restraint}、引导 × ${engagement}）`;
          } else if (engagement > restraint) {
            bg = "#16a34a";
            bgOpen = "#15803d";
            tilt = `偏引导（引导 × ${engagement}、克制 × ${restraint}）`;
          } else {
            bg = "#7c3aed";
            bgOpen = "#5b21b6";
            tilt =
              restraint + engagement === 0
                ? "中性（仅 instructional/corrective 规则）"
                : `平衡（克制 ${restraint} ↔ 引导 ${engagement}）`;
          }
          return (
            <button
              onClick={() => setShowPromptHints((v) => !v)}
              title={`点击展开/收起规则详情。当前 ${tilt}。活跃规则：${tone.active_prompt_rules.join("、")}`}
              style={{
                fontSize: "11px",
                color: "#fff",
                background: showPromptHints ? bgOpen : bg,
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
          );
        })()}
      <span style={{ flex: 1 }} />
      <span style={{ fontSize: "12px", color: "#94a3b8", alignSelf: "center" }}>
        {logsCount} 条日志
      </span>
    </div>
  );
}
