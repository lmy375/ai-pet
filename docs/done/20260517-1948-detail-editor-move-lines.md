# detail.md 编辑器 ⌥↑ / ⌥↓ 上下移当前行（iter #379）

## Background

detail.md 编辑器 IDE-style 行操作集群已有 ⌘D 复制行 / ⌘L 选中行 /
⌘⇧K 删除行 / Tab 缩进 / ⌘B / ⌘I / ⌘/ markdown 注释。缺最后一个常
用：**行移动**。owner 在 markdown list 调整顺序、把段落往上 / 下挪
当前需"复制 + 删 + 粘"三步。本 iter 加 ⌥↑/⌥↓ 一键移动，与 VSCode
/ Sublime / JetBrains IDE 通用习惯一致，补完 IDE-cluster。

## Changes

### `src/components/panel/PanelTasks.tsx`

#### 1. `handleDetailMoveLines` callback（~line 2940，⌘⇧K 删除行附近）

```ts
const handleDetailMoveLines = useCallback((e): boolean => {
  if (!e.altKey) return false;
  if (e.metaKey || e.ctrlKey || e.shiftKey) return false;
  if (e.key !== "ArrowUp" && e.key !== "ArrowDown") return false;
  if (isComposing) return false;
  // ... swap logic ...
}, []);
```

**算法**：
1. 计算选区覆盖的行范围 `[firstLineStart, lastLineEnd]`（与
   handleDetailSelectLine 同算法 — lastLineEnd 不含末尾 `\n`；end >
   start 时用 `end - 1` probe 避免选区止于行起点时误选下一行）
2. `block = value.slice(firstLineStart, lastLineEnd)`
3. **⌥↑**：firstLineStart === 0 → no-op（首行已是顶）。否则：
   - 上一行范围 `[prevLineStart, prevLineEnd)`，prevLineEnd 是
     firstLineStart - 1（上一行末的 `\n` 位）
   - 新 value = before-prev + block + "\\n" + prev + after-block
   - 选区平移 delta = `prevLineStart - firstLineStart`（负数）
4. **⌥↓**：lastLineEnd >= value.length → no-op（末行已是底）。否则：
   - 下一行范围 `[nextLineStart, nextLineEnd)`，nextLineStart =
     lastLineEnd + 1（跳过中间 `\n`）
   - 新 value = before-block + nextLine + "\\n" + block + after-next
   - 选区平移 delta = `nextLine.length + 1`（正数）

#### 2. 接两 textarea onKeyDown chain（split + edit modes）

注入位置：handleDetailDeleteLine 之后、handleDetailTabIndent 之前
（行操作集群相邻 — `⌘D 复制 / ⌘L 选中 / ⌘⇧K 删除 / ⌥↑↓ 移动`
连续）。

#### 3. placeholder + cheatsheet

- 两 textarea placeholder 加 "⌥↑/⌥↓ 上下移行"
- 速查 modal 增条目 `["⌥↑ / ⌥↓", "上下移当前行（或选区多行 — 与
  VSCode / Sublime IDE 通用）"]`，紧贴 Tab/⇧Tab 行（IDE-cluster
  集中）

## Key design decisions

- **alt 单 modifier**：与 VSCode/Sublime 一致。shift/ctrl/meta 一律
  不响应让位其它快捷键（⇧↑ 选区扩展是原生 textarea 行为，⌘↑ 是
  textarea 跳文首）。
- **选区平移而非保留绝对位置**：让 owner "我刚移动这块" 直觉一致 —
  选区跟着块走，再按一次 ⌥↑ 继续往上推。这是 VSCode 默认行为。
- **首行 ⌥↑ / 末行 ⌥↓ no-op + preventDefault 吃事件**：返 true 让
  下游 handler 不再处理 + 阻止浏览器默认（macOS Safari 可能滚滚动
  条；Tauri webview 通常无害但兜底）。
- **`probe = end > start ? end - 1 : end`**：与 handleDetailSelectLine
  / handleDetailTabIndent 同模板 — 选区末端正好在行起点时不应算
  覆盖那条 line。
- **不引入 ⇧⌥↑/↓ 复制行**：scope 守住 move；duplicate 用 ⌘D 已覆
  盖。如未来 owner 需要 "复制行往上插" 再加 ⇧⌥↑ 等价键。
- **不为单 fn 引 unit test runner**：项目无 .test.tsx 历史；
  行为是 IO + state ops 非纯函数，build pass + 手测足够（单行 /
  多行选区 / 首末行 noop / cursor 平移 四场景）。

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.23s)
- 后端无改动
