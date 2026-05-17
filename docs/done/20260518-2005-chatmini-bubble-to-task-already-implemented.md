# ChatMini 气泡右键「📌 转 P3 task」菜单项 — 已实现 pivot（iter #495）

## Discovery

本 TODO 项「ChatMini 气泡右键「📌 转 P3 task」菜单项：复用气泡 text
作新 task title — 快速 lift 聊天精华成任务」在加入 TODO 前已实现过。

定位：`src/components/ChatMini.tsx:3241-3283` — bubble 右键 ctx menu
中已含 `💾 转 task` 项：

```tsx
{hasText && (
  <button
    ...
    onClick={async () => {
      setCtxMenu(null);
      const flat = text.replace(/\s+/g, " ").trim();
      const titleRaw = flat.slice(0, 30);
      if (!titleRaw) return;
      try {
        await invoke<string>("task_create", {
          args: { title: titleRaw, body: text, priority: 3, due: null },
        });
        ...
        console.log(`💾 转 task 成功：${titleRaw}`);
      } catch (err) {
        console.error("create task from bubble failed:", err);
      }
    }}
    title={`一键把这条 bubble 转 task（P3，无 due）— 标题取前 30 字「...」，body = 全文。`}
  >
    💾 转 task
  </button>
)}
```

行为与 TODO 完全吻合：
- 入口：bubble 右键 ctx menu
- 输出：P3 task，title = 前 30 字 flat，body = 全文
- 错误 / 成功反馈：console.log + 1.5s ✓ 视觉确认

## Decision

不再重复实现。仅 emoji 选 `💾` vs TODO 提的 `📌` —— `📌` 在系统内已
表 "pinned" 语义（PanelTasks / PanelMemory），改 emoji 会引语义冲突。
`💾` ("save" → "convert to persistent task") 是更准的隐喻；保留既有。

TODO 项删除，本 doc 作记录 — 未来若再误提同需求时 retrospective 可
查 implementation 已就位。

## Verification

- 手测：ChatMini 任意 user / assistant bubble → 右键 → ctx menu 第 5
  项「💾 转 task」→ click → 后端 task_create P3 → bubble ✓ flash 1.5s
  → PanelTasks 看到新条目 title 是 bubble 前 30 字，body 是全文
- 无新代码 / 无新测试

## Future iters (out of scope)

- 弹「priority 选择 popover」让用户切 P3 → P5 / P7（当前 hardcoded P3）
- 弹「title edit popover」让用户先调标题再创建（当前自动取前 30 字）
- 加 "+ tags" 复选（基于既有 tags 矩阵） — 让 task 入队就含分类
