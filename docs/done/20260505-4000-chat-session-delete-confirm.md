# Chat session 删除二次确认 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> chat session 删除二次确认：当前点 X 立即删，长会话误点会丢历史；加一句"确定？"内联弹层（仅 5s，5s 后回 X）即可降低误删风险。

## 目标

PanelChat session dropdown 里每条 session 旁有「删除」按钮，点击立即调
`handleDeleteSession` —— 没有撤销，长会话误点直接丢历史。本轮加内联二次确认：
- 第 1 次点击 → 按钮文案变 "确定？"，标记此 session 为 pending-delete
- 5 秒内再次点击 → 真正调 `delete_session`
- 5 秒过期 → 自动 revert 回 "删除"

复用既有 inline state，不引入 modal / dialog。多个 session 同时只能有 1 个
处于 pending（点 B 时 A 自动取消），与单 session-active 心智一致。

## 非目标

- 不做"undo 已删除会话"——session 文件已 fs::remove_file，无法回滚。本轮重
  心是"确定？"门槛，不做删除后的撤销。
- 不修改 `delete_session` Tauri 命令本身（仍是单步删除）。
- 不写 README —— chat 面板防误操作。

## 设计

### 状态

```ts
const [pendingDeleteId, setPendingDeleteId] = useState<string | null>(null);
const pendingDeleteTimerRef = useRef<number | null>(null);
```

### Handler

```ts
const handleDeleteClick = (id: string) => {
  if (pendingDeleteId === id) {
    // 二次点击 → 真删
    if (pendingDeleteTimerRef.current) {
      window.clearTimeout(pendingDeleteTimerRef.current);
      pendingDeleteTimerRef.current = null;
    }
    setPendingDeleteId(null);
    handleDeleteSession(id);
    return;
  }
  // 一次点击 → 转 pending，5s 自动 revert
  if (pendingDeleteTimerRef.current) {
    window.clearTimeout(pendingDeleteTimerRef.current);
  }
  setPendingDeleteId(id);
  pendingDeleteTimerRef.current = window.setTimeout(() => {
    setPendingDeleteId((prev) => (prev === id ? null : prev));
    pendingDeleteTimerRef.current = null;
  }, 5000);
};
```

### 按钮

```tsx
<button
  onClick={(e) => {
    e.stopPropagation();
    handleDeleteClick(s.id);
  }}
  style={{
    padding: "2px 6px",
    borderRadius: "4px",
    border: "none",
    background: pendingDeleteId === s.id ? "#dc2626" : "#fee2e2",
    color: pendingDeleteId === s.id ? "#fff" : "#dc2626",
    fontSize: "11px",
    cursor: "pointer",
    fontWeight: pendingDeleteId === s.id ? 700 : 400,
  }}
  title={pendingDeleteId === s.id ? "再点一次确认删除（5 秒后自动撤回）" : "删除会话"}
>
  {pendingDeleteId === s.id ? "确定？" : "删除"}
</button>
```

红填充 + 加粗 + 白字 = 显式警示用户"再点就删了"。

## 测试

无后端改动；纯 UI 状态机。无 vitest 设施，靠 tsc + 手测。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | pendingDeleteId 状态 + handleDeleteClick + 按钮渲染 |
| **M2** | tsc + build + cleanup |

## 复用清单

- 既有 `handleDeleteSession`（不动）
- 既有 session dropdown 行布局

## 进度日志

- 2026-05-05 40:00 — 创建本文档；准备 M1。
- 2026-05-05 40:10 — 完成实现：`PanelChat.tsx` 加 `pendingDeleteId` 状态 + `pendingDeleteTimerRef` 5s timer ref + `handleDeleteClick` 二次点击门槛（同 id 第 2 次点击即删；点别的 session 替换 pending 取消旧）。session dropdown 删除按钮文案 / 配色随 pendingDeleteId 切换：默认浅红 "删除"，pending 红填充加粗 "确定？"。`pnpm tsc --noEmit` 干净；`pnpm build` 497 modules 全过。TODO 移除条目；本文件移入 `docs/done/`。
  - **README 不更新** —— chat 面板防误操作微调。
  - **未做手动 dev 验证**：当前会话不便启动 Tauri 桌面 app；状态机简单且 timer ref 模式既有先例。
