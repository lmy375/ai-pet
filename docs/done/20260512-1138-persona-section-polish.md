# PanelPersona Section + PanelChat 发送按钮（UI 美化 迭代 2）

## 背景

接 UI 抛光迭代 1（全局 CSS / shadow token / 按钮 hover 等）后，继续单点深入。`PanelPersona.tsx` 内复用的 `Section` 组件被多处套用，是单组件 → 全页面级影响的高 ROI 入口。

旧版：8px 圆角、纯 border、无阴影、header 仅一个 `<h3>` + `<p>`，视觉扁平。

## 改动

`PanelPersona.tsx::Section`：

1. `borderRadius` 8 → 12，更现代。
2. `padding` 16/18 → 18/20，每块多一点呼吸空间。
3. 加 `boxShadow: var(--pet-shadow-sm)`（迭代 1 注入的 token），淡浮起。
4. Header 用 flex 横排：accent **小圆点** + 标题 + 副标题，强信息分块感；圆点带 18% alpha halo 与 sky/绿/紫等 accent 色族协调。
5. `letterSpacing: 0.1` 给标题 / 副标题更精致的字距。
6. subtitle 与 title 同行（baseline 对齐）而非旧"标题下小字"，整段更紧凑。

## 不做

- 不动 `Section` 调用方 —— 全部继承新外观。
- 不做 Section hover 交互效果（只读卡片，加 hover 误导用户点击）。
- 不动其它 panel 的 inline section 容器 —— scope 控制在单组件；待下一轮统一抽。

## 验收

- 切到「人格」tab，每个 Section（陪伴时长 / 自我画像 / 心情 / ...）有：
  - 12px 圆角、淡阴影、accent 圆点 + halo
  - 标题与副标题同一基线
- 浅 / 深主题下圆点 halo 都柔和不刺眼
- `npx tsc --noEmit` 通过

## 附加：PanelChat 发送按钮

`PanelChat.tsx` 末尾 form 的 submit button：

- `padding` 10/20 → 10/22，`fontWeight` 500 → 600，`letterSpacing: 0.4`
- 新增 `boxShadow: 0 4px 14px <accent 35% alpha>` —— 让最高频交互按钮"立"在 input bar 上方
- isLoading 时阴影消失（与 disabled 视觉一致）

## 完成

- [x] PanelPersona.tsx Section 重写
- [x] PanelChat.tsx 发送按钮抛光
- [x] 移到 docs/done/
