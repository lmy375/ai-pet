# 桌面 pet 窗口图片拖入多模态

## 需求

ChatPanel 输入框 inner onDrop 已经支持拖图，但 pet 窗口的 Live2D / ChatMini
历史 / mood 显示等区域不处理 drop —— 用户把 Finder 的图片拖到宠物身上不
仅没反应，浏览器还可能默认行为把 image 当 URL 打开（白屏）。让"拖到 pet
窗口任何位置"都能进多模态发送队列。

## 实现

### `src/App.tsx`

- 新 useEffect 加 window-level `dragover` 和 `drop` 监听：
  - `dragover`：检测 `dataTransfer.types.includes("Files")` → preventDefault
    + `dropEffect = "copy"`，让 drop 能正常 fire（浏览器默认 dragover 不允
    许 drop，所以必须 preventDefault）
  - `drop`：先 `if (e.defaultPrevented) return` 守门 —— ChatPanel 内 onDrop
    已经 preventDefault，避免双触发同张图入两次
  - 否则：扫 files → 过 image/* → FileReader 转 data URL → 全部完成后
    `window.dispatchEvent(new CustomEvent("pet-pending-image-drop", { detail: urls }))`
- 用 native window 监听而非 React onDrop on App root：React onDrop 不能拦
  Live2D 内 canvas 的事件（canvas 自己消化部分）；window-level 是最外层
  兜底

### `src/components/ChatPanel.tsx`

- 新 useEffect 监听 `pet-pending-image-drop` CustomEvent：
  - 取 `event.detail` 作为 data URL 数组
  - `setPendingImages(prev => [...prev, ...urls])` 让缩略图条出现，与既有
    paste / inner drop 同一队列；用户按 Enter 时 multimodal 一起发

## 设计选择

- 不直接在 App.tsx 调 sendMessage：用户可能 drop 完想加一句"这是什么？"
  再发；推到 ChatPanel pendingImages 保留用户构造消息的窗口，UX 与 inner
  drop 一致
- 不在 App.tsx 内联渲染缩略图：让 ChatPanel 作为单一来源持有 pending
  状态；UI 紧凑且 ✕ 删图、多模态守门、清空都在那里处理
- defaultPrevented 守门避免双触发：React 的 preventDefault 内部调 native
  preventDefault，native 事件的 defaultPrevented 是真值，window-level 监
  听看得到

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 桌面把 Finder 图片拖到 Live2D 身上 → ChatPanel 输入框上方出现缩略图条
  - 拖到 ChatPanel 输入框区域 → 同样行为（inner onDrop 一路）
  - 拖非图片文件（pdf / mp4 等）→ blobs 过滤后空，无反应
  - 拖多个图（同时选 3 张）→ 3 张全进 pendingImages
  - 浏览器不再默认导航到 file:// URL（preventDefault 截住）
  - 用户继续敲文字 + Enter → multipart 文本 + 图发出

## 不在本轮范围

- 没在 Live2D canvas 上加 hover-only 落点指示器：拖入即可，无需可视引导
- 没把"拖入即发送（空文本）"作 default：保留"用户主动 Enter"路径，避免
  误拖立刻消耗 API 配额
- 没改桌面拖入文件夹的逻辑：只支持单个图片文件；文件夹枚举走 OS dialog
  更稳

## TODO 池剩余

- PanelTasks task title 双击 inline 编辑
