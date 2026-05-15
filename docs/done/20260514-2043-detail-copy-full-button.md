# detail.md 编辑器顶部「📋 复制全文」按钮

## 背景

TODO 上 auto-proposed 一条："detail.md preview 模式「📋 复制全文」按钮：preview 渲染层节点不易选中，owner 想拷整段 markdown 得切回 edit；按钮一键写剪贴板。"

PanelMemory 已经给每个 memory item 都加了 📋 (copy detail.md full) + 📝 (copy item as markdown) 双复制按钮。PanelTasks 的 detail.md 编辑器却缺 —— owner 想把整段 markdown 笔记拷出来贴到外部（chat / issue / 笔记软件 / git PR description）时：

- **edit / split 模式**：要手动 ⌘A 全选 + ⌘C 复制 textarea —— 多一步操作
- **preview 模式**：渲染层用自定义 React 节点（ImageThumb / LinkCard / checkbox），原生 selection API 在跨节点边界经常卡或不能正确包含全部文本

直接一键按钮是 owner 期望的"我现在就要拷"。

## 改动

### `src/components/panel/PanelTasks.tsx`

在 view-mode 切换按钮行（✏️ 编辑 / 🔀 分屏 / 👁 预览）之后、`● 未保存` chip 之前插入新按钮：

```tsx
{editingDetailContent.length > 0 && (
  <button
    type="button"
    onClick={async () => {
      try {
        await navigator.clipboard.writeText(editingDetailContent);
        const len = Array.from(editingDetailContent).length;
        setBulkResultMsg(`已复制 detail.md 全文（${len} 字）`);
      } catch (e) {
        setBulkResultMsg(`复制失败：${e}`);
      }
      window.setTimeout(() => setBulkResultMsg(""), 4000);
    }}
    style={{
      fontSize: 11,
      padding: "2px 8px",
      border: "1px solid var(--pet-color-border)",
      borderRadius: 4,
      background: "var(--pet-color-card)",
      color: "var(--pet-color-muted)",
      cursor: "pointer",
    }}
    title="把当前 detail.md 全文写到系统剪贴板（含未保存改动 —— textarea 当前值，不是磁盘版本）。便于贴到外部 markdown 笔记 / chat / issue。"
    aria-label="copy detail.md content to clipboard"
  >
    📋
  </button>
)}
```

## 关键设计

- **位置在 view-mode 切换按钮行**：与 edit/split/preview 三按钮 + 后续 status chip 同一 flex 行，视觉上属于"编辑器顶栏控件"集群。不沉到底部状态栏（那里是字数 / 行号 / 进度信息 chip）。
- **`editingDetailContent.length > 0` gate**：空 detail.md 没必要显按钮（复制空字符串无意义）。owner 写了第一个字符之后才出现。
- **复制 `editingDetailContent` 而非磁盘版本**：owner 可能改了一半还没保存（dirty 态），此时按钮拷"当前 textarea 值"才符合直觉。tooltip 明确说"含未保存改动 / textarea 当前值"。
- **复用 `setBulkResultMsg` 通道**：与既有"已压缩 N 张图片" / "归档导出" 等 toast 同一 channel，UI 一致。4 秒自清。
- **不需要 IPC**：与 PanelMemory 的 📋 不同 —— PanelMemory 调 `memory_read_detail_full` 是因为 memory item 的内容不在前端 state；PanelTasks 编辑态下 textarea 内容就在 React state，直接读 `editingDetailContent` 即可。
- **跨 view-mode 统一**：edit / split / preview 三模式都显按钮。edit / split 下 owner 也能用（省 ⌘A + ⌘C 两步）；preview 下 owner 几乎必须用（渲染节点选不全）。
- **不显示"复制成功"小图标 in-button**：toast 已足够反馈，按钮本身保持静态（避免按钮在多状态之间闪烁分散注意）。

## 不做

- **不做"复制 markdown without images" 选项**：detail.md 内嵌 base64 dataURL 图片很大；owner 想拷"只文字部分"是合理需求。但 markdown 标准里 `![](data:...)` 是文本一部分，分开复制反而失语义。等真有用户诉求再加 modifier key 选项（⌥ + click = 不含图）。
- **不写测试**：纯 onClick → `navigator.clipboard.writeText`，无可单测的纯函数；既有 PanelMemory 同款复制路径无单测。视觉验证（点 📋 → toast 显字符数 → 粘到外部）足够。
- **不动 PanelMemory 路径**：那边的 📋 已经稳定运行；本 iter 只补 PanelTasks 这个 gap。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.13s
- 改动 ~50 行（按钮 JSX + comment）；既有 view-mode 切换按钮 / 状态 chip 行 / `setBulkResultMsg` 路径完全不动。

## TODO 状态

6 条候选 auto-proposed 已完成 1 条，余 5 条留池：
- 任务行键盘 `p` 切 pinned
- 桌面 pet 右键菜单加「📂 打开数据目录」
- 桌面 pet Esc 收起窗口
- detail.md LinkCard 特殊域名 emoji
- 任务行 hover preview 段也走 LinkCard

## 后续

- ⌥ + click 复制时不含 `![](data:...)` 图片段（适合贴 issue / chat 不想带大附件）。
- 按钮拷成功后短暂变 ✓ 高亮 1.5s（与 PanelMemory 同模式）—— 当前只靠 toast 反馈，视觉冗余更稳。
- 给 split 模式专门加"复制左 / 复制右" 二选一：left = textarea raw / right = preview 渲染后 HTML / 富文本。复杂度大，等真有诉求再做。
