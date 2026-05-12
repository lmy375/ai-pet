# 设置页加 image_model 输入框

## 需求

`/image` 命令在上一轮加好了，但 image_model 字段只能手动改 `~/.config/pet/config.yaml`。多数非 OpenAI 后端（SiliconFlow / Together / 自部署 Ollama 走 stable-diffusion）的 model 名不是默认的 `dall-e-3`，用户没法在 UI 里发现 / 切换 → /image 实际不可用。

## 实现

1. `src/hooks/useSettings.ts`：`AppSettings` 加 `image_model: string`，DEFAULT_SETTINGS 给 `"dall-e-3"`。
2. `src/components/panel/PanelSettings.tsx`：
   - form initial state 加 `image_model: ""`（首次未加载时保持空，挂载后 get_settings 会覆盖）
   - 加 `IMAGE_MODEL_PRESETS = ["dall-e-3","dall-e-2","stable-diffusion-xl","flux-1.1-pro","flux-schnell"]`
   - Model 字段下方加 Image Model `<input>` + 同款 datalist 预设
   - 输入框下面挂一行 muted 说明文案：
     - 空串：`未配置 — /image 命令会拒绝执行`
     - 非空：`/image <prompt> 会调用 ${api_base}/images/generations，model = ${image_model}`
   - 字段 label 配 `title=` tooltip 说明"image model 与 chat model 解耦"

## 验证

- `npx tsc --noEmit` clean
- 后端 `image_generate`（上一轮）已经按 `settings.image_model` 读取并在空串时返回错误："image_model 未配置"，与 UI 文案对齐。

## 不在本轮范围

- 多模态 chip（task #40）
- /image 重试按钮（#41）
- ChatMini 渲染图片（#42）
