# README §7 键盘速查 refresh

## 背景

§7 键盘速查只覆盖了 `?` 帮助层 + `⌘1-⌘5` PanelApp tab 跳转。今天又加了：
- DebugApp `⌘1-⌘4` tab 跳转
- 搜索框（Memory/Tasks/Chat）`Esc` 清 + `Enter` 入历史 datalist
- 聊天输入框（PanelChat + 桌面 ChatPanel）`↑/↓` shell 风历史召回（跨窗口共享）

README 应同步反映这些。

## 改动

`README.md` §7：

- PanelApp `⌘1-⌘5` 一句结尾补 "调试窗口同款 `⌘1` – `⌘4` 跳到（应用 / 日志 / LLM 日志 / 统计）"
- 新增 "**搜索框三件套**" 一行（Memory / Tasks / Chat 共享 Esc 清 + Enter datalist）
- 新增 "**聊天输入历史栈**" 一行（pet 窗 + Panel chat 共享 cap 20 dedup 跨窗口）

## 不做

- 不改 §7 节标题
- 不引入其它 sections（slash 命令 `/done /retry /cancel /stats /today /mood /version /clearstats` 已在 §1 / §4 内涵）

## 验收

- README 渲染 §7 多出三行（一行扩展 + 两行新增）

## 完成

- [x] README.md §7 增补
- [x] 移到 docs/done/
