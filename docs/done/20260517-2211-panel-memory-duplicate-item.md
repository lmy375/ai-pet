# PanelMemory item 「📑 复制副本」按钮（iter #405）

## Background

PanelMemory item 既有 📋 复制 detail.md 全文到**剪贴板**（外发场景）
但缺「在同 category 内 clone 出一份新 item」入口 — owner 想做：

- 把 weekly_review 模板 clone 一份做 daily_review（结构相同但分类异）
- 把上次「面试候选人评估」复制一份给下个候选人填（detail.md 结构沿
  用 + 填新数据）
- 把 chat_persona/expert 复制一份调成 expert-v2（A/B 试验 persona）

只能手敲新 title + 手抄 description + 手开 detail editor 复制粘贴
（三步）。本 iter 加 📑 复制副本按钮：一键复 description + detail.md
全文 到新 item「<title> -copy[-N]」（冲突 N 自增）。

## Changes

### `src/components/panel/PanelMemory.tsx`

#### 1. busy state

```ts
const [copyingItemKey, setCopyingItemKey] = useState<string | null>(null);
```

避免双击重复创建 -copy 副本；与既有 alarmBusy / renameMemoryBusy
同模式。busy 时按钮 disabled + 显「…」字样让 owner 知道处理中。

#### 2. 按钮 JSX（紧贴既有 📋 之后）

```tsx
{(() => {
  const itemKey = `${catKey}::${item.title}`;
  const busy = copyingItemKey === itemKey;
  return (
    <button
      style={s.btn}
      disabled={busy}
      onClick={async (e) => {
        e.stopPropagation();
        setCopyingItemKey(itemKey);
        try {
          // 1. 拉 source detail.md 全文（empty / IO 错都视作"空"继续）
          let detailContent = "";
          if (item.detail_path) {
            try {
              detailContent = await invoke<string>(
                "memory_read_detail_full",
                { detailPath: item.detail_path },
              );
            } catch { detailContent = ""; }
          }
          // 2. 算 unique title：append " -copy"，冲突 -copy-2 / -copy-3 ...
          const existing = new Set(
            (index?.categories[catKey]?.items ?? []).map(i => i.title),
          );
          let candidate = `${item.title} -copy`;
          if (existing.has(candidate)) {
            let n = 2;
            while (existing.has(`${item.title} -copy-${n}`)) n++;
            candidate = `${item.title} -copy-${n}`;
          }
          // 3. memory_edit create（与既有 alarm chip / quick add 同 channel）
          await invoke("memory_edit", {
            action: "create",
            category: catKey,
            title: candidate,
            description: item.description,
            detailContent: detailContent || null,
          });
          setMessage(`📑 已复制为「${candidate}」`);
          await loadIndex();
        } catch (e) {
          setMessage(`复制副本失败：${e}`);
        } finally {
          setCopyingItemKey(null);
          setTimeout(() => setMessage(""), 3000);
        }
      }}
      title={`复制 description + detail.md 到新 item「${item.title} -copy[-N]」(冲突时自增 N) — 模板复刻 / fork 场景。`}
    >
      {busy ? "…" : "📑"}
    </button>
  );
})()}
```

设计要点：
- **emoji 📑 vs 既有 📋**：📋 已被「复制 detail.md 到剪贴板」占；
  📑（bookmark tabs）语义上更接近「复制为新 item」— 视觉区分两个
  动作避免误触
- **unique title 算法**：先试 "X -copy"；冲突再试 "X -copy-2/-3/..."。
  与 backend memory_edit("create") 内部的 filename collision 自增
  分工：backend 自增的是 .md filename（防文件冲突），本前端自增的
  是 title（防 index 内 title 冲突）。两层都需要 — index 内 title
  唯一是 PanelMemory UI 约定（双击 inline rename 用 title 作 key）
- **detail.md 读不到不阻塞**：source 没 detail_path / IO 失败时
  detailContent="" 仍创建副本（新 .md 也空）— 与 backend
  detail_content=null 路径一致；owner 后续手填 detail
- **复用既有 memory_edit + loadIndex**：与 alarm chip create / quick
  add modal 同 channel，dirty state / 历史快照 / index 刷新一致
- **toast 复用 setMessage**：与既有 📋 复制全文 / 改名 / alarm 同 UI
  feedback — 不引第二个反馈系统
- **e.stopPropagation 防 row click bubble**：与既有 alarm chip / 历
  史 picker 等同模式 — 避免点击 chip 时同时触发 item row 的 click
  handler

## Key design decisions

- **不弹「确认要复制吗」modal**：复制是创建动作不破坏，反向操作
  （删除副本）成本低；多一个 modal 反而拖慢 template fork 节奏。
  busy 状态 + toast 反馈足够让 owner 知道发生了什么
- **不在 popover 里让 owner 改 title / 改 description**：那是「克隆
  后编辑」场景 — owner 看到新副本 chip 显已 inline 改 title / 双击
  desc 改 description 用既有 UI，本按钮聚焦"一键创建"
- **不跨 category 复制**：副本仍落同 category — 想跨 category（如
  ai_insights → general）走「inline rename 拖到另一段」UX 在另一
  iter 实现，本 iter 是模板复刻 / fork 场景，95% 同 cat
- **不为单按钮引 unit test runner**：行为是既有 invoke + setState；
  build pass + 手测足够：（1）点 📑 看 toast 显「已复制为 X -copy」
  + 新 chip 出现 → （2）再点同一 source 看出 X -copy-2 → （3）多 
  按几次看 -copy-3 / -copy-4 自增 → （4）detail editor 打开 X -copy
  看 detail.md 内容与 source 一致

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.24s)
- 后端无改动 — 复用 memory_read_detail_full + memory_edit("create")
