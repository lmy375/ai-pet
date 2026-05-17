# PanelDebug「⏰ 最近主动开口」chip 行（iter #384）

## Background

PanelDebug 已有 speech timeline tab（activeTimeline === "speech"）显
recentSpeeches 列表 + HH:MM + 全文。但藏在 tab 后 — owner 想"瞄一眼
pet 最近何时开口"需点 tab 切过去。

TODO 还希望"显触发原因（feedback_band / cooldown）" — 但 speech_history.log
schema 没存 per-speech trigger 元数据；要做需扩 entry 字段（every
proactive cycle 写 speech 时同时写 band/cooldown），scope 超 iter
范围。

Pivot：本 iter 仅做"何时"半边 — discoverability chip 行让 owner
不必切 tab 即可瞄到最近 5 次开口时刻；hover title 显完整 text +
相对时间；click chip 切到 speech tab 看全表。触发"为何"维度由
PanelToneStrip 已有的 feedback_summary / cooldown_breakdown chip
（上方）承担当前态展示。

## Changes

### `src/components/panel/PanelDebug.tsx`（~line 4091，timeline tab 行之前）

```tsx
{recentSpeeches.length > 0 && (
  <div style={{ ...purple-tint bg, padding: "6px 16px" }}>
    <span>⏰ 最近开口</span>
    {recentSpeeches.slice(-5).map((line, i) => {
      const [ts, text] = parse(line);
      const tShort = ts.slice(11, 16);
      const ageMin = (Date.now() - Date.parse(ts)) / 60000;
      const ageLabel = ageMin < 60 ? `${ageMin}m 前` : ...;
      const preview = text.slice(0, 16) + (text.length > 16 ? "…" : "");
      return (
        <button onClick={() => setActiveTimeline("speech")}
                title={`${tShort} (${ageLabel})\n\n${text}\n\n点击切到...`}>
          {tShort} · {preview}
        </button>
      );
    })}
    {recentSpeeches.length > 5 && <span>+{N-5}</span>}
  </div>
)}
```

设计要点：
- **空 speeches 时整行不渲**：无内容显占空间（同 ChatMini ambient
  hint #383 同模式）
- **slice(-5)**：最新 5 条；右侧 +N 显总数 - 5
- **purple tint**：与既有 speech timeline tab 同色族（用户切换 tab
  视觉一致）
- **chip max-width 160 + ellipsis**：preview 截断 16 字 + 再 CSS
  ellipsis 防长行挤排
- **click → setActiveTimeline("speech")**：直接跳到速 speech tab
  看 full text，避免双重 UI 维护（chip 只是 sneak peek）
- **hover title 拼三段**：HH:MM (Nm 前) + full text + 跳转 hint —
  覆盖 owner 三个想知道的点

## Key design decisions

- **不显 per-speech feedback_band / cooldown**：speech_history.log
  entry shape 是 `"timestamp text"` 单行，无元数据字段。要做 per-
  speech trigger 需后端扩 entry shape + 所有 caller 同步写 — TODO
  原文要的，但 scope 超本 iter（涉及 speech_history.rs 接口签名 +
  proactive cycle 内 6+ 调用点）。
- **discoverability 优先于 trigger 元数据**：先让 owner 看到"何时
  开口"列表（无门槛），需要更深 audit 再切 speech tab + 上方
  ToneStrip 当前 band 信息。两层渐进披露。
- **chip 行位置 timeline tab 之前**：tab 是"看 X 类历史"分类入口；
  本 chip 行是"speech 类专属 sneak peek"自然属于 tab 上方（一眼
  preview → 想看全表点 chip 切 tab）。
- **不引入"触发上下文"伪信息**：用 current tone snapshot 作所有
  speech 的 trigger reason 会误导（owner 以为 chip[3] 时的 band
  就是显示的当前 band，实际不是）。诚实显仅"何时"+ 跳转引导，让
  PanelToneStrip 承担当前态。

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.26s)
- 后端无改动 — 复用既有 recentSpeeches state + setActiveTimeline
