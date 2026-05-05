import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { parseInlineMarkdown } from "../../utils/inlineMarkdown";

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

// Sparkline 用：mood_history::DailyMotion 的 JSON 表示。motions 字段在后端
// 用 BTreeMap 序列化，前端拿到的就是按 key 升序的对象 —— 直接 entries() 即可。
interface DailyMotion {
  date: string;
  motions: Record<string, number>;
  total: number;
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
  const [moodDaily, setMoodDaily] = useState<DailyMotion[]>([]);
  const [currentMood, setCurrentMood] = useState<CurrentMood>({
    text: "",
    motion: null,
    raw: "",
  });
  const [consolidating, setConsolidating] = useState(false);
  const [consolidateMsg, setConsolidateMsg] = useState("");
  // R115: user_name 内联编辑状态。editingName 切 display vs input；nameDraft
  // 持编辑值；savingName 防 race 重复点；nameError 显短暂错误（不阻断 UI）。
  const [editingName, setEditingName] = useState(false);
  const [nameDraft, setNameDraft] = useState("");
  const [savingName, setSavingName] = useState(false);
  const [nameError, setNameError] = useState("");

  // mood_history 清理入口的折叠状态 + 输入 + 反馈。`clearDays` 默认 7（与
  // mood-sparkline 窗口对齐，让 "清掉过去 7 天" 是直觉默认）；输 0 = 全部。
  const [showClearPanel, setShowClearPanel] = useState(false);
  const [clearDays, setClearDays] = useState<number>(7);
  const [clearing, setClearing] = useState(false);
  const [clearMsg, setClearMsg] = useState("");

  // Sparkline 窗口长度：7d default / 14d 复盘双周。后端 days 参数已现成；
  // useEffect 依赖让切换立即重新 fetch。临时视角，不持久化。
  const [sparklineDays, setSparklineDays] = useState<7 | 14>(7);

