# 工具风险设置面板 — 开发计划

> 对应需求（来自 docs/TODO.md「已确认」）：
> 工具风险设置面板：把 tool_risk 等级暴露到设置页，可逐项配置自动通过或人工审核。

## 目标

把现有 `tool_risk` 模块对每个工具的"自动分级 → 自动审核 / 自动放行"决策**暴露给用户**，让用户能逐项覆盖：
- 默认（Auto）：跟着分类器走（`assess_tool_risk` 决定 `requires_human_review`）
- 总是放行：哪怕分类器说该审核也直接执行
- 总是审核：哪怕分类器说放行也弹审核

设置面板需要呈现：每个工具的名字 + 自动判定的"基线风险"标签 + 当前覆盖模式下拉。

## 非目标

- 不改 `assess_tool_risk` 的内部分级 — 它仍是真相源；本轮只在它的输出之上叠用户偏好层。
- 不暴露 MCP 工具 — MCP 工具列表是动态的（按已配置的 server 注册），UI 抓不到稳定列表；本轮先 cover 内置工具。MCP 工具继续按分类器默认 Medium 处理。
- 不改 ToolReviewRegistry 的审核流（registry / receiver / panel modal 不动）—— 仅改"是否进 registry"的入口判定。
- 不做"按 args 模式精细化覆盖" —— 用户能选的粒度就是工具名。memory_edit 的 delete vs create 仍由分类器自己判，再过一遍用户的工具级覆盖。

## 设计

### 配置形态

新增字段 `AppSettings.tool_review_overrides: HashMap<String, String>`：

```yaml
tool_review_overrides:
  bash: always_review
  read_file: auto
  write_file: always_approve
```

值是字符串（不是 enum）—— 让未知值自动退回 Auto，前向兼容。

合法值：
- `auto`（默认，等同于不设置）
- `always_review`（强制审核，用户必须 panel 点同意）
- `always_approve`（强制放行，不进 registry）

### 纯函数：`effective_requires_review`

新建 `src-tauri/src/tool_review_policy.rs`：

```rust
pub enum ToolReviewMode { Auto, AlwaysReview, AlwaysApprove }

pub fn parse_mode(s: &str) -> ToolReviewMode { ... }

pub fn effective_requires_review(auto_required: bool, mode: ToolReviewMode) -> bool {
    match mode {
        ToolReviewMode::AlwaysApprove => false,
        ToolReviewMode::AlwaysReview => true,
        ToolReviewMode::Auto => auto_required,
    }
}
```

调用点（chat.rs）从 `assessment.requires_human_review` 改成：

```rust
let mode = parse_mode(settings.tool_review_overrides.get(tc_name).map(String::as_str).unwrap_or("auto"));
let needs_review = effective_requires_review(assessment.requires_human_review, mode);
```

### 工具基线风险标签（pure）

`tool_review_policy.rs` 加：

```rust
/// 给面板用的"工具的基线风险"短描述。比 assess_tool_risk 的逐次输出
/// 更稳定 —— 不依赖 args，回答"这个工具最严重时风险多大、为什么"，
/// 让用户能凭这个决定要不要覆盖。
pub fn nominal_risk_label(tool_name: &str) -> NominalRisk {
    NominalRisk { level: "high"/"medium"/"low", note: "..." }
}
```

为常见工具硬编码：bash/write_file → high；edit_file → medium；memory_edit → mixed (delete=high, create/update=medium)；read_file/get_*/memory_list/memory_search/check_shell_status → low；propose_task → low。

未识别工具 → medium (与分类器兜底一致)。

### Tauri 命令：`get_tool_risk_overview`

返回 `Vec<{name, nominal_risk, note, mode}>`，前端直接渲染。

`tools` 字段：枚举内置工具的稳定名字。维护一份 `BUILTIN_TOOL_NAMES: &[&str]` 常量在 `tools::registry`。

### Panel UI

