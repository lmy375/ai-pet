# PanelTasks detail.md 编辑器 Esc 取消（Iter R138）

> 对应需求（来自 docs/TODO.md）：
> PanelTasks detail.md 编辑器 Esc 取消：现 detail 编辑必须点"取消"按钮（dirty 时走 cancelEditArmed 二次确认）；textarea 内按 Esc → 触发同 cancel 路径（与 R110 PanelMemory modal Esc / R127 chat input Esc 习惯一致）。

## 目标

PanelTasks 详情面板的 detail.md textarea 编辑器现支持 ⌘S 保存；但取消必
须鼠标点"取消"按钮 / 在长 textarea 中找按钮。R110 / R127 已建立 Esc 取消
modal / 输入习惯；本轮镜像。

按 Esc 直接调既有 `handleCancelEditDetail`，让 dirty 时走 armed 二次确认
路径（保护用户已编辑的内容不被一键丢）。

## 非目标

- 不绕过 dirty 守卫 —— `handleCancelEditDetail` 内部已 dirty + 第 1 次 Esc
  → armed，第 2 次 Esc → 真退；R138 复用，不直接强制丢编辑
- 不动 detailPreviewMode（preview 时 textarea 不渲染，Esc 行为由 preview
  div 决定 —— 默认浏览器行为，不拦截）
- 不挂全局 Esc —— 仅 textarea 内捕获，避免与其它 panel Esc 冲突

## 设计

### 加 Esc 分支到 textarea onKeyDown

现 onKeyDown 仅处理 ⌘S 保存。加 Esc 分支：

```diff
 onKeyDown={(e) => {
   if (
     (e.metaKey || e.ctrlKey) &&
     e.key.toLowerCase() === "s"
   ) {
     e.preventDefault();
     if (savingDetail) return;
     handleSaveDetail(t.title);
+    return;
+  }
+  if (e.key === "Escape") {
+    e.preventDefault();
+    handleCancelEditDetail();
   }
 }}
```

`handleCancelEditDetail` 内部已包含 dirty 二次确认 + 实际清理逻辑，Esc
路径直接复用。

### placeholder 文案补提示

```diff
-placeholder="在这里追加 / 修改进度笔记…保存后覆盖 detail.md。（⌘S 保存）"
+placeholder="在这里追加 / 修改进度笔记…保存后覆盖 detail.md。（⌘S 保存 / Esc 取消）"
```

让用户首次 focus textarea 时看到完整快捷键提示。

### 测试

无单测；手测：
- 编辑 textarea 没改动 → Esc → 直接退出编辑
- 编辑 textarea 改了内容 → Esc → cancelEditArmed=true（"取消"按钮变红警示）
- 3s 内再 Esc → 真退编辑，丢改动
- 3s 后再 Esc → 重新 armed
- 与 ⌘S 不冲突（不同 keys）

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | onKeyDown 加 Esc 分支 + placeholder 补提示 |
| **M2** | tsc + build |

## 复用清单

- 既有 `handleCancelEditDetail` + dirty armed 守卫
- R110 / R127 Esc 习惯

## 进度日志

- 2026-05-10 19:00 — 创建本文档；准备 M1。
- 2026-05-10 19:08 — M1 完成。textarea onKeyDown 现 ⌘S 分支末加 return；新插 Escape 分支 preventDefault + handleCancelEditDetail（既有 dirty armed 二次确认逻辑直接复用，不需重复实现）。placeholder 文案补 "/ Esc 取消"。
- 2026-05-10 19:11 — M2 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (500 modules, 977ms)。归档至 done。
