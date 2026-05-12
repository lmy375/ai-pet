# PanelChat 自定义模板 "🛠 管理" modal

## 需求

iter #248 加了 "💾 保存为模板" 让用户存自定义模板（cap 10 FIFO）。
但没"删除 / 重命名"入口 —— 想清理只能去 localStorage devtools 改 JSON。
补管理 modal。

## 实现

`src/components/panel/PanelChat.tsx`：

- 新 helper `persistCustomTemplates(arr)` —— 全替换 + 写盘
- 新 state `manageTemplatesOpen: boolean`
- 在 "📋 模板…" dropdown 旁加 "🛠" 按钮：
  - 仅 `input.length === 0 && customChatTemplates.length > 0` 时浮
    （input 非空时藏起，与 dropdown 同 gate）
  - 点击 → setManageTemplatesOpen(true)
- modal：520 宽 / 70vh 高，列表 + 关闭按钮
  - 每条 entry：label（粗体 ellipsis）+ ✏️ rename + 🗑 delete + 内容
    3-line clamp 预览
  - ✏️ → window.prompt 输新 label；同名覆盖
  - 🗑 → 立即从 array 去掉 + 写盘
  - backdrop / ✕ 关 modal

## 验证

- `npx tsc --noEmit`：clean
- 行为：
  - 无自定义模板 → 🛠 按钮不浮
  - 攒 3 条后 → 🛠 显，点开 modal 列 3 条
  - 点 ✏️ → prompt 输新 label → 提交后改名生效
  - 点 🗑 → 立即从列表消失 + localStorage 同步
  - 重命名为已存在的 label → 去重（保留新值）
  - 关闭 modal → 状态保留（再开还在）

## 不在本轮范围

- 没做"恢复内置模板"：内置 4 条在源码常量，不能被用户删；管理 modal
  仅自定义
- 没做"复制内置模板做副本":  内置存在 + 自定义存在两套是分组的，复
  制后用户在自定义里改不会影响内置
- 没做拖拽重排序：append-only 顺序对用户够直观；想置顶可删后重存
- 没做"导出 / 导入模板集"：localStorage 已经持久化跨重启；外部备份
  可手抓 localStorage

## TODO 池剩余

空。下一轮需自主提需求。
