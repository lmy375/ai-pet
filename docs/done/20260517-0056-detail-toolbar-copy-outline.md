# detail.md 编辑器 toolbar 加 📑📋 复制大纲按钮

## 背景

iter 早期给 detail editor 加了 📑 大纲浮窗 toggle（扫 H1-H3 显锚点 + click 跳节）。但 owner 想 "把大纲作 TOC paste 到 README / chat / 思维导图根" 时仍要手抄。

加 toolbar 按钮 📑📋 一键复制全部 H1-H3 标题为缩进 markdown 列表（owner 直接 paste 到外部）。

## 改动

### `src/components/panel/PanelTasks.tsx`

detail toolbar 末（紧贴 📋➕ 选区→新 task 按钮之后）加 IIFE 📑📋 按钮：

```tsx
{(() => {
  const lines = editingDetailContent.split("\n");
  const headings: Array<{level: number; text: string}> = [];
  for (const line of lines) {
    const m = line.match(/^(#{1,3})\s+(.*)$/);
    if (m) headings.push({level: m[1].length, text: m[2].trim()});
  }
  const hasHeadings = headings.length > 0;
  return (
    <button
      disabled={!hasHeadings}
      onClick={async () => {
        if (!hasHeadings) return;
        const indent = (lv) => "  ".repeat(Math.max(0, lv - 1));
        const outline = headings.map(h => `${indent(h.level)}- ${h.text}`).join("\n");
        try {
          await navigator.clipboard.writeText(outline);
          setBulkResultMsg(`📑 已复制大纲（${headings.length} 条 heading）`);
        } catch (e) {
          setActionErr(`复制失败：${e}`);
        }
        window.setTimeout(() => setBulkResultMsg(""), 3000);
      }}
      title={hasHeadings ? `扫 H1-H3 标题（共 N 条）拼缩进 markdown 列表...` : "无 heading..."}
      style={{...mdToolbarBtnStyle, opacity: hasHeadings ? 1 : 0.4, cursor: hasHeadings ? "pointer" : "default"}}
    >
      📑📋
    </button>
  );
})()}
```

示例输出（H1 / H2 / H3 混排）：
```
- 项目背景
  - 现状
  - 痛点
    - 数据延迟
    - 用户流失
  - 目标
- 实施计划
  - 第一阶段
```

## 关键设计

- **复用既有 heading 扫描算法**：与 iter 早期 detail outline toggle 同 regex `/^(#{1,3})\s+(.*)$/`，scan 行计 level + text。两条路径产出同样 heading 数据（一条用作 inline outline panel，本 iter 用作复制）。
- **缩进 = 2 spaces × (level - 1)**：H1 = 0 indent / H2 = 2 / H3 = 4，markdown 标准缩进列表格式。
- **`- ` 前缀**：让 paste 到 markdown 渲染器自然成无序列表。owner 想换 `*` 或 `+` 自行 sed。
- **disabled gate hasHeadings**：无 H1-H3 时按钮禁用 + opacity 0.4 + tooltip hint "先加 # 标题"。
- **复用既有 setBulkResultMsg / setActionErr toast slot**：与 toolbar 其它复制按钮（📤 / 📂 / 📋➕）同 message 反馈区。
- **不写测试**：纯 string regex + indent；与 outline toggle 同算法已视觉验证；视觉验证（含 H1/H2/H3 detail 点 📑📋 → paste 看缩进格式）足够。

## 不做

- **不复制 heading id（如 `pet-detail-h{n}`）**：内部锚点对外部 paste 无意义。
- **不复制带 link 的 markdown TOC**（如 `[标题](#anchor)`）：anchor URL 需要 GitHub-style slug 计算，复杂 + slug 与 pet-detail 内部 id 不通用。本 iter 仅做"扁平大纲列表"。
- **不绑键盘快捷**：toolbar 按钮已近 reach。
- **不写测试**：见上。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 1.20s
- 改动 ~70 行（IIFE scan + 按钮 + onClick + 注释）。既有 toolbar 11 按钮 / 📑 outline panel toggle / heading id 注入路径完全不动。

## TODO 状态

剩 4 条留池：
- PanelSettings 顶 search input
- PanelMemory "今天新增" chip drill-down
- PanelTasks 拖行改 priority toast 反馈
- PanelChat session 右键菜单加「📌 钉住会话」

## 后续

- ⌥+click 📑📋 改复制"H1-H3 markdown 含 link 的 GitHub-style TOC"。
- 加 "📑✂" 按钮选当前 heading 节 + 子节作 markdown 段复制（与 paragraph-level select 互补）。
- detail.md preview 模式底加"复制本文档为 TOC"按钮 inline，让阅读态也能一键 export。
