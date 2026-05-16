# detail.md 自动每 60s 存草稿到 localStorage + 恢复 banner

## 背景

TODO 上 auto-proposed 一条："任务详情 detail.md 自动每 60s 把 textarea 内容存草稿到 localStorage：防意外关闭 / Esc 误触丢失改动。"

刚 ship 的"dirty > 60s 红色 pulse 警示" 是被动 reminder。但若 owner 真没看到、强制关 panel / Esc 误触 / 浏览器崩溃，写了一半的 detail.md 仍然丢。

补一个主动的 auto-save 草稿 safety net：每 60s 把 textarea 内容 dump 到 localStorage（key 含 task title），下次打开同任务的 detail 编辑器时检测 draft 与磁盘版不同 → 弹"恢复 / 忽略" banner。保存成功清掉 draft；取消 / 关 panel / 崩溃 → 保留 draft 让下次恢复。

## 改动

### `src/components/panel/PanelTasks.tsx`

#### 60s autosave tick

紧贴 dirty stale tracking 之后：

```ts
const DRAFT_AUTO_INTERVAL_MS = 60_000;
const draftKeyFor = (taskTitle: string) =>
  `pet-detail-draft-${taskTitle}`;

const [pendingDraft, setPendingDraft] = useState<{
  title: string;
  content: string;
  ts: number;
} | null>(null);

useEffect(() => {
  if (editingDetailTitle === null) return;
  const id = window.setInterval(() => {
    const dirty = editingDetailContent !== editingDetailOriginalRef.current;
    if (!dirty) return;
    try {
      window.localStorage.setItem(
        draftKeyFor(editingDetailTitle),
        JSON.stringify({ content: editingDetailContent, ts: Date.now() }),
      );
    } catch (e) {
      console.error("detail draft autosave failed:", e);
    }
  }, DRAFT_AUTO_INTERVAL_MS);
  return () => window.clearInterval(id);
}, [editingDetailTitle, editingDetailContent]);
```

仅 dirty 时写（content == original 无意义保存）。失败静默 console（隐私模式 / 配额满）。

#### 打开 editor 时检查 draft

`handleEnterEditDetail` 末尾新增：

```ts
try {
  const raw = window.localStorage.getItem(`pet-detail-draft-${taskTitle}`);
  if (raw) {
    const parsed = JSON.parse(raw) as { content?: unknown; ts?: unknown };
    if (
      typeof parsed.content === "string" &&
      typeof parsed.ts === "number" &&
      parsed.content !== currentMd
    ) {
      setPendingDraft({ title: taskTitle, content: parsed.content, ts: parsed.ts });
    } else {
      // 与磁盘版一致 / 格式坏 → 清掉 stale
      window.localStorage.removeItem(`pet-detail-draft-${taskTitle}`);
      setPendingDraft(null);
    }
  } else {
    setPendingDraft(null);
  }
} catch {
  setPendingDraft(null);
}
```

#### 保存成功后清 draft

`handleSaveDetail` 内 `task_save_detail` 成功后追加：

```ts
try {
  window.localStorage.removeItem(`pet-detail-draft-${taskTitle}`);
} catch {}
setPendingDraft(null);
```

#### 恢复 banner

editor 顶部（toolbar 之上）渲染：

```tsx
{pendingDraft && pendingDraft.title === t.title && (
  <div style={{ amber tint banner ... }}>
    <span>📝 检测到上次未保存的草稿（{X 分钟前}）—— 与磁盘版差 Y 字符</span>
    <button onClick={() => { setEditingDetailContent(pendingDraft.content); setPendingDraft(null); }}>🔄 恢复</button>
    <button onClick={() => { localStorage.removeItem(...); setPendingDraft(null); }}>✕ 忽略</button>
  </div>
)}
```

age 计算分四档：刚刚 / N 分钟前 / N 小时前 / N 天前；字符数差用 `Math.abs(draft.length - current.length)` 给 owner 量级直觉。

## 关键设计

