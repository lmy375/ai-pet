# PanelMemory category "⊞ 全展开 / ⊟ 全折叠" 一键工具栏按钮

## 需求

PanelMemory 各 category section 默认 > 10 条折叠到前 5，用户想"看
全部"或"概要扫读"得逐 section 点按钮。Memory 5 个 category 时点 5
次太累。补一键 collapse-all / expand-all。

## 实现

`src/components/panel/PanelMemory.tsx` 顶部工具栏在 💾 .md 按钮后追加
两个按钮：

- "⊞ 全展开"：onClick → 取 `Object.keys(index.categories)` 全 set 为
  expandedCategories，写 localStorage `pet-memory-expanded-cats`
- "⊟ 全折叠"：onClick → set empty Set + 同 key 写空数组
- 都 disabled={!index}（loading 时 noop）
- 复用每段折叠按钮已有的 localStorage 持久化路径，跨重启保留

## 验证

- `npx tsc --noEmit`：clean
- 行为：
  - 默认状态：butler_tasks > 10 条折前 5；其它 cat ≤ 10 不折叠
  - 点 "⊞ 全展开" → 所有 section 显全部 items
  - 点 "⊟ 全折叠" → 所有 section 回默认折叠态（≤ 10 全显 / > 10 折前 5）
  - 重启 panel → 状态从 localStorage 还原
  - 单 section 自己的折叠 toggle 仍可用，与全局按钮共用同一 set

## 不在本轮范围

- 没做"全展开"按钮变态指示当前是否已全展开（如变 ⊟）：用户切换语义
  快速 — 双按钮简单
- 没让 button 显当前 N 段展开 / 全段 badge：信号边际，section 自己折叠
  态视觉化已经够
- 没在 PanelTasks 加同款（任务卡折叠是 expand-1 模式）：模型不同
- 没改"暂无记忆"空 category 也算展开：set 含 key 即展开；空 cat 渲
  染时无 items 列表，无视觉副作用

## TODO 池剩余

- PanelChat marks modal 加 search 输入框过滤
- PanelMemory butler_tasks item "✏️ 改 schedule" 一键按钮
- PanelDebug "上次 manual fire 历史 ring" 显近 5 条
- PanelChat marks modal item 显标记时间
