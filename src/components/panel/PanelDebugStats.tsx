import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type {
  CacheStats,
  EnvToolStats,
  LlmOutcomeStats,
  MoodTagStats,
  PromptTiltStats,
} from "./panelTypes";

/**
 * 统计 tab 的轻量主体：把原本挤在 PanelDebug chip strip 上的累计计数器
 * 切到独立卡片视图，左上标题、中间大字读数、右下重置按钮。每张卡都有
 * 自己的「无样本」空态，与有数据态用同一个外框，避免 page 跳动。
 *
 * 数据来源 = `get_debug_snapshot`，与 PanelDebug 共享 IPC，所以打开两个
 * tab 看到的数字一致。轮询周期 5s —— Debug 用，5s 体感够新鲜，比
 * PanelDebug 的 1s 慢一档减少 IPC 噪音。
 */
const POLL_MS = 5000;

interface DebugStatsBundle {
  cache_stats: CacheStats;
  mood_tag_stats: MoodTagStats;
  llm_outcome_stats: LlmOutcomeStats;
  env_tool_stats: EnvToolStats;
  prompt_tilt_stats: PromptTiltStats;
  companionship_days: number;
  lifetime_speech_count: number;
  today_speech_count: number;
  week_speech_count: number;
}

const cardStyle: React.CSSProperties = {
  background: "var(--pet-color-card)",
  border: "1px solid var(--pet-color-border)",
  borderRadius: 8,
  padding: "14px 16px",
  display: "flex",
  flexDirection: "column",
  gap: 8,
};

const cardHeaderStyle: React.CSSProperties = {
  display: "flex",
  alignItems: "center",
  justifyContent: "space-between",
};

const cardTitleStyle: React.CSSProperties = {
  fontSize: 13,
  fontWeight: 600,
  color: "var(--pet-color-fg)",
};

const cardSubtitleStyle: React.CSSProperties = {
  fontSize: 11,
  color: "var(--pet-color-muted)",
  lineHeight: 1.5,
};

const cardNumberStyle: React.CSSProperties = {
  fontSize: 22,
  fontWeight: 700,
  fontFamily: "'SF Mono', 'Menlo', monospace",
  color: "var(--pet-color-fg)",
  lineHeight: 1.2,
};

const resetBtnStyle: React.CSSProperties = {
  fontSize: 11,
  padding: "2px 8px",
  borderRadius: 4,
  border: "1px solid var(--pet-color-border)",
  background: "var(--pet-color-card)",
  color: "var(--pet-color-muted)",
  cursor: "pointer",
};

function ratioPct(numerator: number, denominator: number): number {
  if (denominator <= 0) return 0;
  return Math.round((numerator / denominator) * 100);
}

function StatCard({
  title,
  subtitle,
  primary,
  detail,
  onReset,
  empty,
}: {
  title: string;
  subtitle: string;
  primary: string;
  detail?: string;
  onReset?: () => void;
  empty?: boolean;
}) {
  return (
    <div style={cardStyle}>
      <div style={cardHeaderStyle}>
        <span style={cardTitleStyle}>{title}</span>
        {onReset && !empty && (
          <button type="button" onClick={onReset} style={resetBtnStyle} title="清零">
            重置
          </button>
        )}
      </div>
      <div style={cardSubtitleStyle}>{subtitle}</div>
      <div style={{ ...cardNumberStyle, color: empty ? "var(--pet-color-muted)" : "var(--pet-color-fg)" }}>
        {empty ? "—" : primary}
      </div>
      {detail && !empty && (
        <div style={{ fontSize: 11, color: "var(--pet-color-muted)" }}>{detail}</div>
      )}
    </div>
  );
}

/// active session 的 LLM 上下文统计（与 `get_active_session_context_stats`
/// 后端返回结构一致）。messages / chars / tokens 都是 system-excluded 的
/// "会被 /reset 砍掉的部分"。session_id / session_title 用来告诉用户"哪
/// 个 session 数据"。
interface SessionContextStats {
  messages: number;
  chars: number;
  tokens: number;
  session_id: string;
  session_title: string;
}

/// `tokens > N` 时浮出"该 /reset 一下"提示。4000 是经验值：常见 LLM context
/// 8k-128k 都有，4000 留出一倍的回头空间给后续对话不至于撞墙。
const SESSION_TOKEN_WARN_THRESHOLD = 4000;

