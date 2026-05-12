# PanelTasks title inline #tag 高亮

## 需求

task body 已经按"#urgent 整理 Downloads"等 inline #tag 抽到 `t.tags` +
渲染为 tag chip 行（颜色 / 可点筛选 / 右键改色）。但 task title 里若用户
也写 `#urgent`（如直接把 priority 信号嵌进 title），渲染只是纯文本。视觉
不一致 —— 同一个 #urgent 在 body 里五彩、在 title 里素颜。

## 实现

`src/components/panel/PanelTasks.tsx`：

- title 渲染 IIFE 替代原 `<HighlightedText text={t.title} query={search} />`：
  - 正则 `#([\p{L}\p{N}_-]+)` (unicode aware, 中英文 + 数字 + `_` / `-`)
    split title
  - 非 tag 段 → `<HighlightedText text query>` 保 search 关键字高亮
  - tag 段 → 一个 span 包 `#tagName`：
    - `getTagTintStyle(tagName)` 命中（用户已配色）→ 用那个 tint
    - 未配色 fallback → `--pet-tint-blue-bg` / `-fg`（一致的"这是 tag"
      视觉提示，与已配色不冲突）
    - 加 5px padding + 圆角 + 加粗 + 0.92em 字号，chip 风格但比 body chip
      轻量（不抢主标题视觉）
    - hover tooltip "（右键 tag chip 行可改色）"提示怎么 customize
- 无 `#tag` 命中时 return 单个 HighlightedText —— 零 tag title 完全走原路径

## 与 body tag 的一致性

颜色：tagColors map 同源 → 同一 tag 在 title / body / chip filter / 任意
位置颜色保持一致。改色生效路径：右键 body tag chip 行的 chip → 颜色 popup
→ 选 → 所有出现该 tag 的位置（含 title inline）即时更新。

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - title "整理 #urgent Downloads" → "#urgent" 显蓝（默认）chip 风格
  - 给 #urgent 设红色（右键 body 的 #urgent chip）→ title 内的 #urgent 也
    立刻变红
  - search "整理" + title 含 "整理 #urgent" → "整理" 黄高亮 + "#urgent"
    tag 高亮共存（分段渲染）
  - 无 #tag 的 title → 完全走原 HighlightedText 路径，0 副作用
  - 中文 tag `#紧急` 也命中 unicode 正则；纯英文 / 数字 / `-` / `_` 都支持
  - rename 输入态不显高亮（input 是 text-only，rename 期间用户专注于编辑）

## 不在本轮范围

- 没让 title #tag 也被 backend 计入 t.tags：那要改后端 parse_tags 扫描范
  围（目前只看 description）；本轮纯视觉
- 没做 description / body 部分的二次高亮：body 区已有 chip 行明示，inline
  highlight 容易重复
- 没做 detail.md preview 内的 tag 高亮：那是 markdown 渲染域，parseMarkdown
  不动

## TODO 池剩余

- ChatMini hover 显已 mark NOW 任务列表
- PanelChat session ⑂ fork 按钮
- PanelMemory consolidate 进度 + cancel
- PanelDebug 统计窗口快速切换 1d / 3d / 7d
