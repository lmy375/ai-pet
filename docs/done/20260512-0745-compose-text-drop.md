# PanelChat compose 拖入文本文件

## 需求

compose 区现 onDrop 只接 image/*（多模态路径），常用场景"拖一份 .md /
.txt 让宠物分析"得手动打开文件、复制粘贴。扩成 drop 文本文件即自动读
内容拼 markdown 代码块塞 textarea。

## 实现

`src/components/panel/PanelChat.tsx`：

- compose root `onDrop` 内分类 files：
  - `f.type.startsWith("image/")` → imageBlobs 走既有 ingestImageBlobs
  - 文本文件判定双路径：MIME（`text/*` / `application/json`）OR 后缀正则
    `.(md|markdown|txt|json|jsonl|csv|tsv|log|ya?ml|toml|ini|conf|env|sh|rs|py|ts|tsx|js|jsx|html|css)`
    —— 因为部分 OS 给 .md 报 `application/octet-stream`，单 MIME 不够
- 文本文件读取：
  - FileReader.readAsText 异步并行
  - 100KB 软上限：超长 slice 到前 100KB + "（已截断到前 N 字节）"提示
  - 拼 markdown code fence：```<lang>\n<content>\n``` —— lang 取扩展名，
    .md / .markdown 给空（避免 markdown 内嵌 markdown 渲染冲突）
  - 文件名作 emoji 前缀提示："📎 README.md"
- 全部读完后 `setInput(prev => prev + chunks.join(""))` 把内容追加到 textarea
  现有内容尾部（不覆盖 user 已敲的字）

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 从 Finder 拖一个 `notes.md` 到 compose 区 → textarea 末尾出现
    "📎 notes.md\n```\n<内容>\n```"
  - 同时拖 `screenshot.png + tasks.json` → 图进缩略图条，json 文本拼入
    textarea
  - 拖 200KB 的 .log → 内容截断到前 100KB + 提示
  - 拖 .pdf / .docx 等非文本 → 既不当图片也不当文本，跳过（与 drop overlay
    自动消失）
  - 已有"分析一下："在 textarea → drop 后变成"分析一下：\n\n📎 notes.md
    \n```\n...\n```"（前置文本保留）

## 不在本轮范围

- 没做 PDF / docx / pptx 等富文档解析：那要后端集成解析库（pdfminer 等），
  本轮只解 plain text 类
- 没做"拖入文件夹批量读取"：跨平台目录递归 API 在 web 端复杂；用户多文件
  时一次性多选拖入即可
- 没让用户配置 MAX_TEXT_BYTES：100KB 对 chat 文本是合理上限（再大模型也
  消化不良）；高级用户需要时改源码或后续加 settings

## TODO 池剩余

- ChatMini assistant bubble 单条"再回应"快捷
- PanelTasks "now" 标记 + 桌面 nudge
- PanelDebug 快照对比 diff
