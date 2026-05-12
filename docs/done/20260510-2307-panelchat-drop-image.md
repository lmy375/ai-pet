# PanelChat 拖拽图片输入

## 需求

paste 已可用，但用户从访达 / 浏览器拖图过来更顺手 —— 截图复制 → 粘贴 → 发送 是 3 步，拖图直接到输入框是 1 步。

## 实现

抽出 `ingestImageBlobs(blobs: Blob[])` 把 paste 路径里的 FileReader → setPendingImages 逻辑变共享 helper（非内联在 onPaste 里）。

PanelChat 根 div 加四个 drag handler：

- `onDragEnter`：检查 `dataTransfer.types` 含 `"Files"`（DOM 拖拽 / 文本不响应），preventDefault + dragDepthRef +1 + setDragActive(true)
- `onDragOver`：同样守门 + preventDefault；`dropEffect = "copy"` 让 OS 光标显复制角标
- `onDragLeave`：dragDepthRef -1，归零时 setDragActive(false)。计数防抖 —— dragenter / leave 在子元素冒泡里也会触发，单 boolean 会闪烁
- `onDrop`：从 `dataTransfer.files` 拉 `image/*` 文件，走 `ingestImageBlobs` 同 paste 路径

dragActive 时渲染 absolute inset:0 蓝色 dashed overlay "📎 松开把图片加到输入区"，`pointerEvents: none` 让 dragOver / drop 仍走 root（overlay 不要把事件接走）。

发送守门、缩略图条、✕ 移除、多模态拒绝提示、save_session 多模态化路径全部复用 paste 路径既有逻辑 —— drop 与 paste 在 pendingImages 之后完全同质。

## 验证

- `npx tsc --noEmit` clean
- 行为：从 finder 拖一张 png 到 panel chat → 蓝色 overlay 出现 → 松开 → 图片缩略图浮在 input 上方 → 输入文本 + Enter → 多模态消息发出
- 文本拖拽（如从浏览器拖一段链接）不触发 overlay —— types 守门生效

## 不在本轮范围

剩余 TODO：due date 颜色等级 / 桌面气泡主题色 / /clear 二次确认 / /image -n。
