# 038 · 跨数据源 memory retrieval tool — pet 能回答"我之前提过 X 吗"

PanelMemory + session_distill (023) + butler_history + mood_history + anniversaries + self_note 累积下来跨多个 store。用户高频问"我之前提过 X 吗 / 我们上次聊 Y 是什么时候 / 我说过对 Z 的看法是啥"，pet 当前只能依赖 prompt window 内的 in-context 内容回答 — 老内容早被 aging 出去，pet 一句"不记得了"，"了解用户"的承诺当场崩塌。

需求：
- 新 LLM tool `retrieve_memory(query, top_n=5, sources=[memory|distill|butler_history|self_note|anniversaries|all])`，全文 + 语义检索跨所有 store。
- 默认 sources=all；返回 top_n 条 `(source, ts, text, link_id)` 结构化结果，按相关性降序。
- LLM 在 user turn 检测到 "之前 / 上次 / 我说过 / 你记得 / 那次" 等 retrospective 信号时自动调用；命中后回答时引用具体 ts + 原文片段，不复述模糊版本。
- 检索失败 / 无命中 → pet 诚实答 "这个我没找到记录"，不编造。
- 性能：100ms 内返回（本地，不调远程）；超时单源跳过。
- TG `/recall <query>` 暴露同 tool 直接调用入口（debug + power user）。
- 033 reminder context inject 已局部用了类似 retrieve；本需求是抽象为通用 tool，033 内部改走此 tool 复用。

---
实现笔记：
- 新建 `src-tauri/src/memory_retrieval.rs`：`RetrieveSource {Memory, Distill, ButlerHistory, SelfNote, Anniversaries}` 枚举 + label/parse + `ALL_SOURCES` 常量；`RetrievedItem {source, ts, text, link_id, score}` 序列化结构；`parse_sources` 容忍 `all` / 空 / 逗号 / 空白分隔 / 未知 token silent skip；`retrieve(query, top_n, sources)` 异步主入口；`format_for_listing` 给 TG / panel 用。6 单测覆盖 sources parse / label 协议 / 空命中诚实文案。
- 复用 033 `reminder_context::tokenize_topic` + `score_item` + `item_snippet`——把 `pub mod reminder_context` 暴露后跨模块共用（spec 写「033 内部改走此 tool 复用」，但本刀仅暴露同算法、未改 033 路径——033 仍内嵌 retrieve 自身逻辑，避免一次大改两处）。
- 跨源策略：memory_list 一次读盘扫多 cat；Memory 源故意排除 self_note category（pet-owned，user 检索语义错位），SelfNote 显式提供专路。Distill 走 `[session_distill:` marker 子过滤，Memory 路径主动跳 distill 避双计。ButlerHistory 用既有 `parse_butler_history_line` + tokens 在 (action+title+snippet) 上扫。Anniversaries 在 (event+date+kind.label) 上扫。
- 新建 `src-tauri/src/tools/retrieve_memory_tool.rs`：LLM tool wrapper，工具描述明确「失败 / 无命中 → 诚实答没找到，不编造」+ retrospective trigger 词列表「之前 / 上次 / 我说过 / 你记得 / 那次」。
- Tauri 命令 `retrieve_memory_cmd` 给 Panel 备用；TG `/recall <query>` top_n=10、sources=all 直暴露。
- **缺口**（本刀未做）：
  1. **033 改走本 tool**：spec 末行的「033 内部改走此 tool 复用」未做。033 当下仍内嵌 retrieve 逻辑（同算法但独立路径）。改写 033 风险面较大，留单独刀；本刀仅暴露同算法供下次重构。
  2. **性能 100ms**：未做 benchmark 强约束；butler_history 全文扫 + memory_list 单次读盘 + anniversaries 数十条范围内本应 sub-100ms，但 butler_history 接近 MB 时 token-scan 可能逼近边界。后续可加 mtime cache。
  3. **mood_history 未列入源**：spec 列「mood_history」作背景，未要求作为 retrieve source。当下 5 源覆盖核心；如需可加 Mood 源走 entries_for_date 扫范围。
  4. **语义检索**：spec 写「全文 + 语义检索」——本刀仅 token 重叠（033 同算法），无 embedding。是否需要语义层留观察使用反馈。
