# PanelMemory item action row「🔖 加 #tag」mini-input 按钮（iter #329）

## Background

owner 在 PanelMemory 想给某 item 加自定义 `#tag` 标记，当前只能：
- 点「编辑」action button → 打开完整 modal → 改 description → 末尾加
  `#name` → 保存 → 关 modal

5 步只为加一个 tag 太重。本迭代加 🔖 按钮 + 弹 mini input popover，输
入 tag 名 + Enter 即追加到 description 末尾。

## Changes

仅 `src/components/panel/PanelMemory.tsx`：

- 新 state：
  - `addTagPicker: {catKey, title} | null` — null = 关
  - `addTagDraft: string` — input value
  - `addTagBusy: boolean` — 防 double-submit
- 新 useEffect：outside-click / Esc 关 picker + 清 draft（与既有
  `moveCatPicker` close-on-outside pattern 同源）
- 新 callback `submitAddTag(catKey, item)`：
  - trim + strip 前缀 `#`；空 → "tag 名不能为空" 短反馈
  - 含 `\s` → "tag 名不能含空白字符"
  - 已存在该 tag（word-boundary regex 与后端 parse_tags 同语义）→ "tag
    #name 已存在" 静默拒
  - 否则 `description.trimEnd() + ' #' + raw` → `memory_edit("update")`
  - 成功后 setMessage 反馈 + 关 picker + 清 draft + loadIndex
- 在 action row 既有 🏷 改类目 之后插「🔖」mini-input popover：
  - autoFocus input + `#` prefix 提示 + Enter 提交 + Esc 关
  - 提交按钮 disabled when busy / empty draft

## Key design decisions

- **emoji 选 🔖 而非 🏷**：🏷 已被「改类目」占用（不同语义 — 改 category
  vs 加 tag）；🔖 是 bookmark / tag 的另一通用 emoji，区分度高。
- **追加到 description 末尾而非前置**：与既有 `[task pri=...]` 等
  markers 通常在前的约定对偶 — owner 自定义 tag 是 owner-intent，挂尾
  让 LLM-生成 markers 仍在 head 位置不被打断阅读。
- **word-boundary 重复检测**：`(?:^|\\s)#name(?:\\s|$)` 匹配避免误判
  （如 `#abc` 不算 `#abcdef` 已存在）。与后端 `parse_tags` 同语义 —
  让 frontend dedup 检测与 `/tags` 命令统计结果一致。
- **空 / 含空白 / 重复都给短反馈**：3 种 invalid 情况各自 setMessage
  2-3s 短 toast — owner 立即知道为啥拒，不必猜。
- **追加而非替换 description**：保 description 其它内容不动（含 task
  markers / topic body / 其它 tags）。trimEnd 防末尾空白累积 + 单空格
  分隔。
- **复用 memory_edit("update") 路径**：不引新 backend 命令 — tag 追加
  是 description 字段的小修改，走 update 通用入口让 SQLite mirror /
  butler_history hook 自动跟进。
- **autoFocus + Enter 提交**：popover 一打开 input 即聚焦，Enter 提交，
  Esc 关 — 与既有 mini-popover (reminderMin quick-picker / move-cat) UX
  对齐。owner 学习成本零。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.20s)
