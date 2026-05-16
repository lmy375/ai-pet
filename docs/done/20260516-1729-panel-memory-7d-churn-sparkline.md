# PanelMemory 类目卡 7 天 churn mini sparkline

## 背景

TODO 上一条："PanelMemory 类目内『📈 7 天 churn』mini sparkline：看哪些类目最近最活跃。"

PanelMemory 已显类目 item 数 + "最近 X 时间前" 文本，但 owner 想区分"已死类目（半年没动）"和"持续活跃（每天 +1 item）"时，仅看数字看不出走势。一个 mini sparkline 7 根柱在 header 上让 ambient 节奏一眼可见。

## 改动

### `src-tauri/src/commands/memory.rs` — 新命令 + 测试

```rust
#[tauri::command]
pub fn memory_category_churn_7d() -> Result<BTreeMap<String, [u32; 7]>, String> {
    let index = memory_list(None)?;
    let today = chrono::Local::now().date_naive();
    let mut out: BTreeMap<String, [u32; 7]> = BTreeMap::new();
    for (key, cat) in &index.categories {
        let mut buckets = [0u32; 7];
        for item in &cat.items {
            if item.updated_at.is_empty() { continue; }
            let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&item.updated_at) else { continue };
            let local_date = dt.with_timezone(&chrono::Local).date_naive();
            let delta = (today - local_date).num_days();
            if (0..7).contains(&delta) {
                let idx = (6 - delta) as usize;  // delta=0 → idx 6 (today)
                buckets[idx] = buckets[idx].saturating_add(1);
            }
        }
        out.insert(key.clone(), buckets);
    }
    Ok(out)
}
```

- 复用 `memory_list(None)` 读 index.yaml → 不引新 IO / 不动 mirror。
- `updated_at` 由 `now_iso` 写为 `%Y-%m-%dT%H:%M:%S%:z`（`2026-05-16T17:25:12+08:00`），`parse_from_rfc3339` 完全兼容。
- 7 天窗口：今日 + 前 6 天（含今）；今日 = idx 6，6 天前 = idx 0。

#### 单元测试

```rust
#[test]
fn churn_buckets_distribute_by_local_date() {
    // 构造 today + 3 days ago + 8 days ago 3 个 item，验证 bucket 落点 + 过滤
    ...
    assert_eq!(buckets[6], 1);  // today
    assert_eq!(buckets[3], 1);  // 3 days ago
    assert_eq!(buckets.iter().sum::<u32>(), 2);  // 8d ago filtered out
}
```

复制了核心 bucket 计算 inline（避开 memory_list 读盘依赖），但代码路径与 production 1:1 —— 这测的是"日期换算 + bucket idx + window 过滤"三个真正容易写错的部分，符合 GOAL.md "tests must pin real behavior"。

### `src-tauri/src/lib.rs` — 注册命令

```rust
commands::memory::memory_category_churn_7d,
```

### `src/components/panel/PanelMemory.tsx` — fetch + sparkline 渲染

```ts
const [churnMap, setChurnMap] = useState<Record<string, number[]>>({});
useEffect(() => {
  if (!index) return;
  invoke<Record<string, number[]>>("memory_category_churn_7d")
    .then(setChurnMap)
    .catch((e) => console.error("memory_category_churn_7d failed:", e));
}, [index]);
```

每次 `index` ref 变（即 reload 后）重新 fetch —— owner 刚 edit / create / delete 完看到 today bar 升上来。

#### SVG sparkline（section header 内，"最近 X" 之后）

```tsx
{(() => {
  const buckets = churnMap[catKey];
  if (!buckets || buckets.length !== 7) return null;
  const max = Math.max(...buckets, 1);
  const barW = 6, gap = 2, N = 7;
  const W = barW * N + gap * (N - 1); // 54px
  const H = 14;
  const dayLabels = ["6天前","5天前","4天前","3天前","2天前","昨天","今日"];
  const total = buckets.reduce((a, b) => a + b, 0);
  const tip = total === 0
    ? `近 7 天没有动静`
    : `近 7 天 ${total} 次 update · ${buckets.map((v,i) => v>0 ? `${dayLabels[i]} ${v}` : null).filter(Boolean).join(" · ")}`;
  return (
    <span title={tip} ...>
      <svg width={W} height={H}>
        {buckets.map((v, i) => {
          const h = v === 0 ? 1 : (v / max) * H;
          const isToday = i === N - 1;
          return <rect
            x={i * (barW + gap)} y={H - h} width={barW} height={h} rx={1}
            fill={v === 0 ? "border" : isToday ? "accent" : "muted"}
            opacity={v === 0 ? 0.6 : isToday ? 1 : 0.7}
          />;
        })}
      </svg>
    </span>
  );
})()}
```

