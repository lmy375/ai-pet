# PanelTasks detail.md 字数 counter（Iter R121）

> 对应需求（来自 docs/TODO.md）：
> PanelTasks detail.md 字数 counter：detail 编辑器无 maxLength 上限；编辑模式时（textarea 或 preview 切换 row 旁）显 "X 字" muted 小字，纯计数让用户感知笔记体量，配合 R109 history fold / R117 preview mode 形成"detail 信息密度感"。

## 目标

PanelTasks detail.md 编辑器没 maxLength 限制，长跑任务的 detail 可累积
几千字。R109 给 history timeline 折叠了，R117 加了 preview 切换；但仍
缺一个"当前笔记多少字"实时感知。

加纯计数（无阈值警示）：在 R117 编辑/预览 toggle row 末尾附 "X 字"
muted 小字。让用户写长 detail 时随手知道体量；与 R113 / R119 字数 counter
风格一致但语义不同（detail 无上限，仅信息感知）。

## 非目标

- 不限制 maxLength —— detail 是 markdown 笔记，长度受用户场景驱动，前端
  不该卡
- 不分行数 / 段数 —— 单一总字数足够；细分增加噪音
- 不附 amber / red 警示色 —— 无上限阈值，纯信息

## 设计

### 渲染

R117 加的 toggle row 现状：

```tsx
<div style={{ display: "flex", gap: 4 }}>
  {[...].map((mode) => <button ... />)}
</div>
```

加 counter 在 row 末尾用 `marginLeft: auto`：

```diff
 <div style={{ display: "flex", gap: 4, alignItems: "center" }}>
   {[...].map((mode) => <button ... />)}
+  <span
+    style={{
+      marginLeft: "auto",
+      fontSize: 10,
+      color: "var(--pet-color-muted)",
+      fontFamily: "'SF Mono', 'Menlo', monospace",
+    }}
+    title="当前笔记字符数（Unicode code units 计；含换行 / 空白）"
+  >
+    {editingDetailContent.length} 字
+  </span>
 </div>
```

`alignItems: "center"` 让按钮（高 ~22px）与小字（高 ~12px）垂直居中。

### 测试

无单测；手测：
- 进入编辑：toggle row 末显 "0 字"（默认 detail 空）/ 实际长度
- 输入更多 → 实时跟进
- 切到 preview → counter 仍显（content 共享）
- 切回 edit → counter 不变
- 保存 detail → state 重置为空 / 退出 edit 模式 → counter 自然不显（外
  层条件 `editingDetailTitle === t.title` 不满足）

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | toggle row 加 counter span + alignItems center |
| **M2** | tsc + build |

## 复用清单

- 既有 R117 toggle row 容器
- 既有 editingDetailContent state
- R113 / R119 muted counter 视觉风格

## 进度日志

- 2026-05-10 02:00 — 创建本文档；准备 M1。
- 2026-05-10 02:08 — M1 完成。R117 toggle row container 加 alignItems: center；row 末追加 `<span>` 显示 `editingDetailContent.length 字`，marginLeft: auto 推到右端，muted fontSize 10 monospace；title hover 解释"含换行 / 空白"语义。
- 2026-05-10 02:11 — M2 完成。`pnpm tsc --noEmit` 0 错误。归档至 done。
