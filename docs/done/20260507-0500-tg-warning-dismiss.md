# TG bot 启动告警支持 dismiss — 开发计划

> 对应需求（来自 docs/TODO.md）：
> TG bot 启动告警支持 dismiss：当前 banner 一直挂着；加 ✕ 按钮 dismiss 单条警告（仅前端隐藏，不删除 store），让用户读完知情即可清理视觉。

## 目标

上一轮 (0400) 落地的 TG 启动告警 banner 在 `tgStartupWarnings` 非空时一
直挂着。用户读完后想"知道了，眼不见心不烦"，但当前没入口。本轮在每条
警告右侧加 ✕ 按钮，dismiss 后该条仅前端隐藏；后端 store 不动（同一进程
内若同种告警再次 push，仍会重新可见）。

## 非目标

- 不动后端 store —— 后端 store 是"客观事实"通道，前端 dismiss 只是阅读
  态偏好；删除后端会让 reconnect 等其它入口拿不到了。
- 不持久化 dismiss 列表 —— 进程重启会清空 store 也清空告警，dismiss 状态
  自然失效；写 localStorage 反而要做"该 fp 是否仍存在于 store"的对账。
- 不做"全部 dismiss" —— 警告通常 ≤ 2 条，per-row dismiss 一次点完不繁琐。

## 设计

### 指纹

每条警告的稳定 key = `${timestamp}|${kind}|${message}`。timestamp 含到 ms
+ 时区，单进程内不会撞。

### state

```ts
const [tgDismissed, setTgDismissed] = useState<Set<string>>(new Set());
```

filter 时跳过 dismissed：
```ts
const visibleTgWarnings = tgStartupWarnings.filter(
  (w) => !tgDismissed.has(`${w.timestamp}|${w.kind}|${w.message}`),
);
```

banner outer 条件改为 `visibleTgWarnings.length > 0`，count 与 map 也用
visible 列表。

### UI

每条 warning 行末端加一个 ✕ 按钮：
- 与既有 row 同一 flex line（修改 row 容器为 `display: flex`）
- 视觉与 reason / decision-log 复制按钮同款（10px 字、灰边、白底）
- 点击 → 把指纹加进 dismissed set

不复用 hover-only 显隐 —— banner 本身就是临时态，按钮始终可见让 dismiss
路径明确（hover 反而让用户找按钮）。

## 测试

PanelDebug 是 IO 重容器；前端无 vitest，靠 tsc + 手测足以。指纹拼接是
单行字符串，复杂度 0。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | tgDismissed state + visibleTgWarnings 派生 + per-row ✕ 按钮 |
| **M2** | tsc + build + cleanup |

## 复用清单

- 既有 banner 容器 / 配色
- 既有 setCopyMsg-style 小按钮视觉

## 进度日志

- 2026-05-07 05:00 — 创建本文档；准备 M1。
- 2026-05-07 05:10 — M1 完成。`tgDismissed: Set<string>` state；指纹 = `${timestamp}|${kind}|${message}`；外层 IIFE 派生 visibleTgWarnings；每条 row 改为 flex 布局加 ✕ 按钮；空 / 全 dismiss 时整 banner 不渲染。
- 2026-05-07 05:15 — M2 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (498 modules, 979ms)。归档至 done。
