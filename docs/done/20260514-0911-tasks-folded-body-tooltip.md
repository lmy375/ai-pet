# PanelTasks 折叠 body 加 hover tooltip 显示全文

## 背景

PanelTasks 长描述（> 200 字）默认折叠到前 120 字 + "展开 (N 字)"按钮。但折叠态下用户得点开按钮才能看到剩余文字 —— 仅为"扫一眼判断要不要细读"也得点。

加 native `title` 让 hover 浮 browser tooltip 显全文。零依赖、零样式、accessible。

## 改动

`src/components/panel/PanelTasks.tsx`：

body 容器（`<div style={s.itemBody}>`）加 `title={folded ? t.body : undefined}`：

- 折叠时挂 title → hover 浮 browser tooltip 显完整 body
- 展开时不挂（避免 hover 弹一长串与可见 content 重复）

## 不做

- 不用自定义 popover：browser tooltip 已足够（accessible + 无需任何 outside-click 关闭逻辑）
- 不在折叠态点击行展开（与现有 Enter 切换详情面板的语义冲突）
- 不动 search 命中分支（强制展开走另一路）

## 验收

- `npx tsc --noEmit` ✅
- 「任务」tab 看到折叠态 body → hover 浮 native tooltip 显完整文字

## 完成

- [x] PanelTasks.tsx: itemBody div 加 title 属性
- [x] `npx tsc --noEmit` 通过
- [x] 移到 docs/done/
