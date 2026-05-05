# PanelMemory 编辑 modal Esc 关闭（Iter R110）

> 对应需求（来自 docs/TODO.md）：
> PanelMemory 编辑 modal Esc 关闭：点击 backdrop 已能关闭，但 Esc 更顺手（PanelChat 搜索 panel 已有此模式）；textarea / input focus 时按 Esc 也应整窗关闭。

## 目标

PanelMemory 编辑/新建 modal 现在两路退出：
1. 点 backdrop / 取消按钮
2. 输完保存

textarea 编辑长描述时 Esc 应能取消整个编辑（与浏览器 / Notion / VSCode
modal 的通用直觉一致）。当前 Esc 不响应。

加 global keydown 监听器（仅在 modal 开启时挂），Esc → 关闭 modal（等价
点 backdrop / 取消按钮）。

## 非目标

- 不做"已编辑确认丢弃"提示 —— 与既有 backdrop click 行为一致（无 dirty
  detect 直接丢弃）。R105 ⌘S 已提供"保存离开"路径
- 不监听其它快捷键 —— 仅 Esc，避免与 ⌘S（R105 textarea 内已捕获）冲突

## 设计

### useEffect 挂全局监听器

```ts
useEffect(() => {
  if (!editingItem) return;
  const handler = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      setEditingItem(null);
    }
  };
  window.addEventListener("keydown", handler);
  return () => window.removeEventListener("keydown", handler);
}, [editingItem]);
```

挂在 `window` 而非 modal 内 —— 让无论 focus 在 textarea / input / select /
modal 空白处都能捕获。`editingItem` 改为 null 时 effect 卸载，监听器自动
清掉。

`!editingItem` 短路返回让 modal 关时不挂任何 listener，避免无谓的事件
派发。

### 与 R105 ⌘S 的协调

R105 在 description textarea 上挂 onKeyDown 处理 ⌘S 保存。新加的 window
keydown 是 capture/bubble 双向都跑：
- textarea focus 时按 ⌘S：textarea onKeyDown 触发 + 阻止后 window 也跑（但
  window listener 只关心 Escape，不冲突）
- 任意位置按 Esc：window listener 触发关闭

无冲突。

### 测试

无单测；手测：
- 点编辑某 item → modal 打开 → focus 在 select
- 按 Esc → modal 关闭
- 重新打开 → focus 在 textarea → 按 Esc → modal 关闭
- 快速重复打开关闭：每次都正常（useEffect cleanup 正确）

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | useEffect + keydown listener |
| **M2** | tsc + build |

## 复用清单

- 既有 PanelChat 搜索 panel onKeyDown Escape 模式（线 input scoped）
- 既有 setEditingItem(null) 关闭 modal 路径

## 进度日志

- 2026-05-09 15:00 — 创建本文档；准备 M1。
- 2026-05-09 15:08 — M1 完成。useEffect 依赖 editingItem，!editingItem 短路返回；window keydown listener 仅监听 Escape，preventDefault + setEditingItem(null)；cleanup 在 effect 重跑或卸载时自动 removeEventListener。
- 2026-05-09 15:11 — M2 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (500 modules, 949ms)。归档至 done。
