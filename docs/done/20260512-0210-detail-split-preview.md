# PanelTasks detail.md 分屏预览

## 需求

任务 detail.md 编辑器已有"✏️ 编辑 / 👁 预览"二态 toggle（R117），但只能 ON/OFF
切换看 raw / rendered，不能"边写边看"。加 🔀 分屏模式让 textarea 与渲染
preview 并排，宽 panel 大段写笔记 + markdown image 等元素时实时可见。

## 实现

`src/components/panel/PanelTasks.tsx`：

- 旧 state `detailPreviewMode: boolean` 升级为 `detailViewMode: "edit" |
  "split" | "preview"`，type alias 局部定义在组件内
- 视图模式 toggle 行从 2 个按钮扩为 3 个（edit / split / preview），各加
  hover tooltip 解释 split 适用场景
- 渲染分支三态：
  - `preview` → 单 preview 容器（沿用原有渲染）
  - `split` → flex row：左 `<div flex:1>` 包裹 textarea + 右 `<div flex:1>`
    包裹 preview，gap 8px
  - `edit`（默认）→ 单 textarea
- split / 单 edit 的 textarea 共享同一份 props（onPaste / onDrop / onKeyDown
  ⌘S Esc / placeholder / 样式）—— 直接复制 JSX 重复 ~70 行，不抽 helper
  让 hot path 阅读直观；JSX 结构差异由外层 flex wrapper 表达
- preview pane 在 split 下加 `overflowY: auto` —— 防长 detail 把 panel 拉
  太高，preview 自带独立滚动

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 默认 detail.md 进编辑态 → ✏️ 编辑 active（蓝底白字）
  - 点 🔀 分屏 → textarea 收成左半，右半实时显 parseMarkdown 结果
  - 改 textarea → preview 立即跟着重渲（共享 state 自动 re-render）
  - 点 👁 预览 → 单 preview 视图
  - 三态切换不丢未保存内容（editingDetailContent state 共享）
  - ⌘S / Esc / paste image 等键盘 / 鼠标交互仍只对 textarea 生效（preview
    不可编辑）
  - split 下 preview pane 过长 → 内部独立滚动，不撑大 panel

## 不在本轮范围

- 没做"窄 panel 时自动 fallback 单 column"：用户开宽 panel 用 split，窄时
  自己切回 edit / preview；media query 反而难懂
- 没做"拖拽中间分隔条调比例"：固定 50/50 满足常用；可调 splitter 要 mouse
  drag state 多写一倍，留给后续
- 没改非编辑态（detailMdRenderMode 浏览态）：那是只读视图的 rendered /
  source 切换，与本轮编辑器无关

## TODO 池剩余

- PanelDebug 工具风险表 inline 调整
