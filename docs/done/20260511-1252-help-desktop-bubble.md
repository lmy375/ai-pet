# help 速查加桌面气泡快捷键

## 需求

桌面气泡 / ChatMini 上一路加了一堆快捷键（Esc 取消 streaming、⌘+C 复制最近
assistant、Shift+G 跳底、双击气泡进 panel）。但 KeyboardHelpOverlay 只列了
"Panel 全局 + 任务 tab"两组，桌面端的快捷键没人录入。`?` 帮助层应该是用户的
单点入口，缺了不行。

## 实现

`src/components/panel/KeyboardHelpOverlay.tsx` 加新 group "桌面气泡 / ChatMini"
（scope "桌面宠物窗口（不在 panel）"）：

```ts
items: [
  { keys: ["Esc"], description: "streaming 中：取消生成（已写出的内容保留 + [已取消] 标记）" },
  { keys: ["Esc"], description: "空闲 + 焦点在 ChatPanel textarea：清空草稿（ChatPanel 本地 handle）" },
  { keys: ["⌘C", "Ctrl+C"], description: "选区为空时复制最近 assistant 一条；有选区走原生复制" },
  { keys: ["Shift+G"], description: "vim 风格跳到 mini chat 末尾 + 重启 follow-tail" },
  { keys: ["双击气泡"], description: "打开 Panel chat 页（与右上角 ⛶ 等价）" },
],
```

## 验证

- `npx tsc --noEmit` clean
- 行为：用户在 panel 任意 tab 按 `?` 弹帮助层，多出第三组"桌面气泡 / ChatMini"
  五行，覆盖最近几轮加的所有快捷键

## 不在本轮范围

- 没把"按 Esc 关闭工具审核"细化到桌面：用户在桌面时不会看到工具审核弹窗（那
  在 panel 调试态用），脚注在 Panel 全局组的 Esc 描述已经覆盖
- 没把命令面板（slash）的快捷键加入：那是 PanelChat tab 的局部行为，单独
  一组没必要，已经在命令面板自己的 SlashCommandMenu hints 里说明

## TODO 池剩余

- PanelChat 全部 session 打包成 snapshot
- PanelTasks 历史归档按日期分组导出 markdown
