# detail.md 编辑器 📏 wrap ruler（iter #443）

## Background

detail.md 编辑器既有 🔢 行号 gutter toggle — 在 textarea 左侧浮一列
按 `\n` 分段的行号。但纯灰单色 gutter 看不出哪行超长 — markdown 写作
时一些行写到 100+ 字会撑出 wrap 多行视觉混乱（PR diff / GitHub 渲染
也会自动 wrap），owner 想"哪些行需要拆"得手动数字符。

本 iter 加 📏 wrap ruler toggle — 当 gutter 渲染时，line.length > 80
的逻辑行的行号 cell 染黄 + 加粗 + tooltip 显具体字数。IDE 80-col
ruler 惯例同 lens — code-like 写作辅助。

## Changes

### `src/components/panel/PanelTasks.tsx`

#### 1. `wrapRuler` state + toggle

紧贴既有 `showDetailGutter` state，复用同 localStorage 模板：

```tsx
const [wrapRuler, setWrapRuler] = useState<boolean>(() => {
  try {
    return window.localStorage.getItem("pet-detail-wrap-ruler") === "1";
  } catch {
    return false;
  }
});
const toggleWrapRuler = useCallback(() => { ... }, []);
```

独立持久化 key `pet-detail-wrap-ruler`（与 `pet-detail-gutter` 平行）—
owner 可单独 toggle ruler 不影响 gutter 状态。

#### 2. 📏 工具栏按钮

紧贴现有 🔢 gutter 按钮：

```tsx
<button
  onClick={toggleWrapRuler}
  disabled={!showDetailGutter}
  title={
    !showDetailGutter
      ? "先开启 🔢 行号 gutter — ruler 在 gutter 上染色"
      : wrapRuler ? "关闭 80 字 ruler …" : "显 80 字 ruler …"
  }
  style={{
    ...mdToolbarBtnStyle,
    background: wrapRuler && showDetailGutter
      ? "var(--pet-tint-yellow-bg)" : ...,
    opacity: !showDetailGutter ? 0.5 : 1,
    cursor: !showDetailGutter ? "not-allowed" : "pointer",
  }}
  aria-pressed={wrapRuler}
>📏</button>
```

- `disabled={!showDetailGutter}`：ruler 没 gutter 可染 → button 灰显
  + not-allowed + tooltip 引导先开 gutter
- active 状态：黄 tint pill 与既有 🔢 蓝 tint 区分（两 toggle 同时
  开时一眼看到"两个都生效"）

#### 3. Gutter 渲染分支

原 gutter `Array.from(...).join("\n")` 单 block 渲染保留作 fast-path —
ruler off 时无 per-line DOM 开销。ruler on 时切到 per-line `<div>` map：

```tsx
return (
  <div ref={detailGutterRef} aria-hidden style={baseStyle}>
    {Array.from({ length: lineCount }, (_, i) => i).map((i) => {
      const over = (lines[i]?.length ?? 0) > 80;
      return (
        <div key={i} style={{
          background: over ? "var(--pet-tint-yellow-bg)" : "transparent",
          color: over ? "var(--pet-tint-yellow-fg)" : "var(--pet-color-muted)",
          fontWeight: over ? 600 : 400,
          paddingRight: 2,
        }} title={over ? `第 ${i+1} 行 ${lines[i].length} 字 — 超 80 字` : undefined}>
          {i + 1}
        </div>
      );
    })}
  </div>
);
```

baseStyle 提取为 `React.CSSProperties` const 避免两分支重复样式漂移；
仅 `whiteSpace: "pre"` 在 off-path 加（per-line 时不需要）。
inherit lineHeight 1.65 + fontSize 12 让每行 `<div>` 自然撑到 textarea
单行高，scroll 同步逻辑（detailGutterRef.scrollTop = textarea.scrollTop）
无需改动。

## Key design decisions

- **threshold 80 字**：与 IDE 行业 80-col ruler 惯例一致（PEP-8 / Linux
  kernel coding style / GitHub side-by-side diff 默认宽度都 80）。中文
  cjk 同样 1 字单位 — 一句话 80 个汉字 ≈ 160 ASCII 字符宽度，超过
  GitHub 渲染就要 wrap。pet 风格用「字」单位与中文文档语境匹配
- **line.length（UTF-16 code unit）非 grapheme**：CJK 1 / ASCII 1 / 多数
  emoji 2 — 与 IDE 标尺逻辑一致（VS Code / Sublime 80-col 标尺也是
  UTF-16 单位）。grapheme cluster 精确算法（含 ZWJ emoji sequence）
  对 ruler 这个粗粒度警示 lens 是过度工程
- **per-line render 仅 ruler on 时**：off 时单 block + `join("\n")` 是
  最便宜的 textarea-mirror 协议 — 这条路径已经 production 验证；保
  fast-path 不引入 ruler 关时 N 个 `<div>` 的 mount cost
- **disabled 不藏 button**：button 位置稳定 + tooltip 提示先开 gutter
  比"动态藏"更稳；owner 知道 ruler 入口在哪不必猜
- **ruler 与 gutter 平行 toggle 而非合并**：合并意味着「开 ruler 自动
  开 gutter」 — 但 gutter 是常用功能（短笔记定位），ruler 是写长段落
  时偶用的辅助。让 owner 明确两次点击 = 明确两个意图
- **不引 cursor 当前行高亮**：只染「超长」状态。如果还染「当前光标
  行」 → 两套 state 并存视觉乱；owner 关心的是「我哪行写太长了」非
  「光标在哪」（textarea 本身就有 cursor 指示）
- **不引每行字数 inline counter**：仅 hover tooltip 显具体字数。inline
  会撑宽 gutter（36px → 50+px），破坏 textarea 主体的视觉重心
- **不写 unit test**：纯 CSS 条件渲染 + localStorage toggle — `tsc` +
  `vite build` 通过即足够；GOAL.md「meaningful tests only」规则下，
  这类装饰性 render 测试不该引入
- **不动 scroll sync 协议**：per-line 渲染保持同总高（每行 1.65em）—
  既有 `detailGutterRef.scrollTop = textarea.scrollTop` 行为不变

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.25s)
- 后端无改动 — 纯前端 UI 增强
- 手测：打开 detail.md 编辑 → 点 🔢 开 gutter → 点 📏 开 ruler → 写一
  行 100 字 → 行号 cell 染黄 + hover tooltip 显「第 N 行 100 字 — 超
  80 字」→ 关 📏 → 染色消失但 gutter 仍在
