# PanelMemory 类目卡 sparkline 全 7 天 0 update 时附「闲置 Xd+」灰 hint

## 背景

iter #185 加了 7 天 churn sparkline。但用户反馈"完全空的 sparkline"（7 天 0 柱）容易被忽略 / 误读为渲染失败。需要一个明确的"该类目闲置 X 天"文本标签，让 owner 一眼看到"这个 cat 已经放了很久没动"。

## 改动

### `src/components/panel/PanelMemory.tsx` —— sparkline IIFE 内加 idle hint 分支

在既有 sparkline 渲染分支后追加：

```tsx
let idleDays: number | null = null;
if (total === 0 && cat.items.length > 0 && latestTs !== null) {
  idleDays = Math.floor((now.getTime() - latestTs) / 86400000);
}
return (
  <>
    <span title={tip} ...><svg>...sparkline...</svg></span>
    {idleDays !== null && idleDays >= 7 && (
      <span
        title={`该类目 ${idleDays} 天没动 — 可考虑 consolidate / 调整 / 删该类目`}
        style={{
          fontSize: 10, color: "var(--pet-color-muted)",
          background: "var(--pet-color-border)", borderRadius: 8,
          padding: "1px 6px", opacity: 0.7, fontWeight: 400,
        }}
      >
        闲置 {idleDays >= 30 ? `${Math.floor(idleDays / 30)}mo+` : `${idleDays}d+`}
      </span>
    )}
  </>
);
```

显示规则：
- 7 天 0 update（`total === 0`）
- AND 类目非空（`cat.items.length > 0`）—— 空类目不是"闲置"，是"还没开始用"
- AND 能算出 latestTs（防 yaml 损坏 / 全字段空时显错误数）
- ≥ 30 天 → `Nmo+`（月单位更醒目，例 `闲置 3mo+`）
- < 30 天 → `Nd+`（天单位）
- < 7 天虽 7 天 0 update 也不显（理论上不可能：total=0 时 latestTs 必≥ 7 天前；double-gate 安全）

视觉：fontSize 10（比 badge 11 更小）+ border 灰背景 + opacity 0.7 + 圆角 8 chip 形 → 不抢眼，与 sparkline 错位组合形成"该 cat 没动" 信号。

## 关键设计

- **gate cat.items.length > 0**：空类目本来就该新建（出现"立刻处理 (N)"或"+ 新建"按钮）—— 不应再额外说 "闲置"。owner 看到 "0 条 / + 新建" 已知道空 cat。
- **gate latestTs !== null**：本来 cat.items.length > 0 时 latestTs 应该有值，但 yaml 字段损坏 / 老数据 updated_at 缺失时 latestTs 仍可能 null。double-gate 兜安全。
- **复用既有 `now` 变量**：line 2137 处已声明 `const now = new Date();` 用于 butler overdue 判断 —— sparkline IIFE 在它 scope 内可直接拿。
- **复用 `latestTs`**：line 2151 已经 inline 算过 latestTs（cat 最近一条 item 的 updated_at），sparkline IIFE 直接用。
- **30d / mo+ 双单位**：超过 30 天用 "Nmo+"（month）让"半年没动"这种 case 不显成 "180d+"（数字噪音多）。< 30 天保留 "Nd+"。
- **opacity 0.7 + muted bg**：低调灰 chip，不抢 badge / "最近 X" / 闹钟 / + 新建 等更主要的 hit area；ambient hint 而非 actionable。

## 不做

- **不让 hint 可点击 trigger consolidate**：actionable 走 owner 自己思考 + 走既有按钮路径。一键 consolidate 全闲置类目风险高（删用户内容），不在本 iter 范围。
- **不显具体到分钟 / 小时**：≥ 7 天后日级精度足够；hour 级会信息过载。
- **不在 sparkline 0-柱 + cat 有 item 但 latestTs 在 7 天内（理论不可能）显 fallback**：双 gate 已防御该 case；万一发生（时钟回拨 / 解析失败）也只是不显 chip，不报错。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 1.15s
- 改动 ~50 行（IIFE 内 idleDays 计算 6 + chip span 25 + fragment 包裹微调）。既有 sparkline rect 渲染 / tooltip / latestTs / now 计算路径完全不动。

## TODO 状态

剩 4 条留池：
- ChatMini 历史区双击 user/assistant 气泡内的「title」ref token 跳 PanelTasks
- 桌面 pet 右键菜单加「切 Live2D 模型」子菜单
- butler_task 描述新增 [reminderMin: N] 标记
- PanelTasks 任务行右键菜单加「复制为 markdown 引用块」

## 后续

- 闲置 ≥ 90 天的 cat 顶上加一个"📦 归档此 cat"按钮 —— 软移到 archive cat 的快捷路径。
- 闲置 chip click 弹一个对话框列该 cat 全部 items 让 owner 一次 multi-select 批删 / 转移到 general。
- 全 7 天 0 + items.length === 0 时显 "💤 空 + 闲置" 双 hint 让"已死类目"更显眼。