  useEffect(() => {
    let cancelled = false;
    const fetchAll = async () => {
      try {
        const [date, days, summary, trend, mood, name, daily] = await Promise.all([
          invoke<string>("get_install_date"),
          invoke<number>("get_companionship_days"),
          invoke<{ text: string; updated_at: string }>("get_persona_summary"),
          invoke<string>("get_mood_trend_hint"),
          invoke<CurrentMood>("get_current_mood"),
          invoke<string>("get_user_name"),
          invoke<DailyMotion[]>("get_mood_daily_motions", { days: sparklineDays }),
        ]);
        if (cancelled) return;
        setInstallDate(date);
        setCompanionshipDays(days);
        setPersonaSummary(summary.text);
        setPersonaUpdatedAt(summary.updated_at);
        setMoodTrend(trend);
        setCurrentMood(mood);
        setUserName(name);
        setMoodDaily(daily);
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
  }, [sparklineDays]);

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

  /// 清理 mood_history。N=0 全清；N>0 清掉最近 N 天。成功后即时刷新本地
  /// trend / sparkline，不必等 5s polling。
  const handleClearMoodHistory = async () => {
    setClearing(true);
    setClearMsg("");
    try {
      const remaining = await invoke<number>("clear_mood_history", { days: clearDays });
      setClearMsg(
        clearDays === 0
          ? `已清空 mood_history。`
          : `已清掉过去 ${clearDays} 天的 mood_history（剩余 ${remaining} 条）。`,
      );
      // 即时刷新当前两个相关指标，sparkline 与 trend hint 立刻同步
      try {
        const [trend, daily] = await Promise.all([
          invoke<string>("get_mood_trend_hint"),
          invoke<DailyMotion[]>("get_mood_daily_motions", { days: sparklineDays }),
        ]);
        setMoodTrend(trend);
        setMoodDaily(daily);
      } catch (e) {
        console.error("post-clear refetch failed:", e);
      }
      setTimeout(() => setClearMsg(""), 8000);
    } catch (e: any) {
      setClearMsg(`清理失败：${e}`);
    } finally {
      setClearing(false);
    }
  };

  // R115: user_name 内联编辑三件套。startEditName → 进入 edit 模式；
  // commitName → load_settings → patch user_name → save_settings round-trip
  // （避免引入新 backend 命令）；cancelEditName → 直接退出。
  // commit 内 next === userName.trim() 短路：避免无变化时浪费一次 IO。
  const startEditName = () => {
    setNameDraft(userName);
    setEditingName(true);
    setNameError("");
  };
  const commitName = async () => {
    if (savingName) return;
    const next = nameDraft.trim();
    if (next === userName.trim()) {
      setEditingName(false);
      return;
    }
    setSavingName(true);
    try {
      const settings = await invoke<Record<string, unknown>>("get_settings");
      settings.user_name = next;
      await invoke("save_settings", { settings });
      setUserName(next);
      setEditingName(false);
    } catch (e: any) {
      setNameError(`保存失败：${e}`);
    } finally {
      setSavingName(false);
    }
  };
  const cancelEditName = () => {
    setEditingName(false);
    setNameError("");
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
          <span style={{ fontSize: "14px", color: "var(--pet-color-muted)" }}>
            {companionshipDays === 0 ? "天（今天初识）" : "天"}
          </span>
          {installDate && (
            <span
              style={{
                fontSize: "12px",
                color: "var(--pet-color-muted)",
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
            prompt both use it via Cτ/Cυ).
            R115: 加内联编辑入口 — ✏️ 切到 input；Enter / blur 保存；Esc 取消。 */}
        <div
          style={{
            marginTop: "10px",
            fontSize: "12px",
            display: "flex",
            alignItems: "center",
            gap: 6,
          }}
        >
          {editingName ? (
            <>
              <span style={{ color: "var(--pet-color-muted)" }}>🐾 宠物称呼你为</span>
              <input
                autoFocus
                value={nameDraft}
                onChange={(e) => setNameDraft(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    e.preventDefault();
                    void commitName();
                  } else if (e.key === "Escape") {
                    e.preventDefault();
                    cancelEditName();
                  }
                }}
                onBlur={() => void commitName()}
                disabled={savingName}
                placeholder="留空 = 宠物用「你」"
                style={{
                  padding: "2px 6px",
                  fontSize: 12,
                  border: "1px solid var(--pet-color-accent)",
                  borderRadius: 3,
                  background: "var(--pet-color-card)",
                  color: "var(--pet-color-fg)",
                  outline: "none",
                  minWidth: 140,
                }}
              />
              {savingName && (
                <span style={{ color: "var(--pet-color-muted)" }}>保存中…</span>
              )}
              {nameError && <span style={{ color: "#dc2626" }}>{nameError}</span>}
            </>
          ) : (
            <>
              <span
                style={{
                  color: userName.trim() ? "var(--pet-color-fg)" : "var(--pet-color-muted)",
                  fontStyle: userName.trim() ? "normal" : "italic",
                }}
                title={
                  userName.trim()
                    ? `宠物会用这个名字称呼你 — settings.user_name 注入 persona_layer (Cτ) 和 proactive prompt (Cυ)。`
                    : "你还没设名字 — 留空时宠物用默认「你」。点 ✏️ 直接设。"
                }
              >
                {userName.trim()
                  ? `🐾 宠物称呼你为「${userName.trim()}」`
                  : "🐾 还没设名字"}
              </span>
              <button
                type="button"
                onClick={startEditName}
                style={{
                  padding: "0 4px",
                  border: "none",
                  background: "transparent",
                  color: "var(--pet-color-muted)",
                  cursor: "pointer",
                  fontSize: 12,
                }}
                title="点击修改名字（Enter 保存 / Esc 取消）"
                aria-label="edit user name"
              >
                ✏️
              </button>
            </>
          )}
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
                color: "var(--pet-color-fg)",
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
                    color: stale ? "#dc2626" : "var(--pet-color-muted)",
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
            <p style={{ fontSize: "13px", color: "var(--pet-color-muted)", margin: 0, fontStyle: "italic" }}>
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
                  <span style={{ fontSize: "10px", color: "var(--pet-color-muted)" }}>
                    {currentMood.motion}
                  </span>
                </div>
              )
            )}
            <p
              style={{
                fontSize: "14px",
                color: "var(--pet-color-fg)",
                lineHeight: 1.7,
                margin: 0,
                flex: 1,
                whiteSpace: "pre-wrap",
              }}
            >
              {currentMood.text || (
                <span style={{ color: "var(--pet-color-muted)", fontStyle: "italic" }}>
                  （只有 motion 标签没文字）
                </span>
              )}
            </p>
          </div>
        ) : (
          <p style={{ fontSize: "13px", color: "var(--pet-color-muted)", margin: 0, fontStyle: "italic" }}>
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
              color: "var(--pet-color-fg)",
              lineHeight: 1.7,
              margin: 0,
              whiteSpace: "pre-wrap",
            }}
          >
            {moodTrend}
          </p>
        ) : (
          <p style={{ fontSize: "13px", color: "var(--pet-color-muted)", margin: 0, fontStyle: "italic" }}>
            数据不足（还没攒到 5 条心情记录）。每次主动开口后会记一条；早期使用看不到很正常。
          </p>
        )}
        <MoodSparkline
          daily={moodDaily}
          windowDays={sparklineDays}
          onWindowDaysChange={setSparklineDays}
        />

        {/* 折叠的「管理」入口：清理脏数据 / 早期 dedupe 漂移 / 测试期残留。
            默认折叠避免抢主视觉；文案 + 反馈贴在原 section 内。 */}
        <div
          style={{
            marginTop: "10px",
            paddingTop: "8px",
            borderTop: "1px dashed var(--pet-color-border)",
            display: "flex",
            flexDirection: "column",
            gap: "6px",
          }}
        >
          <button
            type="button"
            onClick={() => {
              setShowClearPanel((v) => !v);
              setClearMsg("");
            }}
            style={{
              alignSelf: "flex-start",
              padding: "2px 0",
              border: "none",
              background: "transparent",
              color: "var(--pet-color-muted)",
              fontSize: "11px",
              cursor: "pointer",
              textDecoration: "underline dotted",
            }}
            title="折叠管理入口：清理 mood_history.log 的脏数据 / 旧条目"
          >
            {showClearPanel ? "▾ 管理" : "▸ 管理"}
          </button>
          {showClearPanel && (
            <div style={{ display: "flex", flexWrap: "wrap", alignItems: "center", gap: "6px", fontSize: "12px", color: "var(--pet-color-fg)" }}>
              <span>清掉过去</span>
              <input
                type="number"
                min={0}
                value={clearDays}
                onChange={(e) => {
                  const n = parseInt(e.target.value, 10);
                  if (Number.isNaN(n)) return;
                  setClearDays(Math.max(0, n));
                }}
                style={{
                  width: "60px",
                  padding: "3px 6px",
                  border: "1px solid var(--pet-color-border)",
                  borderRadius: 4,
                  fontSize: "12px",
                }}
                disabled={clearing}
              />
              <span>天的 mood_history</span>
              <button
                type="button"
                onClick={handleClearMoodHistory}
                disabled={clearing}
                style={{
                  padding: "3px 10px",
                  border: "1px solid #fecaca",
                  borderRadius: 4,
                  background: clearing ? "#f1f5f9" : "var(--pet-color-card)",
                  color: clearing ? "#94a3b8" : "#b91c1c",
                  cursor: clearing ? "not-allowed" : "pointer",
                  fontSize: "12px",
                }}
                title="N=0 等同清空全部；非 0 时保留比 N 天更早的条目"
              >
                {clearing ? "清理中…" : clearDays === 0 ? "确认清空全部" : "确认清理"}
              </button>
              {clearMsg && (
                <span
                  style={{
                    fontSize: "11px",
                    color: clearMsg.startsWith("清理失败") ? "#dc2626" : "#0d9488",
                  }}
                >
                  {clearMsg}
                </span>
              )}
            </div>
          )}
        </div>
      </Section>

      {/* Footer note explaining how this powers the prompts */}
      <div
        style={{
          fontSize: "11px",
          color: "var(--pet-color-muted)",
          marginTop: "auto",
          paddingTop: "12px",
          borderTop: "1px dashed var(--pet-color-border)",
          lineHeight: 1.6,
        }}
      >
        以上三层信息会被注入 proactive prompt 和 desktop chat 的 system prompt（Telegram 路径默认开启，可在设置里关），让宠物在每次发言前都"知道"自己和你的相处时长 / 自我观察 / 长期情绪倾向。
      </div>
    </div>
  );
}

/**
 * 心情趋势 sparkline — 把最近 7 天的 mood_history motion 频次画成等宽 stacked bar。
 *
 * - 每一列 = 一天，最旧→最新从左到右；空日子留 1px baseline 占位。
 * - 颜色按 motion 类型走 `MOTION_META`（与「当下心情」的 glyph 配色保持一致）；
 *   未识别 / "-" 走浅 slate 兜底，避免抢主色。
 * - hover：tooltip 列出 `2026-05-04 · Tap × 3、Idle × 1`。
 * - 数据全空（最近 7 天无任何记录）→ 不渲染，让上方 trend hint 文案/empty state
 *   独自承担"目前没数据"的语义，避免双重打击。
 */
const SPARKLINE_BAR_HEIGHT = 44; // px，柱子最高时填满
const FALLBACK_MOTION_COLOR = "#cbd5e1"; // 浅 slate；MOTION_META 没命中时用

interface MoodEntry {
  timestamp: string;
  motion: string;
  text: string;
}

// 早晚分段开关用：与后端 `HalfDayMotion` 一一对应。total 等于
// am.values + pm.values 求和（数学一致，让单段渲染时整柱高度不变）。
interface HalfDayMotion {
  date: string;
  am: Record<string, number>;
  pm: Record<string, number>;
  total: number;
}

