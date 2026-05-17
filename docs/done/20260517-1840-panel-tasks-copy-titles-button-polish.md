# PanelTasks「📋 复制选中 N 条标题」按钮 discoverability 提升（iter #367）

## Background

TODO 写"加「📋 复制选中 N 条标题」批量按钮"，实际查代码发现批量
工具栏已有「复制标题」按钮（PanelTasks.tsx ~7468，handleBulkCopyTitles
@ ~3967）— 功能 100% 等价。这是 iter #342 / #347 同 pattern 的"already
implemented" 发现。

Pivot：把这条 TODO 视为 discoverability polish — 既然功能在但 owner
没发现 / 没用上，说明按钮 label 不够吸引眼球。改 label 加 📋 emoji
+ 显选中条数 + title attribute 列具体用例（团队 / 周会 / 外部 ticket）。

## Changes

### `src/components/panel/PanelTasks.tsx`（line 7471）

#### Before

```tsx
<button
  ...
  title="复制选中任务的标题清单（一行一个），适合快速贴 todo dump 到聊天"
>
  复制标题
</button>
```

#### After

```tsx
<button
  ...
  title={`复制选中 ${selected.size} 条任务的标题清单（一行一个）到剪贴板：贴团队 / 周会 todo / 外部 ticket（Linear / Jira / Notion）单条转写等。order 走当前视图顺序。与「复制为 MD」/「🔗 拼为 ref」三种粒度互补 — 这条最朴素只标题。`}
>
  📋 复制标题 ({selected.size})
</button>
```

改动点：
- label 加 📋 emoji prefix — 与 batch bar 其它 emoji 按钮（✓ 标 done
  / 📌 钉住 / 🔗 拼为 ref）风格统一
- label 显选中条数 — owner 一眼知"我要复制几条"，避免心算
- title 列具体场景（团队 / 周会 todo / 外部 ticket / Linear / Jira /
  Notion）— hover discover 用例后 owner 更愿意尝试
- title 强调"与其它两种粒度互补" — 让 owner 心智明确三选一时怎么挑

## Key design decisions

- **不改 handler 逻辑**：handleBulkCopyTitles 已经精确实现了 TODO
  描述的"选完一键拷 title 列到剪贴板"行为，order 走 visibleTasks
  排序也合理。改 handler 风险大于收益。
- **不引入键盘快捷键 ⌘⇧C**：scope creep — TODO 本意是按钮加 discoverability
  polish；键盘快捷键是另一条独立 TODO 的事。
- **count 显示在 label 而非 badge**：与隔壁「重试 / 取消 / 改优先级」
  等同视觉密度，badge 会让本按钮独占空间。`(${selected.size})` 紧
  凑且自然。
- **不在「🔗 拼为 ref」/「复制为 MD」也加 count**：本 TODO 只针对
  "复制标题" 单条；其它两条另开任务。如果后续 TODO 也有 polish 那
  两条，再批量改 / 抽 helper。

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.25s)
- 后端无改动