export function PanelDebugStats() {
  const [data, setData] = useState<DebugStatsBundle | null>(null);
  const [errMsg, setErrMsg] = useState("");
  /// active session 上下文规模独立抓 —— 来自 commands::session 的命令而非
  /// debug_snapshot；分离让两个数据源失败时彼此不互拖。
  const [sessionCtx, setSessionCtx] = useState<SessionContextStats | null>(
    null,
  );

  const fetchData = useCallback(async () => {
    try {
      const snap = await invoke<DebugStatsBundle>("get_debug_snapshot");
      setData(snap);
      setErrMsg("");
    } catch (e) {
      setErrMsg(`抓取失败：${e}`);
    }
  }, []);

  const fetchSessionCtx = useCallback(async () => {
    try {
      const stats = await invoke<SessionContextStats>(
        "get_active_session_context_stats",
      );
      setSessionCtx(stats);
    } catch (e) {
      // 后端走"读失败 → 0 兜底"，理论上不会到这里；偶发 IPC 异常静默
      // 让卡片回到"—"态而不显错误吐司（debug 卡片不该挡用户其它操作）。
      console.error("get_active_session_context_stats failed:", e);
    }
  }, []);

  useEffect(() => {
    void fetchData();
    void fetchSessionCtx();
    const id = window.setInterval(() => {
      fetchData();
      fetchSessionCtx();
    }, POLL_MS);
    return () => window.clearInterval(id);
  }, [fetchData, fetchSessionCtx]);

  const reset = async (cmd: string) => {
    try {
      await invoke(cmd);
      await fetchData();
    } catch (e) {
      setErrMsg(`重置失败：${e}`);
    }
  };

  if (!data) {
    return (
      <div style={{ padding: 24, color: "var(--pet-color-muted)", fontSize: 13 }}>
        {errMsg ? errMsg : "加载中…"}
      </div>
    );
  }

  const cache = data.cache_stats;
  const cacheEmpty = cache.total_calls === 0;

  const mood = data.mood_tag_stats;
  const moodTotal = mood.with_tag + mood.without_tag;
  const moodEmpty = moodTotal === 0;

  const llm = data.llm_outcome_stats;
  const llmTotal = llm.spoke + llm.silent + llm.error;
  const llmEmpty = llmTotal === 0;

  const env = data.env_tool_stats;
  const envEmpty = env.spoke_total === 0;

  const tilt = data.prompt_tilt_stats;
  const tiltTotal = tilt.restraint_dominant + tilt.engagement_dominant + tilt.balanced + tilt.neutral;
  const tiltEmpty = tiltTotal === 0;
  const tiltDominantBucket = (() => {
    const buckets: { key: keyof PromptTiltStats; label: string }[] = [
      { key: "restraint_dominant", label: "克制" },
      { key: "engagement_dominant", label: "引导" },
      { key: "balanced", label: "平衡" },
      { key: "neutral", label: "中性" },
    ];
    let best = buckets[0];
    for (const b of buckets) if (tilt[b.key] > tilt[best.key]) best = b;
    return best;
  })();

  return (
    <div
      style={{
        padding: 16,
        height: "100%",
        overflowY: "auto",
        background: "var(--pet-color-bg)",
        boxSizing: "border-box",
        fontFamily: "system-ui, sans-serif",
      }}
    >
      {errMsg && (
        <div
          style={{
            padding: "8px 12px",
            marginBottom: 12,
            background: "var(--pet-tint-orange-bg)",
            color: "var(--pet-tint-orange-fg)",
            borderRadius: 6,
            fontSize: 12,
          }}
        >
          {errMsg}
        </div>
      )}

      <div
        style={{
          display: "grid",
          gridTemplateColumns: "repeat(auto-fill, minmax(220px, 1fr))",
          gap: 12,
        }}
      >
        <StatCard
          title="陪伴天数"
          subtitle="自首次启动至今的本地日数"
          primary={`${data.companionship_days} 天`}
        />
        {/* 当前 session LLM 上下文规模：与 PanelChat `/reset` 配合让用户感
            知"上下文是否该清"。messages / chars / tokens 都排除 system
            （/reset 保留 system 不动）。tokens 超阈值时 detail 走 yellow
            tint + "/reset 提示"，与既有 stat card 的 empty 灰 / 实色 fg
            视觉对偶。 */}
        {(() => {
          const ctx = sessionCtx;
          const empty = !ctx || ctx.messages === 0;
          const tooBig =
            !!ctx && ctx.tokens > SESSION_TOKEN_WARN_THRESHOLD;
          const subtitle = ctx?.session_title
            ? `当前会话「${ctx.session_title.length > 14 ? ctx.session_title.slice(0, 14) + "…" : ctx.session_title}」，排除 system / 持久化态`
            : "排除 system / 当前 active session";
          const detail = empty
            ? undefined
            : tooBig
              ? `~${ctx!.tokens} tok · ${ctx!.chars} 字 · ${ctx!.messages} 条 — 考虑敲 /reset 清掉以省 token`
              : `~${ctx!.tokens} tok · ${ctx!.chars} 字 · ${ctx!.messages} 条`;
          return (
            <div style={cardStyle}>
              <div style={cardHeaderStyle}>
                <span style={cardTitleStyle}>当前会话 LLM 上下文</span>
              </div>
              <div style={cardSubtitleStyle}>{subtitle}</div>
              <div
                style={{
                  ...cardNumberStyle,
                  color: empty
                    ? "var(--pet-color-muted)"
                    : tooBig
                      ? "var(--pet-tint-yellow-fg)"
                      : "var(--pet-color-fg)",
                }}
              >
                {empty ? "—" : `~${ctx!.tokens} tok`}
              </div>
              {detail && (
                <div
                  style={{
                    fontSize: 11,
                    color: tooBig
                      ? "var(--pet-tint-yellow-fg)"
                      : "var(--pet-color-muted)",
                  }}
                >
                  {detail}
                </div>
              )}
            </div>
          );
        })()}
        <StatCard
          title="主动开口（今天）"
          subtitle="今日 proactive 实际开口次数"
          primary={`${data.today_speech_count}`}
          detail={`本周 ${data.week_speech_count} · 累计 ${data.lifetime_speech_count}`}
        />
        <StatCard
          title="LLM 决策结果"
          subtitle="gate 放行后 LLM 选 spoke / silent / error 的累计分布"
          primary={
            llmEmpty
              ? "—"
              : `沉默 ${ratioPct(llm.silent, llmTotal)}%`
          }
          detail={
            llmEmpty
              ? undefined
              : `开口 ${llm.spoke} · 沉默 ${llm.silent} · 失败 ${llm.error}`
          }
          onReset={() => void reset("reset_llm_outcome_stats")}
          empty={llmEmpty}
        />
        <StatCard
          title="环境感知"
          subtitle="开口轮中至少调用一次环境工具的比例"
          primary={
            envEmpty
              ? "—"
              : `${ratioPct(env.spoke_with_any, env.spoke_total)}% (${env.spoke_with_any}/${env.spoke_total})`
          }
          detail={
            envEmpty
              ? undefined
              : `window ${env.active_window} · weather ${env.weather} · events ${env.upcoming_events} · memory ${env.memory_search}`
          }
          onReset={() => void reset("reset_env_tool_stats")}
          empty={envEmpty}
        />
        <StatCard
          title="Prompt 倾向"
          subtitle="累计派发的 prompt 主导倾向（克制 / 引导 / 平衡 / 中性）"
          primary={
            tiltEmpty
              ? "—"
              : `${tiltDominantBucket.label} ${ratioPct(tilt[tiltDominantBucket.key], tiltTotal)}%`
          }
          detail={
            tiltEmpty
              ? undefined
              : `克制 ${tilt.restraint_dominant} · 引导 ${tilt.engagement_dominant} · 平衡 ${tilt.balanced} · 中性 ${tilt.neutral}`
          }
          onReset={() => void reset("reset_prompt_tilt_stats")}
          empty={tiltEmpty}
        />
        <StatCard
          title="环境工具缓存"
          subtitle="同 turn 内重复调用命中本地缓存的比例"
          primary={
            cacheEmpty
              ? "—"
              : `${ratioPct(cache.total_hits, cache.total_calls)}% (${cache.total_hits}/${cache.total_calls})`
          }
          detail={cacheEmpty ? undefined : `已观察 ${cache.turns} 个 turn`}
          onReset={() => void reset("reset_cache_stats")}
          empty={cacheEmpty}
        />
        <StatCard
          title="心情前缀遵守率"
          subtitle={"LLM 写入心情时是否带 [motion: ...] 前缀"}
          primary={
            moodEmpty
              ? "—"
              : `${ratioPct(mood.with_tag, moodTotal)}% (${mood.with_tag}/${moodTotal})`
          }
          detail={
            moodEmpty
              ? undefined
              : `带前缀 ${mood.with_tag} · 缺前缀 ${mood.without_tag} · 心情字段为空 ${mood.no_mood}`
          }
          onReset={() => void reset("reset_mood_tag_stats")}
          empty={moodEmpty}
        />
      </div>

      <div
        style={{
          marginTop: 16,
          fontSize: 11,
          color: "var(--pet-color-muted)",
          lineHeight: 1.6,
        }}
      >
        每 {POLL_MS / 1000} 秒自动刷新。重置仅清零本卡内的累计计数器，不影响其它窗口的数据。完整日志在「应用日志」tab，prompt / reply 录像在「LLM 日志」tab。
      </div>
    </div>
  );
}
