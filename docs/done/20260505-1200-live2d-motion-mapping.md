# Live2D motion 自定义映射 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> Live2D motion 映射：「设置」让用户给 Tap/Flick/Flick3/Idle 映射到自定义模型的 motion group 名，避免 model 不匹配时退化成 Idle。

## 目标

`useMoodAnimation` 现在把 LLM 写的 `[motion: Tap]` / 关键词匹配出的语义标签
（Tap / Flick / Flick3 / Idle）**直接**当 motion group 名传给 pixi-live2d-display
的 `model.motion(group, ...)`。这在 miku 默认模型上 OK，但用户换自定义模型
（group 可能叫 `Happy` / `Energetic` / `Idle1` 之类）时，4 个语义标签全部
miss → motion 调用静默失败 → 桌面宠物没动画。

本轮加一个 4 项映射设置，让用户把语义键映射到自家 model 的真实 group 名。
LLM 与后端协议不动（仍 emit Tap/Flick/Flick3/Idle 这 4 个语义键），仅在前端
触发动画那一帧做"语义键 → 实际 group 名"翻译。

## 非目标

- 不让 LLM 知道用户的自定义 group 名 —— 协议外延、prompt 占位都不必动；
  4 语义键就够分辨情绪 register。
- 不做"自动嗅探 model 的 group 列表"自动填表 —— pixi-live2d-display 拿
  motion group 列表的 API 在不同模型实现上不一致，本轮先让用户手填。
- 不写 README —— 自定义 model 是少数用户场景，不是新亮点。

## 设计

### 后端

`commands/settings.rs::AppSettings` 加：

```rust
/// Live2D motion 自定义映射：把语义键（Tap / Flick / Flick3 / Idle）映射
/// 到当前模型的实际 motion group 名。空 / 缺省 = 用语义键当 group 名（与
/// 内置 miku 模型行为一致）。键不在此 map 里时也走 fallback。
#[serde(default)]
pub motion_mapping: HashMap<String, String>,
```

Default 实现里加 `motion_mapping: HashMap::new()`。

无新 Tauri 命令——`get_settings` / `save_settings` 已经走全字段序列化，自动
包含新字段。

### 前端

#### Type / Default 扩

`hooks/useSettings.ts`：
- `AppSettings` 加 `motion_mapping: Record<string, string>`
- `DEFAULT_SETTINGS` 加 `motion_mapping: {}`

`PanelSettings.tsx`：
- form state 默认值加 `motion_mapping: {}`
- 新增一个 Section "Live2D motion 映射"：4 行，每行 label + input。label 形如
  "Tap（开心 / 活泼）"，input 的 placeholder 显示语义键名（"Tap"），用户留空
  就走 fallback。change handler 写到 `form.motion_mapping[key]`，空字符串视
  作"删除该键"避免脏数据。

#### Hook 翻译

`hooks/useMoodAnimation.ts`：
- `useMoodAnimation(modelRef, motionMapping?)` 加可选第二参
- `triggerMotion` 拿 mapping，如果 `mapping[semantic]?.trim()` 非空 → 用
  mapped 名，否则 fallback 到 semantic（保持当前行为）
- 错误捕获不变（pixi 抛异常 → console.debug + 静默）

`App.tsx`：
- 已经 `useSettings()` 拿到 settings；`useMoodAnimation(modelRef,
  settings.motion_mapping)` 传第二参

mapping 通过 useEffect 闭包捕获问题：现有 hook 的 effect 只 mount 一次，
不重订阅 listen。要让 mapping 变化即时生效，需用 ref 模式（同 hiddenRef）—
拿 mapping 存 ref，监听器回调读 `mappingRef.current`。

### 测试

后端：AppSettings 序列化含新字段（旧 settings.yaml 缺 `motion_mapping` 时
`#[serde(default)]` 会填空 map） —— 不需新单测，与现有 `tool_review_overrides`
同模式。

