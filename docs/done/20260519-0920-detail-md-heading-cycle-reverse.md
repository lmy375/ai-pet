# detail.md 编辑器加「⌘⇧B 反向 heading-level cycle」shortcut（iter #551）

## Background

iter #548 加 ⌘⇧H forward cycle（none → h1 → h2 → h3 → h4 → none）。
但常见场景：

- h1 标题 owner 觉得太大想直接降到 none —— forward 路径要按 4 次
  （h1 → h2 → h3 → h4 → none）
- h2 想降为 h1 —— forward 要按 4 次（h2 → h3 → h4 → none → h1）

本 iter 加 **⌘⇧B 反向 cycle** 让降级 / 反向一键搞定。

## Cycle 状态机（反向）

```
none ──── #### ────► h4
  ▲                  │
  │             unshift ──► h3 (`### `)
  │                          │
  │                          unshift ──► h2 (`## `)
  │                                       │
  │                                       unshift ──► h1 (`# `)
  │                                                    │
  └──────────────────────── strip ◄───────────────────┘
```

- none → 加 `#### `（从 cycle「最深级别」开始，与 forward 镜像）
- h4 / h3 / h2 → 上一级（少 1 个 `#`）
- h1 → 删 `# ` 回 none
- h5 / h6（边界，与 ⌘⇧H 同 reset）→ none

## Changes

### `src/components/panel/PanelTasks.tsx`

新 `handleDetailHeadingCycleReverse` callback（紧贴
`handleDetailHeadingCycle` 之后）：

```tsx
const handleDetailHeadingCycleReverse = useCallback(
  (e: React.KeyboardEvent<HTMLTextAreaElement>): boolean => {
    if (!(e.metaKey || e.ctrlKey)) return false;
    if (!e.shiftKey || e.altKey) return false;
    if (e.key.toLowerCase() !== "b") return false;
    ...
    const m = /^(#{1,6}) (.*)$/.exec(line);
    let newLine: string;
    let cursorOffset = 0;
    if (m) {
      const lv = m[1].length;
      const rest = m[2];
      if (lv === 1) {
        newLine = rest;  // h1 → none
        cursorOffset = -2;
      } else if (lv <= 4) {
        newLine = `${"#".repeat(lv - 1)} ${rest}`;  // -1 级
        cursorOffset = -1;
      } else {
        newLine = rest;  // h5/h6 → none
        cursorOffset = -(lv + 1);
      }
    } else {
      newLine = `#### ${line}`;  // none → h4
      cursorOffset = 5;
    }
    ...
  },
  [],
);
```

#### 接入 onKeyDown 链

两个 textarea（split / edit-only）都接入。

#### Keyboard help modal

```tsx
["⌘⇧B", "当前行 heading level 反向循环（none→h4→h3→h2→h1→none；⌘⇧H 反向）"],
```

## Key design decisions

- **none → h4 是「反向开始」语义**：⌘⇧B 行为镜像 ⌘⇧H — forward 从 h1
  开始向深扩；reverse 从 h4 开始向浅收。两 shortcut 在 none 状态各走
  自己起点
- **5 步闭环对称**：⌘⇧H 5 步循环 vs ⌘⇧B 5 步循环 — 任意起点按 5 次
  同方向都回到原态，可预测
- **复用 ⌘⇧H 行扩边算法**：lastIndexOf "\n" + indexOf "\n" 同行级单元
  + regex `/^(#{1,6}) (.*)$/` — 行为一致让两 shortcut 在边界（h5/h6 /
  无 prefix）case 上协调
- **cursor offset 镜像**：⌘⇧H 加 `#` 时 +1，⌘⇧B 减 `#` 时 -1；⌘⇧H
  h4→none -5，⌘⇧B none→h4 +5；Math.max(lineStart, ...) 防越界与 forward
  同
- **modifier ⌘⇧B**：⌘B 是 bold（既有 ⌘B / ⌘I wrap shortcut）；shift
  修饰避开。⌘⇧B 在 IDE mostly 空 — "Backward heading" 助记。考虑过 ⌘⇧⌥H
  作 forward 反向，但 alt + shift 难按；单字母键位 ⌘⇧B 更顺手
- **不写 unit test**：纯字符串拼接 + 行扩边 + cursor 算术；逻辑 trivial
  + 既有 ⌘⇧H forward cycle 同 algorithm production 验证。GOAL.md
  "meaningful tests only" 规则下不引装饰性测试

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.43s)
- 后端无改动 — 纯前端 keyboard shortcut
- 手测：
  - 普通行 → ⌘⇧B → `#### foo`
  - `#### foo` → ⌘⇧B → `### foo`
  - `### foo` → ⌘⇧B → `## foo`
  - `## foo` → ⌘⇧B → `# foo`
  - `# foo` → ⌘⇧B → `foo`
  - h1 一键降 none（vs ⌘⇧H 要 4 次） — 主用例 ✓
  - ⌘⇧H + ⌘⇧B 任意起点 5 次循环回原态
  - 跨 split / edit-only 模式都触发
  - ⌘/ 帮助 modal 看到「⌘⇧B」行

## Future iters (out of scope)

- 「⌘⇧⌥H 多行 wrap as heading」批量版 — 选区每行都加 heading prefix；
  按需 propose
- 「⌘⇧Number」直接跳 level — ⌘1/⌘2 等冲突浏览器 tab；维持 cycle 模式