- **60s 间隔**：与 dirty-stale 红色警示 60s 阈值一致 —— "你已经 dirty 60s 了"瞬间也是"第一次 autosave 到 localStorage"瞬间。两个机制天然同步。
- **`pet-detail-draft-${title}` key 含 title**：butler_tasks 标题唯一（memory_edit 重名拒绝），可以安全做 key namespace。重命名 task 会让旧 draft 孤儿 —— 但 task rename 极少 + 孤儿 draft 在 localStorage 占小空间，先不主动 GC。
- **draft 仅 dirty 时写**：与磁盘版相同时写 draft 毫无意义（恢复 = 不变）。每 tick 检查 `dirty` 守门。
- **进入编辑器时检测**：handleEnterEditDetail 在 setState 后立即 try-catch 读 localStorage。content 不同 → setPendingDraft 触发 banner；content 同（用户上次 save 但 draft key 没清干净 / panel 重启 race）→ 清 stale。
- **save 成功后清 draft**：磁盘已是真相，draft 没价值。setPendingDraft(null) 关 banner。
- **cancel / 关 panel 保留 draft**：与"持续 dirty 警示"对偶 —— 主动忽略 ⌘S 时 owner 期望草稿不丢。Esc cancel 走 armed 二次确认（既有），即便最终丢编辑内容也保 localStorage 草稿。
- **JSON 格式校验**：解析 raw 时 `typeof parsed.content === "string" && typeof parsed.ts === "number"` 防 corrupt / 旧格式 / 用户手 patch localStorage 误打 draft 进去等。
- **age 文案 4 档**：刚刚 (<1m) / N 分钟前 (<1h) / N 小时前 (<1d) / N 天前。天数 cap 自然防"N 月前"的荒诞值（极端老 draft 多半是真垃圾）。
- **字符数差量化**：`Math.abs(draft.length - current.length)` 给 owner "改了多少" 量级直觉，不必精确 diff。
- **amber tint**：警示但非破坏性（与 dirty-stale 红色 pulse 区分）—— "你需要做决策" vs "你需要立即注意"。

## 不做

- **不写 file system fs:write 作 backup**：detail.md 已是 fs 文件；草稿 backup 到 localStorage 与磁盘是不同 storage layer，互不污染。fs write 草稿要 Tauri fs 权限 + 路径决策 + 清理周期等，复杂度大。
- **不写测试**：纯 UI + localStorage IO + 60s interval；既有 dirty-stale / Esc-cancel-armed 同模式无单测。视觉验证（写 detail → 等 1 分钟 → 关 panel → 重开 → 看到恢复 banner → 点 🔄 验证内容回来）足够。
- **不接 Service Worker / IndexedDB**：60s 间隔 + localStorage 字符串容量足够（典型 detail.md ≤ 几 KB）；上限是 5MB 单 origin，1 个 detail ≤ 几 KB → 可同时跨 ~1000 任务有 draft 也不溢出。
- **不主动 GC 孤儿 draft key**：用户重命名 task 让旧 draft 成为孤儿，但 localStorage 容量足够多年的 task 编辑。等真有用户报"localStorage 满了"再加清理逻辑。
- **不暴露 settings 调 60s 间隔**：60s 与 dirty-stale 警示同步，调 settings 反破坏一致性。极少用户会觉得 60s 不够频繁。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.18s
- 改动 ~150 行（autosave useEffect 25 + handleEnterEditDetail 检测 25 + handleSaveDetail 清 draft 8 + 恢复 banner JSX 90 + 注释）；既有 dirty-stale 警示 / ⌘S save / Esc cancel armed / handleEnterEditDetail 自动滚到 last [x] 等路径完全不动。

## TODO 状态

6 条 auto-proposed 已完成 2 条，余 4 条留池：
- PanelTasks detail 编辑器加「↑ 上 / ↓ 下一条」导航箭头
- 桌面 pet hover 3s 浮 ambient 三段统计微卡片
- PanelMemory 类目 7 天 churn sparkline
- detail.md preview `[task: 标题]` 语法识别为 ref chip

## 后续

- 自动 GC 老 draft：编辑器关闭 N 天后 / 找不到对应 task title 时清理。当前 localStorage 容量充足，等真碎片化再做。
- 草稿对照 diff：banner 上加"📋 看差异" 按钮弹 modal 显 draft 与 currentMd 的 diff。owner 决策更精准。
- 跨设备 sync 草稿：仅 local，不跨设备（与 user memory "drop cross-device sync" 决策一致）。
