# PanelTasks 创建表单 ⌘Enter 提交（Iter R120）

> 对应需求（来自 docs/TODO.md）：
> PanelTasks 创建表单 ⌘Enter 提交：R116 N 已让用户用快捷键唤出表单 + focus title input；补 ⌘Enter / Ctrl+Enter 提交快捷键，让"键盘流"创建任务闭环（input / textarea 内捕获即可，不挂全局）。

## 目标

R116 给 PanelTasks 加了 N 键展开 + focus 创建表单 title input。但现在
要提交还得鼠标点"创建"按钮 / Tab 到按钮再 Space。补 ⌘Enter / Ctrl+Enter
让"输完即提"键盘流闭环。

## 非目标

- 不挂全局快捷键 —— 仅当 focus 在创建表单某个 input/textarea 内时生效，
  避免与其它 panel 的输入冲突
- 不动空标题前置校验 —— `handleCreate` 内部已有 `!title.trim()` 守卫
- 不动 Enter（无 modifier）行为 —— 用户在 textarea 输 Enter 是换行，强行
  提交会破坏多行描述输入流

## 设计

### shared handler

```ts
const handleFormKeyDown = (e: React.KeyboardEvent) => {
  if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
    e.preventDefault();
    if (creating) return;
    void handleCreate();
  }
};
```

定义在 handleCreate 之后；通过闭包捕获 creating + handleCreate 引用，每
帧重新创建（开销 trivial），不需 useCallback。

### 挂到 4 个 input

- title input（line 1655）
- body textarea（line 1663）
- priority number input（line 1672）
- due datetime-local input（line 1687）

每处加 `onKeyDown={handleFormKeyDown}`。

### placeholder 提示

title input placeholder 已 "比如：整理 Downloads" 简洁。可在创建按钮 title
属性补 "(⌘Enter 等价)" 让用户首次扫到入口知道快捷键存在；也可在 title
input placeholder 后加 "（⌘Enter 提交）"。前者更克制，按 R105 PanelMemory
保存按钮的同款做法。

实际上让 placeholder 保持简洁 + button title 注解 "(⌘Enter 等价)" 更优。

### 测试

无单测；手测：
- focus title input → 输标题 + ⌘Enter → 创建任务，重置表单
- focus body textarea → ⌘Enter → 同上（不被 textarea Enter 换行行为吞掉）
- focus priority / due → ⌘Enter → 同上
- 标题空 + ⌘Enter → 红色错误"标题不能为空"，不重置表单
- creating 期间再按 → 短路返回（防 race 重复创建）

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | shared handler + 挂到 4 个 input |
| **M2** | tsc + build |

## 复用清单

- 既有 handleCreate / creating state
- 既有 R116 N 快捷键 + R105 ⌘S 保存模式

## 进度日志

- 2026-05-10 01:00 — 创建本文档；准备 M1。
- 2026-05-10 01:08 — M1 完成。`handleFormKeyDown` 加在 handleCreate 之后；preventDefault + creating 守卫 + void handleCreate；4 个表单字段（title input / body textarea / priority number / due datetime-local）都挂 onKeyDown；创建按钮 title 补 "(⌘Enter / Ctrl+Enter 等价)" 让用户首扫即知。
- 2026-05-10 01:11 — M2 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (500 modules, 942ms)。归档至 done。
