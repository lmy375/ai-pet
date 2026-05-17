# detail.md 编辑器 ⌘⌥↑ / ⌘⌥↓ 复制当前行向上 / 向下（iter #386）

## Background

iter #379 加了 ⌥↑/⌥↓ 移动行；既有 ⌘D 复制当前行到下方。但缺
"复制向上"路径 — 想把当前段往上 dup 一份要手动选 + 复制 + 光标
移 + 粘。Sublime / VSCode 风格 ⌘⌥↑/↓（VSCode 是 ⌥⇧↑/↓ 这里换
⌘⌥ modifier 避开 shift 集群）一键搞定。

完成 IDE 行操作集群：⌘D 复制 + ⌘L 选中 + ⌘⇧K 删除 + ⌥↑↓ 移动
+ ⌘⌥↑↓ 复制（本 iter）+ Tab 缩进 + ⌘/ 注释。

## Changes

### `src/components/panel/PanelTasks.tsx`

#### 1. `handleDetailCopyLines` callback（~line 3068）

```ts
const handleDetailCopyLines = useCallback((e): boolean => {
  if (!(e.metaKey || e.ctrlKey)) return false;
  if (!e.altKey || e.shiftKey) return false;
  if (e.key !== "ArrowUp" && e.key !== "ArrowDown") return false;
  if (isComposing) return false;
  // 算 [firstLineStart, lastLineEnd]，与 handleDetailMoveLines 同算法
  // - ArrowUp: 在 firstLineStart 之前插 block + "\n"；选区保留在新
  //   副本（原位置 — 文本下沉）
  // - ArrowDown: 在 lastLineEnd 之后插 "\n" + block；选区移到新副本
  //   delta = lastLineEnd + 1 - firstLineStart
}, []);
```

#### 2. 接两 textarea onKeyDown chain（split + edit modes）

位置：handleDetailMoveLines 之后、handleDetailTabIndent 之前。同
modifier-family 集群相邻。

handler chain 顺序的 modifier 互斥保证不冲突：
- ⌥↑/↓ 移动行：alt + (no ctrl/meta/shift)
- ⌘⌥↑/↓ 复制行：alt + (ctrl|meta) + (no shift)
- 两 handler 各自第一行 modifier check 互斥，return false 让位

#### 3. placeholder + cheatsheet

- 两 textarea placeholder 加 "⌘⌥↑/⌘⌥↓ 复制行"
- 速查 modal 增条目 `["⌘⌥↑ / ⌘⌥↓", "复制当前行（或选区多行）...
  Sublime 风 — 与 ⌥↑↓ 移动行同字母键、不同 modifier 区分复制 vs
  移动"]`，紧贴 ⌥↑/⌥↓ 行下方

## Key design decisions

- **modifier 选 ⌘⌥↑/↓ 而非 VSCode 标准 ⌥⇧↑/↓**：与 iter #379
  ⌥↑/↓ 移动行同字母键集群，⌘ 修饰扩展"copy 而非 move"语义。⇧
  modifier 留给"扩展选区"等未来用途（textarea 原生 ⇧↑ 已是选区
  expand）。
- **选区平移逻辑差异（up vs down）**：
  - up：插入在 firstLineStart 之前 → 新副本占据原位置 → 选区不动
    （原始 start/end 已对应新副本）
  - down：插入在 lastLineEnd 之后 → 新副本位置 = 原 lastLineEnd +
    1 + (start - firstLineStart) → 选区移到新副本 delta =
    lastLineEnd + 1 - firstLineStart
  这种"选区跟随新副本"让 owner 再按一次 ⌘⌥↓ 连续 dup 自然（VSCode
  ⌥⇧↑/↓ 同行为）。
- **与既有 ⌘D 行为差异**：⌘D 仅复制当前行（多行选区时只在选区
  末插同样选区文本）；本 handler 走"按行 block"语义 — 多行选区
  复制整 line set。多数 owner 用 ⌘D 复制单行 / 用 ⌘⌥↑↓ 处理 line
  block 时差异显现。两条路径覆盖不同 mental model。
- **probe = end > start ? end - 1 : end**：与 handleDetailMoveLines
  / handleDetailTabIndent 同模板，选区末端正好在行起点时不算覆盖
  下一行。
- **不为单 fn 引 unit test runner**：与 iter #368 / #375 / #379
  同 — 项目无 .test.tsx，行为是 IO + state ops；build pass + 手测
  足够（单行 / 多行选区 / 上 / 下 四场景手测一遍）。

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.25s)
- 后端无改动
