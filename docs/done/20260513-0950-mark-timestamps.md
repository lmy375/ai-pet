# PanelChat marks modal item 显标记时间

## 需求

iter #220 把消息标记存在 Set，无时间信息。modal 列表按 Set 插入序，
不直观哪条最近被标。补 timestamp，让 modal 按"刚刚 / N 分钟前 / N
小时前 / N 天前"显示，并以倒序（最新在前）排列。

## 实现

`src/components/panel/PanelChat.tsx`：

### 数据结构：Set → Map

- `markedMessages: Set<string>` → `Map<string, number>` (key→markedAt
  epoch ms)
- localStorage 读时兼容两种形态：
  - Array<string> → 老格式 ts 未知，转 Map ts=0
  - Record<key, number> → 新格式
- localStorage 写：`JSON.stringify(Object.fromEntries(map))`
- `.size` / `.has(key)` API 在 Map 和 Set 上同 shape，调用点无需改
- 唯一改动：`for (const k of markedMessages)` → `for (const [k, ts]
  of markedMessages)`（iteration yields tuple in Map）

### toggleMessageMark 写 ts

- toggle 时若新增 → set(key, Date.now())；删除照旧
- 写盘走 Record 形式

### Modal 改造

- `MarkedEntry` type 加 `markedAt: number`
- openMarksModal 收集 entry 时带 markedAt
- entries.sort(by markedAt desc) — 最新标记在前；老格式 ts=0 自然
  落底
- modal item meta 行加新 chip "📌 {rel}"（仅 ts > 0 显）：
  - < 60s "刚刚"
  - < 1h "N 分钟前"
  - < 1d "N 小时前"
  - else "N 天前"
  - hover title 显完整 toLocaleString

## 验证

- `npx tsc --noEmit`：clean
- 行为：
  - 旧 localStorage 数据（Array<string>）→ 第一次启动后兼容读 + 转 Map
  - 标记一条新消息 → modal 内显 "📌 刚刚"
  - 等几分钟再开 modal → 显 "📌 5 分钟前"
  - 旧标记（ts=0）→ 不显时间 chip，但仍可点跳转
  - 新格式写盘后再读 → 时间还原
  - 排序：最新标记在列表顶部
  - 关闭再开 modal → 重排（reload entries 时重 sort）

## 不在本轮范围

- 没做手动改 markedAt UI（"重新标记一次"刷新时间）：用户重新 toggle
  on/off 就行，timestamp 自然 refresh
- 没做"按时间过滤" filter（"最近 24h" / "本周"）：iter #227 加的 search
  按内容已经覆盖找特定标记；时间筛选场景边际
- 没做 toLocaleString 显原始日期作为主显示：相对时间更符合阅读习惯，
  绝对时间在 hover tooltip 兜底
- 没做老格式 ts=0 → 自动 backfill 当前时间：保留"时间未知"语义诚实
  比假数据更稳

## TODO 池剩余

- PanelMemory butler_tasks "✏️ 改 schedule" 一键按钮
- PanelDebug "上次 manual fire 历史 ring" 显近 5 条
