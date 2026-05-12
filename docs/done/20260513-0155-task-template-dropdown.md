# PanelTasks 创建表单 "📋 从模板" 下拉

## 需求

iter #174 在空 queue 状态加了"📋 用范例预填一条"按钮，预填单一模板
（整理 Downloads）打开 quickAdd。但仅覆盖一种任务形态；用户想做其它
常见任务（总结文档 / 调研 / 翻译等）还得从头敲。给"创建表单"加一个
模板下拉提供多个 prefill 选项，并把空状态按钮也接入同一份模板集。

## 实现

`src/components/panel/PanelTasks.tsx`：

- 模块级新 `TASK_TEMPLATES` 常量：4 条范例 {label, title, body}：
  - 📁 整理 Downloads（原 iter #174 单例升级而来）
  - 📝 总结一段文档（提炼 bullet 到 detail.md）
  - 🔎 调研某主题（搜 + 资料链接 + 摘要）
  - 🌐 翻译一段文字（保留 markdown 格式）
  - 每条都强调"明确动作 + 明确产物 + 明确范围"，引导用户写宠物易执行的形态

- 新 helper `applyTaskTemplate(idx: number)`：set title/body + reset
  priority=3 / due="" —— inline form / quickAdd modal / 空状态按钮三
  处共用

- 在 inline create form（`{createFormExpanded && ...}`）标题 label 同
  行加 select："📋 从模板…" placeholder + 4 options。选中 → 调
  applyTaskTemplate + reset value=""（与 iter #176 "📥 复制现有 schedule"
  下拉同模式）

- quickAdd modal 同款 select 加在标题 label 行

- 空状态按钮 onClick 改为 `applyTaskTemplate(0)` + `setQuickAddOpen(true)`
  —— 单 source of truth，整理 Downloads 仍是首推模板

## 验证

- `npx tsc --noEmit`：clean
- 行为：
  - 空 queue → 显单按钮 "📋 用范例预填一条"，点击 → quickAdd 弹开 +
    整理 Downloads 预填 + 顶部 "📋 从模板" 下拉可见
  - 顶部 select 选另一项 → title/body 立刻替换；priority / due 回默认
  - inline form 模式：展开 → 顶部 "📋 从模板" 在标题 label 同行右侧
  - 选模板后 select 重置 placeholder → 可重新选同一模板
  - 不选 / 关 dropdown → 表单 state 不变（与 schedule-copy dropdown 同
    防误触）
  - 选模板后立刻保存 → 模板原文进 task；可继续改后再保存

## 不在本轮范围

- 没把模板做成用户可配（localStorage 自定义列表）：4 条已经覆盖最常
  见 task 形态；可配化需要管理 UI / 验证 / 导入 / 导出 等基础设施
- 没让模板携带 priority / due 默认值：所有模板默认 P3 无 due，与"用
  户决定紧急度 / 截止"原则一致
- 没做"最近用过的模板上浮"：4 条线性列表，扫读成本低；recency-bias 排
  序在 > 8 条时才有意义
- 没改"复制为 MD" / hover preview 等其它路径的复制模板（这条只在创
  建表单的 prefill 路径）

## TODO 池剩余

- PanelMemory butler_tasks 段 schedule 类别 chip 过滤行
- PanelTasks header "清除全部已结束（done / cancelled）" 按钮