/** 把当日 mood entries 渲染为 Markdown 段，用于 "复制为 MD" 按钮。
 *
 * 输出格式：
 *   ### YYYY-MM-DD
 *   - HH:MM [Motion] text
 *
 * 设计取舍：
 * - **不本地化 motion**：保留 `Flick3` / `Tap` 等接口语义而非中文标签 ——
 *   接口名稳定，便于将来跨工具搜索 / grep。中文描述以后可能演进。
 * - **HH:MM 取 timestamp[11..16]**：与 drill UI 显示一致，避免两边一个
 *   秒一个分钟的认知断裂。timestamp 不足 16 字符时退化为整段（理论不
 *   会发生 — 后端 RFC3339 格式总是 ≥19 字符）。
 * - **空 entries** 输出占位 `- (空)`：让调用方就算意外把空数组传进来
 *   也能拿到合法 markdown 段，不会写一个空字符串到剪贴板。 */
function formatDayEntriesAsMarkdown(date: string, entries: MoodEntry[]): string {
  const lines: string[] = [`### ${date}`];
  if (entries.length === 0) {
    lines.push("- (空)");
    return lines.join("\n");
  }
  for (const e of entries) {
    const hhmm =
      e.timestamp.length >= 16 ? e.timestamp.slice(11, 16) : e.timestamp;
    lines.push(`- ${hhmm} [${e.motion}] ${e.text}`);
  }
  return lines.join("\n");
}

