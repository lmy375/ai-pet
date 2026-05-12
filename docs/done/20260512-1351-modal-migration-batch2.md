# Modal 迁移 batch 2：editItem + quickAdd（UI 美化 迭代 19）

## 背景

接迭代 18，继续把"中等复杂度" dialog 迁移到共享 Modal 组件。这两条单独占据上百行 overlay + card 模板代码，迁移后既统一视觉又减重复。

## 改动

### PanelMemory editingItem dialog

旧：自定义 backdrop `rgba(0,0,0,0.3)`（过浅，dark 主题下几乎透明）+ inline card 模板 + 无 Esc 监听。
新：`<Modal open={editingItem !== null} onClose={...} maxWidth={400}>` 包一层，内部内容保留。
收益：
- backdrop 浅深主题感知一致
- 自动 Esc 关闭（之前只能 backdrop click）
- shadow 跟 token

### PanelTasks quickAdd dialog

旧：自定义 `pet-quickadd-fade-in` + `pet-quickadd-pop` keyframes（两套与 Modal 内置同思路但写法各异）+ inline overlay。
新：`<Modal open={quickAddOpen} onClose={...} maxWidth={520}>` 包一层，sticky header / 关闭 ✕ / 表单字段全部内联保留。
收益：
- 删掉重复 keyframes 块（10 行）
- 删掉 overlay + card 模板（~35 行模板代码缩减为 1 行 `<Modal>`）
- 现在与 markDone / schedule edit / editItem 共享同一种 enter 动画 + Esc 行为

## 不做

- 仍剩 PanelChat marks modal、status picker、Image Lightbox、PanelDebug 系列 modal —— 那些 layout 更"自定义"（sticky header、深嵌内层 modal、自带 max-height / scroll 区），后续单独评估。
- 不动按钮内部的 inline style（quickAdd 内有取消 / 创建按钮各自的样式）。

## 验收

- 切到「记忆」，点 + 新建 / 编辑既有：dialog 弹出有 pop-in 动画，Esc 关闭可用。
- 切到「任务」，按 ⌘N 唤起 quickAdd：同上。
- `npx tsc --noEmit` 通过。

## 完成

- [x] PanelMemory editingItem → Modal
- [x] PanelTasks quickAdd → Modal
- [x] 移到 docs/done/
