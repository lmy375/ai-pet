# PanelMemory butler_history 加载窗口 n=5→20

## 背景

PanelMemory 「butler_tasks」段下面有「最近执行 (N)」区，渲染 `butlerHistory` 数组。代码注释 R95 提到："> 5 条时默认折叠到最新 5 条，加 '展开全部' 按钮"。

但 `loadButlerHistory()` 只拉 `n: 5` —— 永远只有 5 条可显，fold logic 的 `> threshold` 永远不成立，"展开全部" 按钮永远不出现。死代码。

## 改动

`src/components/panel/PanelMemory.tsx::loadButlerHistory`：

```diff
- const lines = await invoke<string[]>("get_butler_history", { n: 5 });
+ const lines = await invoke<string[]>("get_butler_history", { n: 20 });
```

新增 inline 注释解释为何 bump 到 20。

## 不做

- 不动 fold threshold（5 仍合理：默认折到最新 5，想看更多再展开）
- 不动 polling 间隔（15s 不变；每次 n=20 lines payload 仍小）
- 不加配置项让用户自己调 n（不必要的灵活性）

## 验收

- `npx tsc --noEmit` ✅
- 切「记忆」tab，butler_tasks 段「最近执行」如果历史 > 5 条，能看到「… 展开全部 N 条」按钮
- 历史 ≤ 5 条仍直接显全（threshold 短路）

## 完成

- [x] 单行字面量改动 + 注释
- [x] 移到 docs/done/
