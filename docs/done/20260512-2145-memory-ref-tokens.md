# PanelMemory butler_tasks 描述 ref token hover + 双击导航

## 需求

iter #182/#188 把 `「title」` ref token 在 PanelChat 升级成"hover 显
status + 双击跳 PanelTasks 卡片"的导航 affordance。但 PanelMemory
里 butler_tasks item 描述也常含同款 ref（用户用「复制为 ref」抓出
来贴 / 与 LLM 互引 / 心情记忆引用历史任务），在 memory 面板只是纯
文本 —— 同样的 ref token 在两个 surface 行为不一致。统一。

## 实现

### 复用既有 helper

`src/components/panel/panelChatBits.tsx`：

- `renderContentWithTaskRefs` 从 `function` 改为 `export function` ——
  不复制逻辑，PanelMemory 直接 import 用

### PanelApp 拓宽 prop 链

`src/PanelApp.tsx`：

- `<PanelMemory />` 调用加 `onRequestFocusTask={requestFocusTask}`
  prop —— 与 PanelChat 共用同一个 PanelApp lifted state（switch tab +
  set pending focus title 一步完成）

### PanelMemory 内部

`src/components/panel/PanelMemory.tsx`：

- 加 `PanelMemoryProps` interface 接 `onRequestFocusTask` 可选 prop
- import `renderContentWithTaskRefs` from `./panelChatBits`
- 新 `refTaskMap` useMemo：从 `index.categories.butler_tasks.items`
  本地构造 `Record<title, {status, updated_at}>` —— 不发额外 IO，
  status 用既有 parseButlerError / parseButlerDone 推断（与 ✅ / ❌
  chip 同语义）。memo 依赖 `[index]`，loadIndex / fire 推进后自动重建
- 放置位置：useMemo 必须放在 parseButlerError / parseButlerDone 两
  helper 之后（const TDZ），不能跟既有 totalMemoryCount 一起放
- displayDesc 渲染从 `{displayDesc}` 改为 `renderContentWithTaskRefs(
  displayDesc, refTaskMap, onRequestFocusTask)`
- helper 在无 ref 命中时 fast-path 返 parseUrls(content) —— 顺手给
  memory 描述里偶发的 URL 加蓝下划线，比原 plain text 增量提升

## 验证

- `npx tsc --noEmit`：clean
- 行为：
  - butler_tasks 段某 item 描述含 `先做完「整理 Downloads」再处理这条` →
    `「整理 Downloads」` 渲 dotted underline
  - hover → native tooltip 显 `状态：done / pending / error` + 最近更新
    + 提示"双击跳到任务面板该卡片"
  - 双击 → 切到「任务」tab，PanelTasks 自动滚到该任务卡 + 焦点高亮
    （走 iter #188 lifted state 路径）
  - ref 命中已归档 / 重命名（refTaskMap miss）→ underline 走 muted 色，
    tooltip 提示"已归档 / 重命名 / 不存在"
  - 描述含 URL 但无 ref → 蓝下划线（parseUrls 行为）
  - 描述纯文本 → 渲染同既往
  - todo / ai_insights / general 等其它 category 描述里也支持 ref（共
    用 refTaskMap）

## 不在本轮范围

- 没把 hover preview tooltip 升级成"含 detail.md 头 N 字"（PanelTasks
  hover preview 那种）：那需要在 PanelMemory 又一次 invoke memory_read_detail，
  + 缓存等基础设施，scope 翻倍；status + updated_at 已经覆盖"这条 ref
  还活着吗"主诉求
- 没在 search results 视图里也启用 ref token 渲染：search 结果有自己
  的 keyword 高亮路径（mark 黄底）；两者交错 priority 复杂，本轮 scope
  限主 list 渲染
- 没让 ref 也支持 ⌘+click 在新 panel 打开任务详情：单 panel 架构没"新
  窗口"概念

## TODO 池剩余

空。下一轮需自主提需求。
