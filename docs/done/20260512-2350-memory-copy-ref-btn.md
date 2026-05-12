# PanelMemory butler_tasks item 行 "🔗 复制为 ref" 按钮

## 需求

iter #189 给 PanelTasks 右键 ctx 菜单加了"🔗 复制为 ref（「title」）"
入口，但仅在任务面板 / 右键三步操作。用户翻 PanelMemory 看 butler_tasks
列表时想复制某条 ref 也得切到 PanelTasks 找 → 右键 → 选 → 复制。给
PanelMemory item action 行加同款按钮，直达。

## 实现

`src/components/panel/PanelMemory.tsx` 在 item action 行 🚀 按钮后插
入条件渲染的 🔗 按钮：

- 仅 `catKey === "butler_tasks"` 显（其它 category 没 task ref 语义）
- onClick：`navigator.clipboard.writeText(\`「${item.title}」\`)` +
  setMessage 2.5s 反馈
- title tooltip 说明粘到 chat 自动识别为 ref token + 引用 source
- 复用既有 `setMessage` 通道，与 🚀 / 编辑 / 删除 按钮 inline 平级

## 验证

- `npx tsc --noEmit`：clean
- 行为：
  - butler_tasks 段每条 item action 行末显 🔗
  - 点击 → 剪贴板 = `「整理 Downloads」` + toast "已复制 ref：..."
  - 切到 PanelChat 粘贴 → 自动渲为 dotted underline + hover 显状态 +
    双击跳到 PanelTasks 该卡片
  - 其它 category（todo / ai_insights / general 等）不显 🔗 按钮

## 不在本轮范围

- 没把 🔗 提升为主 action（与 编辑 / 🚀 / 删除 平级位置不变）：复制 ref
  是低频操作，emoji button 已经够明显
- 没做 bulk select + bulk copy（多条任务一起拼 ref 列表）：PanelMemory
  无 multi-select UI；PanelTasks 单独 TODO 项覆盖此场景
- 没改 PanelTasks 右键菜单条目：那条仍可用，两个入口互补 —— PanelMemory
  在 list 层直达，PanelTasks 在 card 上下文

## TODO 池剩余

- PanelTasks 手动标 done 时若含 `[every:]` warn recurring
- PanelChat 长消息折叠中段文本可直接点击展开
- PanelTasks 多选 bulk action 加 "🔗 拼为 ref 列表" 复制
- PanelDebug 加 "复制全部 stash + settings 为 issue 模板"
