# 任务详情阅读态字数 counter

## 需求

detail.md 编辑态（R121）已有字数 counter，阅读态没有。用户复盘历史长任务时
想知道笔记规模 → 切到编辑态看一眼再切回 → 多两次 click。直接在阅读态显示
是举手之劳。

## 实现

`src/components/panel/PanelTasks.tsx` 阅读态 header 行末加一个 `<span>`：

- 内容 `${count} 字`，count 用 `Array.from(detail.detail_md).length` 按
  Unicode code point 计（与编辑态 R121 同算法 —— 中文 / emoji 不会按 UTF-16
  surrogate pair 多算一遍）
- 阈值：`> 2000 字` 走红 tint（`var(--pet-tint-red-fg)`），其它走 muted；
  tooltip 解释红色阈值的语义"建议精简"
- 仅 detail.detail_md 非空时渲染（空 detail 显 counter 没意义）
- `marginLeft: "auto"` 推到 header 末尾，与"复制 / 编辑"按钮拉开间距

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 阅读态 detail 不空 → header 末尾显 `500 字`（muted）
  - 内容超 2000 字 → 红字"2500 字" + tooltip"建议精简"
  - 切到编辑态 → 阅读态 counter 消失（条件 `editingDetailTitle !== t.title`
    不再为 true）；编辑态自己的 counter 继续工作
  - 空 detail → 不渲染 counter
  - 中文 / emoji 正确计 1 字 / 字符（Array.from 按 code point 分）

## 不在本轮范围

- 没做 word count（按空格分词的英文 word 计）—— 当前阅读对象多为中文笔记，
  字符数更直观；要 word 维度等英文用户反馈再加
- 没改编辑态 counter 阈值（仍是 2000 字红字提醒）—— 已经对齐
