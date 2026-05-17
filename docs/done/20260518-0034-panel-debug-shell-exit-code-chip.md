# PanelDebug「⚙️ shell exit code 分布」chip（iter #431）

## Background

LLM 用 shell tool 跑命令时若频繁失败（语法错 / 路径错 / 权限错），
owner 当前只能逐条看 logs 找 return_code 行 — 没有聚合视图。

本 iter 加 shell exit code 分布 chip：扫 ShellStore（≤ 1h in-memory
缓存，cleanup_old_tasks 1h cutoff）按 return_code 分桶 success /
failure / running_or_unknown，让 owner 一眼看 LLM shell tool 失败率。

同时丢 TODO「detail.md 编辑器加 📋 复制本节 + 子节」一行 —
既有 `extractSectionFromMarkdown(md, counter)` helper（line 564）
已用 `<= startLevel` boundary，复制 H2 时自动包含其下所有 H3 children
直到下一个 H1/H2；button title 也明确写「heading + 直到下个同级 /
更高级 heading 之前的内容」。功能已完整。

## Changes

### `src-tauri/src/commands/shell.rs`

新增 `ShellExitCodeStats` 结构 + `get_shell_exit_code_stats` Tauri
命令：

```rust
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellExitCodeStats {
    pub success: u32,
    pub failure: u32,
    pub running_or_unknown: u32,
    pub total: u32,
}

#[tauri::command]
pub fn get_shell_exit_code_stats(store: State<'_, ShellStore>) -> ShellExitCodeStats {
    let map = store.0.lock().unwrap();
    let mut success = 0; let mut failure = 0; let mut running_or_unknown = 0;
    for task in map.values() {
        match task.return_code {
            Some(0) => success += 1,
            Some(_) => failure += 1,
            None => running_or_unknown += 1,
        }
    }
    let total = success + failure + running_or_unknown;
    ShellExitCodeStats { success, failure, running_or_unknown, total }
}
```

### `src-tauri/src/lib.rs`

注册 `get_shell_exit_code_stats` 到 invoke handler list。

### `src/components/panel/PanelDebug.tsx`

#### 1. state + 30s 轮询

```ts
const [shellExitStats, setShellExitStats] = useState<ShellExitCodeStats>({...});
useEffect(() => {
  const tick = async () => {
    const stats = await invoke("get_shell_exit_code_stats");
    setShellExitStats(stats);
  };
  void tick();
  const id = setInterval(tick, 30_000);
  return () => clearInterval(id);
}, []);
```

30s 轮询 — shell call 不是高频事件，与 `recent_speeches` 同节奏。

#### 2. chip 渲染（紧贴 📋 导出快照 MD 按钮之后）

```tsx
{shellExitStats.total > 0 && (() => {
  const failRatio = shellExitStats.failure / shellExitStats.total;
  const warn = failRatio >= 0.5;
  return (
    <span title={`shell 工具调用近 1 小时缓存：${success} 成功 / ...`}
      style={{ ...toolBtnStyle, background: warn ? red-bg : card,
        color: warn ? red-fg : muted, fontVariantNumeric: "tabular-nums" }}>
      ⚙️ {success}/{failure}/{runningOrUnknown}
    </span>
  );
})()}
```

设计要点：
- **failure ≥ 50% 染红 warning**：让 owner 一眼看「LLM shell 异常」
  — 否则 muted 中性
- **`success/failure/unknown` 三段紧凑格式**：tabular-nums 防数字
  变化时 chip 宽度抖
- **total === 0 时不渲**：早期窗 / 长时间未触发 shell 的安静状
  态不显 dead chip
- **title attr 详尽**：hover 解释「近 1 小时缓存」+ failure 比例
  + cleanup_old_tasks 行为

#### 3. snapshot markdown 也加 `## shell 退出码` 段

让 owner 用「📋 导出快照 MD」时含 shell exit code 数据，给 bug
report 提供 LLM tool 健康信号。

## Key design decisions

- **ShellStore 而非新 persistent log**：ShellStore 已有 1h 内 in-
  memory 缓存 + cleanup_old_tasks，对「近 1h 失败率」audit 已够；
  新 log 文件增加 IO + 写盘负担不划算
- **3 桶简化分类**：success / failure / running_or_unknown 已 cover
  owner 关心的核心信号；细分按 exit code（127 / 1 / 2 / 130 等）
  对 owner 价值低 — bug report 时 logs 已有详细 code
- **不区分 timeout vs killed vs panic**：都视作 running_or_unknown；
  细分要 backend 加 status 字段超本 iter 范围
- **30s 轮询**：与 recent_speeches 同 — shell call 是 LLM 主动触
  发事件不高频
- **不为单 chip 引 unit test**：纯 ShellStore lock + count；前端是
  setState + render；build pass + 手测足够（让 LLM 跑几条 shell
  → 看 chip 显计数 → 故意触发失败看 chip 染红）

## Verification

- `cargo build --lib`（backend）— clean（仅 pre-existing 8 warnings 无关）
- `cargo test --lib`（全表）— 1472 / 1472 通过
- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.31s)
