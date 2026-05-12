# PanelTasks 任务操作后刷新 detailMap[title]

## 背景

iter #180 把 hover preview 缓存从单独的 `taskPreviewCache: Record<title, string>`
切到 expand 路径共用的 `detailMap: Record<title, TaskDetail>` —— 单一
source of truth 节省重复 fetch。但这次共享引出一个新的陈旧风险：编
辑 detail.md 时本地只 patch `detail_md` 字段，`history` 与 `updated_at`
保持旧值。hover preview 现在显 history 行（iter #180），所以"刚刚保存
了 detail，hover 看到老 history"是真问题。

## 审计

把 PanelTasks 所有 task 改写路径过一遍，看是否需要补刷：

| 操作 | 走 reload() | detailMap 状态 |
|---|---|---|
| rename | ✓ (memory_rename 后) | 清空全表 ✓ |
| retry / mark done / cancel | ✓ | 清空全表 ✓ |
| set_priority / set_due / set_tags | ✓ | 清空全表 ✓ |
| 跨面板 fire（PanelMemory "▶️ 现在跑"）| 用户切回 tab 时组件 remount | 清空 ✓ |
| **save detail** | ✗（仅 patch detail_md 字段）| **stale history / updated_at** |

只有 `handleSaveDetail` 需要补 ——其它路径已经被 reload 路径或 tab
remount 路径"消毒"了。

## 实现

`src/components/panel/PanelTasks.tsx` 的 `handleSaveDetail`：

- 保留既有的 patch `detail_md` —— 让阅读态 UI 在 fetch 期间不 flicker
  空白
- 紧接异步 refetch：`invoke("task_get_detail", { title })`，命中即用
  完整新 TaskDetail 覆盖 cache
- refetch 失败 silently swallow：保留 patch 后的状态比清空更稳；下次
  reload / 重新 hover 时还会再 refresh

## 验证

- `npx tsc --noEmit`：clean
- 行为：
  - 展开任务 A → hover preview cache hit → 编辑 detail → 保存
  - 立即 collapse 再 hover A → tooltip 显新 detail + **新 history 行**
    （之前会显旧 history，新事件缺失）
  - 阅读态 UI 不 flicker（patch 路径同步）
  - refetch 失败（如同步删了任务）→ 仍显 patch 后的 detail，不闪空

## 不在本轮范围

- 没把 reload() 的"清空全表 detailMap"改成"按 updated_at 差异更新"：
  这是更宏观的缓存策略 redesign，避免用户在长展开列表上每次任何 task
  操作都丢全部 cache。当前 detailMap 缓存价值边际，性价比不值
- 没给跨面板 fire 加 PanelTasks 端的事件监听：依赖 tab 切换 remount
  天然消毒；事件监听是更高保真但复杂度高
- 没做"refetch 期间显 loading hint"：< 200ms 的 IO 不值得 spinner

## TODO 池剩余

- PanelChat 复制按钮 alt-click 复制为 markdown
- PanelDebug 加 "上次 manual fire" 行
- PanelChat 双击 `「task title」` ref → 切到 PanelTasks tab + scroll