/// R124: CSV 字段转义。RFC 4180：含 `,` / `"` / 换行的字段用 `"..."` 包，
/// 内嵌 `"` 翻成 `""`；其它字段原样。让导出能直接被 Excel / pandas / sqlite
/// 解析，不破坏含特殊字符的用户原文。
function csvEscape(s: string): string {
  if (/[",\n\r]/.test(s)) {
    return `"${s.replace(/"/g, '""')}"`;
  }
  return s;
}

/// R124: 把当日 mood entries 渲染为 CSV 段，供"复制为 CSV"按钮使用。
/// 三列 `timestamp,motion,text` 全量保留（不应用 motion filter）；空
/// entries 仍输出 header，让用户拿到合法 CSV 而非空字符串。timestamp 用
/// 后端原始 RFC3339 形式，方便跨工具排序 / 过滤。
function formatDayEntriesAsCsv(entries: MoodEntry[]): string {
  const lines = ["timestamp,motion,text"];
  for (const e of entries) {
    lines.push(
      [csvEscape(e.timestamp), csvEscape(e.motion), csvEscape(e.text)].join(","),
    );
  }
  return lines.join("\n");
}

/** 在 sparkline 7 天窗口内取相邻日期。`daily` 升序（最旧 → 最新），所以
 * `delta=-1` 找前一天、`delta=+1` 找后一天。current 不在窗口里 / 越界时
 * 返回 null —— 调用方据此 disable ‹ / › 按钮。
 *
 * 不主动跳过空日：用户可能想看到"那天完全没记录"这件事本身（彻底沉默
 * 也是情绪信号），所以一格一格走而不是 jump-to-next-non-empty。 */
function adjacentDate(
  daily: DailyMotion[],
  current: string,
  delta: -1 | 1,
): string | null {
  const idx = daily.findIndex((d) => d.date === current);
  if (idx < 0) return null;
  const next = idx + delta;
  if (next < 0 || next >= daily.length) return null;
  return daily[next].date;
}

function MoodSparkline({
  daily,
  windowDays,
  onWindowDaysChange,
}: {
  daily: DailyMotion[];
  windowDays: 7 | 14;
  onWindowDaysChange: (n: 7 | 14) => void;
}) {
  const total = daily.reduce((sum, d) => sum + d.total, 0);
  // selectedMotion === null → stacked 全量；非 null → 只看该 motion 段，y 轴
  // 重缩放到该 motion 的最大日 count，让"专注 Tap"等场景仍有可比较的视觉。
  const [selectedMotion, setSelectedMotion] = useState<string | null>(null);
  // 点格子查当日详情：selectedDate 非 null 时 sparkline 下方渲染当日 entries。
  // entries 在 selectedDate 变化时 lazy fetch；切到 null 时清空。
  const [selectedDate, setSelectedDate] = useState<string | null>(null);
  const [dayEntries, setDayEntries] = useState<MoodEntry[]>([]);
  // 当日 entry 列表按 motion 过滤：null = 全部；切换日期或关闭时 reset。
  const [entryFilter, setEntryFilter] = useState<string | null>(null);
  // 当日 entry 文本子串搜索：与 motion chip 双 axis 叠加。selectedDate
  // 切换时 reset，与 entryFilter 同语义（临时 debug 视角，跨日不携带）。
  const [entrySearch, setEntrySearch] = useState("");
  // 「复制为 MD」按钮的 ack 视觉：true 时按钮变绿展示「已复制」，1.5s 后
  // 自动复位。Map / Set 在这里没必要 —— 同一时刻只有一处可点击。
  const [copiedDayMd, setCopiedDayMd] = useState(false);
  // R124: CSV 导出的 toast 反馈。与 copiedDayMd 同语义但独立（让 MD / CSV
  // 各自的"已复制"高亮不互相覆盖）。
  const [copiedDayCsv, setCopiedDayCsv] = useState(false);
  // 单条 entry 复制 ack：key = `${entry.timestamp}-${i}`（与 row key 同源），
  // 同时刻只一条按钮显"已复制"绿字 1.5s 后自动复位。
  const [copiedEntryKey, setCopiedEntryKey] = useState<string | null>(null);
  // 当日 entry 列表超 20 条时默认折叠；用户点 "展开剩余" 切到全显。
  // selectedDate 变化时 reset：每次切日重新评估"是否需要折叠"。
  const [entryListExpanded, setEntryListExpanded] = useState(false);
  // entry 列表渲染顺序：default false = 时间 asc（早→晚，与文件 append
  // 自然序对齐），true = desc（最新在顶，与决策日志反序开关同语义）。
  // 切日时 reset 让"看新一天先从最早一条开始"。
  const [entriesNewestFirst, setEntriesNewestFirst] = useState(false);
  // 早晚分段开关：true 时柱内拆 AM (底) / PM (顶) 两段渲染。半日数据 lazy
  // fetch，关掉开关后保留 cache 不主动清空，让用户来回切只在首次开启付一次 IO。
  const [splitHalfDay, setSplitHalfDay] = useState(false);
  const [halfDaily, setHalfDaily] = useState<HalfDayMotion[]>([]);
  useEffect(() => {
    if (!splitHalfDay) return;
    let cancelled = false;
    invoke<HalfDayMotion[]>("get_mood_half_day_motions", { days: windowDays })
      .then((data) => {
        if (!cancelled) setHalfDaily(data);
      })
      .catch((e) => {
        console.error("get_mood_half_day_motions failed:", e);
        if (!cancelled) setHalfDaily([]);
      });
    return () => {
      cancelled = true;
    };
  }, [splitHalfDay, windowDays]);
  // 用 date 索引半日数据，让 SparklineBar 按 daily 顺序逐柱拿对应 AM/PM。
  const halfDailyByDate = useMemo(() => {
    const m: Record<string, HalfDayMotion> = {};
    for (const h of halfDaily) m[h.date] = h;
    return m;
  }, [halfDaily]);
  useEffect(() => {
    setEntryFilter(null);
    setEntrySearch("");
    setCopiedDayMd(false);
    setCopiedDayCsv(false);
    setCopiedEntryKey(null);
    setEntryListExpanded(false);
    setEntriesNewestFirst(false);
    if (selectedDate === null) {
      setDayEntries([]);
      return;
    }
    let cancelled = false;
    invoke<MoodEntry[]>("get_mood_entries_for_date", { date: selectedDate })
      .then((entries) => {
        if (!cancelled) setDayEntries(entries);
      })
      .catch((e) => {
        console.error("get_mood_entries_for_date failed:", e);
        if (!cancelled) setDayEntries([]);
      });
    return () => {
      cancelled = true;
    };
  }, [selectedDate]);
  // 计算当日 motion → count；按 MOTION_META 顺序输出，保证 chip 顺序稳定。
  const dayMotionCounts = useMemo(() => {
    const map: Record<string, number> = {};
    for (const e of dayEntries) {
      map[e.motion] = (map[e.motion] ?? 0) + 1;
    }
    return map;
  }, [dayEntries]);
  const visibleEntries = useMemo(() => {
    const q = entrySearch.trim().toLowerCase();
    return dayEntries.filter((e) => {
      if (entryFilter !== null && e.motion !== entryFilter) return false;
      if (q !== "" && !e.text.toLowerCase().includes(q)) return false;
      return true;
    });
  }, [dayEntries, entryFilter, entrySearch]);
  if (daily.length === 0 || total === 0) return null;
  const maxTotal = Math.max(...daily.map((d) => d.total));
  const effectiveMax =
    selectedMotion === null
      ? maxTotal
      : Math.max(0, ...daily.map((d) => d.motions[selectedMotion] ?? 0));
  return (
    <div
      style={{
        marginTop: "12px",
        paddingTop: "10px",
        borderTop: "1px dashed var(--pet-color-border)",
        display: "flex",
        flexDirection: "column",
        gap: "6px",
      }}
    >
      {/* CSS hover-only：mood entry 行 hover 时单条复制按钮显出，平时透
          明不打扰阅读（同 PanelDebug 决策日志 row 模式）。 */}
      <style>
        {`
          .pet-mood-entry-row .pet-mood-entry-copy-btn {
            opacity: 0;
            transition: opacity 0.12s ease;
          }
          .pet-mood-entry-row:hover .pet-mood-entry-copy-btn { opacity: 1; }
        `}
      </style>
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: "8px",
          flexWrap: "wrap",
          fontSize: "11px",
          color: "var(--pet-color-muted)",
        }}
        title={`按本地日期聚合 mood_history.log 最近 ${windowDays} 天的 motion 频次。空日 = 当天没主动开口或未记 motion。点 chip 切换只看某种情绪。`}
      >
        <span>最近 {windowDays} 天 motion 频次</span>
        <MotionFilterChips selected={selectedMotion} onChange={setSelectedMotion} />
        <span
          role="button"
          tabIndex={0}
          onClick={() => setSplitHalfDay((v) => !v)}
          onKeyDown={(e) => {
            if (e.key === "Enter" || e.key === " ") {
              e.preventDefault();
              setSplitHalfDay((v) => !v);
            }
          }}
          title={
            splitHalfDay
              ? "再次点击关闭「早晚分段」，恢复整天聚合柱"
              : "把每柱拆成 上午 (00-11h, 底) / 下午 (12-23h, 顶) 两段，看节律变化"
          }
          style={{
            fontSize: 11,
            padding: "2px 8px",
            borderRadius: 10,
            background: splitHalfDay ? "#cffafe" : "#f1f5f9",
            color: splitHalfDay ? "#0e7490" : "var(--pet-color-fg)",
            cursor: "pointer",
            whiteSpace: "nowrap",
            userSelect: "none",
            border: "1px solid transparent",
          }}
        >
          {splitHalfDay ? "✓ " : ""}早晚分段
        </span>
        {/* 窗口长度切换：7d / 14d 互斥；selected 填中性 slate（窗口长度
            不携带情绪语义）。点击切换 → 父级 useEffect 重新 fetch。 */}
        {([7, 14] as const).map((d) => {
          const active = windowDays === d;
          return (
            <span
              key={d}
              role="button"
              tabIndex={0}
              onClick={() => onWindowDaysChange(d)}
              onKeyDown={(e) => {
                if (e.key === "Enter" || e.key === " ") {
                  e.preventDefault();
                  onWindowDaysChange(d);
                }
              }}
              title={
                active
                  ? `当前显示 ${d} 天 sparkline`
                  : `切换到 ${d} 天 sparkline 窗口`
              }
              style={{
                fontSize: 11,
                padding: "2px 8px",
                borderRadius: 10,
                background: active ? "#cbd5e1" : "#f1f5f9",
                color: "var(--pet-color-fg)",
                cursor: active ? "default" : "pointer",
                whiteSpace: "nowrap",
                userSelect: "none",
                border: `1px solid ${active ? "#94a3b8" : "transparent"}`,
                fontWeight: active ? 600 : 400,
              }}
            >
              {d}d
            </span>
          );
        })}
      </div>
      <div
        style={{
          display: "flex",
          alignItems: "flex-end",
          gap: "6px",
          height: `${SPARKLINE_BAR_HEIGHT}px`,
        }}
      >
        {daily.map((d) => (
          <SparklineBar
            key={d.date}
            day={d}
            effectiveMax={effectiveMax}
            filter={selectedMotion}
            selected={selectedDate === d.date}
            onClick={() =>
              setSelectedDate((prev) => (prev === d.date ? null : d.date))
            }
            halfDay={splitHalfDay ? halfDailyByDate[d.date] : undefined}
          />
        ))}
      </div>
      <div
        style={{
          display: "flex",
          gap: "6px",
          fontSize: "10px",
          color: "var(--pet-color-muted)",
          fontFamily: "'SF Mono', 'Menlo', monospace",
        }}
      >
        {daily.map((d) => (
          <span
            key={d.date}
            style={{
              flex: 1,
              textAlign: "center",
            }}
          >
            {shortDateLabel(d.date)}
          </span>
        ))}
      </div>
      {selectedDate !== null && (
        <div
          style={{
            marginTop: "8px",
            paddingTop: "8px",
            borderTop: "1px dashed var(--pet-color-border)",
            display: "flex",
            flexDirection: "column",
            gap: "4px",
            fontSize: "12px",
          }}
        >
          <div style={{ display: "flex", alignItems: "center", gap: "6px" }}>
            {(() => {
              const prev = adjacentDate(daily, selectedDate, -1);
              const next = adjacentDate(daily, selectedDate, +1);
              const navBtn = (
                disabled: boolean,
                onClick: () => void,
                label: string,
                title: string,
              ) => (
                <button
                  type="button"
                  disabled={disabled}
                  onClick={onClick}
                  title={title}
                  style={{
                    fontSize: "10px",
                    padding: "1px 6px",
                    border: "1px solid var(--pet-color-border)",
                    borderRadius: 4,
                    background: "var(--pet-color-card)",
                    color: disabled ? "#cbd5e1" : "var(--pet-color-fg)",
                    cursor: disabled ? "not-allowed" : "pointer",
                    lineHeight: 1.2,
                  }}
                >
                  {label}
                </button>
              );
              return (
                <>
                  {navBtn(
                    prev === null,
                    () => prev && setSelectedDate(prev),
                    "‹",
                    prev
                      ? `跳到 ${prev}（前一天）`
                      : "已是 sparkline 窗口最早一天",
                  )}
                  {navBtn(
                    next === null,
                    () => next && setSelectedDate(next),
                    "›",
                    next
                      ? `跳到 ${next}（后一天）`
                      : "已是 sparkline 窗口最晚一天",
                  )}
                </>
              );
            })()}
            <span style={{ fontSize: "11px", color: "var(--pet-color-muted)" }}>
              {selectedDate} · 当日 {dayEntries.length} 条 mood entry
              {dayEntries.length > 0 && (() => {
                // motion 占比一眼看分布。按计数降序，count 同则 MOTION_META
                // 顺序（稳定）。各占比四舍五入到整数百分比，加起来不一定 = 100；
                // 优先单条直观读，不强求精确求和。
                const order = ["Tap", "Flick", "Flick3", "Idle"] as const;
                const sorted = Object.entries(dayMotionCounts)
                  .filter(([, c]) => c > 0)
                  .sort((a, b) => {
                    if (b[1] !== a[1]) return b[1] - a[1];
                    return order.indexOf(a[0] as typeof order[number]) -
                      order.indexOf(b[0] as typeof order[number]);
                  });
                if (sorted.length === 0) return null;
                const total = dayEntries.length;
                const parts = sorted
                  .map(([m, c]) => `${m} ${c} (${Math.round((c / total) * 100)}%)`)
                  .join(" · ");
                return (
                  <span style={{ marginLeft: 6, color: "var(--pet-color-muted)" }}>
                    {parts}
                  </span>
                );
              })()}
            </span>
            {dayEntries.length > 0 && (
              <button
                type="button"
                onClick={async () => {
                  try {
                    await navigator.clipboard.writeText(
                      formatDayEntriesAsMarkdown(selectedDate, dayEntries),
                    );
                    setCopiedDayMd(true);
                    window.setTimeout(() => setCopiedDayMd(false), 1500);
                  } catch (e) {
                    console.error("clipboard write failed:", e);
                  }
                }}
                title={
                  copiedDayMd
                    ? "已复制 markdown"
                    : "复制为 markdown：### YYYY-MM-DD + 每行 - HH:MM [Motion] text，方便贴到笔记复盘当日心情"
                }
                style={{
                  marginLeft: "auto",
                  fontSize: "10px",
                  padding: "1px 6px",
                  border: "1px solid var(--pet-color-border)",
                  borderRadius: 4,
                  background: "var(--pet-color-card)",
                  color: copiedDayMd ? "#16a34a" : "var(--pet-color-fg)",
                  cursor: "pointer",
                  whiteSpace: "nowrap",
                }}
              >
                {copiedDayMd ? "已复制" : "复制为 MD"}
              </button>
            )}
            {/* R124: CSV 导出。与 MD 按钮成对出现；CSV 适合贴 Excel /
                pandas / sqlite 做离线分析，MD 适合贴笔记复盘。 */}
            {dayEntries.length > 0 && (
              <button
                type="button"
                onClick={async () => {
                  try {
                    await navigator.clipboard.writeText(
                      formatDayEntriesAsCsv(dayEntries),
                    );
                    setCopiedDayCsv(true);
                    window.setTimeout(() => setCopiedDayCsv(false), 1500);
                  } catch (e) {
                    console.error("clipboard write failed:", e);
                  }
                }}
                title={
                  copiedDayCsv
                    ? "已复制 CSV"
                    : "复制为 CSV：timestamp,motion,text 三列；含逗号 / 换行的字段自动加引号转义。粘贴到 Excel / pandas / sqlite 直接可读。"
                }
                style={{
                  fontSize: "10px",
                  padding: "1px 6px",
                  border: "1px solid var(--pet-color-border)",
                  borderRadius: 4,
                  background: "var(--pet-color-card)",
                  color: copiedDayCsv ? "#16a34a" : "var(--pet-color-fg)",
                  cursor: "pointer",
                  whiteSpace: "nowrap",
                }}
              >
                {copiedDayCsv ? "已复制" : "复制为 CSV"}
              </button>
            )}
            <button
              type="button"
              onClick={() => setSelectedDate(null)}
              style={{
                marginLeft: dayEntries.length > 0 ? 0 : "auto",
                fontSize: "10px",
                padding: "1px 6px",
                border: "1px solid var(--pet-color-border)",
                borderRadius: 4,
                background: "var(--pet-color-card)",
                color: "var(--pet-color-muted)",
                cursor: "pointer",
              }}
              title="关闭当日详情"
            >
              ✕
            </button>
          </div>
          {dayEntries.length === 0 ? (
            <div style={{ fontSize: "11px", color: "var(--pet-color-muted)", fontStyle: "italic" }}>
              当日没有 mood_history 记录（早期使用 / 已被「管理 → 清掉过去 N 天」清掉 / IO 错误）。
            </div>
          ) : (
            <>
              <div
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 4,
                  flexWrap: "wrap",
                  marginBottom: 2,
                }}
              >
                <DayMotionChips
                  counts={dayMotionCounts}
                  total={dayEntries.length}
                  selected={entryFilter}
                  hits={visibleEntries.length}
                  onChange={setEntryFilter}
                />
                <input
                  type="search"
                  value={entrySearch}
                  onChange={(e) => setEntrySearch(e.target.value)}
                  placeholder="搜 entry 文字"
                  title="子串过滤当日 entries 的 text 字段。与 motion chip 可叠加。"
                  style={{
                    fontFamily: "inherit",
                    fontSize: 11,
                    padding: "1px 6px",
                    border: "1px solid var(--pet-color-border)",
                    borderRadius: 4,
                    background: "var(--pet-color-card)",
                    color: "var(--pet-color-fg)",
                    width: 140,
                    lineHeight: 1.4,
                    marginLeft: "auto",
                  }}
                />
                {entrySearch.trim() !== "" && (
                  <button
                    type="button"
                    onClick={() => setEntrySearch("")}
                    title="清空 entry 搜索"
                    style={{
                      fontSize: 10,
                      padding: "1px 6px",
                      border: "1px solid var(--pet-color-border)",
                      borderRadius: 4,
                      background: "var(--pet-color-card)",
                      color: "var(--pet-color-muted)",
                      cursor: "pointer",
                      lineHeight: 1.4,
                    }}
                  >
                    ✕
                  </button>
                )}
                <button
                  type="button"
                  onClick={() => setEntriesNewestFirst((v) => !v)}
                  title={
                    entriesNewestFirst
                      ? "当前最新在顶。点击切回最早在顶（与文件追加序对齐）"
                      : "当前最早在顶。点击切到最新在顶（与决策日志反序开关同语义）"
                  }
                  style={{
                    fontSize: 10,
                    padding: "1px 6px",
                    border: "1px solid var(--pet-color-border)",
                    borderRadius: 4,
                    background: "var(--pet-color-card)",
                    color: "var(--pet-color-fg)",
                    cursor: "pointer",
                    lineHeight: 1.4,
                    whiteSpace: "nowrap",
                  }}
                >
                  {entriesNewestFirst ? "↑ 最新在顶" : "↓ 最早在顶"}
                </button>
              </div>
              {visibleEntries.length === 0 ? (
                <div style={{ fontSize: "11px", color: "var(--pet-color-muted)", fontStyle: "italic" }}>
                  {(() => {
                    const hasSearch = entrySearch.trim() !== "";
                    if (hasSearch && entryFilter !== null)
                      return `未匹配「${entrySearch.trim()}」（在 ${entryFilter} 内）`;
                    if (hasSearch) return `未匹配「${entrySearch.trim()}」`;
                    return `当日无 ${entryFilter} entry。`;
                  })()}
                </div>
              ) : (() => {
                // 超 20 条默认折叠：仅显前 20，末尾按钮切展开 / 收起。阈值
                // 来自 PanelTasks 同款"先看 head 再决定要不要全展开"惯例。
                // 排序由 entriesNewestFirst 决定（reverse 在 slice 前，让
                // newest 模式下"前 20"是 newest 20 而非 oldest 20）。
                const LIMIT = 20;
                const ordered = entriesNewestFirst
                  ? [...visibleEntries].reverse()
                  : visibleEntries;
                const overLimit = ordered.length > LIMIT;
                const displayed =
                  !entryListExpanded && overLimit
                    ? ordered.slice(0, LIMIT)
                    : ordered;
                return (
                  <>
                    {displayed.map((entry, i) => {
              const meta = MOTION_META[entry.motion];
              const color = meta ? meta.color : FALLBACK_MOTION_COLOR;
              const hhmm = entry.timestamp.length >= 16
                ? entry.timestamp.slice(11, 16)
                : entry.timestamp;
              const entryKey = `${entry.timestamp}-${i}`;
              const copied = copiedEntryKey === entryKey;
              return (
                <div
                  key={entryKey}
                  className="pet-mood-entry-row"
                  style={{
                    display: "flex",
                    alignItems: "flex-start",
                    gap: "6px",
                    lineHeight: 1.5,
                  }}
                >
                  <span
                    title={entry.timestamp}
                    style={{
                      fontFamily: "'SF Mono', 'Menlo', monospace",
                      color: "var(--pet-color-muted)",
                      flexShrink: 0,
                    }}
                  >
                    {hhmm}
                  </span>
                  <span
                    title={meta ? meta.label : entry.motion}
                    style={{
                      width: 8,
                      height: 8,
                      borderRadius: "50%",
                      background: color,
                      display: "inline-block",
                      flexShrink: 0,
                      marginTop: 5,
                    }}
                  />
                  <span style={{ color: "var(--pet-color-fg)", flex: 1, wordBreak: "break-word" }}>
                    {parseInlineMarkdown(entry.text)}
                  </span>
                  <button
                    type="button"
                    className="pet-mood-entry-copy-btn"
                    onClick={async () => {
                      const text = `${hhmm} [${entry.motion}] ${entry.text}`;
                      try {
                        await navigator.clipboard.writeText(text);
                        setCopiedEntryKey(entryKey);
                        window.setTimeout(() => {
                          setCopiedEntryKey((prev) =>
                            prev === entryKey ? null : prev,
                          );
                        }, 1500);
                      } catch (err) {
                        console.error("clipboard write failed:", err);
                      }
                    }}
                    title={
                      copied
                        ? "已复制"
                        : `复制 \`${hhmm} [${entry.motion}] ${entry.text}\` 到剪贴板`
                    }
                    style={{
                      fontSize: 10,
                      padding: "1px 6px",
                      border: "1px solid var(--pet-color-border)",
                      borderRadius: 4,
                      background: "var(--pet-color-card)",
                      color: copied ? "#16a34a" : "var(--pet-color-fg)",
                      cursor: "pointer",
                      lineHeight: 1.2,
                      flexShrink: 0,
                      // copied 态绕过 hover-only：让 ack 持续可见 1.5s
                      opacity: copied ? 1 : undefined,
                    }}
                  >
                    {copied ? "已复制" : "复制"}
                  </button>
                </div>
              );
                })}
                    {overLimit && (
                      <button
                        type="button"
                        onClick={() => setEntryListExpanded((v) => !v)}
                        style={{
                          alignSelf: "flex-start",
                          marginTop: 4,
                          fontSize: 10,
                          padding: "1px 6px",
                          border: "1px solid var(--pet-color-border)",
                          borderRadius: 4,
                          background: "var(--pet-color-card)",
                          color: "var(--pet-color-fg)",
                          cursor: "pointer",
                          lineHeight: 1.2,
                        }}
                        title={
                          entryListExpanded
                            ? "收起到前 20 条，给 sparkline 腾视觉空间"
                            : `当日还有 ${visibleEntries.length - LIMIT} 条 mood entry，点击展开`
                        }
                      >
                        {entryListExpanded
                          ? "收起，仅显前 20 条"
                          : `展开剩余 ${visibleEntries.length - LIMIT} 条`}
                      </button>
                    )}
                  </>
                );
              })()}
            </>
          )}
        </div>
      )}
    </div>
  );
}

