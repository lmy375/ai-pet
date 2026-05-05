# PanelMemory 全部记忆导出 markdown（Iter R98）

> 对应需求（来自 docs/TODO.md）：
> PanelMemory 全部记忆导出 markdown：「+ 委托任务」按钮旁加"导出"按钮，把全部 categories + items 拼成单 .md 文本复制到剪贴板，便于备份 / 分享 / 跨设备移植。

## 目标

PanelMemory 现在能搜索 / 编辑 / 删单条记忆，但缺一个"批量出库"动作。
用户做以下场景时只能逐个复制，繁琐：
- 跨设备迁移（旧机记忆贴到新机）
- 周复盘 / 备份归档
- 分享给朋友 / 贴 issue 排查
- 跨 LLM 转移人格画像

加 markdown 导出：把 index 全部 categories + items 按 category 分组拼成
markdown 文本一键复制到剪贴板。

## 非目标

- 不写文件 / 不弹 file save dialog —— Tauri save dialog 增加 IPC 复杂度，
  剪贴板覆盖 95% 场景，文件 → 剪贴板用户可自己 paste 进编辑器
- 不导出 detail.md 内容 —— 只导出 index 摘要（title / description /
  updated_at）。完整 detail 路径在描述里已提到，需要用户主动 follow link
- 不 export 元数据（version / 后端 hash）—— 用户不关心，markdown 越简
  洁越友好
- 不做 import —— 反向链路是另一个 feature 维度，本轮只做单向 export

## 设计

### markdown 格式

```markdown
# 宠物记忆全部导出
> 导出时间: 2026-05-09 02:30 · 共 N 条

## 管家任务 (N 条)

### 任务标题 A
> 更新于 2026-05-08 14:30

任务描述正文...

### 任务标题 B
> 更新于 2026-05-08 09:00

...

## 待办 (M 条)
...
```

要点：
- 标题层次：H1 = 文件标题；H2 = category（用 cat.label 中文名）；H3 =
  item title
- updated_at 用 blockquote 显得"附属"而不抢正文
- 正文 description 原样保留（含可能的 schedule 前缀如 `[every: 09:00]`）
- 空 category 跳过（避免 "## todo (0 条)" 占行）

### Helper

```ts
function exportMemoriesAsMarkdown(idx: MemoryIndex): string {
  const lines: string[] = [];
  const now = new Date();
  const totalItems = Object.values(idx.categories).reduce(
    (sum, c) => sum + c.items.length,
    0,
  );
  lines.push("# 宠物记忆全部导出");
  lines.push(`> 导出时间: ${now.toLocaleString()} · 共 ${totalItems} 条`);
  lines.push("");
  // 先按 CATEGORY_ORDER 列出，再追加任何未在 ORDER 中的 category
  // （后端将来可能新增 category，前端 ORDER 还没跟上时不丢数据）
  const orderedKeys = [
    ...CATEGORY_ORDER,
    ...Object.keys(idx.categories).filter((k) => !CATEGORY_ORDER.includes(k)),
  ];
  for (const catKey of orderedKeys) {
    const cat = idx.categories[catKey];
    if (!cat || cat.items.length === 0) continue;
    lines.push(`## ${cat.label} (${cat.items.length} 条)`);
    lines.push("");
    for (const item of cat.items) {
      lines.push(`### ${item.title}`);
      if (item.updated_at) {
        lines.push(`> 更新于 ${item.updated_at.slice(0, 16).replace("T", " ")}`);
      }
      lines.push("");
      lines.push(item.description);
      lines.push("");
    }
  }
  return lines.join("\n");
}
```

### Handler

```ts
const handleExportAll = async () => {
  if (!index) return;
  const md = exportMemoriesAsMarkdown(index);
  try {
    await navigator.clipboard.writeText(md);
    const totalItems = Object.values(index.categories).reduce(
      (sum, c) => sum + c.items.length,
      0,
    );
    setMessage(`已复制 ${totalItems} 条记忆 (${md.length} 字符) 到剪贴板`);
    setTimeout(() => setMessage(""), 4000);
  } catch (e: any) {
    setMessage(`导出失败: ${e}`);
  }
};
```

reuse 既有 `message` state（与新建 / 删除等其它 toast 同通道，避免新增
独立 state）。

### 按钮位置

放在「+ 委托任务」/「立即整理」按钮行末，与既有按钮 sibling：

```tsx
<button
  style={{ ...s.btn, fontWeight: 500 }}
  onClick={handleExportAll}
  disabled={!index}
  title="把全部记忆（按 category 分组）拼成单 markdown 文本复制到剪贴板。可贴到 issue / 备份 / 跨设备移植。"
>
  📋 导出
</button>
```

不强调颜色（与 + 委托任务 / 立即整理 这种 primary action 区分），让它看
起来像辅助操作。

### 测试

无单测；手测：
- index 加载完成 → 点导出 → toast "已复制 N 条记忆 (M 字符) 到剪贴板"
- 粘贴到编辑器 → markdown 渲染正常
- 空 category 不显（"## todo (0 条)" 不出现）
- index 未加载 → 按钮 disabled
- 一条空标题 / 描述 → 正常导出（不 crash）

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | helper + handler + 按钮 |
| **M2** | tsc + build |

## 复用清单

- 既有 `message` state + 自清空 timer
- 既有 `index` MemoryIndex / `CATEGORY_ORDER`
- 既有 `s.btn` 样式

## 进度日志

- 2026-05-09 03:00 — 创建本文档；准备 M1。
- 2026-05-09 03:08 — M1 完成。`exportMemoriesAsMarkdown` helper 加在文件末（formatLastUpdated 上方）；H1 + 摘要 → CATEGORY_ORDER 内 cat → ORDER 外 cat 顺序拼接，空 cat 跳过；item 用 H3 + blockquote ts + description。`handleExportAll` 复用 message state，4s 自清空；按钮放在「立即整理」之后用默认 s.btn 样式（区分于 primary action）。
- 2026-05-09 03:11 — M2 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 同 R97 build 通过 (500 modules, 934ms)。归档至 done。
