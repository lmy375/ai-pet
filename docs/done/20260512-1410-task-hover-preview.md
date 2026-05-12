# PanelTasks 任务卡 hover detail.md preview tooltip

## 需求

PanelMemory 上一轮加了 hover memory item 500ms 后浮 detail.md 截首
600 字的轻量预览（用 memory_read_detail 命令）。PanelTasks 也应有
同款 UX——队列里多个任务都有自己的 detail.md（进度笔记），但展开
看要点 chevron + 等组件重新 render；hover 浮窗轻量得多，扫读队列里
"哪个任务现在做到哪一步"成本远低。

## 实现

后端 `src-tauri/src/task_queue.rs` / `commands/task.rs`：

- `TaskView` struct 加 `detail_path: String` 字段（`#[serde(default)]`
  保兼容旧 cache）；`build_task_view` 用 `item.detail_path.clone()` 透出。
- 不新增后端命令 —— PanelMemory 已经有 `memory_read_detail(detail_path)`
  能复用：路径 traversal 防御 + canonicalize + 600 字截断；文件不在
  返空串。

前端 `src/components/panel/PanelTasks.tsx`：

- `TaskView` interface 加可选 `detail_path?: string`
- 新增 state / handlers（与 PanelMemory 同形）：
  - `taskPreviewHoverTitle: string | null`
  - `taskPreviewCache: Record<title, content>`（key 用 task title，
    重命名 / 删除后 dangling 不要紧，hover 重新 invoke 再读）
  - `taskPreviewTimerRef`：500ms 防抖；扫鼠标滑过队列时不会爆 invoke
  - `startTaskPreviewHover(title, detailPath?)`：500ms 后 setHover + 缓存命中再 invoke
  - `endTaskPreviewHover()`：清 timer + clearHover
  - useEffect cleanup unmount 时清 timer
- 任务卡 wrapper `<div className="pet-task-card">`：
  - 加 `onMouseEnter={() => startTaskPreviewHover(t.title, t.detail_path)}`
  - 加 `onMouseLeave={endTaskPreviewHover}`
  - style 加 `position: "relative"`（tooltip absolute 锚需要）
- 内嵌 conditional tooltip：
  - 仅在 `!expanded`（任务已展开看详情就不浮预览，免重叠）+
    `taskPreviewHoverTitle === t.title` + cache 命中且非空时渲染
  - 样式同 PanelMemory：absolute 紧贴卡片下方，maxHeight 220 +
    overflowY auto，10px 灰色路径行 "📄 {detail_path}"，pointerEvents
    none（不挡卡片自身的点击事件）
  - zIndex 20（drop indicator 等 inline overlay 在 z 0-10 区间，不挡）

## 验证

- `cargo check --tests` clean
- `npx tsc --noEmit` clean
- 行为：
  - hover 队列里任意未展开任务 500ms → 浮 detail.md 头 600 字
  - 鼠标移开 → tooltip 消失，timer 取消
  - 没 detail.md / 路径不存在 → 后端返空串 → 条件 length>0 不渲染
    （不会闪空框）
  - 同一任务再 hover → 缓存命中即时回来（不会再 invoke 一次）
  - 任务展开态 hover → 不渲染 tooltip（避免与详情区视觉重叠）
  - 拖拽 / 右键菜单 / 展开点击 → 都不被 tooltip 拦截（pointerEvents none）

## 不在本轮范围

- 没做 tooltip click-pin / 全文展示：tooltip 是"扫读"，要看全文有
  expand chevron 也可以打开 detail 编辑器，不必重复一遍交互
- 没做 detail 实时 watch 自动 invalidate cache：极少改 + 重新 hover
  会重新 invoke，足够
- 没做主动 prefetch：500ms hover 后才 invoke，是为了避免渲队列时
  N+1 / 防止 panel 打开时 spike 后端
- 没把 task tooltip 抽成共享 `<DetailPreviewTooltip>` 组件复用 PanelMemory
  当前两处 inline JSX 也清晰；抽组件需先确认两边都不会再加分歧字段

## TODO 池剩余

- PanelMemory category 顺序自定义
