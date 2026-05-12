# PanelChat 历史会话过滤"含图片"

## 需求

聊了一阵的 sessions 越来越多，想回看"我那次给宠物看过的截图 / 生过的图"得逐
个开 session 翻。下拉顶部加 toggle，按"items 含非空 images"筛一遍命中的
session。

## 设计

不改 SessionMeta schema（避免 index.json 迁移）。新加后端命令 `list_sessions_with_images()`
按需计算：遍历所有 session 文件，scan items 找 `images` 字段非空。50 个 session
× ~1KB 解析约 < 100ms，可接受。每次 toggle 重新算让用户刚生图的 session 立刻
能命中（不缓存避免新旧不一致）。

## 实现

### 后端

`src-tauri/src/commands/session.rs` 新加 `list_sessions_with_images() -> Vec<String>`：

```rust
for meta in &index.sessions {
    let session = load_session(meta.id.clone())?;
    let has_image = session.items.iter().any(|item| {
        item.get("images").and_then(|v| v.as_array())
            .map(|arr| !arr.is_empty()).unwrap_or(false)
    });
    if has_image { out.push(meta.id.clone()); }
}
```

`lib.rs` 注册命令。

### 前端

`src/components/panel/PanelChat.tsx`：

- 新 state `imageSessionIds: Set<string> | null`（null = filter off）+
  `imageFilterLoading: boolean`
- `toggleImageFilter` callback：null → invoke list_sessions_with_images +
  setState；非 null → 设回 null 关 filter
- session dropdown 顶端加 toggle pill（与现有搜索 / scope chip 同色族 tint-blue），
  显当前命中数、加载态、"再点关闭"tooltip
- 过滤逻辑：`reversed.filter(s => imageSessionIds.has(s.id))` 在 pinned/unpinned
  分组之前
- 空过滤结果显单独 empty message"当前过滤下没有匹配的 session（点 📷 关闭过
  滤）"，避免 dropdown 体内空白困惑

## 验证

- `npx tsc --noEmit` clean
- `cargo check` clean
- 行为：
  - dropdown 顶部"📷 含图片"chip → click → ~50ms 后变高亮 + 显命中数
  - 列表只剩含图 session（含粘贴的、含 `/image` 生的）
  - 再 click → chip 复位，全列表回来
  - 0 命中 → 单独 empty 消息提示"点 📷 关闭过滤"
  - filter on 时新发图 → 用户得手动关再开重算（不自动监听 save_session）

## 不在本轮范围

- 不缓存：每次 toggle 都重扫，简单可靠；session 数 < 100 时性能足够
- 不实时刷：用户在 filter 模式下发新图不会自动加进列表 —— 关再开能解决；
  实时联动需要 emit/listen wiring 价值不高
- 不扩到"按 tool 调用过滤"等其它维度（已经在新 TODO 池里）：先把"含图"这条
  最常用的 ship

## TODO 池清空 → 自主提案

按规则 #1，提出 5 条新需求（已写入 TODO.md）：

1. PanelChat 历史过滤"含任务派单"（含 propose_task / task_create tool 调用）
2. ChatMini streaming 中浮"Esc 取消"hint
3. config 导出 / 导入（压缩 base64 字符串）
4. /image -h help 文案
5. PanelTasks 任务行 hover 显完整 raw_description
