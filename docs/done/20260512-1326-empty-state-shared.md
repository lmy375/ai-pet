# EmptyState 跨面板统一（UI 美化 迭代 16）

## 背景

之前空态散落在各面板里，节奏混乱：
- PanelTasks `s.empty`：padding 24，textAlign center，fontSize 13
- PanelChat session list：padding 12，fontSize 12
- PanelMemory：paddingLeft 4，fontSize 12，无 center
- 没 icon、无 hint 副线、`color: muted` 一刀切看着"灰扑扑"

## 改动

### 新建 `components/panel/EmptyState.tsx`

API：
```tsx
<EmptyState icon="📂" title="暂无历史会话" hint="..." compact>
  <button>清除过滤</button>
</EmptyState>
```

特征：
- 居中 flex column
- icon 32px（compact 24px），opacity 0.55 当锚点
- title fontSize 13（compact 12），color fg + opacity 0.7
- hint fontSize 12（compact 11），color muted，maxWidth 260
- `children` 底部 slot，自动 marginTop + flex wrap 居中，让 caller 挂 action 按钮（清除过滤 / 用范例预填等）

### 替换调用

1. **PanelChat** session list：
   - 空 list → `📂 暂无历史会话` + 提示
   - filter 命中 0 → `🔍 没有匹配的 session`

2. **PanelTasks** 主列表 `s.empty` → `EmptyState`
   - icon 根据状态：`🔍 / ✅ / 🎉`
   - filter 命中 0 时按钮 = 清除过滤
   - 空 queue（showFinished + 无 filter）时按钮 = 用范例预填一条

3. **PanelTasks 归档**："归档为空" → `🗃 EmptyState compact`

4. **PanelMemory** cat.items 空：旧 `paddingLeft:4 fontSize:12` 一行字 → `📭 EmptyState compact`

## 不做

- 不动 marks modal / search results / detail.md 等小提示文案，那些是"操作中临时态"，不是"空态"。
- 不动 `s.detailHint` `detailHint` 等内联斜体细字 —— 是单行 hint，与"完整空态"语义不同。
- 不写测试（纯视觉）。

## 验收

- 切到「任务」tab 全空状态：居中 ✅ icon + "还没有任何任务" + hint + "用范例预填" 按钮。
- 切到「记忆」段内空：📭 + "本段还没有条目"，节奏与其它面板一致。
- session list 空：📂 + 默认 hint。
- `npx tsc --noEmit` 通过。

## 完成

- [x] EmptyState 新组件
- [x] PanelChat / PanelTasks / PanelMemory 4 处替换
- [x] 移到 docs/done/