/// 当日详情顶部的 motion 过滤 chips：包含"全部 N"+ 出现过的 motion 计数。
/// 点 chip → 选中（再点同一 chip 回"全部"）；与 sparkline 顶部的
/// MotionFilterChips 不复用：那里是 7 天全量 axis、配色固定 + 4 个全显，
/// 这里要 dynamic counts + 只显示当日出现过的 motion。
function DayMotionChips({
  counts,
  total,
  selected,
  hits,
  onChange,
}: {
  counts: Record<string, number>;
  total: number;
  selected: string | null;
  /** entrySearch + motion 双过滤后的可见条数。R85: 在选中 chip 后附 `(N 命中)` 时使用。 */
  hits: number;
  onChange: (next: string | null) => void;
}) {
  const order = ["Tap", "Flick", "Flick3", "Idle"] as const;
  const present = order.filter((m) => (counts[m] ?? 0) > 0);
  // 只 1 种 motion 时不显示 chip 行（无过滤价值）。
  if (present.length <= 1) return null;
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: "4px",
        flexWrap: "wrap",
        marginBottom: "2px",
      }}
    >
      <ChipButton
        label={`全部 ${total}`}
        active={selected === null}
        color="var(--pet-color-muted)"
        onClick={() => onChange(null)}
      />
      {present.map((m) => {
        const meta = MOTION_META[m];
        const color = meta ? meta.color : FALLBACK_MOTION_COLOR;
        const cnt = counts[m] ?? 0;
        const isActive = selected === m;
        // R85: 仅在选中 chip 且 entrySearch 又 narrowed 一档（hits ≠ cnt）时
        // 附 `(N 命中)`。motion-only 时 hits === cnt，再贴一次 15 视觉冗余。
        const hitsSuffix = isActive && hits !== cnt ? ` (${hits} 命中)` : "";
        return (
          <ChipButton
            key={m}
            label={`${meta ? meta.glyph : ""}${m} ${cnt}${hitsSuffix}`}
            active={isActive}
            color={color}
            onClick={() => onChange(selected === m ? null : m)}
          />
        );
      })}
    </div>
  );
}

