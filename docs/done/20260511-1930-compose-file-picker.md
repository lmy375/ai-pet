# PanelChat compose 区 📎 文件选择器

## 需求

PanelChat 已经支持粘贴图片（onPaste）+ 拖图片到面板（onDrop overlay），但
没有"点按钮选文件"路径。新用户不一定知道粘贴 / 拖支持，且某些场景（如
Finder 里看的图）拖拽不顺手。加 📎 系统对话框入口，与 paste / drop 共用
ingestImageBlobs 管道。

## 实现

`src/components/panel/PanelChat.tsx`：

- 新 `composeFileInputRef: HTMLInputElement` ref
- compose form 内 textarea 前插入：
  - `<input type="file" accept="image/*" multiple display:none>`：onChange
    扫 files、过滤 `f.type.startsWith("image/")`、重置 value（让同张图能
    再选）、走 `ingestImageBlobs(blobs)` 与 paste / drop 同管道；
    全跳过时 pushLocalAssistantNote 提示
  - `<button type="button" onClick={ref.click()}>📎</button>`：disabled
    when isLoading；样式与 send 按钮同 radius、灰底 + 边框，与 textarea
    左侧并列
- 不动 paste / drop 路径 —— 📎 是补充入口

## 为什么不用 Tauri dialog plugin

`@tauri-apps/plugin-dialog` 的 `open()` 返回 OS 路径而非 File 对象；要再
调后端读文件 + base64 编码，整条链路比浏览器原生 `<input type="file">`
长（多一次 IPC + 一次 fs read）。原生 input 在 WKWebView / WebView2 都
直接给到 `File`（继承 Blob），现成 FileReader.readAsDataURL 可用。

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 点 📎 → 弹 macOS 文件选择对话框（默认只显图，但用户可切"全部文件"）
  - 选 1 / N 张图 → 缩略图条出现 + ✕ 删图按钮可用
  - 同一张图连选两次 → 都进缩略图（不是 dedup —— 用户主动选两次大概率
    是想要重复，与 paste 行为一致）
  - 选了非图（用户切到"全部"选了 pdf） → "⚠ 没选到图片（仅支持 image/*）"
  - 选完 → 发送 → 走多模态守门 + send 路径
  - streaming 中 📎 disabled（与发送按钮一致）

## 不在本轮范围

- 没做"📷 拍照"入口：依赖 `<input type="file" capture="environment">`，
  在桌面 WKWebView 不映射到摄像头，徒增干扰
- 没做"📎 旁加图片库"（最近选过的图）：增加状态成本；用户已能粘贴 + 拖
  + 现在选，三个入口够冗余
- 没做单文件大小限定：后端 chat API 守门 + image_generate 体积守门已存在；
  前端再加判定会重复

## TODO 池剩余

- PanelSettings 主题色 accent 自定义
- PanelTasks 完成任务统计小卡
- PanelMemory 一键导出 .md zip
- ChatMini ⌘F inline 搜历史消息
