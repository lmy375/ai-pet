# LoadingState 跨面板统一（UI 美化 迭代 17）

## 背景

接迭代 16 EmptyState 思路。"加载中"在各面板表现混乱：
- PanelMemory / PanelTasks 主入口：`padding: 20 加载中...`（纯文本）
- PanelSettings 主入口：`<div style={containerStyle}>加载中...</div>`（撑满容器）
- PanelTasks detail.md：`s.detailHint 加载中…`（小字斜体行内）
- PanelTasks 归档：`padding: 12px 0 fontSize: 12 正在加载归档…`
- PanelSettings 一处子区：`fontSize: 12 加载中…`

五种 padding / 字号 / 位置，且 100% 都是"灰字一行"，看不出"还在动"。

## 改动

### 新建 `components/panel/LoadingState.tsx`

API：
```tsx
<LoadingState />                              // 大号默认（页面级）
<LoadingState compact />                      // 中号（modal / 内嵌）
<LoadingState inline />                       // 行内细字（detail / hint 行）
<LoadingState message="..." hint="..." />     // 自定义文案
```

特征：
- 三个 accent-色脉冲圆点（CSS `pet-loading-pulse` keyframes，0/0.15s/0.3s 相位错开）—— 有"还在工作"的运动感
- prefers-reduced-motion 退化为静态 0.6 opacity（与 PanelChat thinking glyph 同思路）
- 与 EmptyState 同一种视觉语言（居中、可选 hint）

### 替换调用

1. **PanelMemory** 主入口加载 → `<LoadingState />`
2. **PanelTasks** 主入口加载 → `<LoadingState />`
3. **PanelTasks** 归档加载 → `<LoadingState message="正在加载归档…" compact />`
4. **PanelTasks** detail.md 加载 → `<LoadingState inline />`
5. **PanelSettings** 主入口加载 → `<div style={containerStyle}><LoadingState /></div>`
6. **PanelSettings** 子区加载 → `<LoadingState inline compact />`

## 不做

- 不动 button 内"处理中…/保存中…"字符串：那是动作触发后的按钮文字 swap，与"页面级 loading"语义不同。
- 不引入 skeleton（占位骨架）—— 三脉冲点已足够表达"在拉数据"，skeleton 需要 layout 知识 / 数据形态，每页定制成本高于本次收益。
- 不写测试（纯视觉）。

## 验收

- 5 处加载态视觉一致：三个 accent 脉冲点 + 文字。
- 浅 / 深主题下脉冲点都跟随 accent。
- reduced-motion 用户看到静态点不会闪。
- `npx tsc --noEmit` 通过。

## 完成

- [x] LoadingState 新组件
- [x] PanelMemory / PanelTasks / PanelSettings 共 6 处替换
- [x] 移到 docs/done/
