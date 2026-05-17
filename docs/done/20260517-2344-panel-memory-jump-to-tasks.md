# PanelMemory item「↗ 跳到任务面板」按钮（iter #422）

## Background

PanelMemory 的 butler_tasks cat 显示所有 butler task — 但它和
PanelTasks 是同一份数据的两个 surface。owner 在 PanelMemory 看到
某条 task 想立即「mark done / 改 priority / 查 schedule / 看
detail.md / 看 history」时只能切到 PanelTasks tab + 用顶部 search
找到这条 — 多步切换。

本 iter 加 ↗ 一键跳转：复用既有 `onRequestFocusTask(title)` prop
（与 chat task ref hover 双击 / memory desc 内 `「title」` token
双击同 channel），click 切 tab + 高亮目标卡片。

## Changes

### `src/components/panel/PanelMemory.tsx`（紧贴 🔗 inline ref 之后）

```tsx
{catKey === "butler_tasks" && onRequestFocusTask && (
  <button
    style={s.btn}
    onClick={(e) => {
      e.stopPropagation();
      onRequestFocusTask(item.title);
    }}
    title={`切到 PanelTasks tab 并高亮「${item.title}」task 卡片 — 想立即 mark done / 改优先级 / 看 detail / 历史时一键跳。`}
    aria-label="jump to task panel for this item"
  >
    ↗
  </button>
)}
```

设计要点：
- **gate by catKey === "butler_tasks"**：其它 cat（ai_insights / todo
  / general / user_profile / task_archive）的 item 不是 task —
  按钮无意义；不渲免误触
- **gate by onRequestFocusTask 传入**：PanelMemory 在 ChatPanel 内嵌
  时未必传 prop，未传时不渲免按了无反应
- **复用 onRequestFocusTask 同 channel**：与既有 task ref hover
  双击（renderContentWithTaskRefs）/ memory desc 内 `「title」` 双击
  同源 — 行为一致：切 PanelTasks + 卡片 scrollIntoView + 1.5s
  高亮闪烁
- **stopPropagation**：防 click 冒泡触发 item row 的展开 / 双击
- **emoji ↗**：方向感强（北东角→），表达「跨 tab 跳转」语义；与
  既有 chip 集群 (📋 / 📑 / 🔗 / 📜 / 📁) 视觉区分
- **s.btn 样式复用**：与既有 per-item action chip 同节奏 — 不引第
  二条样式

## Key design decisions

- **不为其它 cat 加跳转**：仅 butler_tasks ↔ PanelTasks 有 1:1
  对应；ai_insights / general / todo 没有对应 panel tab，跳哪里都
  没意义
- **不显「✏ 在 PanelTasks 编辑」「✓ 在 PanelTasks 标 done」等多
  个按钮**：单 ↗ 一键到位让 owner 自己在 PanelTasks 内挑动作；细
  分按钮反而让 chip row 太密
- **不写 audit「跳了几次」**：跳转是 navigation 不是 mutation，无
  audit 价值
- **不为单按钮引 unit test**：纯 callback 调用 + 既有跳转 pipeline；
  build pass + 手测足够（在 butler_tasks item 上看 ↗ chip 出 →
  click → 看自动切到 PanelTasks tab + 卡片 highlight）

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.29s)
- 后端无改动 — 复用既有 onRequestFocusTask prop / PanelTasks
  scrollIntoView pipeline
