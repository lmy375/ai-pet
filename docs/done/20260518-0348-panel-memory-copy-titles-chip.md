# PanelMemory 段标题加「📋 titles」chip（iter #449）

## Background

PanelMemory 顶部已有 `📋 单段…` 下拉导出整段（H1 段名 + H2 各 item title +
blockquote ts + description）。但 owner 有时只想"这段都有啥"扫读分享 ——
不需要 description，只要 title 列表。当前没快速入口；要么用全段导出 +
手工剪 H2 行；要么 PanelMemory 列表手抄。

本 iter 在 section header 加「📋 titles (N)」chip — 仅 title 拼 markdown
bullet list 一键复制。与既有「📋 单段…」全段 dump 互补 — 那个含描述适合
内容归档；本 chip 仅 title 适合「这段都有啥」清单分享。

## Changes

### `src/components/panel/PanelMemory.tsx`

#### 1. section header 内插 `📋 titles` 按钮

紧贴既有 `🗑 清空` 之前，与 `🔊 全部 unsilent` / `+ 新建` 等同 row：

```tsx
{cat.items.length > 0 && (
  <button
    style={{ ...s.btn, marginLeft: 4 }}
    onClick={async () => {
      const label = categoryLabels[catKey] || cat.label;
      const lines: string[] = [];
      lines.push(`# ${label} · ${cat.items.length} 条 title`);
      lines.push("");
      for (const it of cat.items) lines.push(`- ${it.title}`);
      try {
        await navigator.clipboard.writeText(lines.join("\n"));
        setMessage(`📋 已复制「${label}」${cat.items.length} 条 title`);
      } catch (e: any) {
        setMessage(`复制失败：${e}`);
      }
      setTimeout(() => setMessage(""), 3000);
    }}
    title={`仅复制「${label}」段内 ${N} 条 title 拼成 markdown bullet
            list（不含 description / detail.md）— 适合"这段都有啥"扫读
            分享。与顶部「📋 单段…」全段 + 描述 dump 互补。`}
  >
    📋 titles ({cat.items.length})
  </button>
)}
```

格式：
- header 单行 `# {label} · N 条 title` 让粘到 issue / Notion 自描述
- 空行隔开
- `- {title}` per item

## Key design decisions

- **仅 title 不含 description / detail.md**：与既有「📋 单段…」下拉差异
  化定位 — 那个全 dump 适合「我把这段内容传给同事」；本 chip 适合「这段
  里有哪些事项」 list-view share。详细内容 reader 自己 ✏️ 编辑各条看。
  Spec 的 "不含 description" 直接对齐
- **位置紧贴 🗑 清空 之前**：与 `🔊 全部 unsilent` / `🗑 清空` / `+ 新建`
  同 action row；视觉上动作族集中。`📋` icon 与顶部「📋 导出」/「📋 单
  段…」共用 — 都是「复制 / 导出」语义集，icon 一致让 owner 心智模型统
  一
- **`categoryLabels[catKey] || cat.label`**：尊重 owner 本机自定义类目
  显示名（既有 PanelMemory 改名机制），与 section 标题 / 「🗑 清空」/
  「📋 单段…」 等所有引用 label 的地方同一查找模板
- **header 单行而非裸 bullets**：让粘到任意编辑器（typora / obsidian /
  GitHub issue）渲染时一眼看「这是 PanelMemory 哪段的 N 条 title」。
  无 header 的话粘出去的纯 bullets 缺上下文。一行 header 不算
  "description" — spec 允许
- **不含 timestamp / 更新时间**：要 timestamp owner 走「📋 单段…」获详
  细 dump；本 chip 越简越好。owner 心智："titles only = clean list"
- **空 cat 不显**：`cat.items.length > 0` gate — 空段时按钮无意义（复制
  empty list 是噪音入口）。与 `🗑 清空` 同 gate
- **不写 unit test**：纯字符串拼接 + clipboard 副作用；tsc + vite build
  通过即足够。GOAL.md "meaningful tests only" 规则下不引装饰性测试

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.28s)
- 后端无改动 — 纯前端 UI 增强
- 手测：PanelMemory 任一非空 section header → 看「📋 titles (N)」chip →
  点击 → setMessage 显「📋 已复制「X」N 条 title」→ 粘到 markdown 编辑
  器看 `# X · N 条 title` + bullet list 渲染
