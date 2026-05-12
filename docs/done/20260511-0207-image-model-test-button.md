# 设置页"测试 image_model"按钮

## 需求

image generation 链路依赖四个东西配齐：api_base、api_key、image_model、image_size。任何一个错都会让 `/image` 在用户敲第一句的时候才暴露 → 用户失望"你这功能坏了"。设置页一键发条小图测试，秒级失败 / 成功反馈，省一次 /image 试错。

## 实现

### 设计取舍

- **不**用单独 backend 测试命令 —— 直接复用 `image_generate(prompt: "test cat", n: 1)`。测试就是要验真实路径，越接近 /image 越好。
- **不**改 size 到 256x256 —— dall-e-3 不支持小尺寸；让测试走用户实际配置的 size，反映真情况。慢就慢（~30s），但失败原因和 /image 失败原因完全一样。
- 测试用的是**已保存**的 settings，不是输入框正在编辑的字段。tooltip 提醒"改完先点保存"。

### UI

`src/components/panel/PanelSettings.tsx`：

- 三个新 state：`imageTesting: boolean`、`imageTestResult: { url, elapsedMs } | null`、`imageTestError: string`
- `handleTestImage`：performance.now 计时 → invoke image_generate → 写 result / error → finally 关 testing 旗
- 在 image_size datalist 下方加一行：
  - 🧪 测试生图按钮（accent 色，testing 时灰 + 文案"测试中…"，image_model 空时 disabled）
  - 旁边显结果文案：成功 → 绿色 `✓ 成功，耗时 X.X 秒`；失败 → 红色 `✗ <错误>`
  - 成功时下方再渲一张 200×200 max 缩略图，让用户**看到**确实出图了，不光"接口返回了"

绿/红用 `var(--pet-tint-green-fg)` / `var(--pet-tint-red-fg)`，主题切换跟随。

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 配 dall-e-3 + valid key → 点测试 → ~30s 后绿色"✓ 成功，耗时 28.3 秒" + 小猫缩略图
  - 配错的 model 名 → 红色 `✗ images API 返回 404：...`
  - 配空 image_model → 按钮 disabled 灰态，hover tooltip 提示先设
  - 改了输入框但没保存就测 → 走的是上次保存的值（tooltip 解释这一点）

## 不在本轮范围

- 没做"测试 multimodal model"按钮（验 chat 端能不能识图）—— 多模态识别得拉一张图 + 验文本回复，UI 比生图测复杂，先观察用户反馈
