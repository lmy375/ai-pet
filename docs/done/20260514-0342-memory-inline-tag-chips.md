# PanelMemory 行内 `#tag` chip

## 背景

PanelTasks 任务行已有 `#tag` chip 渲染（dashed tint-purple 小块）。但 PanelMemory 里的同样条目（butler_tasks 类目下的任务、user_profile 里带 `#fav` 之类的偏好笔记）**没有这种视觉**，描述里 `#weekly`、`#organize` 等 inline 标签被 `displayDesc` 原样混在正文里，缺乏可点击 / 一眼分类感。

加 inline tag chips：dedupe + cap 5，让 memory item 与 PanelTasks 风格对齐。

## 改动

### `src/components/panel/PanelMemory.tsx`

在 chip cluster 结束（line 2985 附近的 `</div>` 之前）插一段：

```tsx
{(() => {
  // 从原始 description 抽 #tag。正则与 task_queue::parse_task_tags 同语义：
  // `#` 后接 ASCII 字母数字 / `_` / `-`，长度 1-30。dedupe 保留首次出现序。
  const matches = item.description.match(/#[A-Za-z0-9_一-龥-]+/g) ?? [];
  const seen = new Set<string>();
  const tags: string[] = [];
  for (const m of matches) {
    const t = m.slice(1);
    if (t.length === 0 || t.length > 30) continue;
    if (!seen.has(t.toLowerCase())) {
      seen.add(t.toLowerCase());
      tags.push(t);
    }
  }
  if (tags.length === 0) return null;
  const shown = tags.slice(0, 5);
  const more = tags.length > 5 ? tags.length - 5 : 0;
  return (
    <>
      {shown.map((t) => (
        <span
          key={t}
          style={{
            fontSize: 10,
            padding: "1px 6px",
            borderRadius: 4,
            background: "var(--pet-tint-purple-bg)",
            color: "var(--pet-tint-purple-fg)",
            border: "1px dashed var(--pet-tint-purple-fg)",
          }}
          title={`#${t}`}
        >
          #{t}
        </span>
      ))}
      {more > 0 && (
        <span
          style={{ fontSize: 10, color: "var(--pet-color-muted)" }}
          title={`其余 ${more} 个 tag：${tags.slice(5).map((x) => `#${x}`).join(" ")}`}
        >
          +{more}
        </span>
      )}
    </>
  );
})()}
```

正则含中文 `一-龥` 让 `#组织` 这类中文 tag 也能解析（task_queue 那边目前只 ASCII —— 但前端展示层包容性 > 后端解析层；不一致仍能 render hint，用户至少看到）。

## 不做

- 不让 chip 点击触发筛选（PanelMemory 没有"按 tag 筛"功能；要做就要先加全局 tag filter，scope 大）
- 不动 displayDesc 渲染：tag 文字仍出现在正文里（chip 是 supplemental）。若去掉正文 tag，编辑模式下用户改 description 会发现编辑态与展示态不一致
- 不抽 parseTags helper 到独立模块：仅本处用；30 行内联 + 注释 = 完整可读单元

## 验收

- `npx tsc --noEmit` ✅
- 切「记忆」tab 看 butler_tasks / user_profile 等含 `#tag` 的条目 → 标题右侧 chip 串浮出
- 同 tag 重复多次仅显一次（dedupe）
- > 5 tag → 末尾 `+N` chip，hover 看全集

## 完成

- [x] PanelMemory.tsx: 行内 tag chip 渲染
- [x] `npx tsc --noEmit` 通过
- [x] 移到 docs/done/
