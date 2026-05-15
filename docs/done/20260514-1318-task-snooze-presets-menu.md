# 任务右键菜单 Snooze 预设 + 撤销暂停

## 背景

TODO（本轮 auto-proposed）：

> PanelTasks 任务行右键 Snooze 子菜单：4 预设（30 分 / 今晚 / 明早 / 下周一）+ 撤销 snooze，省去手敲 `[snooze: ...]`。

20260514-1228 上一轮把 `[snooze: ...]` marker + proactive 过滤 + 💤 chip 都做完了，但用户要 snooze 一条任务时只能：
1. 打开"编辑详情" modal
2. 手敲 `[snooze: 2026-05-20 09:00]`
3. 注意 YYYY-MM-DD HH:MM 协议、是空格不是 T、要闭合 `]`
4. 保存

体验断流。`due` 的右键菜单早已有"今日 18:00 / 明日 09:00"两个预设；snooze 是对偶语义，补齐这条交互闭环。

## 改动

### 后端 Rust

**1. `task_queue::strip_snooze_markers(desc) -> String`（pure）**

```rust
pub fn strip_snooze_markers(desc: &str) -> String {
    let mut s = desc.to_string();
    while let Some(start) = s.find("[snooze:") {
        let end_rel = match s[start..].find(']') {
            Some(p) => p,
            None => break, // 未闭合 marker 不破坏数据
        };
        let end = start + end_rel + 1;
        let prefix_trim_end = s[..start].trim_end();
        let suffix_trim_start = s[end..].trim_start();
        let mut next = String::with_capacity(s.len());
        next.push_str(prefix_trim_end);
        if !prefix_trim_end.is_empty() && !suffix_trim_start.is_empty() {
            next.push(' ');
        }
        next.push_str(suffix_trim_start);
        s = next;
    }
    s
}
```

剥多个 marker（循环 find）；空白 normalize（两侧 trim + 必要时单空格分隔）；未闭合 marker 保留原样不破坏数据。

7 个新单测：basic / multiple / no-marker noop / unclosed preserved / whitespace normalization / leading / trailing。

**2. `commands::task::task_set_snooze(title, until)` Tauri 命令**

- `until == None` → 调 `strip_snooze_markers` 清掉所有既有 marker（撤销暂停语义）。
- `until == Some(s)` → 先 strict parse `YYYY-MM-DD HH:MM`（无效 → Err），再 strip 旧 marker + append 新 marker。
- 保 description 整洁：多次 set/unset 不会让 description 越积越长。
- 与 `task_set_due` 同模式 —— 不推 decision_log（日常 UX 调整，非状态转移）。
- 复用 `find_butler_task` + `memory_edit("update", ...)` 既有路径，自动 mirror 双写 SQLite。

注册到 lib.rs `invoke_handler!` 紧贴 `task_set_due`。

### 前端 React

PanelTasks `taskCtxMenu` 渲染：在既有 `⏰ due 今日 18:00 / 明日 09:00` 预设之后追加 snooze 段。

```tsx
{canMarkDone && (() => {
  const cur = tasks.find((x) => x.title === m.title);
  const currentSnoozed = cur?.snoozed_until ?? null;
  // computeUntil 内闭包计算 4 种预设 → "YYYY-MM-DD HH:MM"
  const presets = [
    { key: "30m",         label: "💤 暂停 30 分" },
    { key: "tonight",     label: "💤 暂停至今晚 18:00" },
    { key: "tomorrow",    label: "💤 暂停至明早 09:00" },
    { key: "nextMonday",  label: "💤 暂停至下周一 09:00" },
  ];
  const setSnooze = async (until: string | null) => {
    setTaskCtxMenu(null);
    setActionErr(""); setBusyTitle(m.title);
    try { await invoke("task_set_snooze", { title: m.title, until }); await reload(); }
    catch (e) { setActionErr(`设 snooze 失败：${e}`); }
    finally { setBusyTitle(null); }
  };
  return (
    <>
      {presets.map((p) => (
        <button onClick={() => void setSnooze(computeUntil(p.key))}>{p.label}</button>
      ))}
      {currentSnoozed && (
        <button title={`当前 snooze 至 ${currentSnoozed.replace("T", " ")}`}
                onClick={() => void setSnooze(null)}>
          ☀️ 解除暂停
        </button>
      )}
    </>
  );
})()}
```

`computeUntil` 边界细节：

- `30m`: `now + 30 min`。
- `tonight`: today 18:00；**已过 18:00 自动跳明晚** —— 与 dueTonight chips 同语义，防"点了反而退到过去"。
- `tomorrow`: tomorrow 09:00。
- `nextMonday`: 下个周一 09:00；今日是周一也跳下周一（与 dueNextMonday 同"下周一 = 下周第一天"语义）。

`currentSnoozed` 用 `tasks.find(t => t.title === m.title)?.snoozed_until` 拿最新值；truthy 时多渲一个"☀️ 解除暂停"行（accent 蓝色文字）让 user 随时撤销。

格式串 `YYYY-MM-DD HH:MM`（空格分隔，minute precision）—— 与 `task_set_snooze` Rust 端 strict parse 协议对齐；`due` 协议用 `YYYY-MM-DDThh:mm`（T 分隔）属不同字段不混。

## 不做

- **不支持任意时刻 picker**。4 预设已 cover > 90% 用例；要自定义时刻可走 LLM `butler_task_edit` 改 description。本菜单本意是 1-click，不该弹二级 datetime picker。
- **不让 done / cancelled 任务 snooze**。`canMarkDone` 已 gate（这两态对应任务终态，"暂停"语义无意义）。
- **不动 batch UI**。PanelTasks 多选批量 bar 仍只支持 重试 / done / cancel / priority / due；批量 snooze 是个可独立做的扩展（用户场景：选 5 条任务一起延后到下周）。
- **不动 PanelTasks 行内编辑 modal**。modal 仍是手敲 description 的入口；右键 + 修改 description 两条路径并存，互不干扰。
- **不验证 cross-marker 冲突**（如同时有 `[blockedBy:]` + `[snooze:]`）。两个 marker 语义独立（一个等先决一个等时刻），proactive filter 是 OR —— 任一卡住即过滤。自然 compose。

## 验证

- `cargo test --lib task_queue::tests::strip_snooze` ✓ 7/7
- `cargo test --lib` ✓ **962 / 962 通过**（955 → 962；+7 个 strip 测试）
- `cargo check` ✓ 0 error
- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.12s

## 后续

- TG bot `/snooze <title> <duration>` 对偶（本次 auto-proposed 已写进 TODO）。
- 多选 → 批量 snooze（PanelTasks bulkBar 多一项）。
- 任务卡 hover 缩略图直接显当前 snooze 剩余 "醒来还剩 1 天 3 小时" —— 减少切到 chip tooltip 的操作。
