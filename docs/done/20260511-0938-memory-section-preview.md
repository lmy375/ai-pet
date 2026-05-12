# PanelMemory category 头 hover preview

## 需求 vs 实际

TODO 原话提"sidebar hover preview"，但 PanelMemory 没有 sidebar 导航 ——
所有 category 是顶级 stacked sections，每段自带 header + items 列表。所以
"sidebar hover"概念不适用。

实际能补的预览场景：

1. **section header 上的 badge**（显 count）—— hover 显该 category 最近 3
   条 item title，省一次"展开 + 滚动"瞄内容
2. **fold "展开全部 N 条" 按钮**—— hover 显被隐藏的 N-5 条 title preview，
   让用户展开前先评估"值不值得展开"

两个改动都用原生 `title` attribute tooltip 实现，零额外 JS / 组件。

## 实现

`src/components/panel/PanelMemory.tsx`：

- 计算 `previewLines` = `cat.items.slice(-3).reverse().map("- title").join("\n")`
  假设 backend 按 updated_at 升序排，slice(-3) 即最近 3 条
- 空 category 时 tooltip 显"（空）"
- badge `<span style={s.badge} title={previewTip}>` 增加 title
- fold expand 按钮的 title 在 expanded === false 时显隐藏条 title 预览，
  控制总长 max 20 行（避免长 category 撑爆屏幕），>20 时附"还有 X 条"

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - hover butler_tasks badge → 浮"最近 3 条：- 早起喝水 / - 每周复盘 / - ..."
  - hover 空 category badge → "（空）"
  - 在 12 条 ideas category 上 hover "… 展开全部 12 条" → 显隐藏 7 条 title
    预览；点击后展开

## 不在本轮范围

- 没做完整 floating tooltip（自定义 React 组件）—— 原生 title 跨平台一致，零
  代码量，UX 足够。要 markdown 渲染 / 长 tooltip 滚动等需求再升级
- 没把 badge 改成 button —— 仅展示，没有交互目标；hover preview 就够

## TODO 池清空 → 自主提案

按规则 #1，提出 5 条新需求（已写入 TODO.md）：

1. PanelChat 历史会话过滤"含图片"
2. PanelTasks ⌘+K 跳到搜索框
3. SOUL.md 编辑器加字数 counter
4. /image -r 引用最近 assistant 文本入 prompt
5. ChatMini streaming 中长按 Esc 取消生成
