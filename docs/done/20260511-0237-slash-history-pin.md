# PanelChat slash menu 加 history pin

## 需求

slash menu 当前永远按 SLASH_COMMANDS 声明序展（clear / tasks / search / sleep / image / help）。但用户的实际偏好是不均匀的 —— 我可能 90% 时间都在用 `/image`，每次都要光标下移到第 5 项才能选；常用命令应该沉到顶部。需要按使用频次自适应排序，旧偏好也能慢慢淡出。

## 设计

**衰减计数法**：每次执行命令前，把全局 score 表 `× 0.9`（衰减），然后给当前命令 `+ 1`。半衰期 ≈ 6.5 次（log(0.5)/log(0.9) ≈ 6.6），意味着用户切到新命令几次就能压过旧热点。

`/image × 100, /tasks × 0` → score: `image=100, tasks=0`
之后只用 /tasks × 10 → score: image≈100×0.9^10≈35, tasks≈9.5
继续 × 5 次 → image≈35×0.9^5≈21, tasks≈9.5×0.9^5+5×... 直到反超

新用户没用过任何命令 → 全 score=0 → 排序退回 SLASH_COMMANDS 声明序，体感无差别。

## 实现

`src/components/panel/slashCommands.ts`：

- `recordSlashCommandUsage(name)` 公开函数：read scores → 全表 × 0.9 + 阈值 0.05 prune（防 map 无限增长）→ 当前 +1 → write
- `readSlashScores()` / `writeSlashScores()` 私有 helper：localStorage 读写，禁用 / 解析失败静默吞，配额满也不抛
- `filterCommandsByPrefix` 在原 prefix filter 之后追加 stable sort：score desc 优先，相同 / 零分按原数组下标兜底（V8 Tim sort 稳定）

`src/components/panel/PanelChat.tsx`：

- import `recordSlashCommandUsage`
- `executeSlash` 入口处：action.kind 既非 `unknown` 也非 `incomplete` 时调一次 `recordSlashCommandUsage(action.kind)`。clear 的二次确认首发也算"用一次"，避免要执行真清空才学偏好

## 数据形态

localStorage `pet-slash-history` → JSON `Record<commandKind, score>`，例：

```json
{ "image": 12.4, "tasks": 3.6, "clear": 0.15 }
```

下次启动 readSlashScores 重建 → filterCommandsByPrefix 即刻生效。clear 0.15 已接近 prune 阈值 0.05，再不用就被清掉。

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 新装：菜单按 clear / tasks / search / sleep / image / help 显（声明序）
  - 用 /image 三次后 → 菜单顶端是 image
  - 用 /tasks 多次 → 慢慢顶上去，image 退后
  - localStorage 禁用（隐私窗口） → 写入静默失败，菜单永远按声明序，功能不崩

## TODO 池清空 → 自主提案

按 TODO.md 规则 #1，提出 5 条新需求（已写入 TODO.md）：

1. 桌面 ChatPanel 缩略图条接 ImageThumb 复用 lightbox + 复制
2. PanelTasks 任务详情解析 image markdown / data URL 渲缩略图
3. ChatMini 96px 缩略图也加快速复制 📋
4. 设置页加 chat model 文本测试按钮（与 image 测试对齐）
5. /image 历史 prompt 召回（slash 命令也入 history）
