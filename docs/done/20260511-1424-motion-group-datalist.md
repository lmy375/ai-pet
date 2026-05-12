# motion_mapping 显模型可用 group datalist

## 需求

motion_mapping 输入框是纯 text，用户得知道当前 model 实际有哪些 group 名才
能填对。装 miku 模型时是 Tap/Flick/Flick3/Idle，但用户换了别的 Live2D 模型就
盲填 → 试不响应 → 翻 README 或 model3.json。从 model3.json 自动抽 group 名作
datalist 建议，UX 直接。

## 实现

不需要后端 —— Live2D model3.json 是 public 文件，frontend `fetch(modelPath)`
直接能读。失败兜底空数组（仅没建议，输入仍可手填）。

`src/components/panel/PanelSettings.tsx`：

- 新 state `availableMotionGroups: string[]`
- useEffect on `form.live_2d_model_path`：fetch JSON → 解析
  `FileReferences.Motions` keys → setAvailableMotionGroups。失败 / 空时设
  空数组（不弹错 banner，避免侵入；用户继续手填即可）
- 用 `cancelled` flag 防 path 快变切换时旧 fetch 覆盖新结果

UI：

- "Motion 映射"段落 hint 行追加蓝字"从 model3.json 检测到可用 group: Tap / Flick / ..."
- 共享 `<datalist id="motion-group-presets">` 渲在 mapping 输入组之前
- 4 个 motion 映射 input 都加 `list="motion-group-presets"`，复用同一份建议

datalist 永远渲染（即便空数组没有 option），让 input 的 list 引用始终有效。

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 加载 settings → fetch /models/miku/miku.model3.json → 检测出 Tap/Flick/Flick3/Idle
  - hint 蓝字显"可用 group：Tap / Flick / Flick3 / Idle"
  - 点 motion mapping input → datalist 浮 4 个选项
  - 选项与原 hint 文字一致
  - 改 live_2d_model_path 指向不存在文件 → fetch 失败 → hint 不显（空数组隐
    藏分支） + datalist 空但不报错
  - 换其它 Live2D 模型 → 检测出新的 group 名

## 不在本轮范围

- 没做"motion 缩略动画预览"：要在 settings 里渲染 mini Live2D 加载整个 pixi
  + cubism runtime；过重
- 没把"检测失败"显式提示：用户知道自己 model 路径填对了就行；显错反而打扰
- 没在前端做 Live2D 模型路径校验：保持原 placeholder 路径示例已经够明确

## TODO 池剩余

- ChatMini 桌面气泡可拖动（最后一条）
