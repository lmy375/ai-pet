# Panel 各页 UI 视觉抛光 —— 第一波

## 背景

`docs/TODO.md` 用户确认：「UI 太太太丑了（主要是 Panel 的各个页），修改的美观一些。」

Panel 窗口的各 tab（设置 / 聊天 / 任务 / 记忆 / 人格）虽然已有 CSS var token 系统、shared primitives 与 inline 样式协作，但整体观感偏「素白板 admin 后台」：

- 大片平面色块、缺乏视觉层次
- section 卡片彼此孤立、不像同一份设计语言
- 标题与背景缺乏温度感，主题色仅在 active tab 出现
- PanelMemory 的 `s.section` 是裸 `marginBottom` 没有 card chrome，节奏与 PanelSettings / PanelPersona 不齐

不做大规模 inline 样式重写（PanelTasks 6992 行、PanelChat 5519 行，逐行迁移收益不匹配代价），而是在「全局基底 + shared primitives + 各页 section 风格统一」三层抛光。

## 改动

### `src/PanelApp.tsx` — 全局基底升级

注入到 panel root 的 `<style>` block 扩充：

1. **环境光（`.pet-panel-root::before`）**：fixed 全屏，左上 / 右下两团 5-7% accent 径向光晕，给整面板一点「温度」而非纯白/纯深。`z-index: 0` + `.pet-panel-root > * { z-index: 1 }` 保内容永远在光晕之上。
2. **Tab bar 玻璃感（`.pet-panel-tabbar`）**：accent 4% + card 主体的 180° 渐变 + `backdrop-filter: saturate(140%)`，让 tab bar 与下方内容区有微妙层次。
3. **Active tab 指示器升级**：`box-shadow` 从 8px 50% 加到 10px 60%，halo 暖度更明显。
4. **新 utility class**：
   - `.pet-card-elev` — 进阶 card：顶端 1px accent 渐变 hairline + 180° 顶部 3% accent + `var(--pet-shadow-sm)`，hover → `var(--pet-shadow-md)`
   - `.pet-chip` — 圆角 chip 统一节奏（11px、padding 2x8、letter-spacing 0.2）
   - `.pet-divider` — 渐变 hairline（透明 → border → 透明），比 1px solid 更克制
   - `.pet-row-hover` — 列表行 hover 共用：4% accent 底 + 35% accent 边色 + sm shadow，跨 panel 一致
5. **`prefers-reduced-motion`** 防护：所有 transition / animation 退化。

### `src/components/panel/SectionTitle.tsx` — 标题抛光

- 字号 `text.md (13)` → `text.lg (14)`，让 section 标题更显眼
- accent 圆点从纯色升级为 radial highlight 渐变（左上 70% 白混 → accent），加 8px halo glow
- `divider` 模式改用渐变 hairline（与 PanelMemory `s.sectionTitle` 同语言）而非 1px solid

### `src/components/panel/EmptyState.tsx` — 空态抛光

icon 从「裸 emoji 字符」升级为「accent halo 圆形容器」：

- 圆形径向 accent 10% 光晕 + 14% accent 边
- compact 44px / 默认 64px
- icon 字号略缩，让 halo 成为主视觉锚

### `src/components/panel/Modal.tsx` — 弹窗抛光

- backdrop 加 `backdrop-filter: blur(6px) saturate(120%)`，背景内容隐去更彻底，modal 卡片「浮」出来
- card 顶部 accent 4% 渐变 + accent 8% 边 + radius 14（比 12 略更圆润）
- padding 18x22 → 20x24，呼吸感更舒展

### `src/components/panel/PanelMemory.tsx` — `s.section` 升级为 card

之前 `s.section` 只有 `marginBottom: 20`，是个裸 div，与 PanelSettings / PanelPersona 的 card 节奏对比时显得"破"。改成：

- 顶部 3% accent 渐变 + card 主体
- 1px border + 12 radius + sm shadow
- 16x18 内边距
- container padding 16 → 22 给整页一致呼吸节奏
- `s.sectionTitle` 改用渐变 hairline 取代 1px solid border-bottom
- `s.btnPrimary` 加 accent 28% 投影 + letter-spacing 0.2 + radius 6 → 8

### `src/components/panel/PanelSettings.tsx` — `sectionStyle` / `containerStyle`

- `sectionStyle.background` 改成顶部 3% accent 渐变（与其它 panel section 同语言）
- `containerStyle.padding` 从 `20px 24px` → `22px 24px 24px`

### `src/components/panel/PanelPersona.tsx` — `Section` 内组件

- 卡片 `background` 改成顶部 3% accent 渐变
- accent 圆点升级为 radial highlight + glow（与 SectionTitle 同语言）

### `src/components/panel/PanelTasks.tsx` — `s.container` / `s.sectionTitle` / `s.formCard`

- container padding 16 → 22
- sectionTitle 改用渐变 hairline，字号 13.5 → 14
- formCard 升级到与其它 panel 同语言的 elevated card（顶部 3% accent 渐变 + sm shadow + 12 radius）

## 不做

- **不重写 PanelChat / PanelTasks / PanelSettings / PanelMemory / PanelPersona 的 inline 样式**。这些文件 2k-7k 行，逐行迁移到 CSS class 的收益与代价不匹配；本次仅在「顶层容器 / section card / shared primitive」三处抛光，效果已能传到内部所有子元素。
- **不动 DebugApp**。用户明确说"主要是 Panel"，DebugApp 是独立窗口；其内部 PanelDebug / PanelDebugStats 视觉与 Panel 风格本已差异化。
- **不动 PanelChat 内部 message bubble 样式**。聊天 bubble 已有自己的视觉范式（user / assistant / tool 分色），改风格会冲击聊天的可识别度。
- **不动 DebugApp `.pet-debug-tab`**。它已有自己的指示器；如要后续统一可作为独立 follow-up。
- **不引入新依赖**。所有抛光走 CSS var + color-mix（已是项目主路径），不引 Tailwind / CSS-in-JS lib。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.28s（与改动前一致）
- 改动只影响 className / inline style 字符串，逻辑层 0 变动

## 后续可继续抛光（不在本次范围）

- 按钮 hierarchy class（`.pet-btn-primary` / `.pet-btn-ghost`）让各 panel 内裸 `<button>` 取代各自 `btn` 对象
- PanelChat message bubble 圆角 / shadow / 选区色微调
- 任务行 / 记忆行 hover 状态接入 `.pet-row-hover`
- DebugApp tab bar 与 PanelApp `.pet-panel-tabbar` 统一玻璃感
