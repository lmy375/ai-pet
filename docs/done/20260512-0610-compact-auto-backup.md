# 压缩历史前自动备份 session

## 需求

上一轮的 token badge 点击压缩历史是不可撤销操作（session 文件原地覆盖）。
就算 popover 底部提醒"先 📦 导出"，用户还得手动多走一步且容易忽略。给压
缩链路自动加一层备份保险：trim 前先 invoke `export_sessions_snapshot`
把所有 session base64 payload 写到剪贴板，反悔时直接走"📥 导入快照"恢复。

## 实现

`src/components/panel/PanelChat.tsx`：

- `compactHistory` 内 trim 之前插一段 backup 调用：
  - `invoke<string>("export_sessions_snapshot")` 拿全量 snapshot 字符串
  - `navigator.clipboard.writeText(payload)`
  - 用 `backupOk` / `backupErr` 两个本地变量记录结果
- 即使 backup 失败（剪贴板权限被拒 / IPC 异常）也继续 trim —— trim 是用
  户主诉，备份是保险层，不应让保险失败阻塞主路径
- 后续 toast 拼出来时带"压缩前快照已复制到剪贴板"或"⚠ 备份失败：..."注脚，
  toast 时长从 4s 改 6s 让用户能读完两段
- popover 底部说明文案改成"💾 压缩前会自动把所有 session 备份到剪贴板"

## 为什么 export_sessions_snapshot 而非只备 current session

- 后端 import_sessions_snapshot 接口接收的是全量索引 + sessions 数组；没
  现成"单 session 导入"路径
- 用户对"恢复"的预期是"操作前的快照"——多 session 一起备能 cover "我
  也想恢复其它 session 的 pinned / metadata"边角
- 复制到剪贴板的 base64 字符串约 几 KB 到几十 KB，落地体积无忧

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 点 token badge → 浮窗弹出，底部说明显"自动备份到剪贴板"
  - 选"保留近 1/2"→ trim 完成；toast 显 "已压缩 N 条 · 压缩前快照已复制
    到剪贴板（想反悔点 📥 导入快照）"6s
  - 剪贴板里粘到任意 editor → 是 base64 字符串
  - 想反悔 → session 下拉的"📥 导入快照"按钮 → 自动从剪贴板读 → 恢复全
    部 session 到压缩前
  - 模拟剪贴板失败（私密浏览 / 权限拒）→ toast 显 "⚠ 备份失败：..."；trim
    仍然完成

## 不在本轮范围

- 没做"内置 undo stack 自动跨进程持久化"：剪贴板已是简单可控备份点；built-in
  undo 涉及版本管理 / 存储路径，工程量大很多
- 没改 import_sessions_snapshot 让它只挑当前 session 恢复：那要扩接口
  形态，与本轮"加保险"范围正交

## TODO 池剩余

- ChatMini ⌘L 聚焦输入框
- PanelTasks ⌘N 全屏 quick-add 模态
- PanelMemory 类目折叠状态 localStorage 持久化
- PanelDebug stats 一键导出 markdown
