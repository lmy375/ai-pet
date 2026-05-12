# marks modal entry "🗑" 移除标记按钮

## 需求

iter #225 的 marks modal 只允许跳源 + 复制，没行内移除标记入口 —— 用
户想取消某条标记得先跳到 chat → 找到 📌 按钮再点。给每条 entry 加
🗑 按钮一键移除。

## 实现

`src/components/panel/PanelChat.tsx` modal entry meta 行末尾插入 🗑 按
钮：

- 行首 flex spacer (`<span style={{flex: 1}} />`) 把 🗑 推到右端
- onClick：
  - `ev.stopPropagation()` 阻止行级 jump handler 触发
  - `toggleMessageMark(${sessionId}::${itemIdx})` —— 调既有 helper 移除
  - `setMarksModalEntries(prev => prev?.filter(x => not match))` 同步从
    当前 modal 列表去掉，立即反映视觉
- 视觉：小 ghost 按钮（与 modal 内其它按钮风格统一）

## 验证

- `npx tsc --noEmit`：clean
- 行为：
  - modal 内每条 entry 末尾显 🗑 按钮
  - 点击 🗑 → 立即从列表消失（toggleMessageMark + entries 数组双向同步）
  - localStorage 同步更新（toggleMessageMark 内部已 writeText）
  - 不触发行级 jump
  - 全部移除后 modal 显空 / 关闭按钮仍可点

## 不在本轮范围

- 没做"undo 取消标记" 5s 浮窗：移除是用户主动动作，少误操作；想恢
  复手动重新点 📌
- 没做"批量勾选 + 批量取消"：当前 < 30 条规模逐条点足够；多选需复杂
  UI
- 没做"取消标记后跳到下一条"键盘 nav：modal 无 keyboard nav 接口

## TODO 池剩余

- PanelChat 自定义模板 "🛠 管理" modal
- PanelMemory butler_tasks "📋 复制完整 prefix + topic"
