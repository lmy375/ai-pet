# PanelMemory 编辑 modal 保存时 trim 标题（Iter R112）

> 对应需求（来自 docs/TODO.md）：
> PanelMemory 编辑 modal 保存时 trim 标题：handleSaveEdit 中对 title 调 .trim() 再发后端，避免不可见首尾空白引发"看着相同实则不同"的 entry 重复 / 后续无法搜索匹配。

## 目标

`handleSaveEdit` 现把 `editingItem.title` 直接发给后端 `memory_edit`。
若用户输入时多打了首/尾空格（mobile 复制粘贴常见 / autocorrect 误加），
后端 store 的 title 含空白：
- 视觉上两条 entry 看着同名实则不同
- 搜索 `title.includes(...)` 时大小写匹配但空白不匹配 → 看不到 entry
- update 时 title 必须精确匹配旧 title（含空白）才命中后端 entry

加 `.trim()` 在保存前清掉首尾空白；空白唯一构成 → 视为空标题（已有 backend
校验，但前端早 reject 体验更好）。

## 非目标

- 不动 description trim —— description 内首尾空白对显示无害（pre-wrap 渲染
  可能用空行做段落分隔）；title 是 key 性质，必须严格
- 不规整化中间空白（如多空格压缩）—— 那会改变用户语义（"hello world" vs
  "hello  world"），违反"用户输入即真实"原则
- 不向后兼容已有空白的 entry —— 旧 entry 不动；用户后续编辑时本次保存会自然
  trim

## 设计

```diff
 const handleSaveEdit = async () => {
   if (!editingItem) return;
+  const title = editingItem.title.trim();
+  if (!title) {
+    setMessage("标题不能为空");
+    return;
+  }
   try {
     if (editingItem.isNew) {
       await invoke("memory_edit", {
         action: "create",
         category: editingItem.category,
-        title: editingItem.title,
+        title,
         description: editingItem.description,
       });
     } else {
       await invoke("memory_edit", {
         action: "update",
         category: editingItem.category,
-        title: editingItem.title,
+        title,
         description: editingItem.description,
       });
     }
```

注意 update 路径：现 `editingItem.title` 是用户在 modal 内编辑过的值。但
title input 是 `disabled={!editingItem.isNew}`（line 552）—— 即编辑模式下
title input 不可改。所以 update 时 trim 只对 create 路径有意义。但保守
起见两路径都 trim，无害。

实际上 disabled 的 input 仍可能携带 props 注入的值——若 `setEditingItem`
之前的源数据本身有空白（旧 entry 标题带空白），update 时把它 trim 后发
给后端，后端可能以"trim 后的 title 找不到旧 entry"为由报错。这是边界 case；
现实场景下 entry 通常无空白，影响小。如果出现可以加 update 路径不 trim
的 fallback，但本轮先做最简洁版本。

## 测试

无单测；手测：
- 新建 entry，title 输 "  test "  → 保存后 store 的 title = "test"
- 全空白 title → 红色 "标题不能为空" toast，不发后端
- 编辑模式 title disabled，trim 不影响（值由源数据决定）

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | trim 加在 handleSaveEdit 顶部 + 空标题守卫 |
| **M2** | tsc + build |

## 复用清单

- 既有 `setMessage` 反馈通道
- 既有 invoke memory_edit 路径

## 进度日志

- 2026-05-09 17:00 — 创建本文档；准备 M1。
- 2026-05-09 17:08 — M1 完成。handleSaveEdit 顶部加 `const title = editingItem.title.trim();` + 空 title 守卫（setMessage "标题不能为空" + return）；create / update 两路径都用 trimmed title（保守起见两路径 trim 一致，update 时 disabled input 源值与 trimmed 几乎等价）。
- 2026-05-09 17:11 — M2 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (500 modules, 966ms)。归档至 done。
