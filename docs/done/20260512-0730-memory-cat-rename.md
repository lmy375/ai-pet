# PanelMemory 类目显示名重命名

## 需求

memory 类目 key 来自后端写死的 CATEGORY_ORDER（butler_tasks / todo /
ai_insights / general / user_profile / task_archive），cat.label 也由后端
固定。用户想把"butler_tasks"显示为"宠物排班"、"ai_insights"显示为"内
心独白"等个性化中文 / 表达不到位的小调整，没有入口。加双击 section 标题
inline 改名，仅前端展示层覆盖，不动后端 key。

## 实现

`src/components/panel/PanelMemory.tsx`：

- 新 state `categoryLabels: Record<catKey, customLabel>`，从 localStorage
  `pet-memory-cat-labels` 懒加载（与 pinnedKeys / expandedCategories 同模式）
- helper `setCategoryLabel(catKey, label)`：trim 空 → 删 key 退回后端默认；
  非空 → 写 map + localStorage
- 新 state `renamingCatKey / renameCatDraft`（同时只一个 section 可编辑）
- section title 渲染包装：
  - 编辑态：autofocus input，Enter / Blur 保存 + 关闭，Esc 取消，placeholder
    显后端默认（让用户对"留空 = 默认"有预期）
  - 非编辑：`<span onDoubleClick>` 包 `categoryLabels[catKey] || cat.label`，
    cursor:text + tooltip 解释"仅本机生效；空 = 用后端默认"

## 设计选择

- 仅 UI 层覆盖：不调 memory_rename（那是单 item 改名）。Memory category key
  是后端 LLM-side 写入 path（butler_tasks 等是 LLM 已知的字符串）；如果
  rename 后端 key，LLM prompt / 工具调用全部断
- localStorage per-machine：与"不做云同步"一致。用户跨设备会回到后端默认
- 空字符串等同 reset：UI 简单，不必单独"恢复默认"按钮

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 双击"butler_tasks (宠物任务)"标题 → 变 input，placeholder 显原 label
  - 输入"宠物排班"→ Enter → section 标题更新为"宠物排班"；其它 UI（导出
    MD / search 结果 / chip 等）仍按后端 cat.label 行为，因为本轮只动 section
    header 渲染
  - 双击新 label → 清空 → Blur → 退回后端默认"宠物任务"
  - 重启 panel → 自定义 label 保留
  - 后端 catKey 不变 → LLM 调 memory_edit 用 butler_tasks 仍生效

## 不在本轮范围

- 没扩到 search 结果 / 导出 MD 路径：那两处仍走 cat.label。一致性可作单
  独需求，统一引用 `displayLabel(catKey)` helper
- 没做"自定义 emoji prefix"（如 🐾 宠物排班）：用户在 customLabel 字符串
  里自带即可，不必新加 emoji 输入控件
- 没限制类目重命名给 advanced 用户专属：双击改名门槛足够，普通用户不会
  误触

## TODO 池剩余

- PanelChat compose 拖入 .md / .txt 自动塞 textarea
- ChatMini assistant bubble 单条"再回应"快捷
- PanelTasks "now" 标记 + 桌面 nudge
- PanelDebug 快照对比 diff
