# PanelMemory 搜索结果数对照（Iter R140）

> 对应需求（来自 docs/TODO.md）：
> PanelMemory 搜索结果数对照：现 "搜索结果 N" badge 只显命中数；加 "/ M" 显全局总记忆数（仿 R83 决策日志 N/M 显过滤强度），让用户感知"搜词命中率"。

## 目标

PanelMemory 搜索结果 section title "搜索结果 [3]"。用户不知道全局记忆共
多少条 → 难判断"我搜得是不是太精了 / 关键词覆盖率怎样"。

加 "/ M" 显全局总数，与决策日志 R83 N/M 模式一致。

## 非目标

- 不动 badge style（仍 muted slate-50 + slate-500 文）
- 不动 search 算法 / 后端 memory_search

## 设计

### total 计算

```ts
const totalMemoryCount = useMemo(() => {
  if (!index) return 0;
  return Object.values(index.categories).reduce(
    (sum, c) => sum + c.items.length,
    0,
  );
}, [index]);
```

复用既有 `index` state（`MemoryIndex` 含 categories + items）；R98 导出
helper 也用同款 reduce 求和。

### 渲染

```diff
 <div style={s.sectionTitle}>
-  搜索结果 <span style={s.badge}>{searchResults.length}</span>
+  搜索结果 <span style={s.badge}>
+    {searchResults.length} / {totalMemoryCount}
+  </span>
 </div>
```

### 测试

无单测；手测：
- index 有 30 条 / 搜 "a" 命中 5 → "搜索结果 [5 / 30]"
- 搜空 → searchResults 是 null，section 不显（既有逻辑）
- 搜没命中 → "搜索结果 [0 / 30]"

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | totalMemoryCount useMemo + badge 文案 |
| **M2** | tsc + build |

## 复用清单

- 既有 index / categories / items
- R98 全部记忆导出的 reduce 求和模式
- R83 决策日志 N/M 风格

## 进度日志

- 2026-05-10 21:00 — 创建本文档；准备 M1。
- 2026-05-10 21:08 — M1 完成。useMemo import 加；`totalMemoryCount` useMemo 在 descTextareaRef 旁，依赖 index reduce 求和；搜索结果 badge 文案改 `{searchResults.length} / {totalMemoryCount}`。
- 2026-05-10 21:11 — M2 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (500 modules, 982ms)。归档至 done。
