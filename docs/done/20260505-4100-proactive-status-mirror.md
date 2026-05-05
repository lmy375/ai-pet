# 「立即开口」状态文案同步到决策日志 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> 「立即开口」按钮位置同步到决策日志：触发后顶部 toolbar 的 「立即开口」按钮的状态文案 (`proactiveStatus`) 也展示在决策日志区域，省得用户视线在两段间来回。

## 目标

按下顶部 toolbar 「立即开口」后，新决策几乎瞬间 push 到决策日志区域。但状态
反馈（"触发成功 / 触发失败" / 错误详情）只在顶部按钮旁边显示一段时间，用户
要把视线从决策日志区拉回顶部对账"是不是失败了 / 触发反应是什么"。本轮把同
样的 `proactiveStatus` 也镜像渲染在决策日志段标题行里，让用户在原视区直接
看到反馈。

## 非目标

- 不动现有顶部反馈（仍保留，给"在 toolbar 上下文里的用户"用）。
- 不引入新状态 / 计时器 —— 复用既有 8s 自动清空策略。
- 不写 README —— 调试器交互微调。

## 设计

决策日志段标题行已是 flex 布局（`display: flex, gap: 8px`），结尾有「清空」
按钮 `marginLeft: auto` 推右。本轮在 `<span>最近 N ...</span>` 与 `{清空 button}`
之间追加 proactiveStatus 镜像：

```tsx
{proactiveStatus && (
  <span
    style={{
      fontSize: "11px",
      color: proactiveStatus.startsWith("触发失败") ? "#dc2626" : "#059669",
      maxWidth: "260px",
      overflow: "hidden",
      textOverflow: "ellipsis",
      whiteSpace: "nowrap",
    }}
    title={proactiveStatus}
  >
    {proactiveStatus}
  </span>
)}
```

`marginLeft: auto` 仍在「清空」按钮上 → 状态文字紧贴标题之后、清空按钮之前。

### 测试

无后端改动；纯 UI 镜像。靠 tsc + 手测。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | 决策日志标题行加 proactiveStatus 镜像 |
| **M2** | tsc + build + cleanup |

## 复用清单

- 既有 `proactiveStatus` 状态 + 8s 自动清空
- 既有顶部 status 文案的 styling

## 进度日志

- 2026-05-05 41:00 — 创建本文档；准备 M1。
- 2026-05-05 41:05 — 完成实现：`PanelDebug.tsx` 决策日志段标题行在 title span 与「清空」按钮之间镜像 proactiveStatus（与顶部 toolbar 同 styling，触发失败红 / 成功绿；title hover 完整文案；既有 8s 自动清空策略不变）。`pnpm tsc --noEmit` 干净；`pnpm build` 497 modules 全过。TODO 移除条目；本文件移入 `docs/done/`。
  - **README 不更新** —— 调试器交互微调。
  - **未做手动 dev 验证**：当前会话不便启动 Tauri 桌面 app；纯条件渲染镜像现有 styled span。
