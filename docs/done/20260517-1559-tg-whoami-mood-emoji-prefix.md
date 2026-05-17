# TG bot `/whoami` reply 加心情 emoji 前缀（iter #311）

## Background

`/whoami` 命令第一行是「🪪 /whoami」纯标签 — 心情信号在第三行「💗 现在
的心情：xxx」才出现。owner 在 TG 视线第一眼扫到的是 paw + name + 天数，
等"心情"信号要往下读三行。

本迭代把宠物当前心情 emoji 提到第一行作为前缀（如 `😊 🪪 /whoami`），
让 owner 一眼看到宠物现在啥状态 — 与 PanelMemory MoodWidget 在桌面顶部
的可见性对偶。

## Changes

仅 `src-tauri/src/telegram/commands.rs`：

- 新 pure helper `mood_emoji_for(text: &str) -> &'static str`：
  - case-insensitive 子串匹配 mood 文本中的关键词
  - 中英 keyword 同表（"开心"/"happy" → 😊；"兴奋"/"excited" → 🤩；
    "难过"/"sad" → 😢；"困"/"tired" → 😴 等共 14 组语义）
  - 表内顺序 = 优先级（"兴奋"先于"开心" — 强信号优先）
  - 无匹配 → 兜底 🐾（paw）让 caller 无需 Option 处理
- `format_whoami_reply` 第一行：mood 非空时改为
  `{mood_emoji_for(text)} 🪪 /whoami`；mood 空或 None 时保持纯
  `🪪 /whoami`（保 backwards-compat 不引入 🐾 兜底前缀）
- 6 个新 unit test：
  - mood_emoji：中文关键词矩阵 / 英文 case-insensitive / 未知 mood 兜底
    🐾 / 空串兜底
  - whoami header：mood 命中时前缀 emoji + 保留 "🪪 /whoami" 标签 / 未
    知 mood 文本走 🐾 / 无 mood 时保持原 plain header

## Key design decisions

- **兜底 🐾 而非 Option**：caller 不必判空 — mood_emoji_for 总返一个
  `&'static str`，减少 if-let / unwrap_or 噪音。🐾 是 pet 全局识别 emoji，
  对未识别 mood text 是温和兜底（不显得"出错"）。
- **mood 空 / None 时不加 🐾 前缀**：保持 backwards-compat — 既有
  `whoami_reply_skips_missing_sources` / `whoami_reply_zero_days_says_today`
  测试就是 None mood + 不渲染"现在的心情"行；如果我无条件加 🐾 前缀
  这些测试就要改。语义上"没 mood signal"和"有 mood 但未知"是两回事 —
  前者不该假装显个兜底 emoji。
- **保留既有「💗 现在的心情：xxx」第三行**：与新前缀互补 —— emoji
  prefix 是顶端 glance；第三行是完整 mood text + motion 详情。owner 想
  知道具体什么心情仍能往下读。
- **keyword 表内顺序 = 优先级**：mood 文本可能多关键词命中（"今天很开
  心也有点兴奋"），按表顺序"兴奋"先于"开心"返 🤩 — 强情绪优先反映宠物
  当下能量。表注释明确"最具体 → 最泛"。
- **不引入 LLM emoji 选择**：mood emoji 选择走纯静态表 — 让回复确定性
  + 零延迟，与 /whoami 本身 read-only 性质对齐。owner 想要 LLM 风格的
  rich mood 描述会走 /mood 命令（另一条路径）。

## Verification

- `cargo test --lib`（backend）— 1153 passed / 0 failed（6 新测试通过）
- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.19s)
