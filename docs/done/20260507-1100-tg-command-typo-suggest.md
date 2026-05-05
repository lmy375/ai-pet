# TG bot 命令拼写纠错提示 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> TG bot 命令拼写纠错提示：用户输 `/tsks` 等错命令时，bot 现回 "未知命令"；改成尝试 levenshtein 距 ≤ 2 的 fuzzy 匹配并建议 "你是想发 /tasks 吗？"，避免来回猜命令名。

## 目标

`format_unknown_command` 当前只回 "未知命令 /xxx。输入 /help 查看可用命令。"
用户输错（漏字母 / 顺序错 / typo）必须再回 /help 翻一遍。本轮加 levenshtein
距离 ≤ 2 的 fuzzy 匹配，把最近的合法命令名作为建议提示出去。

## 非目标

- 不自动执行建议命令 —— 只提示，避免误判执行用户没想做的事（"/tasm" → 
  系统帮我跑 /tasks 但其实我想搞别的）。
- 不做"多个候选都列出" —— 距 ≤ 2 通常只有 1 个真候选；多了反而像乱猜。
- 不依赖外部 levenshtein crate —— 命令名最长 6 字符（"cancel"），手写 DP
  内核极简（5 行），引外部库性价比低。

## 设计

### 纯函数

```rust
/// pure：计算两字符串的 Levenshtein 编辑距离（byte-level）。
/// 命令名都是 ascii lowercase，按 byte 即可；不需要 unicode-aware。
pub fn levenshtein(a: &str, b: &str) -> usize;

/// pure：从 valid_names 里找与 unknown 距离 ≤ MAX 的最近命令；返回 Some 仅
/// 当唯一最近且严格优于"啥都不像"。空 / 全不像 → None。
pub fn suggest_command(unknown: &str, valid: &[&str]) -> Option<&'static str>;
```

实现要点：
- `levenshtein` 标准 DP 但只保留两行（O(min(a, b)) 空间）—— 命令名 ≤ 6
  字符，性能不是事，主要是代码清晰。
- `suggest_command` 阈值用常量 `SUGGEST_MAX_DISTANCE = 2`；超出 → None。
- 距离相同时取 `valid` 数组中**第一个**（顺序由 `tg_command_registry()`
  保证，"task" 在前）。这避免"两个候选距离一样"的歧义模糊化。
- `valid` 必须是 `&[&'static str]` 让返回类型能 `&'static`；调用方传入
  `tg_command_registry()` 的 name 列表（已是 `&'static`）。

### 反馈文案

`format_unknown_command(name, suggestion)` 改签名加 `Option<&str>`：

- 无建议 → 现有文案不变（"未知命令 /xxx。输入 /help 查看可用命令。"）
- 有建议 → 在前面加一行 "你是不是想发 /xxx 吗？"，再换行接现有文本

```
你是不是想发 /tasks 吗？
未知命令 /tsks。输入 /help 查看可用命令。
```

把建议放第一行 —— TG 客户端在通知预览时常只显第一行，"建议" 比 "未知"
更有价值放在最前。

### handler 接入

`bot.rs::handle_tg_command` 的 Unknown 分支：

```rust
TgCommand::Unknown { name } => {
    let valid: Vec<&str> = tg_command_registry().into_iter().map(|(n, _)| n).collect();
    let suggestion = suggest_command(&name, &valid);
    format_unknown_command(&name, suggestion)
}
```

### 测试

- `levenshtein`: 0 距离 / 单字符 / 全不同 / 边界（一空一非空）
- `suggest_command`: 距 1 命中 / 距 2 命中 / 距 3 不命中 / 多候选取首个 /
  unknown="" / valid=[] 边界
- `format_unknown_command_with_suggestion` 包含 suggestion 文案
- handler 路径手测覆盖（无 vitest）

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | levenshtein + 单测 |
| **M2** | suggest_command + 单测 |
| **M3** | format_unknown_command 加 suggestion 形参 + 单测 + 修既有调用 |
| **M4** | bot.rs handler 接入 |
| **M5** | cargo test + tsc + build + cleanup |

## 复用清单

- 既有 `tg_command_registry()` 提供 valid name 集合
- 既有 `format_unknown_command` 文案 spirit

## 进度日志

- 2026-05-07 11:00 — 创建本文档；准备 M1。
- 2026-05-07 11:10 — M1 完成。`levenshtein(a, b)` 标准 DP 实现（双行 O(min) 空间）+ 4 个单测：identical / empty / 单次编辑 / 典型 typo。
- 2026-05-07 11:15 — M2 完成。`suggest_command(unknown, valid)` + `SUGGEST_MAX_DISTANCE=2` 常量；空 input / 空 valid 返 None；4 个单测覆盖命中 / 越阈值 / tie 取首 / 边界。
- 2026-05-07 11:20 — M3 完成。`format_unknown_command(name, suggestion: Option<&str>)` 加形参；建议放第一行（TG 通知预览常只显首行 → 让 hint 优先曝光）；既有 unknown_lists_available_commands 测试更新；新加 unknown_with_suggestion_puts_hint_in_first_line 测试。
- 2026-05-07 11:25 — M4 完成。bot.rs handler `TgCommand::Unknown` 分支：从 tg_command_registry 拿 valid 名单 → suggest_command → format_unknown_command。
- 2026-05-07 11:30 — M5 完成。修一处 tie 测试（"tasx" vs task/tasks 实际不 tie，改用人造 abc/abx/aby）；`cargo build` 7.67s 通过；`cargo test --lib` 957 通过（含新增 9 测）；`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过。归档至 done。