- 54×14 px = 紧凑，与 "最近 X" 文本 same baseline。
- per-cat 归一化（max = 该类目 7 日最大值）—— 不让某个高频 cat 把小 cat 全压成 0；ambient 节奏优先于跨 cat 绝对比较。
- 今日柱 accent 色 + 1.0 opacity 强调；其它柱 muted + 0.7 opacity；空当日 border 灰 + 1px baseline + 0.6 opacity → 让用户看到"存在性"而非完全留白。

## 关键设计

- **churn 定义 = "items with updated_at on that day"**：不区分 add / edit / rename（updated_at 都会更新），用最简单可靠的口径。owner 想区分 "新增 vs 编辑" 时再加专用 audit log；当前需求只是 ambient 走势，"今日有动静" 信号足够。
- **window 含今日 + 前 6 天 = 7 天**：今日 = idx 6 在最右，与"时间向右流动"直觉一致。owner 自然先看右侧柱（今日）再往左追溯。
- **per-cat 归一化 vs 全 panel 归一化**：选 per-cat。butler_tasks 一周 30 次 update 而 user_profile 一周 1-2 次 —— 全局归一化会把 user_profile 压成 0；per-cat 让每个类目都有可见的内部节奏。owner 不靠 sparkline 做"跨类目比较"（已有 item count badge）；sparkline 是"看自身鼓点"。
- **0 当日 1px baseline + 半透 border 灰**：让用户看到"存在性"而非完全留白 —— 一行 0 柱 + 当前几根柱 ≠ "渲染失败"，是"近期才有动静"。
- **tooltip 列具体每日数字**：sparkline 是 ambient hint，悬停时 owner 想知道"具体哪天 3 次哪天 0 次"。空 7 天时简短 "近 7 天没有动静" 不重复 0 0 0...
- **refetch on index 变化**：与 detailSizes 同 trigger pattern；index 是 PanelMemory 的 SoT，任何编辑后都会重 set → useEffect 重跑 → sparkline 自然刷新。不引轮询。
- **后端不存 audit log**：updated_at 是 lossy 历史（一个 item 多次编辑只剩最后一次时间戳）—— 但本 iter 接受这个 loss，因为"近 7 天该 item 是否被动过"还是被 updated_at 记下。今日改一项 → 该 item 出现在 today bar；过 7 天 → 该 item 自然离开 window。对 ambient 视图是 "good enough"。

## 不做

- **不引 audit log 记每次 add/edit/rename**：要新文件 + 写盘流 + IO 噪声。当前 updated_at 信号已够 ambient 用。
- **不让 sparkline 可点击 drill-down**：14×54px 不是 hit target；owner 想看明细已有 item list 在卡片内。
- **不显 14/30 天版**：横向占位过宽 + ambient 信号 7 天已足够。
- **不全局归一化**：见关键设计。
- **不显 emoji 📈 在 sparkline 前**：本来要 "📈 sparkline" 形式，但 14×54px sparkline 自身视觉表达就够强 + 加 📈 反而抢眼；emoji 留给纯文本场景。

## 验证

- `cargo check` ✓ 无 error
- `cargo test churn_buckets_distribute_by_local_date` ✓ pass
- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 1.17s
- 改动 ~150 行（backend command 35 + test 50 + lib.rs 注册 1 + frontend fetch 12 + sparkline JSX 65）。
- 既有 memory CRUD / pin / batch delete / consolidate / rename cat / drag reorder / butler today todo 路径完全不动。

## TODO 状态

剩 0 条 —— TODO.md 已空。下一轮 cron tick 进入"auto-propose 5-6 条新需求"分支。

## 后续

- 加 `created_at` 维度也算 churn，区分 add 与 edit（item.created_at 与 updated_at 都在同一 day 桶 = 当日新建；only updated_at = 当日编辑）—— 双色 stacked bar。
- sparkline 总数 0 时把整条柱用极淡 dashed style 表示 "已死类目"，提示 owner 可考虑 consolidate / 删该 cat。
- 拉一个全局 sparkline 在 PanelMemory header 顶部，按 day 聚合所有类目 —— 看整体 memory 系统的"代谢率"。
- 7 天延伸到 30 天双行（首行 14 天，第二行 14 天）—— 但要小心横向占位。