function ChipButton({
  label,
  active,
  color,
  onClick,
}: {
  label: string;
  active: boolean;
  color: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      style={{
        fontSize: "10px",
        padding: "1px 6px",
        borderRadius: 4,
        border: `1px solid ${color}`,
        background: active ? color : "#fff",
        color: active ? "#fff" : color,
        cursor: "pointer",
        lineHeight: 1.4,
      }}
    >
      {label}
    </button>
  );
}

/// Sparkline 上方的 4 motion + 全部 chip 行。点击切换 selected；再点同 chip
/// 回 null（"全部"）。配色取自 MOTION_META，selected 时填充 + 白字。
function MotionFilterChips({
  selected,
  onChange,
}: {
  selected: string | null;
  onChange: (next: string | null) => void;
}) {
  const motions = ["Tap", "Flick", "Flick3", "Idle"] as const;
  const allActive = selected === null;
  return (
    <div style={{ display: "flex", gap: 4, alignItems: "center", flexWrap: "wrap" }}>
      <button
        type="button"
        onClick={() => onChange(null)}
        style={{
          padding: "1px 8px",
          fontSize: 10,
          border: "1px solid",
          borderColor: allActive ? "var(--pet-color-accent)" : "var(--pet-color-border)",
          borderRadius: 10,
          background: allActive ? "var(--pet-color-accent)" : "var(--pet-color-card)",
          color: allActive ? "#fff" : "var(--pet-color-fg)",
          cursor: "pointer",
          fontWeight: allActive ? 600 : 400,
        }}
        title="不过滤，叠加显示所有 motion"
      >
        全部
      </button>
      {motions.map((m) => {
        const meta = MOTION_META[m];
        const active = selected === m;
        return (
          <button
            key={m}
            type="button"
            onClick={() => onChange(active ? null : m)}
            style={{
              padding: "1px 8px",
              fontSize: 10,
              border: "1px solid",
              borderColor: active ? meta.color : "var(--pet-color-border)",
              borderRadius: 10,
              background: active ? meta.color : "var(--pet-color-card)",
              color: active ? "#fff" : "var(--pet-color-fg)",
              cursor: "pointer",
              fontWeight: active ? 600 : 400,
              display: "inline-flex",
              alignItems: "center",
              gap: 4,
            }}
            title={active ? "再点取消该过滤" : `只看 ${m} (${meta.label})`}
          >
            <span
              style={{
                width: 6,
                height: 6,
                borderRadius: "50%",
                background: active ? "#fff" : meta.color,
                display: "inline-block",
              }}
            />
            {m}
          </button>
        );
      })}
    </div>
  );
}

