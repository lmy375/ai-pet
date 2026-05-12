# PanelChat "💾 保存为模板"

## 需求

iter #233 给 chat 加了 4 条内置 prompt 模板。但用户的常用 prompt 各
不相同，写过一个好的 prompt 后想保存复用。补 "💾 保存为模板" 按钮 +
localStorage 自定义模板。

## 实现

`src/components/panel/PanelChat.tsx`：

- 新 state `customChatTemplates: Array<{label, text}>`，localStorage key
  `pet-chat-custom-templates`，cap 10 FIFO + 同 label 替换
- `saveCustomTemplate(label, text)` helper
- "📋 模板…" 下拉重构：value 编码为 "B:i" / "C:i" 区分内置 / 自定义
  来源；用 `<optgroup label="内置">` + `<optgroup label="自定义">` 分组渲染（自定义空时不渲）
- 新 "💾" 按钮：
  - 仅 `input.trim().length > 0` 时浮（空 input 没东西可存）
  - onClick → `window.prompt` 让用户输 label（默认值用 input 首 12 字）
  - cancel / 空 label → 无副作用
  - 提交后调 saveCustomTemplate

## 验证

- `npx tsc --noEmit`：clean
- 行为：
  - 空 input → 仅显 "📋 模板…" 下拉，无 💾 按钮
  - 写过文字 → 💾 按钮浮出
  - 点 💾 → prompt 弹窗"给当前输入起个 label..."（默认值 = 首 12 字）
  - 输 "周回顾" → 保存到 localStorage + 下拉显"自定义 / 周回顾" entry
  - 清空 input 后再选下拉"周回顾" → input 回填刚保存的文本
  - 重启 panel → 自定义模板还原
  - 同 label 再保存 → 替换旧条
  - 攒到 11 条 → 最老一条 FIFO 挤出

## 不在本轮范围

- 没做"删除自定义模板"按钮：用户清 localStorage 即清；UI 删除按钮加
  在下拉行不可行（select option 不支持 inline 按钮），需要 popover
  管理，留 follow-up
- 没做"重命名 / 编辑模板"：cap 10 + replace pattern 已经覆盖；要 edit
  必须做 modal 重 UI
- 没把内置和自定义混排（按使用频次）：分组更清楚来源
- 没把保存路径升级成 modal（替代 window.prompt）：prompt 简洁；modal
  scope 翻倍 + Tauri WKWebView prompt 在 macOS 渲染良好

## TODO 池剩余

空。下一轮需自主提需求。
