# PanelDebug「🚦 LLM 调用耗时直方图」inline 面板（iter #294）

## Background

owner 经常感知"宠物变慢"但缺乏数据 — llm.log 是 JSON-Lines 但要 grep /
tail 才能看；既有 LlmLogView 显逐条详情但没分布视图。让 owner 在主观感知
前用数据印证："上周 p50 1.2s，这周 3.5s — 真变慢了"。

本迭代加 🚦 latency toolbar 按钮 + inline 直方图面板：拉最近 50 条 LLM call
的 `total_latency_ms` → 6 段 bucket（< 500ms / 500ms-1s / 1-2s / 2-5s /
5-10s / 10s+）+ p50 / p95 / max 头部统计。

## Changes

仅 `src/components/panel/PanelDebug.tsx`：

- **state**：
  - `llmLatencyPanelOpen: boolean` — 折叠 / 展开态
  - `llmLatencies: number[] | null` — 解析后的 latency 数组（null = 还没拉
    过；空数组 = 拉了但 log 空 / 全失败 parse）
  - `llmLatencyFetching: boolean`

- **`fetchLlmLatencies` useCallback**：调既有 `get_llm_logs { limit: 50 }`
  → 逐行 `JSON.parse` → 收集 `total_latency_ms`（非 number 跳过）→ setState

- **toolbar 按钮**："🧪 LLM tools" 之后插「🚦 latency」按钮：
  - click → toggle + 首次打开 lazy fetch
  - blue tint when open

- **inline 面板**（在 LLM tools 面板之上）：
  - 头部：`🚦 近 N 次 LLM call 耗时` + `p50 X · p95 Y · max Z`（unit 自适应
    ms / s）+ 🔄 刷新按钮
  - 6 段 horizontal bar 列表，每行：`{label}` 80px 右对齐 + flex bar +
    count 36px 右对齐
  - bar 宽度按各 bucket count / maxCount 归一化；色按 bucket 区间分三
    档（green < 1s / amber 1-5s / red > 5s）；count == 0 时 opacity 0.2
  - 空 ms 数据 → "log 文件还没 LLM call 记录" friendly hint

## Key design decisions

- **6 段非线性 bucket**：1s 以下 owner 觉得"快"（绿）；1-5s "可以接受"
  （amber）；5s+ "太慢了"（red）。三档色 + 6 个区间让 owner 一眼看分布。
- **lazy fetch + 手动刷新**：log 文件可能不断增长，每 2s 自动 poll 浪费
  IO；owner 想看时 toggle 一次拉 50 条够用。手动 🔄 重新拉防 stale。
- **p50 / p95 / max 头部统计**：bucket 给分布，分位数给摘要。两者结合让
  owner 既看到"长尾在哪儿"又看到"中位数 / 极值"。
- **复用 get_llm_logs 既有命令**：不需要新后端；解析在前端做（latency 数
  组 50 条 reduce 极快，不需要 worker）。
- **位置：在 LLM tools 面板之上**：toolbar 序 🚦 在 🧪 之后，但 inline
  render 序倒过来 —— 两面板平级同根，谁先放都行；选 latency 在上是因为
  "性能信号"通常是 owner 排查时第一眼想看的（决定要不要继续看 tools）。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.26s)
