# marks modal entry hover 显完整内容

## 需求

iter #225 marks modal 用 4-line webkit-line-clamp 截断内容显示，长
消息看不全。用户想"扫一眼判断是否值得 jump"得点开跳过去。补 hover
tooltip 显完整文本绕过 clamp。

## 实现

`src/components/panel/PanelChat.tsx`：

- modal entry div 的 title 属性从单行 "跳到「title」#N" 改为多行：
  ```
  跳到「title」#N

  {完整 content}
  ```
- 空 content → "（空）" 占位
- WKWebView 原生支持多行 title 渲染；其它平台合并空白也 graceful

## 验证

- `npx tsc --noEmit`：clean
- 行为：
  - 长内容 entry hover → tooltip 显跳转提示 + 完整内容
  - 短内容 entry hover → tooltip 内容与 4-line 截断版本相同
  - 空内容 entry → tooltip 显 "（空）"
  - 点击仍正常 jump

## 不在本轮范围

- 没改 webkit-line-clamp 数（如改成 8 行）：截断更短让 modal 容更多
  entries 视觉；hover 已能看全
- 没做"点击展开 inline 显完整"（双击展开 vs 单击跳转的语义双层）：
  hover tooltip 已覆盖；inline 展开 + jump 两个动作冲突
- 没做 hover tooltip 内 markdown / ref token 渲染：title attr 是 plain
  text；要 rich preview 走 jump

## TODO 池剩余

- PanelTasks "导出 visible markdown" 加 include detail toggle
- PanelChat "💾 保存为模板" 按钮
- PanelMemory "📋 复制今日 todo"
- PanelTasks ctx menu 加 "设 due 为今日 18:00 / 明日 09:00" preset
