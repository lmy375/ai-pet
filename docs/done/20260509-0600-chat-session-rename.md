# PanelChat 会话标题内联重命名（Iter R101）

> 对应需求（来自 docs/TODO.md）：
> PanelChat 会话标题内联重命名：dropdown 行标题 hover 时露 ✏️ 图标，点击进 inline edit（input + 回车保存 / Esc 取消），调既有 save_session 命令落库。

## 目标

PanelChat 顶 bar 会话标题来自两个路径：
1. `create_session` 默认 "新会话"
2. `saveCurrentSession` 自动从首条 user 消息切前 20 字（line 240）

但用户没法手动改名 —— 一个跑了一周的会话叫"嗯，今天我..."这种切片名不友好；
切回来想改名只能删掉重建。

加 inline rename：dropdown 行末 ✏️ 按钮，点击切到 input 编辑模式；Enter
保存 / Esc 取消 / 失焦保存。

## 非目标

- 不引入 backend `rename_session` 命令 —— 既有 `save_session` 接受完整
  Session struct，前端 load → 改 title → save 已能完成 rename 语义
- 不在顶 bar 标题做 inline rename —— dropdown 是会话管理入口，rename
  放在那里更直觉
- 不限制重名 / 长度 —— title 是软语义，让用户自己负责
- 不持久化"上次重命名时间" / 不区分自动 vs 手动名

## 设计

### state

```ts
const [renamingId, setRenamingId] = useState<string | null>(null);
const [renameDraft, setRenameDraft] = useState("");
```

只一个会话能处于 edit 模式（renamingId 非 null）；切到别的会话或 commit/
cancel 后 reset 到 null。

### handler

```ts
const startRename = (s: SessionMeta) => {
  setRenamingId(s.id);
  setRenameDraft(s.title);
};

const commitRename = async () => {
  const id = renamingId;
  const newTitle = renameDraft.trim();
  if (!id) return;
  if (!newTitle) {
    // 空标题视为放弃（与 cancel 等价）
    setRenamingId(null);
    return;
  }
  try {
    const session = await invoke<Session>("load_session", { id });
    session.title = newTitle;
    await invoke("save_session", { session });
    const idx = await invoke<SessionIndex>("list_sessions");
    setSessionList(idx.sessions);
    if (id === sessionId) setSessionTitle(newTitle);
  } catch (e) {
    console.error("Failed to rename session:", e);
  } finally {
    setRenamingId(null);
  }
};

const cancelRename = () => setRenamingId(null);
```

`load_session` 是必要的 —— 后端 save_session 需要完整 messages / items；
不能光发 metadata。这意味着 rename 会触发一次完整 round-trip，但单 session
load + save 在本机 IO 是廉价的。

### 渲染

```diff
 <div style={{ flex: 1, minWidth: 0 }} onClick={() => switchSession(s.id)}>
+  {renamingId === s.id ? (
+    <input
+      autoFocus
+      value={renameDraft}
+      onChange={(e) => setRenameDraft(e.target.value)}
+      onKeyDown={(e) => {
+        if (e.key === "Enter") commitRename();
+        else if (e.key === "Escape") cancelRename();
+      }}
+      onBlur={commitRename}
+      onClick={(e) => e.stopPropagation()}
+      style={{
+        width: "100%",
+        padding: "2px 6px",
+        fontSize: 13,
+        border: "1px solid var(--pet-color-accent)",
+        borderRadius: 3,
+        background: "var(--pet-color-card)",
+        color: "var(--pet-color-fg)",
+        outline: "none",
+      }}
+    />
+  ) : (
+    <>
       <div style={{ ...title... }}>{s.title} {item_count}</div>
       <div style={{ ...date... }}>{s.updated_at.split("T")[0]}</div>
+    </>
+  )}
 </div>
+{renamingId !== s.id && (
+  <button
+    onClick={(e) => {
+      e.stopPropagation();
+      startRename(s);
+    }}
+    style={...inline 链接式样式...}
+    title="重命名会话"
+    aria-label="rename"
+  >
+    ✏️
+  </button>
+)}
 <button ...delete...>...</button>
```

✏️ 按钮风格：低对比度小图标，让它不与"删除"按钮抢主导位。点击 stopPropagation
防触发 switchSession。

### 测试

无单测；手测：
- 默认渲染：每行右侧出现 ✏️ + 删除两按钮
- 点 ✏️ → 标题区切到 input + 自动 focus + 选中默认值
- 改文字 + Enter → 立即保存 → list 刷新 → 顶 bar 标题（如果是当前会话）也跟着变
- 改文字 + Esc → 不保存退出 edit
- 点别处失焦 → 等同 commit
- 改成空 → 视为 cancel（保留原 title）
- 失败时 console.error，UI 退出 edit 模式（用户看到 list 没变知道失败）

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | state + handler |
| **M2** | dropdown row 渲染改造 + ✏️ 按钮 |
| **M3** | tsc + build |

## 复用清单

- 既有 `load_session` / `save_session` Tauri 命令
- 既有 `list_sessions` 刷新路径
- 既有 `setSessionTitle` 同步顶 bar

## 进度日志

- 2026-05-09 06:00 — 创建本文档；准备 M1。
- 2026-05-09 06:08 — M1 完成。`renamingId / renameDraft` state；`startRename / commitRename / cancelRename` 三件套放 handleDeleteSession 之前；commit 内部 load_session → 改 title → save_session → list_sessions 刷新；当前 session 同步 setSessionTitle。
- 2026-05-09 06:14 — M2 完成。dropdown row 内 `renamingId === s.id ?` 切到 input（autoFocus / Enter 提交 / Esc 取消 / blur 提交 / click stopPropagation 防 switchSession）；非 edit 模式下渲染原 title + item_count；新增 ✏️ rename 按钮放 delete 按钮之前；rename 中隐藏 ✏️ + 删除（避免误操作）。
- 2026-05-09 06:18 — M3 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (500 modules, 945ms)。归档至 done。
