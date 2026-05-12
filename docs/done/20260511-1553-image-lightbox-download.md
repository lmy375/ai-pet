# ChatMini 图片下载到本地

## 需求

ImageLightbox 已经能复制到剪贴板，但想把图存到本地（壁纸 / 资料）目前要在
浏览器开 data URL → 右键保存。在 lightbox 加 💾 另存为按钮一键触发。

## 设计

不引 Tauri 插件（tauri-plugin-dialog 需要额外的 Cargo dep + capabilities 配
置）。直接用 HTML5 `<a download>`：对 `data:image/*` 与 `blob:` URL 都能在
WKWebView / WebView2 中触发原生 save dialog；http(s) URL 同源场景也支持。

文件名：`pet-image-${Date.now()}.${ext}`，ext 按 `data:image/<type>` 头取
（jpeg→jpg、其它直接用 type，未知 fallback png）。

## 实现

`src/components/common/ImageLightbox.tsx`：

- 新 state `downloadState: "idle" | "done" | "err"`，1.5s 自清
- 切图 useEffect 同时 reset copy / download state
- `handleDownload`：
  - 正则抓 src MIME → 选 ext
  - 创建 `<a href={src} download={filename}>` → click → remove
  - 成功 / 失败写 downloadState
- 顶部右侧浮按钮区从单个 📋 改成 flex row：📋 复制 + 💾 另存为，gap 8

两个按钮共享视觉：1.5s 三态 idle / done / err；done 绿 / err 红 / idle 玻璃磨砂。

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - lightbox 打开 → 顶部右侧两按钮"📋 复制 / 💾 另存为"
  - 点 💾 → 触发系统 save dialog → 选位置保存 → 按钮变 ✓ 已下载（1.5s）
  - data URL `image/png` → 文件名 `pet-image-XXXXX.png`
  - data URL `image/jpeg` → `.jpg`
  - 切到下一张图 → done 状态自动复位
  - 点 backdrop 关闭 → 按钮一起消失

## 不在本轮范围

- 没在 ImageThumb 加 💾：缩略图 hover 仅 📋；用户要保存原图 → click 进
  lightbox → 💾。一致的视觉负载，缩略图区不挤
- 没做"批量打包下载多张"：用户单次复用 1-4 张为主；要 ZIP 多张去手动批量
- 没改 filename 引用 prompt 内容：data URL 不带元数据，prompt 是另一条信
  息流（assistant message content）；要做得把 lightbox 接 prompt 参数，扩
  API 收益太低
