# PanelTasks 任务行右键菜单

## 需求

任务行的操作分散在多个入口：

- priority badge → 弹 P0..P9 picker
- status badge → 弹"标 done / 取消…"picker（仅 pending 行）
- "重试"按钮（仅 error 行，行尾另起）
- 展开详情靠点 row 头
- 复制 title / 复制为 MD 仅藏在 bulk 操作里（要先勾 row）

用户得记入口位置 + 视觉扫整条行才能找按钮。统一加个右键菜单把这些都收敛
到一处：点哪条 row 弹哪条 row 的菜单，菜单项按状态动态隐藏不适用项。

## 实现

### `src/components/panel/PanelTasks.tsx`

- 新 state `taskCtxMenu: { title, status, priority, x, y, prioritySubmenu }
  | null`。
- 任务卡 `<div className="pet-task-card">` 加 `onContextMenu` → preventDefault
  + 写入 menu 坐标 / 上下文。
- 既有"outside-click + Esc 关 picker"的 useEffect 把 taskCtxMenu 也纳进
  来 —— 三 picker / 一菜单统一关，避免叠态。
- 菜单 JSX 在 component 根 return 末尾（与 ImageLightbox 同层）：
  - `position: fixed` 用 viewport 坐标，右 / 下越界靠 `Math.min(...,
    window.innerWidth - 180 - 8)` clamp。
  - 顶部 ellipsis 显 title 做语境提示。
  - 菜单项：
    - 📂 展开详情（始终可点；reuse handleToggleExpand）
    - ✓ 标 done（pending / error）
    - 🔄 重试（仅 error）
    - ✗ 取消…（pending / error，调既有 handleCancelOpen 弹行内 reason 输入）
    - ▸/▾ 改 priority（当前 PN）→ 点击展开 P0..P9 5x2 grid（与既有 badge
      picker 共用 handleInlineSetPriority）
    - 📋 复制标题 → navigator.clipboard，复用 bulkResultMsg 通道做 3s toast
    - 📑 复制为 Markdown（仅当 tasks 数组里查到该 task 时显，理论上必中）→
      formatTaskAsMarkdown 写剪贴板

### 设计选择

- 子菜单走"内联展开"（点改 priority 后在该项下方展 grid），不开右侧浮窗 —
  浮窗要二次计算坐标 + 越界，对桌面小屏 panel 体验差。
- 菜单宽固定 180px。priority 子面板展开多 ~60px，clamp 时用 360 上限。
- 不替换既有 badge / row 按钮入口。右键是"快捷"补充，老用户的肌肉记忆不破。

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - pending 行右键 → 看到「展开详情 / 标 done / 取消… / 改 priority / 复制
    标题 / 复制为 Markdown」
  - error 行右键 → 多出「重试」
  - done / cancelled 行右键 → 没有「标 done / 重试 / 取消」（仅展开 + 改 priority + 复制类）
  - 点改 priority → 行内 5x2 grid 展开，点 P 值 → 关菜单 + invoke +
    reload；当前 P 高亮 + 不可点
  - 右键菜单时再左键空白 / 别的 row → 菜单关
  - Esc → 菜单关
  - 菜单坐标贴近右 / 下边缘 → 自动往回挪，不被截断

## 不在本轮范围

- 没做"右键多选行批量操作"：bulk 工具栏已存在；右键纯单条入口更直白
- 没做"右键 tag / due 改"：tag 编辑要 ops 语法字符串解析（add:foo, rm:bar…），
  inline 输入会让菜单变成 mini 表单，违背"一键到位"的初衷。改 due 类似（要
  datetime-local 控件）。两者保留在 bulk 工具栏里
- 没改既有 badge picker / 重试按钮：右键是补充入口，不删老路径

## TODO 池剩余

老的 PanelTasks / PanelChat 待办都已清空。按 TODO.md 规则 #1 自主提案了 5
条新需求：

1. ChatMini 流式时图标小动效（视觉状态可读）
2. PanelMemory 单条记忆 pin 置顶
3. PanelSettings API key 掩码 + 复制
4. ChatMini 拖拽到面板的过渡视觉
5. PanelTasks 拖拽改 priority