function SparklineBar({
  day,
  effectiveMax,
  filter,
  selected,
  onClick,
  halfDay,
}: {
  day: DailyMotion;
  effectiveMax: number;
  /// null = stacked 全量；非 null = 只渲染该 motion 段，且高度按该 motion 的
  /// 当日 count 缩放到 effectiveMax。
  filter: string | null;
  /// true 时柱外加 1px outline，与"sparkline 下方查当日详情"对齐视觉。
  selected: boolean;
  onClick: () => void;
  /// 早晚分段开关启用时由父级注入；本柱内拆 AM (底) / PM (顶) 两段渲染。
  /// undefined → 沿用既有单段渲染逻辑，防回归。
  halfDay?: HalfDayMotion;
}) {
  // hover 浮窗：本柱顶上方居中弹出，鼠标离开消失。pointer-events: none
  // 让浮窗本身不吃 mouseLeave 事件。三个 return 分支共享 wrap 函数。
  const [hover, setHover] = useState(false);
  // dominant motion: count 降序首个；同 count 取 MOTION_META 顺序首个让
  // 视觉稳定（频次相同时偏好"日常 / 安静"系前置）。filter 非 null 时只
  // 看该 motion；day 全空时返 null。
  const dominantOrder = ["Tap", "Flick", "Flick3", "Idle"] as const;
  const dominant = (() => {
    if (filter !== null) {
      const c = day.motions[filter] ?? 0;
      return c > 0 ? ([filter, c] as [string, number]) : null;
    }
    const entries = Object.entries(day.motions).filter(([, c]) => c > 0);
    if (entries.length === 0) return null;
    entries.sort((a, b) => {
      if (b[1] !== a[1]) return b[1] - a[1];
      const ai = dominantOrder.indexOf(a[0] as typeof dominantOrder[number]);
      const bi = dominantOrder.indexOf(b[0] as typeof dominantOrder[number]);
      // 不在 MOTION_META 顺序里的 motion (如未来扩展) 排到末尾
      const af = ai < 0 ? 99 : ai;
      const bf = bi < 0 ? 99 : bi;
      return af - bf;
    });
    return entries[0] as [string, number];
  })();
  const popover = hover ? (
    <div
      style={{
        position: "absolute",
        bottom: "100%",
        left: "50%",
        transform: "translateX(-50%) translateY(-4px)",
        background: "var(--pet-color-card)",
        border: "1px solid var(--pet-color-border)",
        borderRadius: 4,
        boxShadow: "0 1px 4px rgba(0,0,0,0.1)",
        padding: "4px 8px",
        fontSize: 11,
        lineHeight: 1.4,
        whiteSpace: "nowrap",
        zIndex: 10,
        pointerEvents: "none",
        color: "var(--pet-color-fg)",
      }}
    >
      <div style={{ fontFamily: "'SF Mono', 'Menlo', monospace", color: "var(--pet-color-muted)" }}>
        {day.date}
      </div>
      {dominant ? (
        <>
          <div style={{ display: "flex", alignItems: "center", gap: 4 }}>
            <span
              style={{
                width: 8,
                height: 8,
                borderRadius: "50%",
                background:
                  MOTION_META[dominant[0]]?.color ?? FALLBACK_MOTION_COLOR,
                display: "inline-block",
              }}
            />
            <span style={{ fontWeight: 600 }}>{dominant[0]}</span>
            <span>× {dominant[1]}</span>
          </div>
          <div style={{ color: "var(--pet-color-muted)" }}>
            共 {day.total} 次 · 点击展开
          </div>
        </>
      ) : (
        <div style={{ fontStyle: "italic", color: "var(--pet-color-muted)" }}>
          当日无记录 · 点击仍可展开
        </div>
      )}
    </div>
  ) : null;
  const filterCount = filter === null ? day.total : day.motions[filter] ?? 0;
  const segments: ReadonlyArray<readonly [string, number]> =
    filter === null
      ? Object.entries(day.motions).filter(([, c]) => c > 0)
      : filterCount > 0
        ? [[filter, filterCount] as const]
        : [];
  const heightPx =
    effectiveMax > 0
      ? Math.max(1, Math.round((filterCount / effectiveMax) * SPARKLINE_BAR_HEIGHT))
      : 1;
  // 空日 / filter 命中 0：画一条 1px baseline 让"沉默日"也有视觉占位，不要
  // 让用户以为是渲染 bug。
  if (filterCount === 0) {
    const emptyTooltip =
      filter === null
        ? `${day.date} · 没有记录（点击展开当日 entry —— 但已是空）`
        : `${day.date} · 没有 ${filter} 记录`;
    return (
      <div
        style={{
          flex: 1,
          position: "relative",
          display: "flex",
          alignItems: "flex-end",
        }}
        onMouseEnter={() => setHover(true)}
        onMouseLeave={() => setHover(false)}
      >
        {popover}
        <div
          title={emptyTooltip}
          onClick={onClick}
          style={{
            width: "100%",
            height: "1px",
            background: "var(--pet-color-border)",
            cursor: "pointer",
            outline: selected ? "1px solid var(--pet-color-accent)" : "none",
            outlineOffset: 1,
          }}
        />
      </div>
    );
  }

  // 早晚分段渲染：AM 段在底（column-reverse 把首个子节点放底）+ 1px 白线
  // 分隔 + PM 段在顶。子段高度用 flex grow 比 amTotal/pmTotal 自动分配，
  // 数学上仍 = day.total（在 backend `summarize_motions_by_half_day` 单测
  // 覆盖），所以整柱高度 heightPx 不变 —— 切换 split 不会让柱子跳一下。
  if (halfDay) {
    const amSegs: Array<[string, number]> =
      filter === null
        ? Object.entries(halfDay.am).filter(([, c]) => c > 0)
        : (halfDay.am[filter] ?? 0) > 0
          ? [[filter, halfDay.am[filter]]]
          : [];
    const pmSegs: Array<[string, number]> =
      filter === null
        ? Object.entries(halfDay.pm).filter(([, c]) => c > 0)
        : (halfDay.pm[filter] ?? 0) > 0
          ? [[filter, halfDay.pm[filter]]]
          : [];
    const amTotal = amSegs.reduce((s, [, c]) => s + c, 0);
    const pmTotal = pmSegs.reduce((s, [, c]) => s + c, 0);
    const tooltipParts = [
      amTotal > 0 ? `AM ${amTotal}` : null,
      pmTotal > 0 ? `PM ${pmTotal}` : null,
    ].filter((x): x is string => x !== null);
    const tooltip = `${day.date} · ${tooltipParts.join(" · ")}（共 ${filterCount} 次；点击展开当日 entry）`;
    const renderStack = (segs: Array<[string, number]>, subTotal: number) => (
      <div
        style={{
          flexGrow: subTotal,
          flexShrink: 0,
          flexBasis: 0,
          minHeight: 0,
          display: "flex",
          flexDirection: "column-reverse",
        }}
      >
        {segs.map(([motion, count]) => {
          const meta = MOTION_META[motion];
          const color = meta ? meta.color : FALLBACK_MOTION_COLOR;
          return (
            <div
              key={motion}
              style={{
                flexGrow: count,
                flexShrink: 0,
                flexBasis: 0,
                minHeight: 0,
                background: color,
              }}
            />
          );
        })}
      </div>
    );
    return (
      <div
        style={{
          flex: 1,
          position: "relative",
          display: "flex",
          alignItems: "flex-end",
        }}
        onMouseEnter={() => setHover(true)}
        onMouseLeave={() => setHover(false)}
      >
        {popover}
        <div
          title={tooltip}
          onClick={onClick}
          style={{
            width: "100%",
            height: `${heightPx}px`,
            display: "flex",
            flexDirection: "column-reverse",
            borderRadius: "2px",
            overflow: "hidden",
            cursor: "pointer",
            outline: selected ? "1px solid var(--pet-color-accent)" : "none",
            outlineOffset: 1,
          }}
        >
          {amTotal > 0 && renderStack(amSegs, amTotal)}
          {amTotal > 0 && pmTotal > 0 && (
            <div style={{ height: 1, background: "var(--pet-color-border)", flexShrink: 0 }} />
          )}
          {pmTotal > 0 && renderStack(pmSegs, pmTotal)}
        </div>
      </div>
    );
  }

  const tooltip =
    filter === null
      ? `${day.date} · ${segments
          .map(([m, c]) => `${m} × ${c}`)
          .join("、")}（共 ${day.total} 次；点击展开当日 entry）`
      : `${day.date} · ${filter} × ${filterCount}（点击展开当日 entry）`;
  return (
    <div
      style={{
        flex: 1,
        position: "relative",
        display: "flex",
        alignItems: "flex-end",
      }}
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
    >
      {popover}
      <div
        title={tooltip}
        onClick={onClick}
        style={{
          width: "100%",
          height: `${heightPx}px`,
          display: "flex",
          flexDirection: "column-reverse", // 从底部往上堆叠，最常见 motion 在底
          borderRadius: "2px",
          overflow: "hidden",
          cursor: "pointer",
          outline: selected ? "1px solid var(--pet-color-accent)" : "none",
          outlineOffset: 1,
        }}
      >
        {segments.map(([motion, count]) => {
        const meta = MOTION_META[motion];
        const color = meta ? meta.color : FALLBACK_MOTION_COLOR;
        // 段高度按 filterCount（== filter ? 单 motion count : day.total）归一，
        // 让 filter 模式下唯一段填满整柱（视觉上柱子高度 = 当日 X motion
        // count / 全 7 天 max X count，与 stacked 模式高度语义对偶）。
        const segHeight = filterCount > 0 ? (count / filterCount) * 100 : 0;
        return (
          <div
            key={motion}
            style={{
              height: `${segHeight}%`,
              background: color,
              minHeight: "1px", // 单条段不至于因 round 消失
            }}
          />
        );
      })}
      </div>
    </div>
  );
}

