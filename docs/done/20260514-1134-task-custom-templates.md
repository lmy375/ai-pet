# PanelTasks 任务模板个性化（自定义保存 / 删除）

## 背景

TODO：

> 任务模板个性化：让用户在 PanelTasks 模板下拉里保存/删除自己的 prefill 范例（localStorage 持久），不再仅靠 4 条内置。

「📋 从模板」下拉之前只有 4 条 hard-coded 范例（整理 Downloads / 总结一段文档 / 调研某主题 / 翻译一段文字）。这些是 onboarding 引导样本，但每个用户的实际高频任务形态不一样 —— 写代码党会反复用「写一段单元测试」、运营党会反复用「整理本周邮件」。让用户自存自删，下拉就从"内置 onboarding"变成"个人快捷"。

## 改动

### `src/components/panel/PanelTasks.tsx`

**1. 数据层 & 持久化（模块顶）**

```ts
interface TaskTemplate { label: string; title: string; body: string; }

const CUSTOM_TEMPLATES_LS_KEY = "pet-task-templates-custom";
const CUSTOM_TEMPLATES_MAX = 20;       // 防 localStorage 无界增长
const CUSTOM_TEMPLATE_LABEL_MAX = 20;  // 与 task title max 对齐

function loadCustomTemplates(): TaskTemplate[] { /* shape guard */ }
function saveCustomTemplates(list: TaskTemplate[]): void { /* try/catch */ }

// 既有 TASK_TEMPLATES rename → TASK_TEMPLATES_BUILTIN（语义对偶 custom）
const TASK_TEMPLATES_BUILTIN: TaskTemplate[] = [ /* 4 条不动 */ ];
```

`loadCustomTemplates` 每条做 shape guard（label/title/body 全是非空 string）—— hand-edit localStorage 或老版本字段漂移时不至于让组件崩。`saveCustomTemplates` 失败静默吞（localStorage 满 / 禁用都不阻塞表单交互）。

**2. 组件内状态**

```ts
const [customTemplates, setCustomTemplates] = useState<TaskTemplate[]>(() =>
  loadCustomTemplates(),
);
useEffect(() => { saveCustomTemplates(customTemplates); }, [customTemplates]);
const [templatesManagerOpen, setTemplatesManagerOpen] = useState(false);

const allTemplates = useMemo(
  () => [...TASK_TEMPLATES_BUILTIN, ...customTemplates],
  [customTemplates],
);
```

`useMemo(allTemplates)` 让 customTemplates 不变时数组身份稳定 —— dropdown 不会每次 PanelTasks render 都 remount option。

**3. Save / Delete handlers**

```ts
const saveCurrentAsTemplate = () => {
  const t = title.trim(); const b = body.trim();
  if (!t) { setErrMsg("先填标题再存模板。"); return; }
  if (customTemplates.length >= CUSTOM_TEMPLATES_MAX) {
    setErrMsg(`自定义模板上限 ${CUSTOM_TEMPLATES_MAX}…`); return;
  }
  const proposed = window.prompt(`命名这个模板（≤ ${CUSTOM_TEMPLATE_LABEL_MAX} 字）`, t.slice(0, CUSTOM_TEMPLATE_LABEL_MAX));
  if (proposed === null) return;                  // 用户取消
  const label = proposed.trim();
  if (!label) { setErrMsg("模板名不能为空。"); return; }
  if (label.length > CUSTOM_TEMPLATE_LABEL_MAX) { setErrMsg(`≤ ${CUSTOM_TEMPLATE_LABEL_MAX} 字。`); return; }
  if (allTemplates.some(c => c.label === label)) { setErrMsg(`「${label}」已存在。`); return; }
  setCustomTemplates(prev => [...prev, { label, title: t, body: b }]);
  setErrMsg("");
};

const deleteCustomTemplate = (label: string) => {
  setCustomTemplates(prev => prev.filter(c => c.label !== label));
};
```

`window.prompt` 是 native 输入控件 —— 与 due preset chips / schedule prefix 等其它 native 控件同级简朴，不必引入额外的"重命名 modal"层。验证错误复用既有 `errMsg` 红字浮提（在「创建任务」按钮下方）。

**4. UI：dropdown + 两个新按钮**

- dropdown option 用 `<optgroup label="内置范例">` / `<optgroup label="我存的">` 分两组，用户视觉上一眼分辨。
- 紧挨 dropdown 加「💾 存为」按钮（title 空时 disabled 灰底），点击触发 saveCurrentAsTemplate。
- customTemplates.length > 0 时多出「管理 N」按钮，点击打开 templatesManagerOpen Modal。length === 0 时入口根本不渲染（empty modal 无意义）。

两处 dropdown（inline create form + quickAdd modal）都已更新到分组 optgroup 渲染。「💾 存为」/「管理」按钮只加到 inline 表单 —— quickAdd 是 ⌘N 快速新建模式，节奏更轻量，不需要塞管理 UX。

**5. 管理 Modal**

复用既有 `Modal` 组件（已 import）。每条 entry 渲染：

- label（粗体）
- 「标题：xxx」一行单行截断 + title 属性给 tooltip 全文
- 「内容：xxx（换行替换空格 / 80 字截断）」一行单行截断
- 右侧「删除」按钮（红 tint border，无二次确认 —— 用户可随时再存一次，损失极低）

空态显："还没有自定义模板。填好新建任务的标题 / 内容后点「💾 存为」就能加一条。"

## 不做

- **不支持重命名**。删 + 重新存能覆盖；rename UI 要 inline 输入 + 唯一性校验，增量收益不匹配代价。
- **不支持排序拖拽**。dropdown 渲染顺序 = 添加顺序，"我存的" 组里新加在尾。20 条上限内手动找不费劲。
- **不导出 / 导入**。这是个人化数据，与 session snapshot（已有"📥 导入快照"路径）解耦；如未来用户跨设备转配需求强了再加。
- **不复用 saved templates 给"模板转换为内置"**。internal/external 边界清晰更易维护；用户感知"我存的 vs 别人写的"也更直接。
- **不写自动化测试**。前端无 vitest；helper 函数 `loadCustomTemplates` 走 shape guard 路径足够防御，运行时表现可手 verify。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.15s
- 既有 4 条内置模板渲染 / applyTaskTemplate 行为不变 —— `allTemplates` index 与 builtin-only 时一致（自定义索引 = builtin.length + j）。

## 后续

- 桌面 ChatPanel 的 prompt templates（`pet-chat-custom-templates`）已经在用同样模式 —— 后续可抽公共 hook `useCustomTemplates<T>(lsKey, builtin, validator)` 复用。
- 模板预览 hover：dropdown 选项右侧浮一个 tooltip 显 body 头 5 行，选前就能看到要 prefill 什么。
