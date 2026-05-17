# PanelTasks task row ctx menu「📋 复制 detail.md 全文」（iter #277）

## Background

owner 想拿某 task 的 detail.md 全文（贴给同事 / 喂外部 LLM / 备份）当前要
两步：打开 detail 编辑器 → ⌘A + ⌘C。或者走右键「📑 复制为 Markdown」拿到
含元数据 bullet 头的完整段（多余）。

本迭代加专用「📋 复制 detail.md 全文」右键命令：调既有 `task_get_detail`
拿 raw markdown 直写剪贴板，省两步。

## Changes

仅 `src/components/panel/PanelTasks.tsx`：

- 在右键 ctx 菜单的「🔗 复制 detail.md 绝对路径」之后插「📋 复制 detail.md
  全文」按钮：
  - 仅 `t.detail_path` 非空时显（与同侧绝对路径按钮 gate 一致）
  - click → `invoke("task_get_detail", { title })` → `detail.detail_md`
  - 空 detail → 友好提示"detail.md 为空 — 没有内容可复制"
  - 非空 → `navigator.clipboard.writeText` + 显字数 toast `已复制 detail.md
    全文（N 字）`
  - 失败 → setBulkResultMsg 显错误；3s 自清

## Key design decisions

- **专用按钮而非复用「📑 复制为 Markdown」**：📑 路径拼 H2 标题 + 状态 /
  优先级 / due / tags bullet 元数据 + body + ### 进度笔记 + ### 产物；适合
  发 issue / 喂 LLM 完整上下文。本按钮纯粹拿 raw detail 段（无 wrapper），
  适合"我只要这段笔记"场景，二者并存语义清晰。
- **走 task_get_detail 而非读 detailMap 缓存**：detailMap 仅在 owner 主动
  hover preview / 进编辑器时填；右键 ctx 菜单可能命中未缓存的 task。
  invoke 一次单文件读 < 1ms，不需要预先 warm cache。
- **空 detail 友好提示**：复制空字符串到剪贴板对 owner 是 surprise（"什么
  都没复制？"）；显式说明"detail 为空"让 owner 知道不是 bug。
- **字数 toast 反馈**：与既有「已复制 detail.md 绝对路径」/「已复制 N 条」
  等 toast 同模板；字数给 owner 感知"复制了多少内容"。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.25s)
