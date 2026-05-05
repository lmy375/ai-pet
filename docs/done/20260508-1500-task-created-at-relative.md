# 任务卡片显示 created_at 相对值（Iter R87）

> 对应需求（来自 docs/TODO.md）：
> 任务卡片显示 created_at 相对值：itemMeta 当前只显 detail_path + 更新时间，补充"X 天前创建"让用户一眼分辨"新积压 vs 老欠债"。

## 目标

PanelTasks 列表 itemMeta 现在仅显绝对时间 `创建于 2026-05-05 14:30`。用户
快速 skim 时要心算"今天是 5 月 8 日，那个任务是 3 天前的"——多余认知开销。

补充相对时间："创建于 2026-05-05 14:30 · 3 天前"。新积压（< 1 天）vs
老欠债（> 7 天）一眼可辨。

## 非目标

- 不替换绝对时间 —— 绝对时间在排查时间线时仍要参考；用 `·` 分隔附在末尾
- 不改 due / updated_at 显示 —— 那两个已有相应 chip / 红点表达
- 不持久化 / 不动后端

## 设计

### 复用既有模式

文件已有 `formatRecentlyUpdatedHint(updatedAt, now)` 处理"刚动过"绿点 hover
文案（< 60s "刚刚"，else "X 分钟前"）。本轮新增 `formatRelativeAge` 走
更宽量级（minute / hour / day）：

```ts
function formatRelativeAge(createdAt: string, now: number): string {
  const ts = Date.parse(createdAt);
  if (Number.isNaN(ts)) return "";
  const age = now - ts;
  if (age < 60_000) return "刚创建";
  if (age < 3_600_000) return `${Math.floor(age / 60_000)} 分钟前`;
  if (age < 86_400_000) return `${Math.floor(age / 3_600_000)} 小时前`;
  return `${Math.floor(age / 86_400_000)} 天前`;
}
```

### 渲染

替换 `<span>创建于 {t.created_at.slice(0, 16).replace("T", " ")}</span>`：

```tsx
<span>
  创建于 {t.created_at.slice(0, 16).replace("T", " ")}
  {(() => {
    const rel = formatRelativeAge(t.created_at, nowMs);
    return rel ? ` · ${rel}` : null;
  })()}
</span>
```

`nowMs` state 已经每 30s 自动刷新（line 485-），相对时间会自动滚动。

### 测试

无单测；手测：
- 刚创建任务 → "创建于 ... · 刚创建"
- 5 分钟前 → "5 分钟前"
- 3 小时前 → "3 小时前"
- 5 天前 → "5 天前"
- nowMs 30s 间隔自动 tick，长开面板时相对值会前移（59 → 60 → 61 分钟前；分钟跨小时 → "1 小时前"）

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | helper + 渲染 |
| **M2** | tsc + build |

## 复用清单

- 既有 `nowMs` state（30s tick）
- 既有 `formatRecentlyUpdatedHint` 量纲风格（中文 "X 单位前"）

## 进度日志

- 2026-05-08 15:00 — 创建本文档；准备 M1。
- 2026-05-08 15:05 — M1 完成。`formatRelativeAge` helper（4 量级：< 60s "刚创建" / minute / hour / day）；NaN 守卫返空串让调用点降级到只显绝对时间。itemMeta "创建于" span 内附 ` · {rel}`，rel 空时整段不渲染。复用既有 nowMs（30s tick）让相对值自动滚动。
- 2026-05-08 15:08 — M2 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过（与 R86 同 build 通过 499 modules / 1.05s）。归档至 done。
