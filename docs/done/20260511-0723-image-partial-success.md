# /image -n 局部成功 / 失败混合反馈

## 需求

`/image -n 4` 当前要么全成（4 张）要么全败（0 张）—— 后端把 `n: 4` 塞一个请求
里，dall-e-3 之类只支持 n=1 的 provider 整批 400。用户既看不到"有几张成"，
也不知道为什么。改成"画了 3/4 张，第 4 张被 policy 拒了"的混合反馈。

## 设计

后端 `run_image_generate(prompt, n)`：
- n > 1 时拆 N 次串行 n=1 调用；按条收集 urls + errors
- n = 1 仍走单次调，errors 永远空
- 返回 `{ urls: Vec<String>, errors: Vec<String> }` 取代原 `Vec<String>`
- 外层 `Err` 只在 setup 失败时返（无 api_key / model / 空 prompt）；网络 / API
  拒绝都进 errors

## 实现

### 后端

`src-tauri/src/commands/image.rs`：

- 抽出 `fetch_single_image(client, url, key, model, prompt, size)` 底层单次调
- 新公开 struct `ImageGenerateResult { urls, errors }`
- `run_image_generate` 改返回 `Result<ImageGenerateResult, String>`
- 循环 `for i in 0..n_clamped`：Ok → push url，Err → push `"#{i+1}: {error}"`
- `image_generate` Tauri 命令同步改返回类型

`src-tauri/src/tools/give_image_tool.rs`：

- 消费新 `ImageGenerateResult`
- 工具结果 JSON 加 `failed` + `errors`；`ok = count > 0`（只要有一张就算 partial 成功）
- LLM 看到 `{ok:true, count:3, failed:1, errors:["#4: ..."]}`，可以自然说
  "我画了 3 张，1 张被拒了因为 X"

### 前端

`src/components/panel/PanelChat.tsx` `runImageGenerate`：

- invoke 返回类型改 `{ urls, errors }`
- 三档分支：
  - urls.length === 0 → 走原失败路径（错误说明 + 重试按钮）
  - urls.length === n → 全成；title 标 `（N 张）` 或单图就保留 -n 标签
  - 0 < urls.length < n → "画了 X/N 张" + 下方 `⚠ N-X 失败：...` 段落

`src/components/panel/PanelSettings.tsx` 测试按钮：

- 同步更新 invoke 返回类型；urls 空时显第一条 error；非空取 urls[0] 渲缩略图

## 验证

- `npx tsc --noEmit` clean
- `cargo check` clean
- 行为：
  - `/image dragon` (n=1) 成功 → 单图 + `🎨 dragon`，无变化
  - `/image -n 4 dragon` 全成 → 4 张 + `🎨 dragon（4/4 张）`
  - `/image -n 4 dragon` 部分成 → 2 张图 + `🎨 dragon（2/4 张）\n\n⚠ 2/4 失败：#3: ..., #4: ...`
  - `/image -n 4 violentscene` 全败（policy 拒）→ 错误行 + 🔄 重试
  - dall-e-3 模型 + `/image -n 4` 每次都返 1 张 → "画了 1/4 张"（剩 3 张报 same model rejected）

## 不在本轮范围

- 并行 vs 串行：当前串行（4 张 ~40s）。并行能压到 ~10s 但易触 rate-limit；先稳，
  用户反馈"太慢"再加并行配置
- 退到批量：dall-e-2 / SD 支持 n>1 的 provider 这里多花 RTT，但成本（按图付费）
  不变。如果用户配的就是 batch 友好 provider，后续加"先试一次批量，失败再拆"
  fallback

## TODO 池剩余

- PanelTasks 任务卡片拖拽调 priority
