# PanelMemory "📋 今日 todo" 按钮

## 需求

iter #223 给 butler_tasks 段加了"今日要执行" filter chip。但用户常想
把这批清单 copy 出去（早 stand-up / 工作计划 / share 给同事），需要
markdown 输出。补一键导出按钮。

## 实现

`src/components/panel/PanelMemory.tsx` butler_tasks segment header（标
题行 + new 按钮区间）插入 "📋 今日 todo (N)" 按钮：

- 仅 `catKey === "butler_tasks" && todayItems.length > 0` 时浮
- inline 复用 iter #223 today filter 谓词（every 永远 / once+deadline
  当日）
- 输出 markdown 形态：
  ```
  # 📌 今日 todo（YYYY-MM-DD · N 条）

  - [ ] 🔁 09:00 早安日程汇总
  - [ ] 📅 14:00 周末整理 Downloads
  - [ ] ⏳ 18:00 文档提交
  ```
  - 每条 checkbox + kind icon + HH:MM + title
  - 与 iter #223 chip 同款 emoji（🔁 / 📅 / ⏳）保持 schema 一致
- 写剪贴板 + 3s toast 反馈

## 验证

- `npx tsc --noEmit`：clean
- 行为：
  - butler_tasks 段无 schedule / 无今日命中 → 按钮不浮
  - 含 `[every: 09:00]` 1 条 + `[once: 今日 14:00]` 1 条 → 按钮显 "(2)"
  - 点击 → 剪贴板装 markdown 2 条 checkbox + toast "已复制今日 todo（2 条）"
  - 粘到 Notion / Slack / iA Writer → 渲为可勾选 todo 列表
  - filter chip 选择不影响导出（始终基于 today 谓词，与 chip 状态无关）

## 不在本轮范围

- 没集成"今日已 done"过滤：done chip 与 today filter 独立 axis；导出
  仅按 schedule 形态筛
- 没让 button 显在 today chip active 时 enabled / inactive 时 disabled：
  button 自带 today 计数门控（todayItems.length > 0 才浮），独立工作
- 没做 export to file：剪贴板路径足够；future 想要可加
- 没在 PanelTasks 加同款（任务队列已有 due chip + 全部导出按钮）

## TODO 池剩余

- PanelTasks 导出 visible markdown 加 include detail toggle
- PanelChat "💾 保存为模板"