前端：无测试基础设施，靠 tsc + 手测。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | 后端 `motion_mapping` 字段 + Default |
| **M2** | 前端 type / DEFAULT_SETTINGS 扩 |
| **M3** | useMoodAnimation 加 mapping 参 + ref 模式 |
| **M4** | App.tsx 接线 + PanelSettings 新增 section |
| **M5** | tsc + cargo test + build + TODO 清理 + done/ |

## 复用清单

- `tool_review_overrides: HashMap<String, String>` 同模式（前向兼容、空缺省、
  inline 编辑表）
- `hiddenRef` ref-pattern 在 App.tsx 里已有，复制到 mappingRef

## 待用户裁定的开放问题

- 是否引入 `setting.live_2d_idle_motion` 这种独立字段？本轮选**否**——
  统一走 mapping 容器，未来加新语义键不必改 schema。
- 用户输入的 group 名是否实时验证存在？本轮**否**——动态读 model 的 motion
  群组实现 portability 差，错了就静默不动画（损失 = 没动；用户看到没反应自己
  会去查模型 mock）。

## 进度日志

- 2026-05-05 12:00 — 创建本文档；准备 M1。
- 2026-05-05 12:30 — 完成实现：
  - **M1**：`commands/settings.rs::AppSettings` 加 `motion_mapping: HashMap<String, String>` 字段（`#[serde(default)]` 让旧 settings.yaml 缺该字段时自动填空 map），Default 实现里加 `motion_mapping: HashMap::new()`。无新 Tauri 命令——`get_settings` / `save_settings` 已 round-trip 全字段。
  - **M2**：`hooks/useSettings.ts` 把 `AppSettings` TS 类型 + `DEFAULT_SETTINGS` 都加 `motion_mapping: Record<string, string>` / `{}`。
  - **M3**：`hooks/useMoodAnimation.ts` 加可选第二参 `motionMapping`；用 ref 模式（`mappingRef`）让 mapping 变化即时生效，无需重订阅 Tauri listen（与 App.tsx 的 `hiddenRef` 同 idiom）。新增 `resolveGroupName(semantic, mapping)` 在调 `model.motion(group, ...)` 前翻译，空值 / 未映射 fallback 到语义键本身（保持 miku 模型既有行为）。
  - **M4**：`App.tsx` 把 `settings.motion_mapping` 作为第二参传给 `useMoodAnimation`。`PanelSettings.tsx` form state 默认值加 `motion_mapping: {}`，Live2D 模型 section 内嵌 4 行映射输入：每行 monospace 语义键 + 中文情绪 hint + 输入框（placeholder 显示语义键，空输入会从 map 删键避免脏数据）。
  - **M5**：`cargo test --lib` 868/868（无新单测，纯 settings 字段扩；与 `tool_review_overrides` 同模式天然向前兼容）；`pnpm tsc --noEmit` 干净；`pnpm build` 496 modules 全过。TODO 移除条目；本文件移入 `docs/done/`。
  - **README 不更新** —— 自定义 model 是少数用户场景；既有 README "形象" 段已说"基于 Live2D 的桌面窗口"，本轮是配置补强不是新亮点。
  - **设计取舍**：4 语义键不变（LLM 协议 / 后端 prompt 占位都不动），翻译只在最末一帧前端做 —— 后端零改动 / LLM 不需重新对齐 / 用户切回 miku 模型也无需清除映射；ref-pattern 跟随 mapping 而非把 mapping 加进 useEffect deps，避免重订阅 Tauri listen 的窗口竞态。
  - **未做手动 dev 验证**：当前会话不便启动 Tauri 桌面 app；后端字段扩与 settings.yaml round-trip 由 serde 自动生效（`tool_review_overrides` 同模式已验证），前端 hook ref-pattern 与 settings UI 由 tsc + 既有 panel 模式保证。
  - **TODO 后续**：列表清空后按"如果需求列表已空，则自主开始需求分析"规则，新提 5 条候选（任务 detail.md 编辑 / 气泡 markdown / 设置搜索 / 批量改 due / TG /help）。
