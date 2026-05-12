# LLM 日志单条复制为 cURL

## 需求

调试 LLM 行为时常想"把这条 request 拿到外部 ChatGPT Playground / 其它工
具重放"，看不同模型表现 / 改个参数对比。手动从日志条目里把 model / 全部
messages 拷出来手动拼 curl 很烦。给单条日志展开态加"📋 复制 cURL"按钮。

## 实现

`src/components/panel/LlmLogView.tsx`：

- 新 state `apiBaseForCurl`：默认 `https://api.openai.com/v1`，挂载时
  invoke `get_settings` 读取用户的 `api_base` 覆盖；失败兜底默认值
- 新 `buildCurlCommand(entry)`：
  - body = `{ model, messages, tools? }`；tools 只在非空时携带
  - `JSON.stringify(body, null, 2)`，body 内 `'` 用 bash 标准 `'\''` 转义
  - URL = `${api_base.trimRight("/")}/chat/completions`
  - Authorization 写死 `Bearer $OPENAI_API_KEY` —— 让用户拷出去前 export
    env，避免日志里出现真实 key（panel 截图发 issue 时不漏）
  - 不带 `stream:true` —— 外部工具 debug 时多数想看一次完整响应
- 新 `copiedCurlIdx: number | null` 1.5s 反馈
- 新 `handleCopyCurl(idx, entry)`：navigator.clipboard 写入；失败 console
- 展开 detail 区顶部加按钮 row（右对齐），active 时绿色"✓ 已复制"

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 展开任一日志条目 → 详情顶部出现 "📋 复制 cURL" 按钮
  - 点击 → 剪贴板得到完整 curl 命令；按钮变 "✓ 已复制" 绿色 1.5s
  - 粘贴到终端 → `export OPENAI_API_KEY=xxx` 后直接跑能命中后端
  - tools 非空时 body 内带 tools 数组；空 / undefined 时省略 tools 字段
  - body 内含单引号（如用户消息里有 `it's`）→ 转义为 `'\''`，shell 解析
    不会爆
  - api_base 用户改成自托管（如 `http://localhost:8080/v1`）→ curl url
    跟随改

## 不在本轮范围

- 没做"重放并对比"：仅复制；外部跑 / 比较留给用户控制
- 没做"复制为 Python / JS SDK 代码片段"：curl 是最低公因数，先到位；
  SDK 风味的复制可作单独需求
- 没在日志写入时同步加 cURL 字段：不必持久化，前端按需 derive 即可

## TODO 池剩余

- PanelChat session bar token badge 点击压缩历史
