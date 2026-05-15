# detail.md 工具栏 📅 当前时间按钮

## 背景

TODO 上 auto-proposed 一条："detail.md 工具栏「📅 当前时间」按钮：插 `YYYY-MM-DD HH:MM` 或 ISO 字符串，记录里程碑 / 进度更新时常用。"

detail.md 是 owner / 宠物轮流写"任务进度笔记"的地方。每条进度记录"什么时候做的"是高价值信号（owner 周末复盘 / 宠物排序 / TG 通知都用得到时间锚），但手敲 `2026-05-14 17:51` 这 16 个字符是高摩擦动作 —— 用户最常的做法是省略不写，导致 detail.md 流水账没时间感。

按钮一键插入当前本地时间，让"标里程碑"成本接近 0。

## 改动

### `src/components/panel/PanelTasks.tsx`

#### helper

```ts
const insertCurrentTimeAtCursor = useCallback(() => {
  const ta = detailEditorRef.current;
  if (!ta) return;
  const start = ta.selectionStart ?? 0;
  const end = ta.selectionEnd ?? start;
  const value = ta.value;
  const now = new Date();
  const y = now.getFullYear();
  const mo = String(now.getMonth() + 1).padStart(2, "0");
  const d = String(now.getDate()).padStart(2, "0");
  const hh = String(now.getHours()).padStart(2, "0");
  const mm = String(now.getMinutes()).padStart(2, "0");
  const stamp = `${y}-${mo}-${d} ${hh}:${mm}`;
  const next = value.slice(0, start) + stamp + value.slice(end);
  const cursorPos = start + stamp.length;
  setEditingDetailContent(next);
  requestAnimationFrame(() => {
    const cur = detailEditorRef.current;
    if (!cur) return;
    cur.focus();
    cur.selectionStart = cur.selectionEnd = cursorPos;
  });
}, []);
```

#### 按钮

工具栏末尾、紧贴 📊 表格按钮之后：

```tsx
<button
  type="button"
  onClick={insertCurrentTimeAtCursor}
  title="插入当前时间（YYYY-MM-DD HH:MM 本地，与 [snooze:] / [once:] marker 协议同形）。记录里程碑 / 进度笔记 / 调用时间戳都用得到。"
  style={mdToolbarBtnStyle}
>
  📅
</button>
```

## 关键设计

- **`YYYY-MM-DD HH:MM` 本地空格分隔**：与既有 `[snooze: YYYY-MM-DD HH:MM]` / `[once: YYYY-MM-DD HH:MM]` marker 协议**完全同形**。用户敲完按钮后想升级成 marker，复制 → 包 `[snooze: ...]` 即可，零格式转换摩擦。不用 ISO（带 `T`）—— 空格分隔在 markdown 流水账里阅读更自然，且 marker 协议本来就是空格。
- **分钟精度**：秒级在 detail.md 里是噪音（用户不在 1 秒内连续按按钮两次）；与既有 `[snooze:]` / `due` 协议都对齐。
- **本地时区**：detail.md 是 owner-facing；UTC 反而让"我刚才几点做的？"难读懂。Tauri 桌面 app 单设备运行，无跨时区同步问题。
- **光标落到字符串末尾**：与既有 wrap / line-prefix 模式的"select 占位"不同 —— 时间戳是 final-form 文本，用户接下来要敲"完成了 X" / "卡在 Y" 等后续文字，光标自然落尾最顺。
- **不写 useCallback 的 deps**：纯依赖 `detailEditorRef.current` 当前值 + `new Date()` —— 全是 mutable / 系统时钟读取，deps 空数组（与既有 `insertTableSkeletonAtCursor` 同模式）。
- **位置紧贴 📊**：表格、时间都属于"块级内容生成"类工具，与前面的 wrap / line-prefix 字符语法工具分组。视觉上保 7 → 8 按钮的渐进扩展。

## 不做

- **不做秒精度可选 / ISO 格式可选**：单一时间格式让用户心智 + 复用 marker 都简单。重精度需求的用户自己手动改一下。
- **不写 "「📅 17:51」" 包装文本**：纯时间戳让用户自己决定上下文（行内 / 行首 / 标签前缀 / marker）—— 任何固定包装都是 over-engineering。
- **不动 PanelMemory 编辑器**：本入口只在 detail.md 编辑器（与既有 6 + 1 个 toolbar 按钮同位）。memory 编辑场景不同，加按钮会污染界面。
- **不写测试**：纯 DOM textarea + Date 操作，逻辑 20 行；vitest jsdom 下 textarea selection / requestAnimationFrame / `Date` 时钟桩接复杂度远大于价值。视觉验证即可。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.21s
- 改动 ~30 行（helper 25 + 按钮 7 + comment）；既有 7 个 toolbar 按钮 / textarea 路径不变。

## TODO 状态

empty —— 下次启动 TODO 流程会进入 auto-propose 分支提新需求。

## 后续

- 按钮上叠 modifier key 快捷：⌥ + click 改 ISO 格式 / ⇧ + click 加 `[once: ...]` 包装。复杂度有限，按需引入。
- 自动后缀 " — " 让用户"写时间 + dash + 内容"的常见 pattern 一步到位。但这增加默认行为复杂度，先观察用户实际使用模式再决定。
- 配套：写完进度后按钮变 `📅 + 5 分钟前打过卡`，提示"最近一次时间戳"避免重复打卡 —— 但实现要追踪文本内最近时间戳，权衡较大。
