# PanelMemory description 双击 inline 编辑

## 背景

TODO（上一轮 auto-proposed）：

> PanelMemory item description 双击 inline 编辑：免开 modal 改 description（与既有 title 双击改名同模式）。

PanelMemory 已有"双击 title 进 inline rename input"路径（renamingMemoryKey + commitRenameMemory + 同步 pin keys），但 description 改动仍要走"点编辑按钮 → 弹 modal → 改 → 保存"三步。多数小幅改（typo / 加一句小细节）配比 modal 太重。补一个对偶的 description inline 编辑路径。

## 改动（frontend only）

### `src/components/panel/PanelMemory.tsx`

**1. 状态机（紧贴 rename state 之后）**

```ts
const [editingDescKey, setEditingDescKey] = useState<string | null>(null);
const [editingDescDraft, setEditingDescDraft] = useState("");
const [editingDescBusy, setEditingDescBusy] = useState(false);
```

key 复用 `${catKey}::${title}` 跨 category 唯一约定，与 rename / pinnedKeys / armedDeleteKey 同模式。

**2. commit + cancel 助手**

```ts
const cancelDescEdit = () => { setEditingDescKey(null); setEditingDescDraft(""); };
const commitDescEdit = async () => {
  // split key → (category, title)
  // 与原值相等 → noop 短路
  const origItem = index?.categories[category]?.items.find(i => i.title === title);
  if (origItem && origItem.description === newDesc) { cancelDescEdit(); return; }
  setEditingDescBusy(true);
  try {
    await invoke("memory_edit", { action: "update", category, title, description: newDesc, detailContent: null });
    await loadIndex();
    setEditingDescKey(null);
    setEditingDescDraft("");
  } catch (e) {
    setMessage(`保存失败：${e}`);
    setTimeout(() => setMessage(""), 4000);
  } finally {
    setEditingDescBusy(false);
  }
};
```

走既有 `memory_edit` IPC（与既有 modal 编辑同一后端路径），mirror SQLite 双写 / butler_history 记录等都跟随。**与原值相等 noop**：避免无意义写盘 / 触发"已更新"toast 等噪音。

**3. 双击 + textarea render**

description div 加 `onDoubleClick`：

```tsx
{editingDescKey === `${catKey}::${item.title}` ? (
  <textarea autoFocus ... onKeyDown={...} onBlur={() => void commitDescEdit()} />
) : (
  <div onDoubleClick={(e) => {
    if (renamingMemoryKey !== null) return;  // 与 rename 互斥
    e.stopPropagation();
    setEditingDescKey(`${catKey}::${item.title}`);
    setEditingDescDraft(item.description);
  }} title="双击编辑..."> 
    {renderContentWithTaskRefs(displayDesc, ...)}
  </div>
)}
```

**关键设计**：

- **rename 优先级**：`renamingMemoryKey !== null` 时双击 description noop，让两个 inline 编辑器视觉不打架。
- **task ref token 自带 stopPropagation**：双击 ref token（如 `「整理 Downloads」`）仍走任务跳转语义，本 handler 不会被触发。
- **editingDescDraft 来自 item.description**（raw 描述含 `[done] [error: ...]` 等 marker），不是 displayDesc（被 stripped 过的显示版）—— 用户编辑保留 marker 完整性。
- **textarea row 自适应**：`Math.min(6, Math.max(2, lineCount+1))` 让 2-6 行随内容浮动；> 6 行可拖 resize handle。
- **onBlur 自动 commit**：点击别处自动保存（与 PanelTasks detail.md 编辑同模式）；用户不必显式按 Enter。
- **Enter 保存 / Shift+Enter 换行 / Esc 取消**：IM 类肌肉记忆。IME composing 期 Enter 不触发（`nativeEvent.isComposing` 检测）。

提示文案（10px muted）紧贴 textarea 下方：「Enter 保存 · Shift+Enter 换行 · Esc 取消 · 失焦自动保存」让 onboarding 不必先翻文档。

## 不做

- **不挂 detail_content 编辑**：detail.md 内容仍走"编辑详情"button → 大 modal 路径。description 是 ≤ 300 字短描述，inline 处理合适；detail.md 可能上千字，inline 显得拥挤。
- **不动 modal 编辑路径**：modal 仍是"完整编辑（title + description + detail_content）"入口；inline 是 "小修 description" 快捷路径。两条并存互补。
- **不写测试**：前端无 vitest；逻辑是 IO 调度 + render，与既有 commitRenameMemory 同模式。
- **不让按 Esc 关父 panel**：textarea 内的 Esc 在 commit/cancel 时 e.preventDefault，所以不会冒泡到 panel-wide Esc 监听。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.16s
- 改动 ~100 行（state 12 + handlers 35 + render block 55）；既有 rename / delete / pin / hover preview / search 路径全部不动。

## TODO 状态

- 本轮实现 1 条。
- TODO 剩 1 条：会话标题 LLM 自动重写按钮。
