# PanelChat 折叠中段 ellipsis hover tooltip 显首末预览

## 需求

iter #193/#205 让长消息折叠中段并允许点击展开。但 hover ellipsis 文
本只显"点此展开剩余 N 字"这条 generic 提示，看不到折掉的内容大致是
什么。补 hover tooltip 显中段首 20 字 + … + 末 20 字 preview，让用
户判断"是否值得展开"。

## 实现

`src/components/panel/panelChatBits.tsx` 把 ellipsis span 的 `title`
attribute 从 generic 字符串换成 IIFE：

- 抽取中段：`content.slice(HEAD_KEEP, content.length - TAIL_KEEP)`
- 换行换成 `⏎` 让 tooltip 单行紧凑（多行 title 在某些平台显示不一致）
- 段长 ≤ 40 字 → 显完整
- 段长 > 40 字 → 显 `首 20 字 … 末 20 字`
- 整体格式：
  ```
  折叠中段 N 字

  中段首末预览：
  {preview}

  点此展开剩余字数（也可用下方按钮）
  ```

## 验证

- `npx tsc --noEmit`：clean
- 行为：
  - 长消息 1500 字（中段 700 字）→ hover ellipsis → tooltip 显"折叠
    中段 700 字 / 中段首末预览 / 点此展开"
  - 中段含换行 → tooltip 渲 ⏎ 标识
  - 中段 < 40 字 → tooltip 显完整中段
  - 点 ellipsis 展开后再 hover → 不再触发（fold-middle 路径不渲染）
  - 搜索 keyword 高亮模式 → 不走 fold（既有），无 tooltip 变化

## 不在本轮范围

- 没把 preview 渲染成 rich tooltip（custom popover）：native title
  够轻，跨平台稳；rich popover 增加 state / outside-click 复杂度
- 没让用户配 preview 长度（默认 20 字首 + 20 字末）：经验值适中，
  配置 UI 不值
- 没在 tooltip 内显 markers / urls 高亮：纯文字描述够；rich 渲染走
  展开后看
- 没改下方"展开全部"button 的 tooltip：那条是"折回 vs 展开"提示，
  preview 在 ellipsis 上够了

## TODO 池剩余

- PanelChat marks modal "📋 全部复制" 按钮
- PanelMemory 顶部 export 单 category 下拉
- PanelTasks priority badge 色阶渐变
- PanelDebug "立即开口" 加 "✏️ 编辑临时 prompt"
