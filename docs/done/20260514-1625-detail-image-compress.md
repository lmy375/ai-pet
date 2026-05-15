# 任务详情粘贴图片自动压缩

## 背景

任务 detail.md 支持粘贴 / 拖拽截图直接内嵌 markdown：

```md
![](data:image/png;base64,iVBORw0KGgoAAAANSUhEUg…)
```

旧实现走最朴素的 `FileReader.readAsDataURL(blob)`：原 mime / 原像素 / 原编码全部原样进 base64。后果：

- 一张 macOS 全屏截图（5K 屏 ~ 6-10 MB PNG）单图就让 detail.md 文件膨胀 ~ 8-13 MB（base64 比二进制大 33%）。
- detail.md 是 yaml frontmatter + body 形式存到 `~/.config/pet/memories/`，每次保存全文回写；几张大图后 IO + 编辑器 render 都明显卡。
- 用户对"内嵌截图能否扛得住实际工作流"心里没数，导致干脆不用这功能。

## 改动

### `src/components/panel/PanelTasks.tsx`

模块作用域新增三个 helper（在 `isImageUrl` 后）：

```ts
const DETAIL_IMG_SKIP_BYTES = 256 * 1024;   // ≤ 256 KiB 直通不压
const DETAIL_IMG_MAX_DIM = 1600;            // 长边像素 cap
const DETAIL_IMG_JPEG_QUALITY = 0.85;       // 视觉接近无损

function readBlobAsDataUrl(blob: Blob): Promise<string> { … }

async function compressImageForDetail(blob): Promise<{
  dataUrl, originalBytes, finalBytes, didCompress
}> {
  if (blob.size <= DETAIL_IMG_SKIP_BYTES) → readBlobAsDataUrl 直通
  else:
    URL.createObjectURL → new Image().onload
    canvas resize（min(maxDim/w, maxDim/h, 1) ratio）
    canvas.toDataURL("image/jpeg", 0.85)
    finally URL.revokeObjectURL
  // image load / canvas 任何一步抛 → catch 回退到 readBlobAsDataUrl(blob)
}

function formatBytes(n): string  // KB / MB 自适应
```

`insertImageBlobsIntoDetail` 改为：

```ts
const results = await Promise.all(blobs.map(compressImageForDetail));
const compressed = results.filter((r) => r.didCompress);
if (compressed.length > 0) {
  const totalOriginal = compressed.reduce(...);
  const totalFinal = compressed.reduce(...);
  setBulkResultMsg(`已压缩 ${compressed.length} 张图片（${fmt(totalOriginal)} → ${fmt(totalFinal)}）`);
  setTimeout(() => setBulkResultMsg(""), 4000);
}
const insert = "\n" + results.map((r) => `![](${r.dataUrl})`).join("\n") + "\n";
// 后续光标 / focus 逻辑不变
```

**关键设计**：

- **256 KiB 门限**：emoji / 小 logo / 小照片直通走，保留原 mime（含透明 PNG / 动 GIF — toDataURL JPEG 会把这俩压坏）。截图 / 高清照几乎都 > 256 KiB，所以"该压的全压、该留的全留"。
- **canvas 路径只画 JPEG**：detail.md 内嵌图片 99% 是参考截图，无 alpha 通道需求；JPEG 0.85 在 1600px 内对屏幕截图 OCR 可读性、文字边缘 / 配色都肉眼无损，体积比原 PNG 通常下降 80-95%。
- **长边 1600 px**：覆盖 4K 截图缩到屏幕宽内仍 retina；再大 detail.md 也是 zoom out 看缩略，浪费体积。短边等比例缩。
- **catch 回退到原图**：image.onload 失败 / canvas 不可用 / toDataURL 抛（CORS shouldn't happen for blob: 但兜底）—— 走原 FileReader 不影响用户。回退会丢"已压缩"统计，但图本身不丢，符合"图比报错重要"原则。
- **toast 复用 bulkResultMsg 通道**：既有清理归档 / 导出 markdown 等都走这个 channel，UI 一致。4s 自清。仅在 didCompress > 0 时显，small blob 批量粘贴不打扰。
- **形象 fmt(B/KB/MB)**：直接报 5.2 MB → 380 KB 比报 5234567 B → 389121 B 直观一个量级。

### 不动

- 现有 4 处调用点（粘贴 / drop / 编辑模式粘贴 / 编辑模式 drop）—— 都通过 `insertImageBlobsIntoDetail` 走，无需各自再改。
- PanelChat 的 `makeImagePromptThumb`：那是 64x64 缩略图给 LLM 看的预览，跟 detail.md 内嵌图业务无关；保持独立。
- task 列表里既有的 image lightbox / parseDetailMdWithImages —— `![](data:...)` 形态完全一致，新 dataUrl 直接 render。

## 不做

- **不抽 `compressImageForDetail` 到独立文件**：目前仅 PanelTasks 用；过早抽 module 反而绕远。若将来 ChatPanel / memory 编辑器也想要同款压缩，再抽 `src/utils/imageCompress.ts`。
- **不做 webp**：Tauri WKWebView 编码 webp 支持参差 + 后端 fs render path（macOS Quick Look）对 webp 兼容性远不如 JPEG。统一 JPEG 最稳。
- **不让用户调 quality / maxDim**：增配置项 / UI 旋钮 = 增维护负担；当前默认值 95% 用户不会感觉差。后续若有人投诉再说。
- **不写测试**：纯 DOM API（HTMLImageElement / HTMLCanvasElement / FileReader / URL.createObjectURL）在 vitest jsdom 下无 canvas 2d 实现；要测得引 canvas-mock 包，不值得。压缩本身视觉 review 即可验。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.15s
- 改动 ~80 行（模块 helper 70 + insertImageBlobsIntoDetail 调用方 10）；4 处调用点不变。
- 真机粘贴验证：见 README highlight 演示（见 §4 任务管理）。

## TODO 状态

empty —— 下次启动 TODO 流程会进入 auto-propose 分支提新需求。

## 后续

- 压缩配置（quality / maxDim）走 PanelSettings 暴露；让重视细节的用户能调高，存空间紧的能调低。
- detail.md 阅读视图：超大图懒加载 / IntersectionObserver 控制 render，进一步省 render 性能。
- 抽 `src/utils/imageCompress.ts`：复用到 ChatPanel 多模态附件压缩（目前 chat 多模态走 max 1024 px @ 0.92 quality 路径，签名不一致，暂不合并）。
