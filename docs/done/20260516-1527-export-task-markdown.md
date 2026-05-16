# 任务详情顶部「📤 导出整体 markdown」按钮

## 背景

TODO 上 auto-proposed 一条："任务详情顶部『📤 导出整体 markdown』按钮：拼 title + description + detail.md + history 成一段 markdown 写剪贴板，方便 share / issue / 周末复盘。"

PanelMemory 已有 📝 复制 item 整段 markdown 按钮 + bulk 工具栏有「复制为 MD」批量入口。但**单条任务**的完整 markdown 导出（含 history）当前没有专门入口 ——
- 既有 bulk「复制为 MD」需要先勾选才能用，1 条任务还要走批量流麻烦
- PanelMemory 📝 是 memory item 视角（meta + description + detail.md），不含 task history 事件
- 既有 `formatTaskAsMarkdown(t, detail)` 已 cover meta + body + detail.md + result，但缺 history

补一个 📤 按钮在 detail.md 编辑器顶部，1 步打包完整 markdown 含历史事件，让 share / issue / 周末复盘场景免拼凑。

## 改动

### `src/components/panel/PanelTasks.tsx`

在 view-mode 切换行的 📋 复制全文按钮之后插入 📤 按钮：

```tsx
<button
  type="button"
  onClick={async () => {
    setBulkResultMsg("📤 正在拼 markdown…");
    // 优先从既有 detailMap 缓存读 history；没缓存时 task_get_detail 走一次 IO
    let history: TaskDetail["history"] = [];
    let historyIoError = false;
    const cached = detailMap[t.title];
    if (cached) {
      history = cached.history;
      historyIoError = !!cached.history_io_error;
    } else if (t.detail_path) {
      try {
        const fresh = await invoke<TaskDetail>("task_get_detail", { title: t.title });
        history = fresh.history;
        historyIoError = !!fresh.history_io_error;
      } catch (e) {
        console.error("task_get_detail failed:", e);
        // history 拉不到也继续 export；至少 detail.md + meta 仍写得出。
      }
    }
    // synthetic TaskDetail 把当前 editing 值（含未保存）作 detail.md body
    const detailForFormat: TaskDetail = {
      title: t.title,
      raw_description: t.raw_description,
      detail_path: t.detail_path || "",
      detail_md: editingDetailContent,
      created_at: t.created_at,
      updated_at: t.updated_at,
      history,
      detail_md_io_error: false,
      history_io_error: historyIoError,
    };
    const lines = [formatTaskAsMarkdown(t, detailForFormat)];
    // history 段单独追加 —— formatTaskAsMarkdown 不含此段
    if (history.length > 0) {
      lines.push("", "### 历史事件", "");
      for (const ev of history) {
        const ts = ev.timestamp?.slice(0, 16).replace("T", " ") ?? "?";
        const snippet = ev.snippet?.trim() || "(空)";
        lines.push(`- \`${ts}\` ${ev.action}：${snippet}`);
      }
    }
    const md = lines.join("\n");
    try {
      await navigator.clipboard.writeText(md);
      setBulkResultMsg(`已导出整体 markdown 到剪贴板（${md.length} 字符）`);
    } catch (e) {
      setBulkResultMsg(`导出失败：${e}`);
    }
    window.setTimeout(() => setBulkResultMsg(""), 4000);
  }}
  title="导出本任务完整 markdown 到剪贴板：title + 状态 / 优先级 / 截止 / 标签 / 时间戳 + body + detail.md（含未保存）+ result + 历史事件。便于 share / issue / 周末复盘。"
>
  📤
</button>
```

## 关键设计

- **复用 `formatTaskAsMarkdown(t, detail)`**：既有 helper 已 cover meta（status / priority / due / tags / created / updated）+ body + detail.md（如果传入 detail.detail_md 非空）+ result 段。唯一缺少 history，所以本 button 在外侧追加 `### 历史事件` 段。
- **detail.md 用 `editingDetailContent`**：owner 期望"导出我现在看到的"。当前 textarea 值（含未保存改动）比磁盘版本更切合 share / issue 场景 —— 用户可能正在写 issue 描述，要把当前进度同步导出。
- **synthetic TaskDetail object**：formatter 期望 TaskDetail 形态。把当前 t + editingDetailContent + cached/fresh history 拼一个临时对象传入。avoid 改 formatter 签名 / 重写 formatter 路径。
- **history 缓存优先 + IO fallback**：detailMap[t.title] 是 panel 已加载过的 TaskDetail（hover preview / expand 都缓存到这里）。命中 → 直接用；未命中 → invoke `task_get_detail` 拉一次。这条 task 既然 owner 正在编辑 detail.md，缓存大概率命中（编辑前需要先 expand → detail 已加载）。
- **history 拉不到不阻塞**：catch + console.error + 继续走（history 段为空）。owner 至少拿到 meta + detail.md；history 缺失只是少一段，不至于失败 toast。
- **`setBulkResultMsg` 三阶段 toast**：进入 "正在拼…" → 成功 "已导出…" / 失败 "导出失败" → 4s 自清。复用既有 toast channel（与 📋 复制 detail.md 全文同），UI 一致。
- **不要 `task_save_detail` 旁路**：避免"导出附带保存" 的隐式副作用。owner 想保存先 ⌘S。

## 不做

- **不写 file save**：要写文件需 Tauri fs:write_text_file 权限 + 选目录对话框，复杂度大。剪贴板是 80% 场景的最直接落脚点（粘 issue / chat / 笔记软件 1 步）。
- **不让 owner 选择导出哪些段**：scope creep。当前一键 export 全部段；要 partial 走既有 📋 复制 detail.md / PanelMemory 📝 等其它入口。
- **不写测试**：纯 IO 拼装 + clipboard.writeText；formatTaskAsMarkdown 自身的 pure 字符串拼装可单测（已 export 出来）；本 button 是 UI 接入层视觉验证（点 📤 → toast → 粘贴验证整段 markdown）即可。
- **不接桌面 ChatMini / pet 窗口**：任务详情场景，桌面 chat 无 task 上下文。

## 验证

- `npx tsc --noEmit` ✓ 0 error（修正 TaskHistoryEvent 字段名 `timestamp` / `action`，初版误用 `ts` / `kind`）
- `npx vite build` ✓ 524 modules, 1.16s
- 改动 ~105 行（button JSX + onClick handler + history 段拼装 + 注释）；既有 `formatTaskAsMarkdown` / `handleBulkCopyAsMd` / `detailMap` / `task_get_detail` / `setBulkResultMsg` 路径完全不动。

## TODO 状态

6 条 auto-proposed 已完成 4 条，余 2 条留池：
- detail.md 大纲浮窗 active heading 高亮
- detail.md preview hover heading 复制 section 按钮

## 后续

- 多变体：⌥ + click 导出"不含 detail.md"短版（仅 meta + body + result + history，省 share 时附带 base64 图）；⇧ + click 导出"仅历史事件"。
- 后端 task_export_markdown command：让 LLM 自己也能 export 任务整段（与既有 task_get_detail 同源）。
- file save：与既有 export_sessions_snapshot 同模式 Tauri command，让 owner 把任务存档到磁盘 file。
