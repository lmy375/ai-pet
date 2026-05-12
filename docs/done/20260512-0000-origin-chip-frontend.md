# PanelTasks origin 过滤 chip（frontend-only）

## 需求

后端已用 `[origin:tg:<chat_id>]` 标识 Telegram 来源的任务，但前端没有可视
入口。要做"我自己创建 vs 宠物自己派"完整三态需后端给 panel / 工具自创都
打 marker（搁置）。本轮先实现可立即落地的部分：**TG vs 面板**二元 chip
过滤——拿现成数据出 UI 入口，等后端补完 origin 模型后扩到三态。

## 实现

`src/components/panel/PanelTasks.tsx`：

- 新 helper `taskHasTgOrigin(t)`：`t.raw_description.includes("[origin:tg:")`
- 新 state `originFilter: Set<"tg" | "panel">`（与 priorityFilter 同 Set 模式）
- `originCounts: { tg, panel }` useMemo：只数活动态（与 priorityCounts 同语义）
- filteredTasks 链上加一段 origin 过滤：
  ```ts
  .filter((t) => {
    if (originFilter.size === 0) return true;
    return originFilter.has(taskHasTgOrigin(t) ? "tg" : "panel");
  })
  ```
- `filtersActive` 把 `originFilter.size > 0` 也算进；清过滤按钮一并 reset
- 渲染：在 priority chip 行末尾追加 2 个 chip（仅 `originCounts.tg > 0`
  时显，单条 panel 时退化为单选无意义）。配色 sky 蓝与 dueFilter / priority
  灰区分，让 origin 单独成为"入口维度"

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 全是面板创建的任务 → 无 origin chip 显（避免 noise）
  - TG 来过任务 → chip 行末尾出 📨 TG / 💻 面板 + 各自计数
  - 点 📨 TG → 列表只剩 TG 来的；与 priority chip / search / tag 可叠
  - 多选两个 chip → OR 语义（与 priority 同），等同不过滤但 UI 不拦截
  - "✕ 全部"清过滤把 originFilter 也清掉

## 后续

origin 三态完整版（user / pet / tg）需要：
- task_create Tauri 命令默认 append `[origin:user]`（panel + 委托 / propose_task
  确认走的都是这条路径）
- task_create_tool 默认 append `[origin:pet]`（除非 `origin=tg:*` 覆盖）
- TaskOrigin enum 加 `User` / `Pet` variant，parse / format / strip 全套更新
- 老任务无 marker = "legacy 未标"，UI 上单列一段 chip 表"老任务"

那一轮把 chip 从 2 个扩到 3-4 个，是单独的 backend + tooling 任务，不在本
轮范围。本轮的 frontend chip 在那时也能继续生效（panel marker 加上后这里
自动多出一段命中）。

## TODO 池

清空后按规则 #1 自主提案 5 条新需求（写入 TODO.md）。

## TODO 池新提案

1. PanelChat session tab 栏右键菜单（rename / pin / 删除）
2. ChatMini 桌面气泡 hover 显发送时间
3. PanelMemory 单条记忆"打开外部 markdown editor"
4. PanelTasks detail.md 编辑器加 markdown 预览
5. PanelDebug 工具风险 inline 调整
