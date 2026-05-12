# PanelChat 搜索 hit 内文本高亮

## 需求

跨会话搜索点结果会 scrollIntoView + 给整行刷一层黄底背景（1.5s 退去），但
**bubble 内文本**没有标出"具体哪几个字命中"。用户得自己肉眼扫一遍 bubble 找
keyword 位置，对长 bubble 体验差。把 SearchResultRow 里用的同款 `<mark>` 黄
高亮搬进 bubble 里，让命中位置一目了然。

## 实现

### `src/components/panel/panelChatBits.tsx`

- 新加 `renderContentWithKeyword(content, keyword)`：case-insensitive 找所有
  keyword 命中段，命中段包 `<mark>`（与 SearchResultRow 同色：`#fef3c7` 背景
  + `#92400e` 字），其余段走 `parseUrls`（URL 蓝下划线仍生效）。
  - 空 keyword / 空内容 → fallback 到 `parseUrls(content)`，老行为不变。
- `CopyableMessage` props 加 `highlightKeyword?: string`。
- bubble 内的 `{content && parseUrls(content)}` 改成 `highlightKeyword ?
  renderContentWithKeyword(content, highlightKeyword) : parseUrls(content)`。
- `import { Fragment, useState } from "react"` —— mark 之间的 url 化片段用
  `<Fragment key=...>` 包裹避免 `Each child in a list should have a unique
  key` 警告。

### `src/components/panel/PanelChat.tsx`

- 新 state `searchHit: { idx, keyword } | null`：与既有 `highlightedItemIdx`
  分离，因为行级 background flash 1.5s 就退，而 keyword 高亮要存活更久（用
  户慢读时不消失）。
- `handleSelectSearchHit`：进入函数先 `const keyword = searchQuery.trim()`
  缓存（因为下一行会清空 searchQuery），await loadSession 后 `setSearchHit({
  idx, keyword })`。空 keyword → null。
- `items.map` 内每行算一次 `hitKeyword = searchHit && searchHit.idx === i ?
  searchHit.keyword : undefined`，传给 user / assistant 两条 CopyableMessage。

### 不清 searchHit 的边界讨论

切到不同 session、再 reload session、`/clear` 等都不主动清 searchHit。理由：
`renderContentWithKeyword` 对"keyword 在 content 中找不到"路径是 noop（直接
走 parseUrls），不产生视觉副作用。再点新 hit / 同一 hit 都会覆盖，所以也不
存在"过期高亮粘连"。这避免了 sessionId useEffect 与 setSearchHit 的 race
（useEffect 在 commit 后跑会 nuke 同 callback 后续的 setSearchHit）。

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 跨会话搜索 "hello" → 点结果 → 切到目标会话 → bubble 内每个 "hello" 段
    黄底深棕字（与搜索结果列表的高亮同色）
  - 行级 1.5s background flash 退去后，bubble 内 keyword 仍持续高亮
  - 再点别的 hit → 旧高亮清，新高亮加
  - 切到无关 session（dropdown 点别的）→ bubble 内 keyword 若在新 session
    的对应 idx 出现，可能短暂可见但 idx 通常不对，渲染为 noop（无副作用）

## 不在本轮范围

- 没做"高亮所有命中行的 keyword"（仅当前 hit）：跨 session 大面积高亮信息
  过载，等用户实际反馈 "我搜后想 review 全部命中" 再扩
- 没做"上 / 下一个命中 hop"按钮：跨 session 搜索结果列表已能直接点跳，少一
  个层级；同 session 搜索 + n / N 命中跳转可作为后续 enhancement

## TODO 池剩余

- PanelTasks 任务行右键菜单
