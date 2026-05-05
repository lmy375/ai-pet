# PanelTasks detail.md 编辑器 dirty marker（Iter R141）

> 对应需求（来自 docs/TODO.md）：
> PanelTasks detail.md 编辑器 dirty marker：现 textarea 改了不显视觉提示；R117 toggle row 内（字数 counter 旁）加 "● 未保存" muted 小点（仅 editingDetailContent !== original 时显），让用户区分"已保存当前显示" vs "未保存改动"。

## 目标

PanelTasks detail.md textarea 编辑器 cancel armed 路径（line 663）已用
`editingDetailContent !== editingDetailOriginalRef.current` 判定 dirty；
但视觉上没显示，用户长 session 编辑后想"我改过吗"得回想或试 cancel 看
是否触发 armed。加视觉 dirty marker 在 toggle row 内：

- 未改动 → 不显
- 已改 → "● 未保存" muted 小点

R117 toggle row 已是 edit/preview/字数 三件容器；加 marker 紧贴字数前。

## 非目标

- 不动 cancel armed 二次确认逻辑（既有路径）
- 不在 textarea 边框 / 标题加视觉变化 —— 仅 row 内小 hint，不抢主体
- 不引入"自动保存" / "草稿恢复" —— 离题

## 设计

### 内联 dirty 计算

`editingDetailOriginalRef` 是 useRef（stable），`editingDetailContent`
是 state（每改一次 React 重渲染）。inline 计算：

```ts
const isDirty = editingDetailContent !== editingDetailOriginalRef.current;
```

放在 IIFE / map 内（taskCard 内的 detail 编辑分支）。读 ref.current 在
render 期间是 OK 的（不变更）。

### 渲染

字数 counter span 之前插：

```diff
+{isDirty && (
+  <span
+    style={{
+      fontSize: 10,
+      color: "var(--pet-color-muted)",
+      fontFamily: "'SF Mono', 'Menlo', monospace",
+    }}
+    title="textarea 内容已改但未保存（⌘S 保存 / Esc 取消触发 dirty 二次确认）"
+  >
+    ● 未保存
+  </span>
+)}
 <span
   style={{
     marginLeft: "auto",
     fontSize: 10,
     ...
   }}
 >
   {editingDetailContent.length} 字
 </span>
```

由于字数 counter 用 `marginLeft: auto` 推右，dirty marker 只要放在 counter
之前就会被推到字数左侧（gap 4 给视觉分隔）。

### 测试

无单测；手测：
- 进入 detail 编辑 → dirty marker 不显（content === original）
- 键入字 → marker 显 "● 未保存"
- 删回原状 → marker 消失（content === original）
- 保存成功后 → original 被 ref 更新，marker 消失
- 切到 preview 模式 → state 不变 → marker 仍按 isDirty 显隐（preview 模式
  下 textarea 不显，但 dirty marker 在 toggle row 仍显，让用户清楚有改动）

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | inline isDirty 判定 + 渲染 marker |
| **M2** | tsc + build |

## 复用清单

- 既有 `editingDetailOriginalRef` / `editingDetailContent`
- R117 toggle row 容器
- R121 字数 counter 风格

## 进度日志

- 2026-05-10 22:00 — 创建本文档；准备 M1。
- 2026-05-10 22:08 — M1 完成。toggle row 内字数 counter 之前条件渲染 dirty marker (`editingDetailContent !== editingDetailOriginalRef.current`) "● 未保存"；marginLeft 处理：dirty marker 用 marginLeft auto 推右、字数 counter 不再用 auto 改条件 undefined / auto；保证两块都贴右且 dirty 在前 / 字数在后。
- 2026-05-10 22:11 — M2 完成。`pnpm tsc --noEmit` 0 错误。归档至 done。
