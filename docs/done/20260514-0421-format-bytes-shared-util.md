# 抽 `formatBytes` 到共享 util

## 背景

`PanelMemory.tsx` 末尾的 `formatBytes(n)` 是个完整的 B/KB/MB/GB 自适应 formatter（带 `< 0` defensive）。`PanelSettings.tsx` 的 `dbStats.size_bytes` 显示却走 inline 三目（`>= 1024*1024 ? MB : KB`），有两个隐含缺陷：

1. **小于 1024 字节** 显成 `0.0 KB`，看起来像 bug（pet.db 第一次建表后可能就是这状态）
2. **超 GB** 不会切到 GB 单位（罕见但 task_archive 长尾会触及）

把 `formatBytes` 抽到 `src/utils/formatBytes.ts`，两边 import。

## 改动

### `src/utils/formatBytes.ts`（新）

```ts
/// B / KB / MB / GB 自适应单位转换。负值 / NaN / Infinity → "0 B" 兜底。
/// KB+ 用 1 位小数；MB+ 同；GB 同。> 1 PB 时仍按 GB 显示（个人桌面 app
/// 极不可能触及）。
export function formatBytes(n: number): string {
  if (!Number.isFinite(n) || n < 0) return "0 B";
  if (n < 1024) return `${n} B`;
  const kb = n / 1024;
  if (kb < 1024) return `${kb.toFixed(1)} KB`;
  const mb = kb / 1024;
  if (mb < 1024) return `${mb.toFixed(1)} MB`;
  const gb = mb / 1024;
  return `${gb.toFixed(1)} GB`;
}
```

### `src/components/panel/PanelMemory.tsx`

- 删末尾本地 `function formatBytes` 定义
- 顶部 import `import { formatBytes } from "../../utils/formatBytes";`
- 行为不变

### `src/components/panel/PanelSettings.tsx`

- 顶部 import 同
- dbStats size_bytes 显示：

```tsx
{dbStats.size_bytes >= 1024 * 1024
  ? `${(dbStats.size_bytes / 1024 / 1024).toFixed(2)} MB`
  : `${(dbStats.size_bytes / 1024).toFixed(1)} KB`}
```

→

```tsx
{formatBytes(dbStats.size_bytes)}
```

副效应：< 1KB 显 "N B"，> 1GB 显 "X.Y GB"（行为更正）。

## 不做

- 不改 MB 上 2 位小数 vs util 的 1 位小数（行为微调）：保持 util 的 1 位小数（与 PanelMemory 既有 UI 一致；2 位小数原是 inline 默认，无依据）
- 不参数化 precision：默认 1 位小数对个人桌面 app 数据规模够用
- 不写单测：函数 6 行；类型 + 测试一目了然

## 验收

- `npx tsc --noEmit` ✅
- 切「设置」→「本地数据目录」→ pet.db 大小显示与之前等价（KB / MB），但小数位从 2 调到 1（视觉微变）
- 切「记忆」概览 chip 行 → 💾 行为不变（同一 formatBytes）

## 完成

- [x] util/formatBytes.ts 新建
- [x] PanelMemory.tsx: 改 import，删本地定义
- [x] PanelSettings.tsx: 改 import，替换 inline 三目
- [x] `npx tsc --noEmit` 通过
- [x] 移到 docs/done/
