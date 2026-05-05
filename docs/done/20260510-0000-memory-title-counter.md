# PanelMemory 标题 input 字数 counter（Iter R119）

> 对应需求（来自 docs/TODO.md）：
> PanelMemory 标题 input 字数 counter：title 现 maxLength=20 但无实时提示；title input 之后加 "X / 20" muted 小字（与 R113 描述 counter 同模式），>= 90% 转 amber，让用户提前感知截断点。

## 目标

R113 给描述 textarea 加了字数计数器（300 上限）。标题 input 同样有
`maxLength={20}` 上限但没实时提示，用户输入到 20 字时会突然被浏览器拒绝
继续输入，体验"被掐断"。

加同款 counter：title input 之后小字 "X / 20"，三档颜色（< 90% muted /
90-99% amber / 100% red）。

## 非目标

- 不在 update 模式显（title input 是 `disabled={!editingItem.isNew}` —
  更新已有 entry 时 title 不可改，counter 没意义）
- 不改 maxLength=20 —— 后端约定的 entry-key 长度
- 不与 R112 trim 守卫合并 —— counter 显当前实际 input 长度（含空白），
  trim 是 save 时再处理；两件事

## 设计

### 渲染

label / input 之间不插；counter 放在 input 之后（与 R113 描述 counter 同
布局）：

```diff
 <input
   style={s.input}
   maxLength={20}
   value={editingItem.title}
   onChange={(e) => setEditingItem({ ...editingItem, title: e.target.value })}
   disabled={!editingItem.isNew}
 />
+{editingItem.isNew && (() => {
+  const len = editingItem.title.length;
+  const MAX = 20;
+  const WARN = 18; // 90%
+  const color =
+    len >= MAX ? "#dc2626"
+    : len >= WARN ? "#a16207"
+    : "var(--pet-color-muted)";
+  const tip =
+    len >= MAX ? "已达 maxLength=20；继续输入会被浏览器拒绝"
+    : len >= WARN ? "接近 20 字上限"
+    : "标题长度限制 20 字";
+  return (
+    <div
+      style={{ fontSize: 10, textAlign: "right", color, marginTop: 2 }}
+      title={tip}
+    >
+      {len} / {MAX}
+    </div>
+  );
+})()}
```

`editingItem.isNew` 守卫：edit 模式 input disabled，不显 counter（避免误
导用户"还能改"）。

### WARN 阈值

20 × 0.9 = 18，与 R113（300 × 0.9 = 270）等比；让两个 counter 行为一致。

### 测试

无单测；手测：
- 新建 entry，title 空 → "0 / 20" muted
- 输 10 字 → "10 / 20" muted
- 输 18 字 → "18 / 20" amber
- 输 20 字 → "20 / 20" red；继续按键被浏览器拒
- 编辑已有 entry → title input disabled，counter 不显

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | counter IIFE + 三档颜色 + isNew 守卫 |
| **M2** | tsc + build |

## 复用清单

- R113 描述 counter 同款逻辑
- 既有 amber / red motion 色

## 进度日志

- 2026-05-10 00:00 — 创建本文档；准备 M1。
- 2026-05-10 00:08 — M1 完成。title input 之后追加 `editingItem.isNew && (() => ...)` IIFE，MAX=20 / WARN=18 三档颜色（< 90% muted / 90-99% amber / 100% red）；right-aligned fontSize 10，title hover 解释；isNew 守卫让 update 模式（input disabled）不显 counter。
- 2026-05-10 00:11 — M2 完成。`pnpm tsc --noEmit` 0 错误。归档至 done。
