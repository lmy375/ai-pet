import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

/**
 * Persona tab (Iter 105 / route A延展) — surfaces the long-term identity layer that
 * the proactive prompt and chat injection are reading: when this pet was first
 * installed, how many days you've been together, what the pet has written about its
 * own voice, and the shape of its mood trend lately.
 *
 * Data sources, all via Tauri commands so backend remains the single source of truth:
 * - get_install_date / get_companionship_days → companionship section
 * - get_persona_summary → self-authored summary (consolidate-generated)
 * - get_mood_trend_hint → formatted mood distribution
 *
 * Polling is light (every 5s) — this view is for occasional审视, not a live dashboard.
 */
// Iter Cο: shape returned by `get_current_mood`. text/motion are the parsed
// `[motion: X] free text` form; raw is the unparsed description for inspection.
interface CurrentMood {
  text: string;
  motion: string | null;
  raw: string;
}

// Maps the four LLM-written motion groups to compact glyph + Chinese label.
// Mirrors the front-end keyword fallback used by the bubble's Live2D motion
// picker (Iter 8) — kept tight so future motion additions update both places.
const MOTION_META: Record<string, { glyph: string; label: string; color: string }> = {
  Tap: { glyph: "💗", label: "开心 / 活泼", color: "#ec4899" },
  Flick: { glyph: "✨", label: "想分享 / 有兴致", color: "#f59e0b" },
  Flick3: { glyph: "💢", label: "焦虑 / 烦躁", color: "#ea580c" },
  Idle: { glyph: "💤", label: "平静 / 沉静", color: "#64748b" },
};

