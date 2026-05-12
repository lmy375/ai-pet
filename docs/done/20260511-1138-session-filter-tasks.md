# PanelChat 历史会话过滤"含任务派单"

## 需求

📷 含图片 filter 上一轮做完后，工作场景里的另一类常见诉求是"我那次让宠物帮
我建过任务的对话"。补一个 📋 含派单 filter，按 items 是否有 `propose_task` /
`task_create` 工具调用筛。

## 设计：互斥两 toggle

两个 filter 同时启用没意义（取交集语义模糊），UI 改成互斥 enum：

```ts
type SessionFilter = null | "images" | "tasks";
```

同一 chip 再点 → 关；点另一 chip → 切，自动 clear filterSessionIds + 重 fetch。

## 实现

### 后端

`src-tauri/src/commands/session.rs` 新加 `list_sessions_with_task_calls()`：

```rust
const TASK_TOOL_NAMES: &[&str] = &["propose_task", "task_create"];
// 扫 items[].toolCalls[].name 是否含上述任一
```

`lib.rs` 注册。

### 前端

`src/components/panel/PanelChat.tsx`：

- 替换原单 `imageSessionIds + toggleImageFilter` 为统一三态：
  - `sessionFilter: null | "images" | "tasks"`
  - `filterSessionIds: Set<string> | null`
  - `filterLoading: boolean`
- 抽 `toggleSessionFilter(kind)` callback：与当前相同 → 关；否则切 + fetch
- dropdown 顶部 chip row 用 `.map([images, tasks])` 渲两个 toggle，共享 active /
  loading / count 样式
- 空过滤态 message 按当前 sessionFilter 显对应 emoji"点 📷 / 📋 关闭过滤"

filter 用 `array.filter(s => filterSessionIds.has(s.id))` 在 reverse + pinned/unpinned
分组之前，行为与原 image 实现一致。

## 验证

- `npx tsc --noEmit` clean
- `cargo check` clean
- 行为：
  - 点 📋 含派单 → 加载几十 ms → 列表只剩含 propose_task / task_create 的 session
  - 切到 📷 含图片 → 自动 fetch image 集替换；filterSessionIds reset 防 stale
  - 同 chip 再点 → 关，全列表回来
  - 命中 0 时显单独 empty message 带对应 emoji
  - 同时显图片 + 派单 chip 互斥，鼠标点哪个就只看哪类

## 不在本轮范围

- 不做"图片 + 派单"交集 / 并集：两类 session 重合度低，互斥按"看哪类"二选一
  更符合直觉；并集"工作场景 ∪ 图片场景"语义模糊
- 不缓存：与 images 过滤同决策（lazy fetch 简单可靠；50 session 用 < 100ms）
- 不联动 save_session 重新算：用户切换 chip 已经触发 refresh；不要再 emit
  /listen 让 UI 自己刷新（频率不可控）

## TODO 池剩余

- 设置页 config 导出 / 导入
- PanelTasks 任务行 hover raw_description
