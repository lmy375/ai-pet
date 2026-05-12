# PanelDebug manual fire history 复制 markdown 按钮

## 背景

TODO 中 "PanelMemory item 行 hover 高亮" 项审查后发现 R122 已实现
（`pet-memory-item:hover` 全局 style）。同步声明此 entry 冗余，本
iter 改做 manual fire history 的"复制全部为 markdown"按钮。

## 需求

iter #230 加了 manual fire history ring（cap 5）。但调 prompt / 提
issue 时想把整段历史一次复制走，得手抄。补 📋 按钮拼 markdown table。

## 实现

`src/components/panel/PanelDebug.tsx` 在 ▾/▸ N 展开 toggle 旁加 📋
按钮：

- 仅在 `manualFireHistory.length > 0` 时浮
- onClick：拼 markdown：
  - H1 标题 + 总条数
  - markdown table header `| 时间 | 类型 | 结果 |`
  - 每行 `| timestamp | 全局/▶️ 「title」 | result |`
  - result 中的 `|` 转义成 `\|` 防 cell 错位
- writeText + setProactiveStatus 4s toast
- 失败容忍：catch → 显失败原因

## 验证

- `npx tsc --noEmit`：clean
- 行为：
  - history 空 → 按钮不浮
  - history 1 条 → 按钮显（▾ 切换按钮仅 > 1 显，📋 始终在）
  - 点 📋 → 剪贴板装 markdown table 5 条
  - 粘到 issue / Notion / Slack → 渲染为表格
  - result 含 `|`（少见）→ 转义后不错位
  - 4s 反馈 "已复制 history markdown（N 条）"

## 不在本轮范围

- 没做"按 outcome 过滤复制"（仅 spoke / 仅 fail）：复制全部够用；
  filter 是 UI 视图功能
- 没做"复制为 JSON / CSV"：markdown 已覆盖人 / LLM 阅读路径
- 没集成 PanelDebug 既有 issue 模板按钮（iter #207）：那条以单 turn
  为中心；history 是独立 axis

## TODO 池剩余

- PanelChat 输入框 chat prompt 模板下拉
- PanelTasks 排序加 "📌 NOW marker 在前" 模式
- PanelMemory "✏️ 改 schedule" modal 加 kind 切换下拉
