# PanelMemory 📥 import .md（iter #445）

## Background

PanelMemory 既有 📋 导出（剪贴板 markdown）+ 💾 .md（本地文件下载）—
owner 可单方向把记忆导出。但反方向 import 缺失：跨设备 / 备份恢复 /
快速塞一组 idea 进 ai_insights 等场景，只能逐条点「+ 新建」打表单。

本 iter 加 📥 导入按钮 + modal — 粘 markdown 文本，按 H2 = category /
H3 = item 一次性批量导入。与既有 export schema 对偶 — 导出 / 导入形
成往返通路。

## Changes

### `src/components/panel/PanelMemory.tsx`

#### 1. `parseMemoryImport` pure 函数（紧贴 `exportMemoriesAsMarkdown` 之前）

```ts
function parseMemoryImport(text: string, index: MemoryIndex | null): ParsedImport {
  const labelToKey: Record<string, string> = {};
  if (index) {
    for (const [key, cat] of Object.entries(index.categories)) {
      labelToKey[cat.label.trim().toLowerCase()] = key;
      labelToKey[key.toLowerCase()] = key;  // 双向 resolve
    }
  }
  // 按行 walk：
  //   ## label (N 条)? → flush 旧 group + 起新 group + lookup catKey
  //   ### title → flush 旧 item + 起新 item
  //   > ... → 忽略（既有 export 的"更新于 ts"元数据）
  //   # title → 忽略（既有 export 的页眉非 cat）
  //   其余 → 累积到当前 item.description
}
```

- catKey resolve 双向：先 `cat.label` 中文显示名匹配（既有 export 写
  的），再 `cat key` 匹配（如 `ai_insights` — 让 owner 手写时不必查
  label）；case-insensitive trim
- H2 trailing `(N 条)` 自动剥（既有 export 加了 count suffix）
- catKey 无命中 → catKey=null + `unresolvedHeadings += 1`；handler 兜
  底到 `general`（任何 pet 实例都有这个 catch-all cat）
- 空 title item 跳过（防 `### ` 后空白行误产生空条目）
- 空 group（无 item）跳过（防 H2 后没 H3 留废段）

#### 2. State + handleImportRun

```ts
const [importModalOpen, setImportModalOpen] = useState(false);
const [importDraft, setImportDraft] = useState("");
const [importBusy, setImportBusy] = useState(false);
const parsedImport = useMemo(
  () => parseMemoryImport(importDraft, index),
  [importDraft, index],
);
```

`handleImportRun` 逐条 `invoke<string>("memory_edit", { action: "create", category, title, description })`：

- 同 cat 内 title 已存在（既有 `existingTitles` set） → skipped += 1；
  防覆盖既有内容（用户用 ✏️ 编辑既有 item 更明确比"导入覆写"语义不清）
- 刚 create 的 title 加入同 set → 防本批次后续重名再 skip
- 失败累积 errors[]；不阻断后续条目
- 完成后 `invoke<MemoryIndex>("memory_list", {})` 刷新 index — 立即
  显示新增条目（不必 owner 手工 reload）
- message：`📥 导入完成 — 新增 N · 跳过 M (title 已存在) · 失败 K：...`

#### 3. 工具栏 📥 按钮

紧贴 `💾 .md` 之后：

```tsx
<button style={s.btn} onClick={() => setImportModalOpen(true)} disabled={!index}
        title="粘 markdown 文本一次批量导入：H2 (## label) 为 category / H3
                (### title) 为 item / 其余作 description …">
  📥 导入
</button>
```

#### 4. Modal JSX（紧贴既有 schedule modal 之前）

`<Modal maxWidth={620}>` 内三段：

1. **header**：标题 + 说明文案（解释 H2/H3 协议 + general 兜底）
2. **textarea**（10 rows，monospace）：粘 markdown
3. **parsed preview**（dashed border 卡）：实时显将导入到哪些 cat /
   几条 item / 几条会因 title dup 跳过；unresolvedHeadings > 0 时显
   黄色 ⚠️ 标 + 「N 个未识别段 → 兜底进 general」
4. **action row**：取消 + 「确认导入 N 条」primary 按钮（disabled
   when totalItems=0 或 importBusy）

Esc / 点击 backdrop 关 modal（Modal 内置）；importBusy 时禁止关防中断
请求。

## Key design decisions

- **粘文本而非选文件**：clipboard paste 比"选文件 + 读 + 解码"少两步。
  owner 主要场景是「我刚从另一台机器复制了 markdown 出来 / 朋友给了一
  段建议清单」 — 都是 clipboard 已经在。需要走文件的极端场景（备份恢
  复 50KB 大文件）走 `💾 .md` 反向 = 第三方编辑器打开 → 复制全文 → 粘
  本 modal，多一步可接受
- **预览 + 确认 两步**：不让 paste 即写盘 — 让 owner 在「文本不对 /
  cat label 拼错」时有机会修正再确认。预览给 cat 数 + item 数 + dup
  跳过预测 + 未识别 cat 兜底警告 — 四个事前信号让 owner 决策有据
- **catKey 双向 resolve（label + key）**：既有 export 写 `## AI 洞察`
  （label）；owner 手写时可能写 `## ai_insights`（key）。两者都接 —
  减少格式碰运气
- **dup 跳过不覆盖**：title 在同 cat 已存在 → 默认 skip 不创建。语义
  "导入" 是新增；覆写是 owner 用 ✏️ 编辑既有 item 更明确入口。强行
  覆盖会让 owner "粘错文本一次毁掉既有内容" 风险过高
- **unresolved cat 兜底 general 而非 reject**：reject 会让 owner 反复
  改格式（`## todo` vs `## 待办` vs `## todo (3 条)` 等组合）；兜底进
  general 保证「数据一定进系统不丢」+ 黄色 ⚠️ 让 owner 看到「这部分
  没识别到 cat 进了 general」后可手工 🏷 改类目挪走。安全 > 严格
- **不引 SQLite 直写绕过 memory_edit**：逐条 invoke 保 ai_insights /
  butler_tasks 等 cat 的 SQLite mirror 自动同步（`memory_edit` 内部已
  做）。批量绕过会让 db 与 yaml 不同步
- **不为 import 引 history snapshot**：memory_edit("update") 走 history
  snapshot 路径（detail.md 旧版备份）；本命令只调 "create" — 新建无
  history 可言。dup 跳过策略也避免触发 update 路径
- **空 description 允许**：owner 可只导入 title（如「## 待办\n###
  写周报」），description 为空合法
- **textarea 10 rows + maxHeight 200 preview**：保 modal 不超 85vh
  （Modal 内置 cap）— 一屏内可见全部要素 + 滚动 textarea / preview
  各自独立
- **不做撤销 / 批量 undo**：导入后 owner 看见结果 → 不满意走 🗑 单条
  删 / 🗑 清 cat（既有功能）。撤销机制要存 batch op 状态 + diff，工作
  量与价值不成比例

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.23s)
- 后端无改动 — 复用既有 `memory_edit` + `memory_list` Tauri 命令
- 手测：点 📥 → modal 弹起 → 粘 「## AI 洞察\n### test1\nbody1\n\n##
  待办\n### writeup\nbody2」 → 预览显「将导入 2 条到 2 个 category」→
  点确认 → message 显「📥 导入完成 — 新增 2」→ 看 PanelMemory 列表
  立即出现两条
