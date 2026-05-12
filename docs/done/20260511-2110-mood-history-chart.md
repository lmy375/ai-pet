# MoodWidget 心情历史 mini chart

## 需求

桌面 MoodWidget 显当前心情字段（glyph + 短文本），但用户感受不到"心情变
化曲线"——只能在 panel 里看 persona 历史。在 widget 上 hover 时浮出最近 6
条采样的 emoji 串，让"心情走势"一眼可读。

## 实现

`src/App.tsx`：

- 新类型 `MoodSnapshot { glyph, text, motion, ts }` + helper
  `moodSnapshotKey` 给去重比较 + `formatMoodElapsed` 把 ms 差值格式化为
  "Xs 前 / X 分前 / X 小时前"。
- `MoodWidget` 内：
  - `history: MoodSnapshot[]` ring buffer，cap = `MOOD_HISTORY_MAX = 6`
  - 既有 5s polling 内除了 `setMood(m)`，也尝试 push history：
    - 与上一条同 `motion+text` 不入（去重，避免无聊重复）
    - 满 6 条 slice 丢最早
  - 新 `historyVisible` state：onMouseEnter/Leave 切换
  - 新 `nowMs` 10s tick state：让 hover tooltip 里"X 分前"自然刷新
  - 渲染：当前气泡上方浮一行 emoji（仅 hover + past.length>0 时出），
    `opacity: 0.5 + 0.5 * (i+1)/length` 越靠右越亮，与"时间轴自左向右流
    动"直觉一致；每个 emoji hover 显采样时间 + 文本
  - past = history.slice(0, -1)：去掉最末（与当前等价）让 chart 真显"过
    去的脸色"
  - 主气泡 title 改成多行（含 hover hint 提醒小 chart 存在）

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 桌面跑一会儿，pet 心情变过几次 → hover 主 mood 气泡 → 上方浮 1-5 个
    emoji 圆点，最右亮、最左淡
  - 每个 emoji hover → tooltip "3 分前：开心地等着主人摸摸（Tap）" 之类
  - 同一心情连续轮询 → 仍只占一槽（key 去重）
  - 历史 ≤ 1 条 → 不渲染 chart（避免一格无意义）
  - 关 pet 窗口后再开 → history 清零（按 session-only 设计；持久化无价值，
    用户回桌面看的是"最近的脉络"，跨重启的脉络不直观）

## 不在本轮范围

- 没存到 localStorage / 后端：mood 已有 ai_insights 长期画像，这条是 UI
  "最近脉络"，session-only 即可
- 没做趋势 / 情绪正负曲线：emoji 串 + tooltip 已经够"有感觉"；做正/负
  评分要 LLM 调 score，超出"小卡"范围
- 没把 chart 加到 panel persona 页：那里已有 mood 历史完整列表（详情视图）；
  widget 是"快速看一眼"

## TODO 池剩余

- PanelDebug LLM 日志增量加载
- /image 历史 prompt 菜单显缩略图
- PanelChat assistant 三键 reaction
- PanelTasks tag 颜色自定义
