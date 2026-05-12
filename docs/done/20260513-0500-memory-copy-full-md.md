# PanelMemory item "📝 复制本条整段 markdown"

## 背景

TODO 中 "PanelChat pendingImages 全清" 项审查后发现 PendingAttachmentsChip
已含 ✕ 清空全部按钮（line 3768-3791），冗余 entry —— dev log 这条同
步声明，删之转做"复制本条 markdown"。

## 需求

PanelMemory 单条 item 已有三个 copy 路径：
- 🔗 复制 ref token（仅 `「title」`）
- 📋 复制 detail.md 全文（仅 detail body）
- 双击 / 编辑 → 走完整修改路径

但"我想 share 这条 memory 完整快照（含 description + meta + detail）
给别人 / 提 issue"没有单一入口。新增 "📝 复制本条整段 markdown" 把
title / 分类 / 更新时间 / detail_path / 字数 / description / detail.md
全部拼成 H2 段落复制。

## 实现

`src/components/panel/PanelMemory.tsx` 在 📋 detail 全文按钮后插入新
按钮：

- 异步 onClick handler：
  - sync 段先拼 `## {title}` + meta 列表 + `### Description` + body
  - 若 `detailSizes[path] > 0`，async invoke `memory_read_detail_full`
    拉 detail body 并 append `### detail.md` 段
  - clipboard.writeText + setMessage 反馈
  - fetch 失败容忍：sync 段仍写盘，detail 段标 "（读取失败）"
- emoji 📝 与 ✕ 删除按钮间，与既有按钮平级
- title tooltip 解释覆盖范围（share / issue / review 三场景）
- 任意 category 都可用（不限 butler_tasks）

## 验证

- `npx tsc --noEmit`：clean
- 行为：
  - 点 📝 → 剪贴板装完整 markdown：H2 标题 + 4-5 行 meta + Description
    段 + detail.md 段（如有）
  - 粘到 GitHub issue / Notion / Slack → 渲染清晰多段结构
  - detail.md 不存在 → 仅前半部分（不显 ### detail.md 段）
  - detail_path 路径 IO 失败 → "（读取失败）" 占位
  - 任意 category 都浮按钮（与 🔗 只 butler_tasks 不同）

## 不在本轮范围

- 没集成"全部记忆批量导出"（顶部已有 R98 全部导出按钮）：单条 vs 全
  量两条路径并存；用户场景不同
- 没让 markdown 包含 `[error: ...]` / `[done]` 等 marker（display 时已
  剥）：raw description 已含 markers，保留 raw 形态利于 issue 描述
  问题
- 没做"复制为 frontmatter + markdown"两段（如 yaml header + body）：
  当前 list-meta 用 markdown bullets 就够，frontmatter 适合写盘场景
- 没让 detail 段超 N 字时截断：share 场景下用户期待完整，截断会丢
  信息；要 light preview 走既有 📋 detail 全文按钮

## TODO 池剩余

- PanelTasks cancel reason 输入加最近 5 个 reason 历史 datalist
- PanelDebug recent turns ring buffer modal 加 outcome filter chips
- PanelChat 消息加 "📌 标记" 按钮
