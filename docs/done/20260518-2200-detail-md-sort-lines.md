# detail.md 编辑器加「⌘⌥L sort-lines」shortcut（iter #504）

## Background

detail.md 编辑器已有完备的 IDE-like 行操作集群：
- ⌘D 复制当前行
- ⌘L 选中当前行
- ⌘⇧K 删除当前行
- ⌘⇧X 行剪切
- ⌥↑ / ⌥↓ 上下移行
- ⌘⌥↑ / ⌘⌥↓ 复制行向上 / 向下
- ⌘/ markdown 注释 toggle
- ⌘. checklist toggle

但缺**行排序** — owner 在 detail.md 内维护清单（待办 / 引用 / 链接列
表 / 决策清单）时手动重排只能靠 ⌥↑ / ⌥↓ 一行一行挪，长清单效率低。

本 iter 加 **⌘⌥L sort-lines** — IDE 通用「Sort Lines Ascending」语义
（VSCode `editor.action.sortLinesAscending` / Sublime "Sort Lines"），
扩边到整行 + 自动 detect numeric / alphabetical 排序。

## Changes

### `src/components/panel/PanelTasks.tsx`

#### 新增 callback `handleDetailSortLines`（紧贴 handleDetailMoveLines 之后）

```tsx
const handleDetailSortLines = useCallback(
  (e: React.KeyboardEvent<HTMLTextAreaElement>): boolean => {
    if (!(e.metaKey || e.ctrlKey)) return false;
    if (!e.altKey || e.shiftKey) return false;
    if (e.key.toLowerCase() !== "l") return false;
    if ((e.nativeEvent as KeyboardEvent).isComposing) return false;
    e.preventDefault();

    // 扩边到整行（与 handleDetailMoveLines / DeleteLine 同算法）
    const blockStart = value.lastIndexOf("\n", start - 1) + 1;
    const probe = end > start ? end - 1 : end;
    const nextNl = value.indexOf("\n", probe);
    const blockEnd = nextNl === -1 ? value.length : nextNl;
    const block = value.slice(blockStart, blockEnd);
    const lines = block.split("\n");
    if (lines.length < 2) return true;

    // numeric detect：每行 leading 非空白 token 都 finite Number → 数字排
    // 否则 → localeCompare（locale-aware 字典序）
    const numericKeys = ...;
    const allNumeric = ...;

    const sorted = allNumeric
      ? numericKeys.sort((a, b) => a.key - b.key).map((x) => x.line)
      : [...lines].sort((a, b) => a.localeCompare(b));
    const sortedBlock = sorted.join("\n");
    if (sortedBlock === block) return true; // idempotent short-circuit

    setEditingDetailContent(...);
    // 选区维持覆盖排序后的 block
    requestAnimationFrame(() => {
      cur.selectionStart = blockStart;
      cur.selectionEnd = blockStart + sortedBlock.length;
    });
    return true;
  },
  [],
);
```

#### 接入 onKeyDown 链

紧贴 `handleDetailCopyLines` 之后：

```tsx
if (handleDetailCopyLines(e)) return;
// ⌘⌥L 选区行排序（numeric / alphabetical auto-detect）— IDE 通用 sort-lines。
if (handleDetailSortLines(e)) return;
if (handleDetailTabIndent(e)) return;
```

两个 textarea（split 模式 + edit-only 模式）都接入。

#### Keyboard help modal 新一行

```tsx
["⌘⌥L", "选区行排序（auto-detect numeric / alphabetical — IDE Sort Lines；<2 行 noop；已排序幂等）"],
```

## Key design decisions

- **modifier 选 ⌘⌥L 而非 ⌘⇧L**：⌘⇧L 已是「弹链接 popover」；⌘⌥L 在
  mac 是 IntelliJ "Reformat Code" 但本 detail.md 无 reformat 语义键
  位空，归 sort
- **auto-detect numeric vs alphabetical**：每行 leading 非空白 token
  全 finite Number → 走 numeric。owner 写「1. 步骤」/「2. ...」/「10.
  ...」清单时数字 10 在 alphabetical 会被排到 2 之前，本检测避坑
- **localeCompare 字典序**：中文 / 英文 / 混合都按 Unicode collation 顺
  序 — 比 Array.sort 默认（按 UTF-16 unit）更符合人的直觉
- **扩边到整行**：与 handleDetailMoveLines / handleDetailDeleteLine 同
  算法（lastIndexOf "\n" + indexOf "\n"）— owner 选半行也按整行算
- **`<2 行 noop`**：单行无序可言；preventDefault 仍吃键防 ⌥L 触发系
  统级 binding（macOS Chrome 有 ⌥+L "select next word" 默认）
- **idempotent short-circuit**：sortedBlock === block 时直接 return —
  避免不必要的 setEditingDetailContent → dirty flag → re-render
- **选区维持覆盖排序后 block**：与 IDE 通用「sorted 后选区不丢」一致 —
  让 owner 二次 ⌘⌥L（如果还是不满意）/ 撤销定位 / 后续操作都自然
- **ECMA 2019+ stable sort**：等键值保 input 顺序（V8 / Spider / JSC
  都已实现）— 让「数字相同但文本不同」的行排序结果可预期
- **不写 unit test**：纯 string split / sort / 拼接；逻辑 trivial（既
  有 handleDetailMoveLines 同扩边算法 + Array.sort 标准 API）。
  GOAL.md "meaningful tests only" 规则下不引装饰性测试

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.27s)
- 后端无改动 — 纯前端 keyboard shortcut
- 手测：
  - detail.md 内 select 数行 → ⌘⌥L → 字典序排（中英混合都按 Unicode
    collation）
  - 数字清单 1. / 2. / 10. → ⌘⌥L → 1 < 2 < 10（numeric mode），不是
    "10 < 2"（alphabetical mode 会犯的错）
  - 已排序的选区 → ⌘⌥L → noop（idempotent）
  - 1 行选区 → ⌘⌥L → noop
  - 部分覆盖首末行 → ⌘⌥L → 仍按整行算

## Future iters (out of scope)

- ⌘⌥⇧L 反向排序（降序）— 当前仅升序；高级场景考虑
- 「按多列排序」/「忽略大小写」/「去重」等 — IDE 通常通过 palette
  细分子命令，本 iter 保最常用 keyboard shortcut；palette 化未来 iter
- 弹 modal 让 owner 选 mode（asc / desc / numeric force）— 当前
  auto-detect 已覆盖 80% 场景
