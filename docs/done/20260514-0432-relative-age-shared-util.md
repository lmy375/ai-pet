# 抽 `formatRelativeAgeBuckets` 共享 util

## 背景

"X 分钟前 / X 小时前 / X 天前" 的 3-bucket 级联在 PanelMemory / PanelTasks /
PanelChat 等三处独立实现（同样的 60_000 / 3_600_000 / 86_400_000 边界数字、
同样的 Math.floor 除法、同样的"分钟前 / 小时前 / 天前"中文）。各 caller 后
缀略不同（"更新" / "" / ""），但核心分桶是一份逻辑。

抽到 `src/utils/formatRelativeAge.ts`，让边界数字与中文文案有单点 SoT。

## 改动

### `src/utils/formatRelativeAge.ts`（新）

```ts
export function formatRelativeAgeBuckets(ageMs: number): string {
  if (ageMs < 3_600_000) return `${Math.floor(ageMs / 60_000)} 分钟前`;
  if (ageMs < 86_400_000) return `${Math.floor(ageMs / 3_600_000)} 小时前`;
  return `${Math.floor(ageMs / 86_400_000)} 天前`;
}
```

不处理 `ageMs < 60_000`（"刚刚 / 刚创建 / 不到 1 分钟"等）—— 各 caller 选词
偏好不同（更新 / 创建 / 主动开口），统一抽不到一处。

### 三个 caller

- `PanelMemory.tsx` `formatLastUpdated`：`< 60s → "刚刚更新"`；其余 → `${formatRelativeAgeBuckets(age)}更新`。原 4 行级联缩到 1 行
- `PanelTasks.tsx` `formatRelativeAge`：`< 60s → "刚创建"`；其余 → `formatRelativeAgeBuckets(age)`。3 行 → 1 行
- `PanelChat.tsx` 内联（marked-list 显标记时间）：`< 60s → "刚刚"`；其余 → 同。5 行三目链 → 1 行

## 不做

- 不动 App.tsx `formatMoodElapsed`：只 2 个 bucket（minute / hour），不在 3-bucket 模板里
- 不动 PanelPersona 内联（477 / 580）：上下文嵌套深，独立改也行但不属本轮 dedup 主线
- 不动 PanelTasks `formatRecentlyUpdatedHint`（< 5min cap，只 minute bucket）：不在 3-bucket 模板里
- 不抽 "刚刚 / 刚创建" 阈值参数版：3 caller 文案各异，参数化反而把简单逻辑搞复杂

## 验收

- `npx tsc --noEmit` ✅
- 「记忆」/「任务」/ 聊天 marked-list 上的相对时间显示与之前完全等价

## 完成

- [x] util/formatRelativeAge.ts 新建
- [x] PanelMemory.tsx: import + formatLastUpdated 收缩
- [x] PanelTasks.tsx: import + formatRelativeAge 收缩
- [x] PanelChat.tsx: import + marked-list 内联收缩
- [x] `npx tsc --noEmit` 通过
- [x] 移到 docs/done/
