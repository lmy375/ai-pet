# LlmLogView 增量加载

## 需求

调试窗口 LLM 日志 tab 每 2s 重抓 last 200 条。长跑后日志文件 5000+ 行，
JSON 解析 200 条 × 每 2s 一遍仍然让面板卡。改"初始 50 条 + 按需加载更早
50 条"模式。

## 实现

`src/components/panel/LlmLogView.tsx`：

- 新常量 `LIMIT_INITIAL = 50` / `LIMIT_STEP = 50`
- 新 state：
  - `limit: number`（默认 50）—— 当前拉取窗口大小
  - `limitRef: useRef<number>` —— 给 2s polling 拿最新 limit 而不串依赖
  - `atFileStart: boolean` —— 后端返 `lines.length < limit` 即已到日志文件
    起点；按钮 disable
  - `loadingMore: boolean` —— 防双击
- `fetchLogs(overrideLimit?)`：
  - 默认走 `limitRef.current`，让 polling 自动用最新 limit
  - "加载更早" handler 调用时显式传 `nextLimit` —— 因为 setState 调度 →
    effect 同步 ref 之间有一帧空窗，传值避开 race
  - 把 `atFileStart` 算成 `lines.length < curLimit`
- 既有 polling effect 不变（仍 2s 抓一次，但底层 limit 已可变）
- `handleLoadMore`：limit += 50 + 立刻 fetchLogs(nextLimit)
- toolbar 右侧 counter 改成 "<entries> / 窗口 <limit>"，到底加 "· 已到底"
- 底部加按钮：`{atFileStart ? "· 已加载日志文件起点 ·" : "加载更早 50 条"}`

## 为什么不做后端 offset / cursor

后端 `get_llm_logs(limit)` 是从 EOF 倒读 last N；改成 `offset` 需要 seek
+ 行号定位，磁盘读取本来就是顺序 lines.collect()，加 offset 还是要 scan
all。前端把 limit 累加就行，与后端语义自然对齐（且老 log 不会变，limit
扩大永远拿到稳定前缀的 last N）。

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 入 LLM 日志 tab → 显最近 50 条，toolbar "50 / 窗口 50"
  - 滚到底 → 出现"加载更早 50 条"按钮
  - 点 → 显最近 100 条，toolbar "100 / 窗口 100"
  - 重复点 → 150 / 200 / …
  - 日志文件总共只有 87 条 → 第一次扩到 100 时后端返 87，
    `atFileStart = true`，按钮换成"· 已加载日志文件起点 ·"
  - 期间 LLM 调用产生新条 → 2s polling 自动用当前 limit 重抓 →
    最新条出现在顶（list 已 reverse）
  - 双击 / 快速连点不让 limit 跳 N 次：loadingMore disable + 单步顺序

## 不在本轮范围

- 没改"刷新"按钮：它仍走当前 limit fetchLogs；功能与 polling 重复但保留
  让用户有显式控件
- 没做"过滤 by model / round / tool"客户端筛选：增量加载已减轻 JSON 解
  析压力；筛选交互留给后续单独需求（不必混在本轮里）
- 没改 polling 间隔：2s 仍合理；后续若发现大 limit + 高频写入仍卡可改
  到长 polling + 增量 diff（要后端配合）

## TODO 池剩余

- /image 历史 prompt 菜单显缩略图
- PanelChat assistant 消息三键 reaction
