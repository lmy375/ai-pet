# ChatMini 选区 toolbar「📝 记到 note」按钮（iter #296）

## Background

ChatMini bubble 选区浮 toolbar (iter #251) 已有 💾 转 task / 📋 复制 /
🔄 改写 三个动作。但 owner 经常想"这段宠物说的话挺好的，但不是要做的事 —
就想记一笔留着以后看"——目前只能 💾 转 task（语义不对）或 📋 复制再走
panel 新建。

本迭代加 📝 按钮：选中文字 → 一键作 general memory item 存盘，title 自动
按本地秒级时间生成（与 TG /note 同模板）。task / note 二选一覆盖"要做的
事 vs 想记的事"两种意图。

## Changes

- `src/components/ChatMini.tsx`：
  - Props 加 `onSaveAsNote?: (text: string) => void`
  - 函数签名解构同步
  - 选区 toolbar 在 💾 之后插 📝 按钮（`onSaveAsNote` 传入时显），click
    → 关闭 toolbar + 调 callback

- `src/App.tsx`：
  - 新增 `handleMiniSaveAsNote(text)` async callback：
    - 生成 title `note-YYYY-MM-DDTHH-MM-SS`
    - 调 `invoke("memory_edit", action: "create", category: "general", ...)`
    - 成功 → `appendAssistant("📝 已记到 general/<title>")`
    - 失败 → 错误反馈
  - 传入 `<ChatMini onSaveAsNote={handleMiniSaveAsNote} />`

## Key design decisions

- **与 TG /note 同后端 + 同 title 模板**：跨入口（桌面 ChatMini 选区 / TG
  bot）行为一致 — title 都是 `note-YYYY-MM-DDTHH-MM-SS`，description 都是
  trim 后的文本。owner 在 PanelMemory → general 看到的混合来源 notes 按
  时间序自然排。
- **存到 general 而非 ai_insights**：与 TG /note 决策一致 — general 是兜
  底 / 杂项类目，最贴 "owner random thought" 语义；ai_insights 是宠物自己
  的反思空间。
- **与 💾 转 task 互补**：toolbar 现 4 按钮（💾 / 📝 / 📋 / 🔄）覆盖
  4 种意图：转任务 / 记笔记 / 拷贝 / 让 AI 改写。owner 选区后按需选。
- **appendAssistant 反馈**：让 owner 在 ChatMini 内立刻看到"我刚记了啥"
  确认 — 不必切到 PanelMemory 验证。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.20s)
