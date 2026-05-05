# PanelChat 单会话导出 markdown（Iter R106）

> 对应需求（来自 docs/TODO.md）：
> PanelChat 单会话导出 markdown：dropdown row 加"📋 导出"图标，把当前 session 的全部 user/assistant 消息按时间序拼成 .md 复制到剪贴板，配合 R98 PanelMemory 全部记忆导出形成"双端导出"模式。

## 目标

PanelChat 现在能搜索 / 切换 / 重命名 / 删除会话，但缺一个"会话导出"动
作。用户做以下场景时只能逐条复制：
- 把一段重要对话存到日志 / Notion
- 跨 LLM 移植对话作为 few-shot
- 备份关键技术问答

加 markdown 导出：dropdown row 末尾「✏️」「删除」之间加「📋」按钮，点击
把该会话的全部 chat items（user / assistant，跳过 system）按时间序拼成
markdown 复制到剪贴板。

## 非目标

- 不导出 tool calls / error 行 —— 噪音多，对"对话内容"复盘价值低；保留
  user / assistant 两类即可
- 不写文件 —— 与 R98 同语义，剪贴板覆盖 95% 场景
- 不在顶 bar 加导出（占主区视觉空间）—— dropdown 是会话管理入口
- 不导出 system prompt（messages[0]）—— 用户不关心，markdown 简洁优先

## 设计

### markdown 格式

```markdown
# {session.title}
> 导出时间: 2026-05-09 10:30 · 共 N 条消息

## 🧑 user
你最初问的问题...

## 🐾 assistant
我的回答 1...

## 🧑 user
追问...

## 🐾 assistant
回答 2...
```

要点：
- H1 = session title
- H2 = role 角色（与 SearchResultRow line 1140 的 🧑/🐾 emoji 一致）
- content 原样（保留换行；markdown render 时自然 wrap）
- 跳过 type="tool" / "error" / 其它非 user/assistant 行

### Helper

```ts
function exportSessionAsMarkdown(
  title: string,
  items: ChatItem[],
): string {
  const lines: string[] = [];
  const visibleItems = items.filter(
    (it) => it.type === "user" || it.type === "assistant",
  );
  lines.push(`# ${title}`);
  lines.push(
    `> 导出时间: ${new Date().toLocaleString()} · 共 ${visibleItems.length} 条消息`,
  );
  lines.push("");
  for (const it of visibleItems) {
    const glyph = it.type === "user" ? "🧑" : "🐾";
    lines.push(`## ${glyph} ${it.type}`);
    lines.push("");
    lines.push(it.content);
    lines.push("");
  }
  return lines.join("\n");
}
```

### Handler

dropdown 处理时需要 load 该 session 的完整 items（当前 PanelChat 只在
`items` state 里存当前打开会话的数据；其它 session 要点导出按钮时得
load_session）：

```ts
const handleExportSession = async (id: string, fallbackTitle: string) => {
  try {
    const session = await invoke<Session>("load_session", { id });
    const md = exportSessionAsMarkdown(session.title || fallbackTitle, session.items);
    await navigator.clipboard.writeText(md);
    // 临时反馈：reuse copyMsg 通道（PanelChat 已有）
    setCopyMsg(`已导出 "${session.title}" 到剪贴板`);
    setTimeout(() => setCopyMsg(""), 3000);
  } catch (e) {
    console.error("Failed to export session:", e);
    setCopyMsg(`导出失败: ${e}`);
    setTimeout(() => setCopyMsg(""), 3000);
  }
};
```

`copyMsg` 是 PanelChat 既有的浅短反馈通道（顶 bar）—— reuse 而不引入新
state。3000ms 自清空与现有 1500ms 略长（导出成功用户多会主动切窗口去
paste，多 1.5s 让消息可见时间充足）。

### 渲染

dropdown row 在 ✏️ rename 之前插 📋 export：

```diff
+{renamingId !== s.id && (
+  <button
+    type="button"
+    onClick={(e) => {
+      e.stopPropagation();
+      void handleExportSession(s.id, s.title);
+    }}
+    style={{
+      padding: "2px 6px",
+      borderRadius: "4px",
+      border: "none",
+      background: "transparent",
+      color: "var(--pet-color-muted)",
+      fontSize: "12px",
+      cursor: "pointer",
+    }}
+    title="把会话全部 user / assistant 消息复制为 markdown 到剪贴板"
+    aria-label="export session"
+  >
+    📋
+  </button>
+)}
 {renamingId !== s.id && (
   <button ...rename ✏️...>
 )}
```

按钮顺序：📋 export → ✏️ rename → 删除（按"非破坏 → 中等 → 破坏"风险递增
排）。

### 测试

无单测；手测：
- 点击当前 session 的 📋 → 剪贴板含 markdown，顶 bar 显短暂反馈
- 点击其它 session 的 📋 → 同样 load + 复制（不切换会话）
- 空会话（item_count = 0）→ 导出后 markdown 仅有 H1 + 摘要
- 拖一段长会话（10+ 消息）→ markdown 顺序与渲染顺序一致

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | helper + handler |
| **M2** | dropdown row 加 📋 按钮 |
| **M3** | tsc + build |

## 复用清单

- 既有 `load_session` Tauri 命令
- 既有 `copyMsg` state
- 既有 dropdown row 按钮模式（与 ✏️ rename 一致风格）

## 进度日志

- 2026-05-09 11:00 — 创建本文档；准备 M1。
- 2026-05-09 11:08 — M1 完成。`exportSessionAsMarkdown(title, items)` helper 加在 Styles section 上方：H1 + 摘要 → 过滤 user/assistant 两类 → H2 emoji + content。`handleExportSession(id, fallbackTitle)` 调 load_session → 拼 markdown → 写剪贴板。
- 2026-05-09 11:14 — M2 完成。dropdown row 在 ✏️ rename 之前插 📋 export 按钮（顺序：non-destructive → 中等 → destructive 风险递增）。
- 2026-05-09 11:18 — 发现 PanelChat 没 `copyMsg` 全局 state（只有 per-row `copiedIdx`）—— 计划里假设错了。新加 `exportToast` state + 3s 自清空 timer + 在 dropdown 顶部 banner 渲染 accent 色文案，专门做 session 导出反馈。
- 2026-05-09 11:21 — M3 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (500 modules, 944ms)。归档至 done。
