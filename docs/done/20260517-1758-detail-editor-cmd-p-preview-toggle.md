# detail.md 编辑器 ⌘P toggle preview-only 模式（iter #360）

## Background

detail.md 编辑器已有 ✏️/🔀/👁 三模式 UI 按钮（DetailViewMode =
"edit" / "split" / "preview"），但无键盘快捷键。owner 看长 detail.md
时想"焦点纯阅读"需鼠标点 👁 预览，看完想接着写又要鼠标点回 ✏️。本
iter 加 ⌘P 一键 toggle（VSCode preview-lock 风 — 按一下 preview-only，
再按回写作姿态），并记下"进 preview 前的 mode"以便正确恢复。

## Changes

### `src/components/panel/PanelTasks.tsx`

#### 1. ref 记录"进 preview 前的 mode"（~line 2222）

```tsx
const detailViewModeBeforePreviewRef = useRef<DetailViewMode | null>(null);
```

- 仅用 ref（不进 state）：toggle 路径不需要触发 re-render
- 缺省 `null`：表示"当前不在通过 ⌘P 进入 preview 的态"
- owner 手动通过 👁 按钮切到 preview 时不写此 ref → 再 ⌘P 时
  fallback `"edit"`（合理出口）

#### 2. ⌘P 全局 capture-phase keydown（~line 2300）

```tsx
useEffect(() => {
  if (editingDetailTitle === null) return;
  const onKey = (e: KeyboardEvent) => {
    if (!(e.metaKey || e.ctrlKey)) return;
    if (e.shiftKey || e.altKey) return;
    if (e.key.toLowerCase() !== "p") return;
    e.preventDefault();
    e.stopImmediatePropagation();
    setDetailViewMode((cur) => {
      if (cur === "preview") {
        const restore = detailViewModeBeforePreviewRef.current ?? "edit";
        detailViewModeBeforePreviewRef.current = null;
        return restore;
      }
      detailViewModeBeforePreviewRef.current = cur;
      return "preview";
    });
  };
  window.addEventListener("keydown", onKey, { capture: true });
  return () =>
    window.removeEventListener("keydown", onKey, { capture: true });
}, [editingDetailTitle]);
```

- listener 仅在 `editingDetailTitle !== null`（编辑器开着）时挂载，
  与 ⌘F 同模式
- `preventDefault` 拦截浏览器默认 print dialog
- `stopImmediatePropagation` 防御未来重复绑定冲突
- 严格 modifier 匹配：metaKey/ctrlKey 必须，shift/alt 必须不
  在 — 与 useTaskKeyboardNav 的纯 `p`（pinned toggle，要求所有
  modifier 都不在）正交，两路径不冲突

#### 3. 快捷键速查 cheatsheet 加条目（~line 14554）

```tsx
["⌘P", "切到 preview-only 焦点阅读（再按回写作姿态 · VSCode preview-lock 风）"],
```

放在 detail.md 编辑器段，⌘F 之后 — 都是"读 / 找"语义。

#### 4. 👁 预览按钮 title 加 ⌘P 提示（~line 9945）

```
"纯预览（只看渲染结果）— 键盘 ⌘P 一键 toggle，VSCode preview-lock 风"
```

让 owner hover 👁 按钮时发现键盘等价。

## Key design decisions

- **toggle 行为而非"按住"**：VSCode preview-lock 实际也是 toggle 风
  — 一按进 preview，再按出。"按住才 preview" 反人类（手指累 / 选
  词难）。
- **ref vs state for "previous mode"**：toggle 路径不要 re-render
  贡献，ref 更精简。owner 切到 preview 后做的其它事（点 👁 按钮再
  切走 / 关闭编辑器）也不需要看到这 ref 值。
- **手动 vs 键盘进 preview 的恢复语义不同**：
  - 键盘 ⌘P 进 → ref 记下 → ⌘P 出回原态（split / edit）
  - 手动 👁 进 → ref 不动 → ⌘P 出 fallback `"edit"`
  这个区分让"我自己点了 👁 看完，想接着写" 路径下 ⌘P 也有出口，
  不强制 owner 再点 ✏️ 编辑按钮。
- **capture-phase + stopImmediatePropagation**：放未来防御。当前
  ⌘P 没其它绑定，但 ⌘F 已 capture 拦截了 — 保持两 handler 一致
  pattern 让后续阅读不需理解"为何这个 capture 那个不 capture"。
- **不持久化 preview-only 偏好**：⌘P 是会话内 toggle，不该跨 task
  / 跨 session 记忆。detailViewMode 本身已 localStorage 持久（line
  1604），那是 owner "我习惯哪种 mode 开始" 长期偏好；⌘P 是临时
  焦点切换，两层心智分开。

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.22s)
