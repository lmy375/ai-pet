# session import orphan 清理

## 需求

上一轮的 sessions snapshot import 覆盖 index + 写 snapshot 里的 session 文件，
但 disk 上不在 snapshot 里的旧 session.json 不动 → orphan 文件残留（index 不
显，下拉看不见，但占磁盘 + 隐患）。提供可选 prune 开关让用户主动清掉。

## 实现

### 后端

`src-tauri/src/commands/session.rs`：

- `import_sessions_snapshot(payload, prune_orphans: Option<bool>) -> Result<u32, String>`
  - prune_orphans=true 时遍历 sessions_dir，删 disk 上 *.json 不在 snapshot
    内的 session 文件 + 排除 index.json（刚被 write_index 覆写）
  - 返回 u32：实际删的条数，让前端反馈给用户
  - 失败的 rm 计入 console；不阻塞 import 主流程（snapshot 已写入，删几个
    旧文件失败不重要）

### 前端

`src/components/panel/PanelChat.tsx`：

- 新 state `pruneSessionsOnImport: boolean`
- handleImportSessionsSnapshot：invoke 时 `{ payload, pruneOrphans: pruneSessionsOnImport }`
  传给后端；返回的 u32 命名为 prunedCount；toast 加 `· 清理了 N 个 orphan 文件`
  片段（仅 prune 开启且 N>0 时）
- snapshot 工具栏在 导入快照 按钮旁加 `清 orphan` 复选框（label + checkbox 内
  联，与按钮同行），tooltip 解释行为：勾后导入时删 disk orphan；不勾则保留
  本地老 session 但 index 看不见

## 验证

- `npx tsc --noEmit` clean
- `cargo check` clean
- 行为：
  - 新机粘 snapshot 不勾"清 orphan" → 导入成功，老 session 文件残留（下拉不
    显，磁盘占着）
  - 勾"清 orphan" → 导入后 toast 显"已导入 N 个 session · 清理了 M 个 orphan
    文件"
  - 删 rm 失败的不阻塞主流程，console 可见

## 不在本轮范围

- 没做 dry-run 模式（"显将删 N 个" + 确认按钮）：armed 二次确认已经覆盖大风
  险；prune 是 explicit 勾选，用户自己负责
- 没做自动清理（不勾自动 prune）：保守路径，让用户决定。如未来用户反馈"我每次
  都勾"，再加 settings.preference

## TODO 池剩余

- ChatMini 桌面气泡可拖动
- /image 命令在桌面 ChatPanel 也生效
- PanelMemory 显存储占用
- 设置页 motion_mapping group datalist
