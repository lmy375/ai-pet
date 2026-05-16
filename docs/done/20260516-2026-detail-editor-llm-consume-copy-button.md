# detail.md 编辑器 toolbar 加「📤 复制 LLM consume 段」按钮

## 背景

owner 编辑长 detail.md 时常想"把这个 task 完整上下文喂给外部 LLM（ChatGPT / Claude / Cursor / 等），让它帮我分析 / 给方案"。当前路径：
1. 关编辑（保存 / 取消 dirty）
2. 任务行右键
3. 「📑 复制为 Markdown」

3 步。toolbar 已有 9 个常用快捷按钮（B / • / 🔗 / </> / ☐ / ❝ / 📊 / 📅 / ✓ / 📂）—— 加一个 📤 按钮直接在 owner 工作的位置一键复制，省下 close-reopen。

## 改动

### `src/components/panel/PanelTasks.tsx` — detail editor toolbar 末加 📤 按钮

```tsx
<button
  type="button"
  onClick={async () => {
    setActionErr("");
    const stub: TaskDetail = {
      title: t.title,
      raw_description: t.raw_description,
      detail_path: t.detail_path ?? "",
      detail_md: editingDetailContent,  // 用当前编辑态而非磁盘版
      created_at: t.created_at,
      updated_at: t.updated_at,
      history: [],
      detail_md_io_error: false,
      history_io_error: false,
    };
    const md = formatTaskAsMarkdown(t, stub);
    try {
      await navigator.clipboard.writeText(md);
      setBulkResultMsg(`已复制「${t.title}」完整 markdown（含当前 detail.md 编辑态）`);
    } catch (e) {
      setActionErr(`复制失败：${e}`);
    }
    window.setTimeout(() => setBulkResultMsg(""), 3000);
  }}
  title="复制本任务的「LLM 喂养段」：H2 标题 + 状态/优先级/截止/标签 bullet 元数据 + body + ### 进度笔记 (含当前编辑器内容，不必先 ⌘S) + ### 产物，整段 markdown 进剪贴板..."
  style={mdToolbarBtnStyle}
>
  📤
</button>
```

紧贴既有 📂 在 Finder 按钮后；与 toolbar 既有 9 按钮共用 mdToolbarBtnStyle 风格 + gap 4px。

## 关键设计

- **detail_md 用当前编辑态 editingDetailContent 而非磁盘版**：让"边写边复制"反映最新。owner 不必先 ⌘S 才能拷贝当前进度 —— LLM 看的是真正在写的状态。
- **复用 formatTaskAsMarkdown(t, detail)**：已经测过的现行 helper —— 输出 H2 + bullets meta + body + 进度笔记 + 产物，是项目内"标准 markdown 段" 形态。不写新 formatter 避免重复 / 维护负担。
- **TaskDetail stub 全字段填**：history 空 / io_error false 等历史 / 错误字段是 formatTaskAsMarkdown 不读取的字段，但 TS 类型严格 → 全填免 cast 警告。
- **toolbar 末位放 📤**：toolbar 9 个原按钮按"插入语法 → 文件操作"展开（B/•/🔗/</>/☐/❝/📊/📅/✓ → 📂）；📤 与 📂 同属"文件操作"组，紧贴。
- **toast 反馈走 bulkResultMsg slot**：与 row 右键菜单复制类按钮（📑 / 🔗 / 💬）相同 toast 区，UX 一致。

## 不做

- **不写一种"LLM 优化"的新 markdown format**：formatTaskAsMarkdown 已经是标准 markdown 形态 (H2 + bullets + body + sub-headers)；任何 LLM 都能 parse。再发明一种"LLM consume" form 是过度设计。
- **不主动 / fetch 历史 history 段**：detail editor 主要场景是"当前进度 + 任务上下文"，history (按时执行记录) 与"喂给 LLM 让它帮思考" 关联弱；reading history 还会引一次 IO。
- **不写测试**：纯 inline UI 调用 + 复用既有 formatTaskAsMarkdown（已经验证）+ navigator.clipboard.writeText 浏览器 API。视觉验证（开任一 task 编辑 → 点 📤 → 粘到 ChatGPT 看格式）足够。
- **不替换"row 右键 → 📑 复制为 Markdown"按钮**：保留旧入口让 PanelChat / Memory 用户走列表浏览模式也能拷贝。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 1.21s
- 改动 ~55 行（toolbar 按钮 + onClick handler 40 + 注释 15）。既有 toolbar 9 按钮 / formatTaskAsMarkdown helper / 复制为 Markdown 行右键按钮 / bulkResultMsg toast slot / setActionErr 路径完全不动。

## TODO 状态

剩 1 条留池：
- butler_task `[every:]` 解析 "工作日 09:00" / "周末 10:00" 周内限定

## 后续

- 一组 ⌘+Shift+E 全局快捷绑相同行为：在编辑器里按一下立即复制本任务到剪贴板（更快）。
- 加一个"📤 复制选区为 LLM" 按钮：当 textarea 有 selection 时仅复制选中部分（带 H2 标题 + selection 段）。
- 加 "LLM 直接调用" 按钮：把 markdown 段直接通过既有 OpenAI compat client 发给当前模型 + 弹结果 modal —— 不必离开 detail.md。
