# PanelTasks 任务卡 itemMeta 加 "更新于 X · Y 前 · N 次更新"

## 需求

任务卡 itemMeta 行已有"创建于 X · Y 前"对称展示。但用户回看长队列
想知"这条最近还活跃吗 / 多久没动了"—— 当前 created_at 只能告诉年龄，
不分活跃度。补 "更新于 X · Y 前" + 可选 "N 次更新"（基于 detailMap
缓存的 history 长度）。

## 实现

`src/components/panel/PanelTasks.tsx` 在 itemMeta 中"创建于"span 后
追加新 span：

- 仅 `t.updated_at && t.updated_at !== t.created_at` 时浮 —— 刚建未
  动过的任务跳过避免重复信号
- 显 `更新于 ${absolute} · ${relative}`，relative 用既有
  `formatRelativeAge` helper（minute / hour / day 三档）
- 末尾追加 `· N 次更新` 仅当 `detailMap[t.title]` 已加载（hover preview
  或 expand 触发后填充）—— 没加载就 graceful degrade 不显数字
- 视觉与"创建于"完全对称（同 itemMeta 配色 / flex 列）

## 验证

- `npx tsc --noEmit`：clean
- 行为：
  - 刚建任务 created_at === updated_at → 不显新 span（视觉简洁）
  - 任务被 pet 改过描述 → updated_at 推进 → 显"更新于 ... · 3 小时前"
  - hover 任务一次后 detailMap 填充 → 同 span 末尾追加 "· 5 次更新"
  - 任务被频繁更新（10+ 次）→ 数字很大也 fine（不截断 — itemMeta 已
    flex-wrap）
  - 同 row 内"创建于" + "更新于"两 span 并排，flex gap=10 自然分隔

## 不在本轮范围

- 没在卡片折叠状态主动 fetch task_get_detail 拿 history 数：每条加载
  会触发 N 次 IO，大队列下太重；现策略是"hover / expand 后才有数字"，
  没 hover 过的就只显时间
- 没做"未读更新数"（distinguish 用户上次看到的版本与最新）：那是另
  一个 axis（有部分实现，line ~4147 的橙色 dot 是同语义），不混
- 没把 history 数集成进 hover preview tooltip（那已经显 3 条具体事件）：
  数字+列表都显冗余；list 摘要内信号够了
- 没让 N 次更新 0 时显 "0 次更新"：0 = pet 没改过，与 updated_at 信号
  重复；省去

## TODO 池剩余

- PanelChat "查看全部标记消息" modal
