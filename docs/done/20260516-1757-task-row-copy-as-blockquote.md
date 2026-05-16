# PanelTasks 任务行右键菜单加「复制为引用块」（markdown blockquote）

## 背景

任务行右键菜单已有 3 条复制选项：
- 📋 复制标题（裸字符串）
- 🔗 复制为 ref（`「title」`，与 chat `「」` ref token 同语法）
- 📑 复制为 Markdown（H2 + 完整 bullets meta + body — 长 / 重，适合归档单 task）

中间有个缺口：owner 想把某 task 作为"参考"轻量 paste 进别处（detail.md / chat / 别的 task 描述）时，没有合适形态：
- 复制标题：信息太少（不带状态 / due / 描述）
- 复制 ref token：信息也太少（只是个 chip）
- 复制 Markdown：太重（H2 起一段，把上下文 break 掉）

加一个 markdown blockquote `>` 形态填这个缺口。

## 改动

### `src/components/panel/PanelTasks.tsx`

#### 新 `formatTaskAsBlockquote` helper（与 `formatTaskAsMarkdown` 并列）

```ts
export function formatTaskAsBlockquote(t: TaskView): string {
  const STATUS_EMOJI: Record<TaskStatus, string> = {
    pending: "📋", done: "✅", error: "❌", cancelled: "🚫",
  };
  const emoji = STATUS_EMOJI[t.status] ?? "📋";
  const meta: string[] = [];
  meta.push(`P${t.priority}`);
  if (t.due) meta.push(`⏰ ${formatDue(t.due)}`);
  for (const tag of t.tags) meta.push(`#${tag}`);
  const metaStr = meta.length > 0 ? ` (${meta.join(" · ")})` : "";
  const lines: string[] = [`> ${emoji} **${t.title}**${metaStr}`];
  const body = t.body.trim();
  if (body) {
    const preview = body.length > 200 ? body.slice(0, 200) + "…" : body;
    lines.push(">");
    for (const ln of preview.split("\n")) {
      lines.push(ln.length > 0 ? `> ${ln}` : ">");
    }
  }
  return lines.join("\n");
}
```

输出示例（done task with desc 多行）：

```
> ✅ **整理 Downloads** (P3 · ⏰ 2026-05-20 18:00 · #weekend · #cleanup)
>
> 把 ~/Downloads 按文件类型分类：
> - 图片移到 ~/Pictures/Downloads/
> - PDF 移到 ~/Documents/PDFs/
> - dmg/zip 装完即删
```

#### 任务行右键菜单按钮（紧贴「📑 复制为 Markdown」之后）

```tsx
{t && (
  <button
    style={itemBtn}
    onClick={async () => {
      setTaskCtxMenu(null);
      try {
        await navigator.clipboard.writeText(formatTaskAsBlockquote(t));
        setBulkResultMsg(`已复制 "${t.title}" 为引用块`);
      } catch (e) {
        setBulkResultMsg(`复制失败：${e}`);
      }
      window.setTimeout(() => setBulkResultMsg(""), 3000);
    }}
    title="..."
  >
    💬 复制为引用块（&gt; ）
  </button>
)}
```

按钮 label 用 💬 区分（📑 完整段 vs 💬 quote 形态），tooltip 解释与既有两条「📋 标题」/「🔗 ref」差异让 owner 选对工具。

## 关键设计

- **裁剪到 200 字 + `…`**：blockquote 是"轻量 quote"，太长就该用 「📑 完整段」。200 字 ≈ 4-6 行中文，覆盖 desc 概要不嫌啰嗦。
- **每行加 `> ` 前缀，空行加裸 `>`**：markdown 渲染器要求 blockquote 连续行；空行打断 quote 区。所以 `> ` + 空内容 = 视觉空白行 + 仍在 quote 内。
- **meta 用 `·` 分隔单行 paren**：与其它界面 `·` 分隔风格一致；P? + ⏰ + #tags 都塞一行 paren 内，让 quote 第一行就是 "完整概要" 紧凑。
- **status emoji 而不是 label**：📋 / ✅ / ❌ / 🚫 比 "待办 / 已完成 / 失败 / 已取消" 字符省地方 + 视觉信号更强。
- **不带 created_at / updated_at / detail.md 段**：blockquote 是"参考引用" not "归档备份"。owner 想看时间戳 / 进度详情，开任务卡 / 用「📑 完整段」。
- **不解析 description marker（[task pri=...] 等）**：t.body 已经是 description（含 marker）；blockquote 内保留原样让 paste 后仍能被某些 parser 看懂。复杂的"剥 marker 后显示 clean desc" 留给 「📑」段处理。
- **emoji + bold title + meta 在 1 行**：让 quote 段顶 1 行就承载主信息；body 在下面可选。许多场景 (paste 进 chat) owner 不展开 body 也能 OK。
- **TaskStatus 4 种枚举完全覆盖**：`Record<TaskStatus, string>` 编译期保证未来加新状态时 tsc 提醒补 emoji，不会运行时 fallback 静默。
- **export 函数**：与 `formatTaskAsMarkdown` 同样 export，便于将来其它地方（比如 bulk export 也想出 quote 形态）复用。

## 不做

- **不写单元测试**：项目无 frontend test runner（无 vitest/jest），现行 GOAL.md "tests must pin real behavior" 要求每个测试值得维护。`formatTaskAsBlockquote` 是纯字符串拼装幂等函数，最直观的验证是手动右键 → 粘贴 markdown 渲染器看效果。
- **不接入 bulk export 批量 blockquote**：批量场景该用 `formatTaskAsMarkdown`（H2 段间分隔自然），多个 blockquote 堆一起反而模糊；保留单 task 右键场景。
- **不写 detail.md 段也走 blockquote 形态**：detail.md 内容自身可能含 markdown 结构（H/列表/代码），整段 `> ` 前缀会破坏；blockquote 只放轻量描述。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 1.17s
- 改动 ~70 行（formatTaskAsBlockquote helper 35 + 右键菜单按钮 25 + 注释 10）。既有 formatTaskAsMarkdown / 既有 3 条复制按钮 / 右键菜单其它 entries 完全不动。

## TODO 状态

剩 3 条留池：
- ChatMini 历史区双击 user/assistant 气泡内的「title」ref token 跳 PanelTasks
- 桌面 pet 右键菜单加「切 Live2D 模型」子菜单
- butler_task 描述新增 [reminderMin: N] 标记

## 后续

- ⌥+ 右键 / Shift+ 右键时改成"复制 detail.md 段"快捷分支 —— 一键复制带 detail body 的轻 quote。
- blockquote 支持插入 `[task: 标题]` ref，让粘贴后还能 click 跳回 PanelTasks（与 iter #182 detail.md task ref chip 同语法）—— "> 💬 详情见 `[task: 整理 Downloads]`"。
- 让 ⌘C 在选中任务行时自动选 blockquote 形态（结合 ⌘⇧C 走 📑 完整段）—— 单 key 路径快。
