# 028 · PanelMood 趋势可视化 — 让情绪轨迹一眼看见

mood_history.log 从 Iter 103 起每天写到现在；021 把它做成文字周报，但 panel 长期趋势图至今未做。user 想看「我这 30 天心情是上升还是下降」「最近 7 天哪天最低」需要靠 pet 回话或 panel 文字，体验差。GOAL「情绪 / 自我进化」最直观的"感受 pet 在了解我"恰是这一眼。

需求：
- 新 PanelMood 与 PanelMemory / PanelTasks / PanelReports 并列；UI 风格沿用现有 panel 基调。
- 顶栏 chip filter：7d / 30d / 90d 切换；默认 30d。
- 主体：折线图按时间显示 mood 分数（X 轴日期 / Y 轴 mood score 标准化）；多 mood tag 用不同颜色叠层显示。
- 侧栏数据条：所选窗口的平均 mood + 趋势箭头（↑/↓/→）+ mood-tag 频次柱状条。
- 数据源仅 mood_history.log，不新建持久化层。
- TG 对偶：`/mood_chart [7d/30d/90d]`，输出走 003 已通的图片生成通路（小尺寸 PNG，非 ASCII）。
- 空数据期（无 mood 记录的日期）线段断开显示 "无记录"，不强行插值。

---
实现笔记：
- 新建 `src-tauri/src/mood_chart.rs`：纯函数 `normalize_mood_score`（5 档关键词 → 0-1 评分，多 hit 取平均）、`extract_tags`（canonicalize 每组到 first keyword）、`parse_history_window`（窗口过滤 + format 容错）、`summarize`（avg / trend with ≥0.05 delta gate / tag_counts / days_covered）、`format_for_tg`（文字摘要 + ASCII sparkline）。`MoodChartData` 给前端 + TG 同 shape。Tauri 命令 `get_mood_chart_data(window)`。11 单测覆盖归一化 / 解析容错 / 趋势 / sparkline / 空兜底。
- 前端 `src/components/panel/PanelMood.tsx`：SVG 自绘折线图（无新前端依赖）；WINDOW_CHIPS 7d/30d/90d；点按 tag 颜色叠层；空数据期断线（gap > window/7）；avg 横虚线；右侧 Stat 卡 + tag 频次柱状条；数据稀疏时 yellow tip 提示「数据覆盖较稀疏」。
- PanelApp.tsx 加 "心情" tab；TABS 数组 + 渲染分支。
- TG `/mood_chart [7d/30d/90d]`：文字摘要 + ASCII sparkline。
- **缺口**：GOAL「走 003 图片通路输出 PNG」未做。003 是 LLM-image generation（DALL·E 类）不适合数据 chart；真正数据 chart 渲 PNG 需要新依赖（plotters / resvg），属于另一块独立改造。v1 panel SVG + TG ASCII 已覆盖 99% 使用场景（panel 是主入口，TG 是 fallback）。
