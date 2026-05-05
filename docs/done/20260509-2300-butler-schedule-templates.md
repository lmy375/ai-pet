# PanelMemory butler_tasks 模板按钮（Iter R118）

> 对应需求（来自 docs/TODO.md）：
> PanelMemory butler_tasks 模板按钮：编辑 modal 选中 butler_tasks category 时，description 上方显 `[every: HH:MM]` / `[once: YYYY-MM-DD HH:MM]` / `[deadline: ...]` 三按钮，点击在光标位置插入模板字符串 + focus textarea，新用户写 schedule 不再要记忆语法。

## 目标

butler_tasks category 的 schedule 语法（`[every: HH:MM]` / `[once: ...]` /
`[deadline: ...]`）目前只在 placeholder 给范例。但用户实际新建任务时，
要么记不全语法、要么手敲容易写错（冒号 / 空格 / 日期格式）。

加 3 个模板按钮在 description textarea 上方，仅 category=butler_tasks 时
显示。点击 → 在光标位置插入模板字符串 → focus textarea 让用户继续填具
体值。

## 非目标

- 不强制用户用模板 —— 不带 schedule 前缀的描述也合法（pet 自己判断时机）
- 不做日期 picker 弹窗 —— 模板是字面量字符串，用户填具体值就行；picker
  增加复杂度
- 不验证 schedule 语法 —— 后端 `parseButlerSchedule` 会做实际解析；前端
  只插模板即可
- 不动 input/select fields；title disabled in edit 模式不影响

## 设计

### template 字符串

参考 `CATEGORY_PLACEHOLDERS.butler_tasks`（line 31）的范例：

```ts
const SCHEDULE_TEMPLATES = [
  { label: "🔁 every", text: "[every: 09:00] " },
  { label: "📅 once", text: "[once: 2026-05-10 14:00] " },
  { label: "⏳ deadline", text: "[deadline: 2026-05-10 14:00] " },
];
```

text 末尾保留一个空格让用户直接写正文不需先打 space。emoji 与 task 标
签 chip 颜色（R80）一致：每日 = 🔁 / 一次 = 📅 / 截止 = ⏳。

### insertion 逻辑

textarea 用 ref 拿光标位置（selectionStart / selectionEnd），slice 拼接
后用 `setEditingItem` 更新 description；focus 后把光标移到插入末尾（ts +
template.length）。

```ts
const descTextareaRef = useRef<HTMLTextAreaElement>(null);

const insertTemplate = (template: string) => {
  if (!editingItem) return;
  const ta = descTextareaRef.current;
  const cur = editingItem.description;
  let next: string;
  let newCursor: number;
  if (ta) {
    const start = ta.selectionStart ?? cur.length;
    const end = ta.selectionEnd ?? cur.length;
    next = cur.slice(0, start) + template + cur.slice(end);
    newCursor = start + template.length;
  } else {
    next = cur + template;
    newCursor = next.length;
  }
  setEditingItem({ ...editingItem, description: next });
  // 等 React commit 后把光标移到插入末尾
  setTimeout(() => {
    const t = descTextareaRef.current;
    if (t) {
      t.focus();
      t.setSelectionRange(newCursor, newCursor);
    }
  }, 0);
};
```

### 渲染

description label 与 textarea 之间加按钮行（仅 butler_tasks 显）：

```diff
 <label style={{ fontSize: 12, color: "var(--pet-color-muted)" }}>描述</label>
+{editingItem.category === "butler_tasks" && (
+  <div style={{ display: "flex", gap: 4, marginTop: 4, marginBottom: 4 }}>
+    {SCHEDULE_TEMPLATES.map(({ label, text }) => (
+      <button
+        key={text}
+        type="button"
+        onClick={() => insertTemplate(text)}
+        title={`在光标位置插入 \`${text.trim()}\` 模板（butler_tasks schedule 语法）`}
+        style={{
+          padding: "2px 8px",
+          fontSize: 11,
+          border: "1px solid var(--pet-color-border)",
+          borderRadius: 4,
+          background: "var(--pet-color-card)",
+          color: "var(--pet-color-fg)",
+          cursor: "pointer",
+          fontFamily: "inherit",
+        }}
+      >
+        {label}
+      </button>
+    ))}
+  </div>
+)}
 <textarea
   ...
+  ref={descTextareaRef}
 />
```

按钮风格：低对比度 / 中性色（与决策日志 chip / dueFilter chip 不混淆，
按钮性质是"辅助插入"非"过滤"或"动作"）。

### 测试

无单测；手测：
- 切到 butler_tasks → 三按钮显
- 切到 todo / general → 按钮不显
- 在 textarea 任意位置点 🔁 every → 该位置插入 "[every: 09:00] "，光标
  落在末尾
- 已有 description "整理 Downloads"，光标在开头点 📅 once → 变 "[once: ...] 整理 Downloads"
- 选中一段文字点按钮 → 选中段被替换（slice end 取 selectionEnd）

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | template const + ref + insertTemplate handler |
| **M2** | 按钮 row + textarea ref 挂上 |
| **M3** | tsc + build |

## 复用清单

- 既有 `editingItem` state + setEditingItem
- 既有 textarea + maxLength=300 限制（插入超限时浏览器自动 reject）
- R80 schedule chip 配色 emoji 习惯

## 进度日志

- 2026-05-09 23:00 — 创建本文档；准备 M1。
- 2026-05-09 23:08 — M1 完成。`useRef` 加到 import；`SCHEDULE_TEMPLATES` const 在文件顶（紧挨 CATEGORY_PLACEHOLDERS 上方）；`descTextareaRef` state 加在 expandedCategories 旁；`insertTemplate` handler 在 handleExportAll 上方：读 textarea selectionStart/End → slice 拼接 → setEditingItem 更新 description → setTimeout 0 等 React commit 后 focus + setSelectionRange 把光标移到插入末尾。
- 2026-05-09 23:11 — M2 完成。description label 之后、textarea 之前加按钮 row（仅 category=butler_tasks 显）；3 个 schedule template 按钮（🔁 every / 📅 once / ⏳ deadline），中性色 button 视觉；textarea 加 ref={descTextareaRef}。
- 2026-05-09 23:14 — M3 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (500 modules, 1.02s)。归档至 done。
