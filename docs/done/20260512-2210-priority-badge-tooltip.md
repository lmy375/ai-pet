# PanelTasks priority badge tooltip 详细化

## 需求

任务卡上的 `P0..P9` priority badge 原 tooltip 只一行：`点击改 priority
（P0..P9，越小越重要）`。新用户首次见到 P0..P9 数字会猜不清方向 ——
"P0 还是 P9 最重要？"原文案虽含"越小越重要"，但混在指令式 hint 末尾
易被忽略。改为结构化多行，把数字含义单独拎出来摆明。

## 实现

`src/components/panel/PanelTasks.tsx` 单 tooltip 字符串替换：

```
点击改 priority（P0..P9）

数字含义：
  P0 = 最重要 / 紧急（队列优先做）
  P3 = 默认（无特别标注）
  P9 = 最不重要 / 长期 idea 抽屉

当前：P{t.priority}
```

三段结构：
- 第一行：行为提示（点击改 priority）
- 中段：数字 → 语义映射，三点锚定（最重 / 中位 / 最轻），用户在
  两端有参照就能推断中间值
- 末行：当前值，让用户在看 tooltip 时不必再看 badge 字面

`\n` 在 native `title` 属性里有跨浏览器渲染差异：mac WKWebView 支持
多行；其它平台可能合并空白。tooltip 是辅助信息，文本退化也不丢含
义，能接受。

## 验证

- `npx tsc --noEmit`：clean
- 行为：
  - hover priority badge → 多行 tooltip 显数字方向 + 当前值
  - 点击 → 打开 P0..P9 picker（原行为不变）
  - 全部 badge 变体（已 done / cancelled 等终态行的 badge 也同源
    style，tooltip 同样显）

## 不在本轮范围

- 没改 priority picker 子菜单里每个 P0..P9 option 的 tooltip（如
  hover P3 时显"中位 priority"）：picker 是临时弹层，每条 tooltip 体
  验割裂；本轮聚焦 badge 单一入口
- 没改 priority chip filter 行（chip-filter 也是 P0..P9）的 tooltip：
  chip filter 是已选 / 未选语义而非 "P0 是什么"，scope 不同
- 没把 priority semantics 写进 PanelSettings 帮助文档（panel 没此
  section）：单 tooltip 已覆盖最常见接触点

## TODO 池剩余

- PanelMemory 顶部搜索框加最近 5 个 keyword 历史下拉
- PanelChat ⌘K task picker 加 char-order 子序列 fuzzy 匹配
- PanelTasks 新建表单 title input 检测 schedule 前缀时 inline 提示
- PanelDebug 加 "重置 in-process stash" 按钮
