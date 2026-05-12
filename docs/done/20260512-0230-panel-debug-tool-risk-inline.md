# PanelDebug 工具风险 inline 调整

## 需求

工具审核偏好（auto / always_review / always_approve）只在 PanelSettings
的整表里能改：要点开"设置"tab → 滚到工具风险段 → 改 chip → 滚下去点
保存。链路长且容易因 yaml typo 拒绝保存。把同一表搬到 PanelDebug，
chip 点击直写 settings 即时生效。

## 实现

### 后端 `src-tauri/src/tool_review_policy.rs`

- 新 `set_tool_review_mode(name, mode) -> Result<(), String>` Tauri 命令：
  - 校验 mode 在 `auto / always_review / always_approve` 白名单内（防 yaml
    被脏数据污染）
  - `mode == "auto"` 时从 `tool_review_overrides` map **删除** key，让
    settings YAML 保持简洁（默认值不需要 explicit 写入）
  - 其它值 insert / overwrite，调既有 `save_settings` 落盘
- 与现有 `get_tool_risk_overview` 配对（前者读，后者写）

### 注册 `src-tauri/src/lib.rs`

- `tool_review_policy::set_tool_review_mode` 加入 invoke_handler

### 前端 `src/components/panel/PanelDebug.tsx`

- 新 state：
  - `toolRiskRows: { name, level, note, mode }[]` —— 当前表
  - `toolRiskExpanded: boolean` —— 默认折叠 (避免长列表占垂直空间)
  - `toolRiskBusyName: string | null` —— 防双击同一行
  - `toolRiskMsg: string` —— 写盘后的短反馈
- 挂载时 `fetchToolRiskOverview()` 拉一次
- `handleSetToolReviewMode(name, mode)`：busy 锁 → invoke → 重新 fetch →
  toast 2s（失败 toast 4s）
- UI 在 PanelDebug 底部追加一段 section（与上方 stats / 模态层并列）：
  - 折叠 header："🛡 工具风险表 (N 个工具 · 点 chip 改完立刻生效)"
  - 展开后每行：
    - 左：level 徽章（high 红 / medium 橙 / low 灰）
    - 中：工具名（monospace）+ 一句话原因（muted）
    - 右：3 chip toggle（自动 / 审核 / 放行），active = accent 实底白字 +
      disabled；非 active = card 底
  - busy 行 opacity 0.6 防视觉抖动

## 验证

- `cargo check` clean
- `npx tsc --noEmit` clean
- 行为：
  - 进调试窗口"应用" tab，滚到底见"🛡 工具风险表" 折叠条
  - 点 ▸ 展开 → 见 BUILTIN_TOOL_NAMES 各行（bash high / write_file high
    / read_file low / ...）+ 当前 mode 高亮
  - 点 "审核" chip → 立刻 invoke + reload；toast 显 "bash → always_review"
  - 下次 LLM 调 bash → chat.rs 读 settings → 走 always_review 路径 → 入
    pending review 队列。无需重启 pet
  - 改回 "自动" → settings YAML 里该 key 被删，map 保持简洁
  - 改 mode 失败（罕见 IO err） → 红色 toast 4s 显失败原因

## 不在本轮范围

- 没改 PanelSettings 旧 UI：保留作"整表编辑 + 保存"路径；PanelDebug 是
  "一键直改"快捷
- 没动 MCP 工具（动态加载）的 risk 表：BUILTIN_TOOL_NAMES 只覆盖内置；
  MCP 工具 nominal_risk 走 fallback (medium 未分类)，对应行在表里显示但
  无更细一句话原因。MCP 工具 risk 标定要 metadata 协议层扩展，留给后续
- 没做"批量改"按钮：单条 chip 已足够直白，批量改容易误触

## TODO 池

清空后按规则 #1 自主提出 5 条新需求。

## TODO 池新提案

1. PanelPersona mood 强度 mini bar（24h motion 频次条形图）
2. PanelChat session 总字数 + token 估算角标
3. ChatMini drag-drop 图片到桌面气泡走多模态
4. PanelTasks task title hover 双击 inline 编辑
5. PanelDebug speech / butler / feedback timeline 切换 tab
