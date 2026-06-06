# 022 · task 时间表达 ambiguity confirm — 落库前先问清楚

reminder / 011 / 012 / 020 task 创建 LLM 路径接到模糊时间词（"下周末"、"傍晚"、"晚一会儿"、"下个月"、"过两天"）时，目前静默猜一个具体值 — 猜错 user 重设很烦，是管家信任度的直接伤害。

需求：
- task 解析阶段命中模糊词（预设词集 + 长度/精度启发式）→ pet 反问澄清，例："下周末是说周六还是周日？" / "傍晚指 18:00 左右吗？" / "过两天是说后天还是大后天？"。
- 候选不超过 3 个；user 回复任意候选 / 编号 / 自定义具体时间均接受。
- 一次澄清未明确 → 再问一次，仍未明确 → 落"最早合理候选"并明告 "我先按周六晚 8 点定了，要改告诉我"，不无声丢。
- 精确时间表达（"今晚 21:00"、"明天 14:00"、绝对日期）跳过反问，直接落。
- 模糊词集做常量集中可调；不暴露给用户配置。
- 跨 reminder / 011 / 012 / 020 共用同一澄清模块，不四处重写。

---
实现笔记：
- 新建 `src-tauri/src/time_ambiguity.rs`：常量 `AMBIGUOUS_TIME_WORDS`（30+ 中英常见模糊词）+ pure `find_ambiguous_words`（权威检测器，留给将来 Rust-side audit）+ `inject_time_ambiguity_layer` 注入层。规则文本明确教 LLM：≤3 候选反问 / 二轮 fallback 「我先按 X 定了，要改告诉我」/ 精确时间跳过反问。
- 注入挂在 chat.rs + bot.rs::run_chat_turn 两条 user-driven 路径；proactive 自跑路径不挂（不存在 user 给模糊时间词场景）。reminder / 011 / 012 / 020 task 创建工具调用都在这些路径下游，自然共用同一份规则。
- 7 单测：中文 / 英文 / 精确时间不命中 / 去重 / 注入位置正确 / 通用时段词覆盖。
- `find_ambiguous_words` 暂无 runtime caller（依赖 LLM 自判），用 `#[allow(dead_code)]` 标注保留意图。
