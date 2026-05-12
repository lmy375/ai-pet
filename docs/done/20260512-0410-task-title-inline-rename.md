# PanelTasks task title 双击 inline 编辑

## 需求

改 task 标题当前没有直接入口：要么改 memory yaml 里的 description 第一
行（手抖打错就废）、要么 edit detail.md（detail.md 第一行不是 title 的
真值源）。其实 title 是 butler_tasks memory item 的索引 key，应该有专门
的 rename 路径。加 task card title 双击 inline 编辑。

## 实现

### 后端 `src-tauri/src/commands/memory.rs`

- 新 `memory_rename(category, old_title, new_title)` Tauri 命令：
  - trim 空 / 同名 noop
  - 拦 `ai_insights/current_mood`（迁出 memory 索引后没有真实条目）
  - 同 category 内 new_title 重名 → 拒（避免 detail.md 覆盖）
  - 复用 `title_to_filename` 算新 detail_path，碰撞时加 `_N` 后缀
  - `fs::rename` 移动旧文件；不存在则 `fs::write` 起空 detail
  - 更新 item.title / detail_path / updated_at + `write_index`
- 注册到 `lib.rs`

### 前端 `src/components/panel/PanelTasks.tsx`

- 新 state：
  - `renamingTaskTitle: string | null`（同时只允许一条 task 改名）
  - `renameTaskDraft: string`
  - `renamingTaskBusy: boolean`（防双 commit）
- `commitRenameTask` / `cancelRenameTask` callbacks：放在 reload 之后避免
  TS2448（reload 在 useCallback 依赖里要先定义）。失败把 `actionErr` 写
  上让用户看到原因（如重名）
- 渲染分支替换 `<HighlightedText text={t.title} ...>`：
  - renamingTaskTitle === t.title：输入框 autoFocus + 1px accent 边
    - Enter 提交 / Esc 取消 / Blur 也提交
    - input click + mousedown stopPropagation 防触发行展开 / drag
  - 否则：包一层 span，双击进入改名态 + cursor: text + tooltip "双击改名"

## 验证

- `cargo check` + `npx tsc --noEmit` clean
- 行为：
  - task 卡 title 双击 → 变 input；当前 title 已填入 input
  - 改完 Enter → 后端 fs::rename detail.md 到新名 + 索引更新 + 前端 reload
    → 卡里 title 立即变新名
  - 空 / 同名 Enter → 静默关 input（noop）
  - 重名（同 category 已存在）→ `actionErr` 显 "Title already exists" 红字
  - Esc → 取消，input 关，title 保持原值
  - 改名期间 busy=true，input disabled，防重复 click
  - 同时只一条 input（点别的 task 双击会先 commit 当前的，然后切新行 —
    onBlur 同步提交）

## 不在本轮范围

- 没在 PanelMemory 加同款（已留作下一轮 TODO）：memory_rename 已通用，
  PanelMemory 复用是单独的小任务
- 没动 task description / body：title 是单点；body 在折叠 detail 区里另
  开 editor
- 没限制改名重复触发 IPC（race）：busy 锁已覆盖；onBlur + Enter 同发时
  setRenamingTaskBusy 守门

## TODO 池

清空后按规则 #1 自主提出 5 条新需求。

## TODO 池新提案

1. PanelMemory 单条 memory title 双击改名（复用 memory_rename）
2. ChatMini 桌面气泡显 pendingImages 缩略图条
3. PanelChat session bar token badge 点击 → 压缩历史
4. PanelDebug LLM 日志单条复制为 cURL
5. PanelTasks 批量重试所有 error 任务
