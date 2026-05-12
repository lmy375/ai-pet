# PanelChat 折叠中段标记可直接点击展开

## 需求

iter #193 把长 assistant 消息（> 1000 字）默认折叠中段，渲 head + 中
段 `…〔折叠中段 N 字 · 点下方「展开全部」〕…` + tail。展开按钮在
bubble 下方 —— 用户读完 head 后视线已经离开按钮位置，要回去找。直
接让中段标记本身可点更顺。

## 实现

`src/components/panel/panelChatBits.tsx` 重构 bubble 渲染：

- 抽 `renderSegment(text)` helper 复用既有 keyword / taskRefMap /
  parseUrls 三档分发，避免重复
- foldMiddle 路径分三段渲：
  - head segment：`renderSegment(content.slice(0, HEAD_KEEP))`
  - clickable `<span>` 中段标记：cursor pointer + accent 色 + dotted
    underline，onClick → setMiddleExpanded(true)
  - tail segment：`renderSegment(content.slice(-TAIL_KEEP))`
  - 段间塞 `{"\n\n"}` 文本节点保留 white-space: pre-wrap 原视觉
- 不 foldMiddle 路径退回单段 `renderSegment(content)`
- 下方"↕ 展开全部" 按钮保留 —— 两个入口都能展开，rightmost / 视野
  熟悉位置的用户都有路可走

## 验证

- `npx tsc --noEmit`：clean
- 行为：
  - 短消息 → 渲单段，无中段标记
  - 长消息折叠态 → head + 中段 dotted underline accent 文字 + tail
  - 点中段标记 → 立刻展开全文
  - 点下方按钮 → 同等效果
  - 展开后再点下方"折回中段" → 折回 + 标记重现
  - 搜索 keyword 高亮模式 → 不折叠，单段渲染
  - 任务 ref 渲染（dotted underline）与中段标记 underline 视觉冲突？
    两者颜色都是 accent，但中段独占一段（不会同行），与 ref 不会视
    觉混淆
  - 段间 `\n\n` 在 bubble 的 white-space: pre-wrap 下渲染成空行（保
    持与 iter #193 原视觉一致）

## 不在本轮范围

- 没把"展开全部"按钮删掉：用户期待"展开"按钮在 bubble 下方是 chat
  app 常态；同时中段标记 + 下方按钮双入口更稳
- 没做 hover 中段标记时高亮 head + tail（暗示"是这段被折叠"）：成
  本高，affordance 已经够
- 没改 user 消息的折叠行为（CopyableMessage 同 shape 但 user 消息少
  超 1000 字）：保持 user / assistant 渲染对称，需要时一起改

## TODO 池剩余

- PanelTasks 多选 bulk action 加 "🔗 拼为 ref 列表" 复制
- PanelDebug 加 "复制全部 stash + settings 为 issue 模板"
