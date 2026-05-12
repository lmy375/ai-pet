# 设置页 image_size 字段

## 需求

backend `image_generate` 之前硬编 `size: "1024x1024"`，用户想要竖图（手机壁纸 1024x1792）/ 横图（桌面壁纸 1792x1024）只能去改源码或 fork。和 image_model 同 section 加个尺寸字段就解决。

## 实现

### 后端

- `commands/settings.rs`：`AppSettings` 加 `image_size: String`，默认 `"1024x1024"`，serde default fallback 同名 fn
- `commands/image.rs`：从 `settings.image_size` 读，trim 后空 → fallback `"1024x1024"`，再传给 ImageRequest.size

```rust
let size = if settings.image_size.trim().is_empty() {
    "1024x1024"
} else {
    settings.image_size.trim()
};
```

后端守门一道防 raw config 编辑写空值后整体 400。

### 前端

- `useSettings.ts`：`AppSettings` interface 加 `image_size: string`，DEFAULT_SETTINGS 给 `"1024x1024"`
- `PanelSettings.tsx`：
  - form 初始 state 加 `image_size: ""`
  - 加 `IMAGE_SIZE_PRESETS = ["1024x1024","1024x1792","1792x1024"]`（方 / 竖 / 横）
  - LLM 配置 section 里 image_model 字段下方加 `<input list=image-size-presets>` 同款 datalist
  - tooltip 注解 `dall-e-3 支持的三档 / dall-e-2 还支持 256+512 / SD/flux 一般 512-1024 / 空串后端 fallback 1024x1024`

## 验证

- `npx tsc --noEmit` clean
- `cargo check` clean
- 修改后 `/image dragon` 用新 size 调起；改空也不会 500，走默认

## 不在本轮范围

- 没在用户回声 / pending / 成功行里把 size 也显出来 —— size 通常一次配置长期复用，不像 -n 是单次 flag，UI 噪音大于价值
- 设置面板没加"测试一张图"按钮（previously considered for image_model task）—— 触发后台调一次成本不可忽略，留给用户自己用 /image 测