export function PanelPersona() {
  const [installDate, setInstallDate] = useState<string>("");
  const [companionshipDays, setCompanionshipDays] = useState<number>(0);
  const [userName, setUserName] = useState<string>("");
  const [personaSummary, setPersonaSummary] = useState<string>("");
  const [personaUpdatedAt, setPersonaUpdatedAt] = useState<string>("");
  const [moodTrend, setMoodTrend] = useState<string>("");
  const [currentMood, setCurrentMood] = useState<CurrentMood>({
    text: "",
    motion: null,
    raw: "",
  });
  const [consolidating, setConsolidating] = useState(false);
  const [consolidateMsg, setConsolidateMsg] = useState("");

  useEffect(() => {
    let cancelled = false;
    const fetchAll = async () => {
      try {
        const [date, days, summary, trend, mood, name] = await Promise.all([
          invoke<string>("get_install_date"),
          invoke<number>("get_companionship_days"),
          invoke<{ text: string; updated_at: string }>("get_persona_summary"),
          invoke<string>("get_mood_trend_hint"),
          invoke<CurrentMood>("get_current_mood"),
          invoke<string>("get_user_name"),
        ]);
        if (cancelled) return;
        setInstallDate(date);
        setCompanionshipDays(days);
        setPersonaSummary(summary.text);
        setPersonaUpdatedAt(summary.updated_at);
        setMoodTrend(trend);
        setCurrentMood(mood);
        setUserName(name);
      } catch (e) {
        console.error("PanelPersona fetch failed:", e);
      }
    };
    fetchAll();
    const id = setInterval(fetchAll, 5000);
    return () => {
      cancelled = true;
      clearInterval(id);
    };
  }, []);

  // Iter Cφ: handler exposed inside the empty-state of "自我画像". Triggers
  // an immediate consolidate run; on completion the 5s poll will see the
  // updated persona_summary naturally. Auto-clears the status after success
  // so the message doesn't linger past relevance.
  const handleTriggerConsolidate = async () => {
    setConsolidating(true);
    setConsolidateMsg("整理中…宠物在回顾最近发言并写画像。");
    try {
      const status = await invoke<string>("trigger_consolidate");
      setConsolidateMsg(status);
      // Schedule auto-clear so success message doesn't stick.
      setTimeout(() => setConsolidateMsg(""), 12000);
    } catch (e: any) {
      setConsolidateMsg(`整理失败：${e}`);
    } finally {
      setConsolidating(false);
    }
  };

  return (
    <div
      style={{
        height: "100%",
        overflowY: "auto",
        padding: "20px",
        display: "flex",
        flexDirection: "column",
        gap: "20px",
      }}
    >
      {/* Companionship — relational time */}
      <Section title="陪伴时长" subtitle="自首次启动起算">
        <div style={{ display: "flex", alignItems: "baseline", gap: "12px" }}>
          <span
            style={{
              fontSize: "44px",
              fontWeight: 600,
              color: "#0d9488",
              lineHeight: 1,
              fontFamily: "'SF Mono', 'Menlo', monospace",
            }}
          >
            {companionshipDays}
          </span>
          <span style={{ fontSize: "14px", color: "#64748b" }}>
            {companionshipDays === 0 ? "天（今天初识）" : "天"}
          </span>
          {installDate && (
            <span
              style={{
                fontSize: "12px",
                color: "#94a3b8",
                marginLeft: "auto",
                fontFamily: "'SF Mono', 'Menlo', monospace",
              }}
              title="install_date.txt 记录的首次启动日期"
            >
              起始 {installDate}
            </span>
          )}
        </div>
        {/* Iter D8: surface settings.user_name so the user can verify the pet
            actually has a name configured (the persona_layer prompt and proactive
            prompt both use it via Cτ/Cυ). Empty state nudges user to set one in
            Settings → 你的名字 if they want named addressing. */}
        <div
          style={{
            marginTop: "10px",
            fontSize: "12px",
            color: userName.trim() ? "#475569" : "#94a3b8",
            fontStyle: userName.trim() ? "normal" : "italic",
          }}
          title={
            userName.trim()
              ? `宠物会用这个名字称呼你 — settings.user_name 注入 persona_layer (Cτ) 和 proactive prompt (Cυ)。`
              : "你还没设名字 — 设置 → 你的名字 (宠物会用它称呼你) 留空时宠物用默认「你」。"
          }
        >
          {userName.trim() ? `🐾 宠物称呼你为「${userName.trim()}」` : "🐾 还没设名字（Settings → 你的名字）"}
        </div>
      </Section>

      {/* Persona summary — self-authored mid-term identity */}
      <Section
        title="自我画像"
        subtitle="consolidate 时由宠物自己反思生成（ai_insights/persona_summary）"
      >
        {personaSummary ? (
          <div style={{ display: "flex", flexDirection: "column", gap: "6px" }}>
            <p
              style={{
                fontSize: "14px",
                color: "#1e293b",
                lineHeight: 1.7,
                margin: 0,
                whiteSpace: "pre-wrap",
              }}
            >
              {personaSummary}
            </p>
            {/* Iter D5: show freshness so user knows how stale the self-reflection is.
                Stale-warning kicks in past 7 days because consolidate default interval
                is 6 hours — anything older than a week means consolidate hasn't been
                running (likely disabled in settings). */}
            {(() => {
              if (!personaUpdatedAt) return null;
              const updatedDate = new Date(personaUpdatedAt);
              if (isNaN(updatedDate.getTime())) return null;
              const ageMs = Date.now() - updatedDate.getTime();
              const ageDays = Math.floor(ageMs / (24 * 3600 * 1000));
              const ageHours = Math.floor(ageMs / (3600 * 1000));
              const stale = ageDays >= 7;
              const label =
                ageDays >= 1
                  ? `${ageDays} 天前更新`
                  : ageHours >= 1
                  ? `${ageHours} 小时前更新`
                  : "刚刚更新";
              return (
                <span
                  style={{
                    fontSize: "11px",
                    color: stale ? "#dc2626" : "#94a3b8",
                    fontStyle: stale ? "normal" : "italic",
                    fontWeight: stale ? 600 : 400,
                  }}
                  title={
                    stale
                      ? `consolidate 已经超过 7 天没运行了——画像可能已经跟不上你和宠物的相处节奏。开 设置 → 启用 consolidate 或在 Memory tab 点立即整理。`
                      : `从 ai_insights/persona_summary.updated_at 计算：${updatedDate.toLocaleString()}`
                  }
                >
                  {stale ? "⚠ " : ""}
                  {label}
                </span>
              );
            })()}
          </div>
        ) : (
          <div style={{ display: "flex", flexDirection: "column", gap: "10px" }}>
            <p style={{ fontSize: "13px", color: "#94a3b8", margin: 0, fontStyle: "italic" }}>
              还没生成。等 consolidate 跑（默认 6 小时间隔）会基于近期发言写一段自我观察——也可以现在手动触发。
            </p>
            <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
              <button
                onClick={handleTriggerConsolidate}
                disabled={consolidating}
                style={{
                  padding: "6px 14px",
                  borderRadius: 6,
                  border: "none",
                  background: consolidating ? "#94a3b8" : "#8b5cf6",
                  color: "#fff",
                  fontSize: 13,
                  fontWeight: 500,
                  cursor: consolidating ? "default" : "pointer",
                }}
                title="立即让 LLM 回顾最近开口、写一段自我画像到 ai_insights/persona_summary。约耗时几秒到十几秒。"
              >
                {consolidating ? "整理中…" : "立即生成画像"}
              </button>
              {consolidateMsg && (
                <span
                  style={{
                    fontSize: 12,
                    color: consolidateMsg.startsWith("整理失败") ? "#dc2626" : "#0d9488",
                  }}
                >
                  {consolidateMsg}
                </span>
              )}
            </div>
          </div>
        )}
      </Section>

      {/* Iter Cο: current mood — live snapshot of ai_insights/current_mood. Sits
          between persona summary (mid-term) and mood trend (long-term) so the
          three sections form a temporal stack: who I am long-term ↘ how I feel
          right now ↘ how I've trended lately. */}
      <Section
        title="当下心情"
        subtitle="ai_insights/current_mood — 宠物每次主动开口时由 LLM 自己更新"
      >
        {currentMood.text || currentMood.motion ? (
          <div style={{ display: "flex", alignItems: "flex-start", gap: "12px" }}>
            {currentMood.motion && MOTION_META[currentMood.motion] ? (
              <div
                style={{
                  display: "flex",
                  flexDirection: "column",
                  alignItems: "center",
                  gap: "4px",
                  minWidth: "64px",
                }}
                title={`motion: ${currentMood.motion}`}
              >
                <span style={{ fontSize: "32px", lineHeight: 1 }}>
                  {MOTION_META[currentMood.motion].glyph}
                </span>
                <span
                  style={{
                    fontSize: "11px",
                    color: MOTION_META[currentMood.motion].color,
                    fontWeight: 500,
                  }}
                >
                  {MOTION_META[currentMood.motion].label}
                </span>
              </div>
            ) : (
              currentMood.motion && (
                <div
                  style={{
                    display: "flex",
                    flexDirection: "column",
                    alignItems: "center",
                    gap: "4px",
                    minWidth: "64px",
                  }}
                >
                  <span style={{ fontSize: "20px" }}>?</span>
                  <span style={{ fontSize: "10px", color: "#94a3b8" }}>
                    {currentMood.motion}
                  </span>
                </div>
              )
            )}
            <p
              style={{
                fontSize: "14px",
                color: "#1e293b",
                lineHeight: 1.7,
                margin: 0,
                flex: 1,
                whiteSpace: "pre-wrap",
              }}
            >
              {currentMood.text || (
                <span style={{ color: "#94a3b8", fontStyle: "italic" }}>
                  （只有 motion 标签没文字）
                </span>
              )}
            </p>
          </div>
        ) : (
          <p style={{ fontSize: "13px", color: "#94a3b8", margin: 0, fontStyle: "italic" }}>
            还没有心情记录。第一次主动开口后，LLM 会用 memory_edit create 写入 ai_insights/current_mood。
          </p>
        )}
      </Section>

      {/* Mood trend — long-term emotional register */}
      <Section
        title="心情谱"
        subtitle="基于 mood_history.log 最近 50 条记录的 motion 分布"
      >
        {moodTrend ? (
          <p
            style={{
              fontSize: "13px",
              color: "#475569",
              lineHeight: 1.7,
              margin: 0,
              whiteSpace: "pre-wrap",
            }}
          >
            {moodTrend}
          </p>
        ) : (
          <p style={{ fontSize: "13px", color: "#94a3b8", margin: 0, fontStyle: "italic" }}>
            数据不足（还没攒到 5 条心情记录）。每次主动开口后会记一条；早期使用看不到很正常。
          </p>
        )}
      </Section>

      {/* Footer note explaining how this powers the prompts */}
      <div
        style={{
          fontSize: "11px",
          color: "#94a3b8",
          marginTop: "auto",
          paddingTop: "12px",
          borderTop: "1px dashed #e2e8f0",
          lineHeight: 1.6,
        }}
      >
        以上三层信息会被注入 proactive prompt 和 desktop chat 的 system prompt（Telegram 路径默认开启，可在设置里关），让宠物在每次发言前都"知道"自己和你的相处时长 / 自我观察 / 长期情绪倾向。
      </div>
    </div>
  );
}

/**
 * Lightweight section wrapper used by the three persona blocks above. Keeps title /
 * subtitle / body styling consistent without pulling in a full design-system layer.
 */
function Section({
  title,
  subtitle,
  children,
}: {
  title: string;
  subtitle?: string;
  children: React.ReactNode;
}) {
  return (
    <section
      style={{
        background: "#fff",
        border: "1px solid #e2e8f0",
        borderRadius: "8px",
        padding: "16px 18px",
      }}
    >
      <header style={{ marginBottom: "12px" }}>
        <h3
          style={{
            margin: 0,
            fontSize: "14px",
            fontWeight: 600,
            color: "#0f172a",
          }}
        >
          {title}
        </h3>
        {subtitle && (
          <p style={{ margin: "2px 0 0", fontSize: "11px", color: "#94a3b8" }}>{subtitle}</p>
        )}
      </header>
      {children}
    </section>
  );
}
