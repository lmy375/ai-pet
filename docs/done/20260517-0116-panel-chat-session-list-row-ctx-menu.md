# PanelChat session 下拉行加右键 ctx menu（含 📌 pin 置顶）

## 背景

PanelChat 顶 tab bar 上的 session 选项卡已经支持右键 ctx menu（pin / rename / 复制标题 / 复制 ID / 重写标题）。但 owner 通过 "session 下拉" 进入长 session 列表后，要 pin 某个非显示在 tab 的会话必须：
1. click 切到该 session
2. 等加载
3. 右键 tab
4. pin

加 dropdown 行 onContextMenu 复用同 `sessionTabCtxMenu` —— 让 owner 长列表里直接右键就近 pin / rename。

## 改动

### `src/components/panel/PanelChat.tsx`

session 下拉行 (`pet-session-row` div) 加 onContextMenu：

```tsx
<div
  className="pet-session-row"
  onMouseEnter={() => handleSessionPreviewEnter(s.id)}
  onMouseLeave={handleSessionPreviewLeave}
  onContextMenu={(e) => {
    e.preventDefault();
    e.stopPropagation();
    setSessionTabCtxMenu({
      id: s.id,
      title: s.title,
      pinned: !!s.pinned,
      x: e.clientX,
      y: e.clientY,
    });
  }}
  style={...}
>
```

复用 `sessionTabCtxMenu` state + 既有 ctx menu render（line 6341+）—— 与 tab bar 完全一致 UX。

## 关键设计

- **复用 sessionTabCtxMenu**：与 tab bar 右键同一 menu state + render —— owner 心智模型一致（一处学会处处用）。修改菜单选项 / 加新 entry 只需改一处。
- **clientX/Y 锚点 anchor 到当前鼠标位置**：popover viewport-clamped 算法（既有 line 6345 max(8, min(...))）自然让 popover 不溢出。
- **stopPropagation 防 outside-click close**：与 tab bar onContextMenu 相同 stopPropagation 模板。
- **不抢 row.onClick（switch session）行为**：右键不触发左键路径；event 类型不同自然分流。
- **`pinned: !!s.pinned`** 双 negation：session.pinned 是 optional bool，`undefined → false` 兜安全。

## 不做

- **不复用 dropdown 自有 pin / rename / delete inline 按钮**：dropdown 既有 ✏ 改名按钮 + 🗑 删除按钮（line 4480+） 已存在，但行内"修改名 / 删除"模式比 ctx menu 更主动。本 iter 加 ctx menu 是 alternative 路径，与 inline 按钮共存。
- **不加 dropdown-specific menu items**：所有 entry 复用 tab bar 既有的 —— 让 owner 不必区分"在哪里看 ctx menu" 内容会变。
- **不写测试**：纯 onContextMenu handler；视觉验证（开 session 下拉 → 右键某行 → 应弹同样的 ctx menu）足够。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 1.22s
- 改动 ~20 行（onContextMenu handler + 注释）。既有 hover preview / preview click / switch session / inline rename / inline 删除 / sessionTabCtxMenu render 路径完全不动。

## TODO 状态

剩 2 条留池：
- PanelSettings 顶 search input
- PanelMemory "今天新增" chip drill-down

## 后续

- 给 PanelMemory item row 也加同款 ctx menu 框架（当前完全没有；hover hint iter #201 已暗示 ctx menu 存在，但行本身不响应右键）。
- ctx menu 加 "✂ fork 此 session" 按钮（既有 fork pipeline 但入口在 dropdown 顶按钮）。
- ctx menu 加 "🔗 复制 session 跳链" —— 让 owner 把 deeplink 贴别处一键回到此 session。
