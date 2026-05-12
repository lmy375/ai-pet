# 任务详情编辑器粘贴 / 拖入图片

## 需求

上一轮做了 detail.md 渲染层 —— 含 markdown image 的详情会显缩略图。但用户**写**
detail.md 时还得手敲 `![](data:image/png;base64,...)` 这种链子，base64 也得手编。
让编辑器同样支持 paste / drop image，自动插 markdown 行 + base64 编码 → 看 + 写
两侧对称。

## 实现

### 单 ref 够用

只允许一条 detail 在编辑（`editingDetailTitle` 是单值 string state），编辑期同
时只有一个 textarea 渲染 → 单 `useRef<HTMLTextAreaElement>` 就够用。

### 批量插入 helper

`insertImageBlobsIntoDetail(blobs)`：

- `Promise.all` 等所有 reader.readAsDataURL 完成 → 拼成 `\n![](url1)\n![](url2)\n` 单段
- 单次 setEditingDetailContent —— 多 reader 并发改 selectionStart 容易漂移，批量提交后单次 insert 才稳
- React 重渲后 `requestAnimationFrame` 写 selectionStart 到 insert 之后；不等 rAF 直接设会被 React 渲染覆盖回去

前后各包一个 `\n` 让 markdown 段落清晰；多图各占一行。

### Textarea wiring

`onPaste`：扫 clipboardData.items，过滤 `kind === "file" && type.startsWith("image/")`，
有图就 preventDefault + 调 helper；没图（纯文本粘贴）正常默认行为。

`onDrop`：扫 dataTransfer.types 含 `"Files"` + dataTransfer.files 过 `image/*`，同算法。
不挂 dragenter/over/leave 高亮 overlay —— editor 区域已经清晰，多 overlay 反而干
扰用户看正在编辑的内容；ChatPanel / PanelChat 里有 overlay 是因为它们的"target
区"不那么直观。

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 截图 → focus detail editor → ⌘V → 一行 markdown image 自动插光标位置 → ⌘S 保存
  - 阅读模式（rendered）→ 上轮的 `parseDetailMdWithImages` 渲缩略图
  - 拖图到编辑器 → 同款插入
  - 粘普通文本：行为不变（preventDefault 只在 image 命中时生效）
  - 粘多图：单段插入，光标移到段尾

## 不在本轮范围

- 没把 detail markdown 渲缩略图的尺寸设置成可配置 —— 默认 ImageThumb 160px 适配多
  数情况；用户要更大去 lightbox
- 没在编辑模式预览（preview tab 在编辑期是字面 markdown 模式）—— 用户切到阅
  读模式才看渲染图，是符合 R117 设计的"编辑期 raw / 浏览期 rendered"
