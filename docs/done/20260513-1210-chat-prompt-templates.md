# PanelChat 输入框 "📋 模板" 下拉

## 需求

iter #209 给 PanelTasks 加了创建任务的模板下拉。PanelChat 输入框对
应有同款需求 —— 让首次用户 / 不知道怎么提问的用户能选一个常见 prompt
框架直接 prefill。

## 实现

`src/components/panel/PanelChat.tsx`：

- 模块级新 `CHAT_PROMPT_TEMPLATES` 4 条：
  - 🪞 复盘：3 点结构化反思 prompt
  - ❓ 提问：入门到要点 prompt
  - 📝 写笔记：让宠物 consolidate 到 memory
  - 🛠 派任务：让宠物推荐任务结构
- input bar 在 📎 文件按钮前插入 `<select>`：
  - 仅 `input.length === 0` 时浮（已敲字时藏起，避免误触清掉用户输入）
  - value="" placeholder + reset 让重选同条可用
  - onChange → setInput(tpl.text) + setTimeout 0 focus 让用户立即编辑
    占位 `[...]` 字段
  - disabled isLoading 期间，与其它输入按钮一致
- 视觉与 📎 / send 按钮高度对齐（padding 10px 6px / radius 10px /
  card bg / muted fg）

## 验证

- `npx tsc --noEmit`：clean
- 行为：
  - chat input 空 → 浮 "📋 模板…" 下拉
  - 选 "🪞 复盘" → input 立即填多行 prompt + focus textarea
  - 用户改 [事项] 占位再敲事 → 发送
  - input 已敲字 → 下拉自动隐藏，不打扰
  - 清空 input（Esc / Send / 删除） → 下拉再次浮出

## 不在本轮范围

- 没让模板可配（localStorage 自定义列表）：4 条覆盖最常见 chat 场景；
  可配化需要 UI / 校验 / 导入导出
- 没做模板里支持 `[占位]` 一键 highlight：当前 setInput 直接填 + 让
  用户手敲修改；用户能自己看占位 brackets
- 没集成 task templates / detail.md 模板：那些是不同 surface 不同
  scope
- 没做"最近使用模板上浮"：4 条少，扫读 fine

## TODO 池剩余

空。下一轮需自主提需求。
