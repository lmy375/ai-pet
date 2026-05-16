# ChatMini bubble 「💾 转 task」按钮（iter #244）

## Background

Owner 在宠物窗口聊天时，AI 偶尔会输出值得长期跟进的内容（计划、提醒、灵感）。
此前只能 ⌘C 复制 → ⌘⇧Esc 切到 panel → 任务面板 → 手动 quickAdd 粘贴。
本迭代加 ChatMini 气泡内联 "💾 转 task" 按钮，一键跨窗口送到 Panel 任务面板的
quickAdd modal，预填本条消息内容。属于「pet → panel」反向 deeplink，
与 iter #189（pendingTaskFocusTitle）、iter #239（PanelTasks → PanelChat）
方向相反，复用同套 `pet-panel-deeplink` localStorage + TTL 10s 通道。

## Changes

四个文件改动：

- `src/components/ChatMini.tsx`
  - 新增 `onSaveAsTask?: (text: string) => void` prop
  - 新增 `saveAsTaskBtn`（`text && onSaveAsTask` 时渲染），按钮 emoji 💾
  - user / assistant 气泡都挂上 saveAsTaskBtn（user 在 copyBtn 之前；assistant
    在 respondBtn 后、copyBtn 前）

- `src/App.tsx`
  - 新增 `handleMiniSaveAsTask(text)` 回调：
    - `localStorage.setItem("pet-panel-deeplink", JSON.stringify({ tab:"任务",
      quickAddBody: body, ts: Date.now() }))`
    - `invoke("open_panel")`
    - `appendAssistant("💾 已把这条消息发去 Panel → 任务面板 quickAdd 预填")`
  - 传给 `<ChatMini onSaveAsTask={handleMiniSaveAsTask} />`

- `src/PanelApp.tsx`
  - deeplink 解析的 `p` 类型断言扩展 `quickAddBody?: unknown`
  - 新增 `pendingQuickAddBody` state（与 pendingChatPrefill 同模式）
  - `consumePanelDeeplink` 读 `p.quickAddBody`：
    `setPendingQuickAddBody(...)` + `setActiveTab("任务")`
  - 传给 `<PanelTasks pendingQuickAddBody onConsumePendingQuickAddBody />`

- `src/components/panel/PanelTasks.tsx`
  - PanelTasksProps 加 `pendingQuickAddBody?: string | null` /
    `onConsumePendingQuickAddBody?: () => void`
  - 函数签名解构这两个 prop
  - 新增 useEffect 消费：
    ```ts
    const titleDefault = body.split("\n")[0].replace(/^\s+/, "").slice(0, 30);
    setTitle(titleDefault);
    setBody(body);
    setQuickAddOpen(true);
    onConsumePendingQuickAddBody?.();
    ```

## Key design decisions

- **Title 默认值 = 首行前 30 字符**：quickAdd 的标题输入有 30 字符上限，
  截断而不是留空，让 owner 直接回车保存也能得到合理标题；body 保留完整文本。
- **TTL 10s + ts 字段**：复用 [[panel-app-deeplink]] 现有过期机制（PanelApp
  已校验 `ts` 是否在 10 秒内），避免 owner 关 Panel 窗后再开导致旧 deeplink
  突袭。
- **`appendAssistant` 反馈**：保留宠物窗口聊天可追溯轨迹，owner 即便没切到
  Panel 也能看到「发出去了」的提示。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.19s)

## Notes

复用既有 quickAdd modal（无需新 UI），仅是数据通道扩展。
