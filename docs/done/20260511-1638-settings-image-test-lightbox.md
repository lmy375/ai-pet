# image_model 测试缩略图加 lightbox

## 需求

设置页测试生图成功后只显 200px 小缩略图。要看清楚效果得改设置走 `/image`
路径。让缩略图 click 弹 lightbox 大图（复用既有 ImageLightbox：📋 复制 / 💾
另存为 / Esc 关）。

## 实现

`src/components/panel/PanelSettings.tsx`：

- import `ImageLightbox`
- 新 state `settingsLightboxSrc: string | null`，命名带 settings 前缀以让将
  来加其它图（如 motion preview）可复用同 state，不必再开
- 测试缩略图 `<img>` 加 `onClick={() => setSettingsLightboxSrc(imageTestResult.url)}`
  + `cursor: zoom-in` + tooltip"点击放大查看 / 复制 / 另存为"
- 根 `</div>` 前挂一次 `<ImageLightbox src={settingsLightboxSrc} onClose={...} />`

零额外组件 —— ImageLightbox 已经做完了 portal / Esc / 复制 / 下载，
直接复用。

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 点 🧪 测试生图 → 成功 → 200px 缩略图 → click 弹全屏 lightbox
  - Esc / 点暗背景关 lightbox
  - lightbox 顶部"📋 复制 / 💾 另存为"按钮正常工作
  - 切换测试再次跑后 → 新缩略图 click → 新大图

## 不在本轮范围

- 没把别处的 200px 缩略图（CopyableMessage / ChatMini 96px）改成相同 hover
  hint：那些已经通过 ImageThumb 走完整 zoom + copy 路径，无需特别处理

## TODO 池剩余

- 重启 pet 窗口加 reload 当前窗口语义
- PanelTasks 任务行右键菜单
- PanelChat 跨会话搜索 hit 高亮
- ChatMini 流式中显当前 tool 名
