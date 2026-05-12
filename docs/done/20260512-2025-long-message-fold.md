# PanelChat 长消息中段折叠

## 需求

LLM 偶尔输出超长 reply（代码生成 / 全文翻译 / 总结报告等）—— 长几千
字会一气推屏，挤掉对话历史。让 > 1000 字的消息默认显头 500 + 中段
省略 + 尾 300，并提供"展开全部"按钮。"头 + 尾"的折叠模式让用户既看
到开场也看到结论，比纯首段截断的信息密度高。

## 实现

`src/components/panel/panelChatBits.tsx` `CopyableMessage`：

- 新 local state `middleExpanded: boolean`（每条 message 独立 state；
  跨重启 / 切 session 自然回到折叠基线）
- 阈值常量：`LONG_LIMIT=1000` / `HEAD_KEEP=500` / `TAIL_KEEP=300`
- 折叠条件：`isLong && !middleExpanded && !highlightKeyword` —— 搜
  索模式（keyword 高亮）禁用折叠，否则命中段落落在中段会被藏，违反
  搜索语义
- `renderableContent`：折叠时拼 `${head}\n\n…〔折叠中段 N 字 · 点下
  方「展开全部」〕…\n\n${tail}`；展开 / 短消息时原 content
- 把渲染路径里的 content 全替换成 renderableContent —— keyword 高
  亮 / task ref / parseUrls 三条路径都基于 string，无侵入
- 折叠 / 展开 toggle 按钮：
  - 仅 `isLong && !highlightKeyword` 时浮（避免搜索 mode 误显）
  - 文案三态："↕ 展开全部 (N 字)" / "↕ 折回中段 (N 字)"
  - 视觉：accent 色 border + 浅 transparent 底，alignSelf flex-start
    紧贴 bubble 左下，不撑满气泡宽度
  - title tooltip 解释折掉了多少字、按下来展示全部多少字

## 验证

- `npx tsc --noEmit`：clean
- 行为：
  - 800 字消息 → 全显，无按钮
  - 1500 字消息 → 显前 500 + `…〔折叠中段 700 字 · 点下方「展开全部」〕…`
    + 末 300 + "↕ 展开全部 (1500 字)" 按钮
  - 点击 → 全文显 + 按钮变 "↕ 折回中段 (1500 字)"
  - 跨会话搜索 → 命中长消息，高亮模式 enable 时强制展开（不折叠）—
    避免命中在中段被藏
  - 多条长消息独立 state，互不影响
  - 切 session / 重启 panel → 全部回到折叠态
  - task ref token / URL parsing / keyword 高亮三条渲染路径在折叠态
    下也正常工作（renderableContent 是普通 string）

## 不在本轮范围

- 没把阈值（1000 / 500 / 300）做成可配：根据 panel 宽度 + 用户偏好
  可以再调；当前是经验值
- 没做"渐变 fade 出"效果（CSS mask）：当前用文字标记 `…〔折叠中段 N
  字〕…`，比 CSS fade 更明确告诉用户折掉了多少
- 没做键盘快捷展开 / 折回：tabIndex 在 chat 滚动里相对噪声；按钮鼠
  标点击成本已极低
- 没做"按段落折叠"（保留前后各 N 段而非按字符）：段落识别 unreliable
  （LLM 输出可能没换行）；字符截断更稳
- 没扩到桌面气泡 / mini chat：那两个 surface 都是即时弹窗，长 reply
  少见且本就 maxHeight 限制；scope 集中在主 PanelChat 历史

## TODO 池剩余

空。下一轮需自主提需求。
