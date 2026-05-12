# PanelChat 全部 session 打包 snapshot

## 需求

config 快照已能搬 settings + SOUL；session 历史也得能搬。否则用户换机后聊天
历史 / 任务派单上下文全丢，宠物"重新认识你"的体感很重。

## 实现

### 后端

`src-tauri/src/commands/session.rs`：

- 新 struct `SessionsSnapshot { version, index, sessions }`：version 字段给
  schema 演进留口子；index 是 SessionIndex（含 pinned / item_count）；
  sessions 是 Vec<Session>（完整 message + items）
- `export_sessions_snapshot()`：遍历 index，逐个 load_session 收集 → 整体序
  列化 JSON → base64 → 返回
- `import_sessions_snapshot(payload)`：base64 → JSON → SessionsSnapshot →
  校验 version==1 → 每条 session 写文件 + write_index 覆盖

### 覆盖语义

Import 是 **覆盖 index + 写 snapshot 里的所有 session 文件**。本地 disk 上不
在 snapshot 里的旧 session 文件 → orphan（index 不显，dropdown 看不见）。这
个 trade-off 让 import 行为可预测："带过去什么就有什么"，不混杂本地遗存。
清理 orphan 留给后续（已加入 TODO 池）。

`lib.rs` 注册两个命令。

### 前端

`src/components/panel/PanelChat.tsx`：

- `handleExportSessionsSnapshot`：invoke → writeText 剪贴板 → exportToast 反
  馈"已导出 N 字符 · ⚠ 含全部聊天明文"
- `handleImportSessionsSnapshot`：armed 二次确认（5s 撤回），与 config import
  同模式。第一次点读剪贴板 + 提示再点确认；5s 内再点 → invoke → 刷
  sessionList。不自动切到某个 session（loadSession 在文件后段定义，且强切让
  用户失位 —— dropdown 会重渲，让用户自己点）
- session dropdown 顶部加一行工具栏，两个按钮：
  - `📦 导出全部 sessions`（普通灰）
  - `📥 导入快照` / armed 态变红 `⚠ 确认导入？`

## 验证

- `npx tsc --noEmit` clean
- `cargo check` clean
- 行为：
  - 老机点导出 → 剪贴板出 base64 + toast 含 ⚠ 安全提醒
  - 新机点导入 → 弹确认提示 → 5s 内再点 → toast"已导入 N 个 session" → 下拉
    显新会话列表 → 点任意一条切过去看到完整 messages + items
  - 失败（base64 / JSON / version 不匹配）→ 后端错误透传到 toast
  - 5s 不点确认 → armed 自动撤回，按钮回灰

## TODO 池清空 → 自主提案

按规则 #1 提 5 条新需求（已写 TODO.md）：

1. session import 后清理 orphan 文件
2. ChatMini 桌面气泡可拖动调整位置
3. /image 命令在桌面 ChatPanel 也生效
4. PanelMemory 显存储占用
5. 设置页 motion_mapping 显模型实际 group 名 datalist
