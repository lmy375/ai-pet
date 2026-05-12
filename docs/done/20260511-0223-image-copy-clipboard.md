# 图片消息支持复制到剪贴板

## 需求

聊天历史 / give_image 工具结果里的图片想"丢到飞书 / Notion / 设计稿"目前只能截图或下载到本地再上传 —— 多绕几步。直接复制图片**二进制**到剪贴板，粘贴到任何 app 自动当图处理就行。文本"复制 markdown 引用"是另一回事，不是这条 TODO。

## 实现

### 共享 helper

`src/utils/clipboard.ts`：

```ts
export async function copyImageToClipboard(src: string): Promise<void> {
  const resp = await fetch(src);
  const blob = await resp.blob();
  await navigator.clipboard.write([
    new ClipboardItem({ [blob.type]: blob }),
  ]);
}
```

`fetch(dataUrl)` 浏览器原生支持把 data URL 转 blob，省去手写 base64→bytes 的转换。`ClipboardItem.write` 在 Tauri WebView（macOS WKWebView / Windows WebView2）都受支持。

### 共享 ImageThumb 组件

`src/components/common/ImageThumb.tsx`（新文件）：

- 包裹 `<img>` + 右上角 hover-only 浮 📋 复制按钮
- 内部 copyState 三态：idle / done / err，1.5s 自清
- 三态对应 button 背景：透明深 / 绿 / 红；icon: 📋 / ✓ / ✗
- `onOpen` 回调把"点图本体 → 打开 lightbox"职责留给 caller —— 避免每个 thumb 都 portal 一份 lightbox

### 接入

- `panelChatBits.tsx` `CopyableMessage`：images.map 内联 `<img>` 改 `<ImageThumb>`，onOpen → setLightboxSrc
- `ToolCallBlock.tsx`：give_image 等工具的 _attachments map 同上
- ChatMini 暂不接：96px 缩略图 hover 出 📋 视觉太挤；用户从 lightbox 复制即可

### Lightbox 也加 📋

`ImageLightbox.tsx`：右上角浮一个 📋 按钮（更大版，padding 6×12，玻璃磨砂 backdropFilter），同 idle/done/err 三态切色。切图时 effect 重置 copyState，避免上一张的"已复制"飘到新图。

### CSS hover 控制

PanelChat 的 `<style>` 块加：

```css
.pet-image-thumb:hover .pet-image-thumb-copy { opacity: 0.92 !important; }
.pet-image-thumb .pet-image-thumb-copy:hover { opacity: 1 !important; }
```

`!important` 反压 ImageThumb 内联的 `opacity: 0`（idle 默认隐藏），避免优先级冲突。

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 鼠标 hover 缩略图 → 右上角浮 📋
  - 点 📋 → 1.5s 闪绿 ✓ → 飞书 / Notion / Figma 粘贴 → 出图
  - 失败（非 secure context / mime 不支持）→ 红 ✗，console 有 stack
  - 点图本体（不是 📋）→ lightbox 弹出 → lightbox 内右上角也有 📋（更大版）
  - lightbox 切图（多张时）→ 上一次的 done / err 反馈重置回 idle

## 不在本轮范围

- ChatMini 96px 缩略图没 hover 📋 —— 视觉太挤；通过 lightbox 复制
- 没做"批量复制所有图"按钮 —— 单图操作场景为主，多选语义复杂；如未来用户反馈再扩
