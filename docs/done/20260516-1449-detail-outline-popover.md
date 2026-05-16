# detail.md preview「📑 大纲」浮窗

## 背景

TODO 上 auto-proposed 一条："detail.md preview 段首浮『📑 大纲』浮窗：扫 H1-H3 标题显锚点列表，长 detail.md 跳节用。"

长 detail.md 用 H1 / H2 / H3 划分小节是 owner 自然的组织方式（"实施步骤 / 已知问题 / 后续 TODO" 等）。但 preview / split 模式下没结构化导航 —— 想跳到"已知问题"节得手动滚找。大纲浮窗扫所有 heading 列出可点击锚点，jump-to 一步到位。

## 改动

### `src/utils/inlineMarkdown.tsx`

#### `ParseMarkdownOpts.headingIdPrefix`

新增可选 option：当 set 时，每 H1-H3 div emit `id={prefix-h{counter}}`，counter 按出现顺序累计（不论 level）。slug-by-counter 而非 slug-by-text 避免：
- 同名标题碰撞
- 中文 slug 化复杂度
- 文本含特殊字符 (`/` `?` 等) 不能安全做 DOM id

#### heading 渲染

```tsx
headingCounter += 1;
<div
  id={opts?.headingIdPrefix ? `${opts.headingIdPrefix}-h${headingCounter}` : undefined}
  style={{
    fontWeight: 600,
    fontSize,
    marginTop: 4,
    marginBottom: 2,
    scrollMarginTop: 12,  // 为 sticky toolbar 留空间
  }}
>
```

`scrollMarginTop: 12` 让 `scrollIntoView({block: "start"})` 跳过来后 heading 不紧贴顶部被工具栏遮住。

### `src/components/panel/PanelTasks.tsx`

#### state + cleanup

```ts
const [detailOutlineOpen, setDetailOutlineOpen] = useState(false);
useEffect(() => {
  if (editingDetailTitle === null) setDetailOutlineOpen(false);
}, [editingDetailTitle]);
```

#### parseMarkdown 调用传 prefix

两处 preview/split 模式 parseMarkdown 加 `headingIdPrefix: \`pet-detail-${t.title}\``。task 标题作 namespace 隔离避免多任务同时打开 detail（实际互斥，但防御）。

#### 📑 按钮

视图模式切换行末加：仅 `split / preview` 模式 + content 含 heading 时显（`/^#{1,3}\s+/m.test(content)`）。click 切 detailOutlineOpen。激活态走 accent border + tint bg + 加粗。

#### 大纲浮窗 inline panel

`detailOutlineOpen` + split/preview 时渲染。扫 lines 提取 `^(#{1,3})\s+(.*)$` 累计 `headings: { level, text, counter }[]`。

```tsx
{headings.map((h) => (
  <button
    key={h.counter}
    onClick={() => {
      const id = `pet-detail-${t.title}-h${h.counter}`;
      const el = document.getElementById(id);
      if (el) el.scrollIntoView({ behavior: "smooth", block: "start" });
    }}
    style={{
      paddingLeft: (h.level - 1) * 12 + 4,  // 缩进显层级
      // ...
    }}
    title={`跳到「${h.text}」（H${h.level}）`}
  >
    <span style={{ color: muted, fontFamily: mono, fontSize: 10 }}>
      {"#".repeat(h.level)}
    </span>
    {h.text}
  </button>
))}
```

panel 最高 200px overflowY auto，shadow-sm + card bg + border 让 panel 视觉独立。

## 关键设计

- **inline panel 而非浮 absolute overlay**：浮窗推开下方编辑内容一段（max 200px），不挡 preview pane 本身。owner 选锚点时不必关浮窗 → click 跳节，浮窗仍开方便连续跳。overlay 会盖住 preview，跳节后看不到位置。
- **counter-by-occurrence id**：第 N 个 heading id = `${prefix}-h${N}`，与级别无关。outline 同序号匹配。不依赖 text slug 避免：同名 heading 互覆盖 / 中文 slug 复杂 / 特殊字符 DOM id 不合法。
- **`scrollMarginTop: 12`**：scrollIntoView 跳到目标后，heading 顶部留 12px 空隙避免被 sticky toolbar / status row 遮挡。CSS scroll-margin-top 是浏览器 native scroll behavior 的 standard signaling，零运行时 hack。
- **gate `split / preview` + 含 heading**：edit 纯文本模式没有渲染 pane，scrollIntoView 找不到元素 → 按钮 disable。无 heading 的 detail.md 大纲为空 → 按钮 disable（gate 在按钮渲染条件而非按钮 disabled state，UI 更干净）。
- **task title 作 id prefix**：`pet-detail-${title}` 让 DOM 单独 namespace。理论上 PanelTasks 同时只展开一条 detail（互斥），但防御性命名 + 防未来扩多 detail 同时编辑。
- **smooth scrollIntoView**：让跳转感知"位移"而非"闪动"，与 panel 内其它 scrollIntoView（focus row / search hit 等）一致。
- **缩进显层级**：H2 缩 12px，H3 缩 24px。视觉树结构一目了然。`{"#".repeat(level)}` 前缀让"这是几级"直观（防只看缩进认错）。

## 不做

- **不做 outline 在 edit 模式可用**：edit 模式渲染 `## text` 是 raw 文字，没 DOM id 对应。强行让 edit 模式按钮 fallback "切到 preview 再跳"会让按钮行为不一致。owner 真要 jump-to 自己切模式。
- **不做"outline 自动跟随当前 cursor"**：实现复杂（需追踪 cursor 是否落在某 heading 段内），且 preview 模式根本没 cursor。本 iter 是一击 jump，不做活跃 highlight。
- **不做"折叠节"**：outline 仅"导航"职责；折叠节内容是 markdown 渲染层的事。复杂度高 + 与 owner 主要工作流偏离。
- **不写测试**：DOM scrollIntoView + getElementById 在 jsdom 下 polyfill 不真实；既有 parseMarkdown / detail.md 编辑器路径都视觉验证。点 📑 → outline 浮出 → 点节 → preview 滚到 heading → 视觉确认。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.19s
- 改动 ~150 行（inlineMarkdown headingIdPrefix 15 + state + cleanup 7 + 按钮 30 + 浮窗 inline panel 80 + 两 parseMarkdown 调用各加 1 行 + 注释）；既有 parseMarkdown / detail.md 编辑器 / 9 按钮工具栏 / status chip 等路径完全不动。

## TODO 状态

empty —— 6 条 auto-proposed 全部完成（其中 1 条 stale 此前已移除）。下次启动 TODO 流程进入 auto-propose 分支。

## 后续

- outline 跟踪 preview pane 当前可见 heading（IntersectionObserver）→ 高亮 active 节，让 owner 知道"我在哪节"。
- outline 拖拽 reorder heading 段：把整节往上 / 往下挪 → 改 markdown 顺序。复杂度高 + 与 markdown 编辑边界冲突，等真有诉求再做。
- outline 右键 heading "复制锚点 link"：让 owner 跨任务 detail 互引（如果将来引入 task → task heading 跳转）。
- detail.md 顶部加 floating 大纲快捷栏（H1-H3 按钮直接显），不必先开浮窗 —— 短 outline (≤5 节) 时直接展开。
