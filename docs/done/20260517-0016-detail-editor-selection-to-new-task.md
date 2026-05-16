# detail.md 编辑器 toolbar 加 📋➕ 选区→新 task 按钮

## 背景

owner 在长 detail.md 里写计划 / 进度 / 思考时，常发现"这段子项值得独立成 task" —— 但当前必须：手动复制选区 → 关编辑器 → 滚到顶 → 打开 quickAdd → 粘 body → 写 title。6+ 步。

加一个 toolbar 按钮 "📋➕"：检测当前选区 → 自动提取首行作 title + 全段作 body → 弹 quickAdd modal 预填。让"长 detail 拆子任务"流不必离开编辑器。

## 改动

### `src/components/panel/PanelTasks.tsx`

detail toolbar 末（紧贴 📤 复制 LLM consume 段按钮之后）加 IIFE 渲 📋➕ 按钮：

```tsx
{(() => {
  const selStart = Math.min(detailCursorPos, detailSelectionEnd);
  const selEnd = Math.max(detailCursorPos, detailSelectionEnd);
  const hasSel = selEnd > selStart && selStart >= 0 && selEnd <= editingDetailContent.length;
  return (
    <button
      disabled={!hasSel}
      onClick={() => {
        if (!hasSel) return;
        const text = editingDetailContent.slice(selStart, selEnd).trim();
        if (!text) return;
        // 首行 strip 常见 markdown 前缀（list / number / quote / checklist）→ title
        const firstLine = text.split("\n")[0]
          .replace(/^\s*(?:[-*+]\s+|\d+\.\s+|>\s+|\[[ xX]?\]\s+|-\s*\[[ xX]?\]\s+)/, "")
          .slice(0, 80);
        setTitle(firstLine);
        setBody(text);
        setQuickAddOpen(true);
        setBulkResultMsg(`📋 已把选中 ${text.length} 字带到新建任务...`);
      }}
      title={hasSel ? `把选区 N 字带到「新建任务」...` : "无选区..."}
      style={{...mdToolbarBtnStyle, opacity: hasSel ? 1 : 0.4, cursor: hasSel ? "pointer" : "default"}}
    >
      📋➕
    </button>
  );
})()}
```

## 关键设计

- **复用 iter #207 加的 selection state**：`detailCursorPos` (selectionStart) + `detailSelectionEnd`；Math.min/max 兜反向 selection。
- **首行 → title heuristic**：第一行去掉常见 markdown 前缀（无序列表 `-/*/+` / 有序列表 `N.` / 引用 `>` / checklist `[ ]/[x]`）+ slice 80 char。owner 选 "- 整理 Downloads" 直接得 title "整理 Downloads"。
- **全段 → body**：保留原文内容（含换行 / markdown / checklist 等），让 owner 想要"原文复制" 即可。
- **slice 80 限 title 长度**：backend title input cap 30 char；前端 80 给余量让 modal 弹时 input 仍能显出 + owner 缩，不至于硬截。
- **disabled gate hasSel**：无选区时按钮禁用 + opacity 0.4 视觉降级 + title hint "先选一段"。
- **复用既有 quickAdd modal**：setTitle + setBody + setQuickAddOpen(true) 三步走 既有 modal 入口 — owner 看到一致 UX。
- **不删除原选区文本**：仅复制到新 task；保留原 detail.md 处不动。owner 可以保留为"上下文"或之后手动删。

## 不做

- **不写"复制选区 + 删除原文 + 在 detail 内插入 [task: <new title>] ref"** 联动：scope creep；当前先做单向拆 + owner 决定后续。
- **不绑键盘快捷**：toolbar 按钮已近 reach；hotkey 留 owner 实际反馈后再加。
- **不写测试**：纯 React state + 既有 quickAdd 入口；视觉验证（detail 编辑器选一段 → 点 📋➕ → quickAdd modal 弹 + 预填）足够。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 1.17s
- 改动 ~55 行（IIFE 计算 + 按钮 + 注释）。既有 toolbar 9 按钮 / 📤 复制按钮 / 选区感知字数 chip / quickAdd modal 完全不动。

## TODO 状态

剩 2 条留池：
- PanelTasks "+ 新建" chip 显未读 / 错误任务计数
- pet 区右键加「📡 ping LLM 测延迟」

## 后续

- 加 ⌥+click 📋➕ 选项：拆出 + 在原文位置插入 `[task: <new title>]` ref token（与 detail.md task ref chip iter #182 同语法）—— 拆 + 留 ref 一气呵成。
- "📋⚡" 按钮选区 → 立即派单（task_create 直接走，不弹 modal）—— 对超 confident owner 一键拆。
- selection 含 markdown checklist（- [ ]）时智能拆多 task：每条 checklist 行 = 一个 task，弹批量预览 modal。