/// 把 `YYYY-MM-DD` 转成紧凑标签：今天显示「今」，昨天显示「昨」，其它显示
/// `M/D`。本地日期判断走客户端 Date —— 与后端聚合用的 `chrono::Local` 同时区，
/// 在用户日常使用场景下不会出现"图上是今天但 label 显示昨天"的错位。
function shortDateLabel(isoDate: string): string {
  const today = new Date();
  const todayStr = `${today.getFullYear()}-${pad(today.getMonth() + 1)}-${pad(today.getDate())}`;
  if (isoDate === todayStr) return "今";
  const yesterday = new Date(today);
  yesterday.setDate(today.getDate() - 1);
  const yStr = `${yesterday.getFullYear()}-${pad(yesterday.getMonth() + 1)}-${pad(yesterday.getDate())}`;
  if (isoDate === yStr) return "昨";
  const parts = isoDate.split("-");
  if (parts.length !== 3) return isoDate;
  return `${parseInt(parts[1], 10)}/${parseInt(parts[2], 10)}`;
}

function pad(n: number): string {
  return n < 10 ? `0${n}` : `${n}`;
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
        background: "var(--pet-color-card)",
        border: "1px solid var(--pet-color-border)",
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
            color: "var(--pet-color-fg)",
          }}
        >
          {title}
        </h3>
        {subtitle && (
          <p style={{ margin: "2px 0 0", fontSize: "11px", color: "var(--pet-color-muted)" }}>{subtitle}</p>
        )}
      </header>
      {children}
    </section>
  );
}
