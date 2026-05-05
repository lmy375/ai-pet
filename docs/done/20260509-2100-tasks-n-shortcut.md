# PanelTasks "n" 快捷键打开创建表单（Iter R116）

> 对应需求（来自 docs/TODO.md）：
> PanelTasks 加 "n" 快捷键打开创建表单：现创建任务必须点"+ 新任务"或拉到顶部。在 PanelTasks active tab 时按 n（无 modifier）自动展开 createForm + focus title input（与 ⌘F / "/" focus search 的模式同步），加进既有 keydown useEffect。

## 目标

PanelTasks 已有 ⌘F / "/" focus 搜索框的全局快捷键（line 1138-）。用户经
常在浏览长列表后想"加个新任务"，得滚到顶 / 点"+ 新任务"按钮。加 n 单键
快捷键：

- 展开 createForm（若已折叠）
- focus title input

模式与 "/" 完全一致：单键无 modifier，在非输入控件 focus 时生效。

## 非目标

- 不限制 only 当前 tab —— PanelTasks 渲染只在 tasks tab，组件挂载本身已
  暗含 "tab active"
- 不在 input/textarea/button 内拦截 —— 既有 tagName 守卫已覆盖
- 不引入 "Cmd+N" —— mac 系统级快捷键（新窗口）冲突；单键 n 与 "/" 同设计

## 设计

### titleInputRef

PanelTasks 已有 `searchInputRef` for 搜索框。同样加 `titleInputRef`：

```diff
+const titleInputRef = useRef<HTMLInputElement>(null);

 <input
   style={s.input}
+  ref={titleInputRef}
   value={title}
   onChange={(e) => setTitle(e.target.value)}
   placeholder="比如：整理 Downloads"
 />
```

### keydown handler 加分支

在既有 useEffect 内 "/" 处理之后插：

```diff
       if (
         e.key === "/" &&
         !e.metaKey && !e.ctrlKey && !e.altKey && !e.shiftKey
       ) {
         e.preventDefault();
         const el = searchInputRef.current;
         if (el) { el.focus(); el.select(); }
         return;
       }
+      // R116: "n" 快捷键 —— 展开创建表单 + focus 标题输入。与 "/" 同设
+      // 计：单键无 modifier，依赖 tagName 守卫不在 input/textarea 内拦截。
+      if (
+        e.key === "n" &&
+        !e.metaKey && !e.ctrlKey && !e.altKey && !e.shiftKey
+      ) {
+        e.preventDefault();
+        setCreateFormExpanded(true);
+        // 等下一帧 form 渲染完，input 才能 focus；用 setTimeout 0 排到
+        // microtask 之后让 React commit 走完。
+        setTimeout(() => {
+          const el = titleInputRef.current;
+          if (el) { el.focus(); el.select(); }
+        }, 0);
+        return;
+      }
```

### setTimeout 0 的必要性

`setCreateFormExpanded(true)` 触发 React schedule 重渲染；但 ref `titleInputRef.current`
在重渲染前还是 null（form 还没挂）。setTimeout 0 把 focus 排到下一个 tick，
那时 React commit 完成，DOM 已挂上 ref。useLayoutEffect-flush 的另一种
做法更复杂，setTimeout 0 在交互场景足够稳。

折叠状态下 ref 为 null 时第二轮调用 focus 顺利。已展开时 ref 已有，setTimeout
0 也无副作用（focus 仍生效）。

### 测试

无单测；手测：
- focus 在面板（非 input/textarea/button）上：按 n → form 展开 + title 输入获焦
- focus 在搜索框：按 n → 走 input 路径，不触发 form 展开（输入字 "n"）
- form 已展开：按 n → 仅 focus title（state 已 true）
- 切到别 tab：n 不触发（PanelTasks 卸载，listener 也卸）

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | titleInputRef + ref 挂到 input |
| **M2** | keydown handler 内加 "n" 分支 |
| **M3** | tsc + build |

## 复用清单

- 既有 keydown useEffect（line 1138-）
- 既有 ⌘F / "/" focus pattern + tagName 守卫
- 既有 `createFormExpanded` state + localStorage 持久化

## 进度日志

- 2026-05-09 21:00 — 创建本文档；准备 M1。
- 2026-05-09 21:08 — M1 完成。`titleInputRef = useRef<HTMLInputElement>(null)` 加在 searchInputRef 旁；title input 加 ref={titleInputRef}（folded 时 ref 是 null，由 setTimeout 0 等 React commit）。
- 2026-05-09 21:11 — M2 完成。keydown useEffect 内"/" 分支后插 "n" 分支：守卫 modifier 都不按 → preventDefault → setCreateFormExpanded(true) → setTimeout 0 focus + select titleInput；tagName 守卫已挡 INPUT/TEXTAREA/BUTTON 内部不拦截。
- 2026-05-09 21:14 — M3 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (500 modules, 980ms)。归档至 done。
