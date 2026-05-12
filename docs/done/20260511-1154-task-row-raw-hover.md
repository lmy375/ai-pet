# PanelTasks 任务行 hover 显完整 raw_description

## 需求

任务行只显 body（已剥 `[task pri=...]` / `[origin:...]` / `[result:...]` 等
marker 的清洁版本）。LLM 加的状态标记 `[done]` / `[error: foo]` / `[cancelled: bar]`
都要展开详情才看得到。Hover tooltip 显原始 description，让定位 marker 不必
点开。

## 实现

### 后端

`src-tauri/src/task_queue.rs`：

- `TaskView` 加 `raw_description: String` 字段（紧接 body 之后）
- 字段含全部 marker（与 TaskDetail.raw_description 同语义；但通过 task list
  接口预先附带，不需另发 invoke）
- 测试 helper builder 同步补 `raw_description: String::new()`

`src-tauri/src/commands/task.rs build_task_view`：

- 已经在算 raw（line `let raw = item.description.as_str()`），构造时加
  `raw_description: raw.to_string()`

### 前端

`src/components/panel/PanelTasks.tsx`：

- TaskView 接口加同名字段（JSDoc 说明）
- Task row 内层 header div 的 `title` attribute 从单一"点击展开"改成：
  ```
  ${点击 hint}\n\n原始 description：\n${raw 截 400 字符}
  ```
  保留点击 hint（用户最需要的提示），追加 raw 让懂 marker 的用户能定位状态
- 400 字符上限避免长描述把 OS tooltip 撑爆（OS 自身也会截断）

## 验证

- `npx tsc --noEmit` clean
- `cargo check` clean
- 行为：
  - hover 任务行 header → 浮 native tooltip：第一段"点击展开任务详情"+ 第二
    段"原始 description：[task pri=3 due=...] 整理桌面 [done] #weekly"
  - 一目了然 status marker 不必展开 detail tab
  - raw 极长（>400 字符）→ 截断 + `…`
  - 已展开行的 hint 改成"点击折叠详情"，raw 仍跟在后面

## 不在本轮范围

- 没在 raw 段做语法着色 / marker 高亮 —— native tooltip 不支持富文本；要那种
  得自实现 popover 组件，超出本轮成本
- 没新加 IPC 命令拉单条 raw —— 把字段加进 TaskView 一次性传，比延迟 fetch
  简单；50 个任务 × 几百字 raw ≈ 25KB，IPC 一次往返可接受