`PanelSettings.tsx` 加一段「工具风险」section：
- 每行：工具名 / 风险徽章（颜色按 high/medium/low）/ 备注 / 模式下拉
- 改完后正常通过 `save_settings` 持久化

不做"批量重置"按钮 — v1 让用户逐项选；如果反馈需要再加。

## 阶段划分

| 阶段 | 范围 | 状态 |
| --- | --- | --- |
| **M1** | `tool_review_policy.rs` 纯函数 + 单测 | ✅ 完成（11 条单测） |
| **M2** | settings 字段 + chat.rs 接入 + Tauri 命令 + 注册 | ✅ 完成 |
| **M3** | PanelSettings UI + 收尾（README / TODO / done/） | ✅ 完成 |

## 复用清单

- `tool_risk::ToolRiskAssessment` —— 分类器输出
- `tool_review::ToolReviewRegistry` —— 审核 receiver（不改）
- `commands::settings::AppSettings` —— 配置 round-trip
- `tools::registry::ToolRegistry::new` 的工具列表 —— 抽出 BUILTIN_TOOL_NAMES 常量复用

## 待用户裁定的开放问题

1. **MCP 工具是否也要进面板**：本轮不做（MCP 列表动态、tool 名不稳定），但保留扩展空间 —— 配置字段是 HashMap<String, String>，未来加 MCP UI 即可写入对应 key。
2. **是否需要"全 always_approve"/"全 always_review"**两个全局开关：暂不加，避免用户一键关掉所有审核或一键塞爆审核队列；逐项设置更稳。
3. **写入 always_review 的工具和 user purpose 缺失的 reject**怎么交互：现有 purpose gate 在 risk 评估之前 — purpose 缺失仍直接 reject，与本轮覆盖无冲突。

## 进度日志

- 2026-05-04 18:00 — 创建本文档；准备进入 M1。
- 2026-05-04 18:35 — M1-M3 一次性合到 main：
  - **M1**：`src-tauri/src/tool_review_policy.rs` 落 `ToolReviewMode` enum + `parse_mode`（前向兼容：未知值退回 Auto）+ `effective_requires_review`（pure 三态决策）+ `nominal_risk_label`（每个内置工具的稳定基线 level/note）。11 条单测覆盖三种 mode、未知字符串退回、各工具 label 命中、edit_file 的 medium 中等档、未识别工具 fallback。
  - **M2**：`AppSettings.tool_review_overrides: HashMap<String, String>`；`commands/chat.rs` 在 `assess_tool_risk` 之后从 settings 读 mode → `effective_requires_review(auto, mode)` 决定最终是否进 ToolReviewRegistry，覆盖前的纯分类器结果。`tools::registry` 抽出 `BUILTIN_TOOL_NAMES: &[&str]` 常量并 `pub use`。新 Tauri 命令 `get_tool_risk_overview()` 返回 `Vec<{name, level, note, mode}>`，注册到 lib.rs。
  - **M3**：`useSettings.ts` 加 `tool_review_overrides: Record<string,string>` + 默认 `{}`；`PanelSettings.tsx` 在「早安简报」与「记忆整理」之间加「工具风险」section：用 `display: contents` 三列网格（工具名 + level 徽章 / 备注 / 下拉），下拉值为 auto / always_review / always_approve；选回 auto 时从 form 里 delete 掉该 key（保持配置文件干净）。
  - cargo test --lib 735/735，tsc --noEmit 干净。README 加亮点；TODO 移除条目；本文件移入 `docs/done/`。
- **开放问题答复**：
  - Q1 MCP 工具：仍不做。MCP 工具表是动态的，加进面板会让"刷新 MCP 后清单变化"的状态机变复杂。配置层 HashMap<String,String> 已留扩展位，未来加 MCP UI 直接写入对应 key 即可。
  - Q2 全局开关：仍不做。逐项设置足够灵活，全局"一键放行"风险太高（用户失误代价大）。
  - Q3 与 purpose gate 的交互：purpose gate 在 risk 评估之前。覆盖层只影响 risk → review 这一段，无冲突。
