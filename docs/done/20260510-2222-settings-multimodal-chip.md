# 设置页 Model 旁挂"多模态"chip

## 需求

`/image` 与图片粘贴上线后，能不能用都取决于"当前 model 是不是多模态"。用户切 model 时没有任何视觉反馈，得真去聊天页粘一张图，被守门弹"当前模型不支持图片输入"才知道踩雷。设置页 Model 字段旁直接挂个 chip 能省一次试错。

## 实现

### 后端

- `commands/settings.rs`：原 `is_current_model_multimodal()` 读 saved settings.model，对设置页正在编辑的 form.model 不可用。新增 `check_multimodal_model_name(name: String) -> bool`，复用同一 `is_multimodal_model` helper（substring match against `MULTIMODAL_MARKERS`）。
- `lib.rs`：注册 `commands::settings::check_multimodal_model_name`。

### 前端

- `PanelSettings.tsx`：
  - 加 `modelMultimodal: boolean | null` state；`useEffect` 监听 `form.model`，trim 后空 → false；非空 → 250ms debounce 调 `check_multimodal_model_name`，结果写 state。失败保持 null（不显 chip，避免误导）。
  - Model `<label>` 改成 flex 行，右侧渲染 chip。绿色边 / 绿底 / 绿字 = 多模态；灰边 / bg / muted = 纯文本。
  - chip `title=` tooltip 解释判断来源（substring match 关键字）+ 边缘情况引导（不在列表的多模态模型 → 提示加到后端 markers）。

## 验证

- `npx tsc --noEmit` clean
- `cargo check` clean
- 切 model：从 `gpt-4o-mini` 改到 `gpt-3.5-turbo` → 250ms 后 chip 从绿"多模态"变灰"纯文本"
- 空字段 → chip 显"纯文本"（与守门拒绝逻辑对齐）

## 不在本轮范围

- /image 重试按钮（#41）
- ChatMini 渲染用户图片（#42）
