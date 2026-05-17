# detail.md 编辑器「⌘⇧F find-replace」modal — 已实现 pivot（iter #498）

## Discovery

本 TODO 项「detail.md 编辑器「⌘⇧F find-replace」modal：弹双输入 find /
replace，预览匹配数 + 一键全替换 — 长 doc 批量改名」在加入 TODO 前已
完整实现。

定位：`src/components/panel/PanelTasks.tsx`

- **line 2479-2522**：state — `detailSearchOpen` / `detailSearchQuery`
  / `detailSearchActiveIdx` / `detailReplaceMode` / `detailReplaceText`
  + `detailSearchMatches` memo（case-insensitive substring 找位置数组）
- **line 2523-2551**：`⌘F` 监听 — 仅 detail textarea / search input
  内劫持；其他焦点放过去（让顶部 task 搜索框走默认）
- **line 2553-2594**：`⌘⇧F` 监听 — 打开 search bar 并同时切 replace
  模式；query 空 focus search、非空 focus replace input
- **line 2629-2649**：`handleDetailReplaceAll` — 倒序 splice 每条命中
  避免位置漂移；focus 保留在 replace input
- **line 12214+**：bar UI 渲染（find 半边 + replace 半边按 detailReplaceMode
  gate；count chip 显「N/M」）

行为与 TODO 完全吻合：
- 双输入（search / replace）
- 实时匹配数预览（detailSearchMatches.length）
- 一键全替换（handleDetailReplaceAll）
- ⌘⇧F 触发（VSCode `⌘⇧F` 全工程搜的 Web 端 detail-scope 映射）

## Decision

不再重复实现。TODO 项删除，本 doc 作记录 — 未来若再误提同需求时
retrospective 可查 implementation 已就位（line 2479-2649 + 12214）。

为什么我重复提了这个：在 propose 6 个新需求时（iter #492 后 TODO 空），
未 grep 既有 `detailReplaceMode` / `handleDetailReplaceAll` 关键词就拍
脑袋写了。这是 "already-implemented pivot" 的第二次（iter #495 是
ChatMini bubble→task）。procedure 改进：未来 propose 前先 grep 关键词
确认是否已实现。

## Verification

- 手测路径：detail.md 编辑器内按 `⌘⇧F` → search bar 弹 + replace 输
  入框可见 → 输 find / replace 文字 → 计数 chip 显「N/M」→ 「全部替换」
  按钮触发 handleDetailReplaceAll → editingDetailContent 更新所有命中
- 无新代码 / 无新测试

## Future iters (out of scope)

- 「regex 模式」switch — 当前仅 case-insensitive substring；regex 引
  转义 / 错误提示 / preview 复杂度，单独 iter 评估
- 「按选区限定 replace」— 长 doc 局部改名场景；需 selectionRange 持
  久化跨 input focus
- 「替换历史 / undo」— 单次 replace-all 错了 owner 需 ⌘Z 多次撤；可
  改成单步合并 undo 单位
