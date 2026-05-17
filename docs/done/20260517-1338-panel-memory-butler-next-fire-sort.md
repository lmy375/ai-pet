# PanelMemory butler_tasks 「⏰ next-fire 升序」sort toggle（iter #301）

## Background

owner 在 PanelMemory butler_tasks 段看任务清单时，最常问的"接下来 N 分钟
/ 小时会 fire 的有什么"信号目前只能通过逐条扫每条 ⏰ N 分后 chip 拼凑。
当任务多（10+）时这种扫读成本高。

本迭代加 butler_tasks 段专属「⏰ next-fire 升序」一键 toggle。激活时段内
items（pinned + rest 各自）按下次触发时刻升序，最近会 fire 的浮顶。
sortByRecent (📅 全局)互补 —— 那个是"近改视角"，这个是"未来视角"。

## Changes

仅 `src/components/panel/PanelMemory.tsx`：

- 新 state `sortBulterByNextFire` + setter；localStorage key
  `pet-butler-sort-next-fire` 持久化（pattern 与 sortByRecent 同源）
- 新 helper `nextFireMs(schedule, now)` —— 返下次触发绝对 ms 或 null：
  - `every`：今日 HH:MM；已过 → 明日同点
  - `every_weekdays`：今日 mask 命中且未过 → 今；否则向前找 ≤ 7 天首个
    命中日（mask === 0 → null）
  - `once` / `deadline`：绝对时间（绝对组 Date → getTime；NaN 兜底 null）
- pinned + rest 排序分支扩 ——`useNextFire = catKey==='butler_tasks' &&
  sortBulterByNextFire` 时按 nextFireMs 升序；解析失败 / null 排到 +∞
  即段尾。与 sortByRecent 互斥（同时开取 next-fire，更贴 owner 意图）
- chip 渲染：插在 catKey === "butler_tasks" 专属 chip 块的 ✅ 完成率
  chip 之前。indigo tint 与既有 silent / snooze / done-rate / pinned /
  high-pri 五色族错开

## Key design decisions

- **不在 next-fire 倒计时 chip 内单独算**：抽 `nextFireMs` 公共 helper
  把"下次触发时刻"算法集中在一处；既有倒计时 chip 内联算法暂保（避免本
  迭代触改 50+ 行 render），follow-up 整理时可统一回填。helper 注释明
  确"任一处改算法两边都要同步"。
- **解析失败 / null 排段尾而非省略**：butler_tasks 段允许有"无 schedule
  前缀"的 item（owner 可能新建忘加 prefix）/ once 已过期 item。这些 item
  仍应渲染（owner 仍要看到 / 编辑），只是不参与 next-fire 排序优先级 —
  放段尾用 `+∞` sentinel 实现。
- **与 sortByRecent 互斥优先 next-fire**：同时开两个 toggle 时取 next-fire
  排序（more forward-looking）；sortByRecent 是 fallback。语义上"接下来
  要发生什么"比"我最近改了什么"更接近 butler_tasks 段的"管家" semantic.
- **localStorage 持久 + 全局而非 per-cat**：butler_tasks 是唯一有 schedule
  的 cat —— 不必做 per-cat 状态。owner 切走再回到 panel 仍保留偏好；与
  sortByRecent 同 pattern 防"用户每次都要点一遍"。
- **pinned 行为不变**：pinned items 仍挂头，但段内也按 next-fire 排 —
  与 sortByRecent 行为一致。pin 是"owner intent 强信号"，应当压过排序
  策略。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.21s)
