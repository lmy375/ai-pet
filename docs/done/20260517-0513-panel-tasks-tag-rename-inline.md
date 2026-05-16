# PanelTasks tag chip 双击 inline rename（iter #252）

## Background

owner 用 tag 给 task 分类（`#工作` / `#家务` / `#health` 等），偶尔会发现
取名不准想改 —— 比如 `#health` 改成 `#健康`，或把误打的 `#workk` 改成
`#work`。原路径：必须先选中所有持该 tag 的 task → bulk「改 tags」→ 输入
`-old +new` 提交。多步 + 易遗漏（filter 不一定覆盖所有）。

本迭代加 tag chip 双击 → inline 改 input → Enter 后跨全表批量改名。直观、
一步到位、不会漏。

## Changes

仅 `src/components/panel/PanelTasks.tsx`：

- **state**：
  - `renamingTagName: string | null` — 当前正在改的旧 tag 名（null = 不在
    改名态）
  - `renameTagDraft: string` — input 当前值
  - `renameTagBusy: boolean` — 串行 invoke 期间禁用 input 防双触

- **`commitRenameTag` useCallback**：
  - 空 / 同名 → 静默 noop（与 task title rename 同模式 — 用户 blur 不想改也
    要能优雅退出）
  - 遍历 `tasks.filter((t) => t.tags.includes(oldName))` 串行 invoke
    `task_set_tags { title, opsInput: "-oldName +newName" }`
  - 后端 `parse_tag_ops` 校验 newName 合法字符；非法时第一条抛错被 catch 计入
    failed
  - 全部跑完后 `reload()` 一次刷视图（比每条 reload 高效）
  - failed > 0 → setActionErr 显"失败 X / Y 条"；否则 setBulkResultMsg
    "✓ tag 改名：N 条 #old → #new" 4s 自清

- **`cancelRenameTag`**：清 state 退出改名态

- **render 改造**：tag chip 渲染分两路：
  - `renamingTagName === tag` → 用 `#<input>` 替换 chip 文字；input 宽度
    随 draft 长度自适应（`${draft.length + 1}ch`，min 2ch）；Enter 走 commit，
    Esc 走 cancel，blur 也走 commit（静默关）；input keydown.stopPropagation
    防止冒泡到全局快捷键
  - 普通态 → 加 `onDoubleClick` 触发 `setRenamingTagName(tag) +
    setRenameTagDraft(tag)`；tooltip 末尾加"双击改名（跨全表）"提示

## Key design decisions

- **走 `task_set_tags` 而非新写 `task_rename_tag` 后端命令**：后端已有 tag
  add/remove 原子操作，前端遍历 N 次 invoke 是 O(N) 网络但简单可靠；写新后端
  命令需要：路径设计 / 安全校验 / 测试 — 不值得。后端 ops 串 `-old +new`
  原子性也对：单 task 改名失败不影响其它 task（其它继续）。
- **空 / 同名 = 静默 noop**：与 task title rename 同模式。owner 双击进 input
  发现不想改，按 Esc 或 blur 即可优雅退出；强弹"请输入新名字" alert 多余。
- **input 宽度自适应而不是固定 100px**：tag 名通常 2-8 字符；固定宽度要么浪
  费空间要么截断；ch 单位贴字号自然，min 2ch 防输入框塌成 0 宽。
- **同时只允许一条 tag 处于改名**：multi-input 同屏分散注意力 + 防多条同时
  commit 时改名 race（如 owner 同时改 #work #worker 容易选错）。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.20s)

## Notes

跨全表批量改：用 `tasks`（完整列表）而不是 `visibleTasks`（过滤后），避免
filter 关键字选错把部分 task 漏改的隐患。
