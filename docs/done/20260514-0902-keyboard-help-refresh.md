# KeyboardHelpOverlay 补两段最近添加的快捷键

## 背景

最近几轮加了不少键盘快捷键但帮助层没跟上：
- 搜索输入框（Memory / Tasks / Chat 三处）`Esc` 清 query / `Enter` 入历史
- 聊天输入框（PanelChat / 桌面 ChatPanel）`↑/↓` shell 风历史召回

帮助层 README 写"新增快捷键时帮助层会同步更新"，本轮兑现。

## 改动

`src/components/panel/KeyboardHelpOverlay.tsx`：`GROUPS` 数组在「任务 tab」后插两段：

1. **搜索输入框**：
   - `Esc` 非空时清 query；空 input 让出键位
   - `Enter` 入历史 datalist
   - scope: "记忆 / 任务 / 跨会话搜索三处共享同模式"

2. **聊天输入框（PanelChat / 桌面 ChatPanel）**：
   - `↑` 空输入或浏览中 → 上一条；多按往前翻
   - `↓` 反向；超过最新一条退出 + 清空
   - scope: "两个聊天输入框共享 shell 风历史栈（cap 20 · localStorage 持久 · 跨窗口）"

## 不做

- 不加 slash 菜单 / mention picker 提示：它们自带 footer 行（上轮已加）显式列出 keys，重复列入帮助层反而冗余
- 不加 Image prompt history 菜单：那有自己的 header 提示
- 不加 ⌘1-5 Tab 跳转：Panel 全局段已有

## 验收

- `npx tsc --noEmit` ✅
- 任一 panel 按 `?` → 弹出帮助层 → 看到新增两段

## 完成

- [x] KeyboardHelpOverlay.tsx: GROUPS 加 "搜索输入框" / "聊天输入框" 两段
- [x] `npx tsc --noEmit` 通过
- [x] 移到 docs/done/
