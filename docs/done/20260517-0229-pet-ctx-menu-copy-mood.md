# pet 右键加「📋 复制当前 mood」+ 上轮 stale TODO 替换

## 背景

### 上轮 TODO "PanelMemory item description hover preview" stale 移除

PanelMemory item description 已在行级以全文渲染 (line 4870-4895 `renderContentWithTaskRefs(displayDesc, ...)`)，无 200 字截断。无需 hover tooltip 补"完整内容"。移除该项，替换 5 条新提案。

### 本 iter：pet 右键加「📋 复制当前 mood」

owner 想"抄宠物当前心情发朋友圈 / 写日记 / issue 截图配文" 时，目前只能看 MoodWidget 然后手抄。加 pet 右键一键复制 "心情：X · 动作：Y" 字符串。

## 改动

### `src/App.tsx`

pet ctx menu 加 📋 复制 mood 按钮（紧贴 ⏰ 倒计时 nudge 之后、📡 ping LLM 之前）：

```tsx
<button onClick={async () => {
  setPetCtxMenu(null);
  try {
    const m = await invoke<CurrentMood>("get_current_mood");
    if (!m || (!m.text?.trim() && !m.motion)) {
      appendAssistant("📋 当前 mood 为空，无可复制");
      return;
    }
    const parts: string[] = [];
    if (m.text?.trim()) parts.push(`心情：${m.text.trim()}`);
    if (m.motion) parts.push(`动作：${m.motion}`);
    const text = parts.join(" · ");
    await navigator.clipboard.writeText(text);
    appendAssistant(`📋 已复制当前 mood：${text}`);
  } catch (e) {
    appendAssistant(`📋 复制 mood 失败：${e}`);
  }
}}>
  📋 复制当前 mood
</button>
```

H 从 400 调 440（+ 1 button + 1 separator）。

## 关键设计

- **复用 get_current_mood IPC**：与 MoodWidget 同源 —— mood text + motion 是后端单源。
- **空 mood 兜底**：mood file 缺失 / 老 session 都返空；用 appendAssistant 软提示替代抛错。
- **format 简洁**：仅 "心情：X · 动作：Y" —— 不带 ts 不带 elapsed；owner 想要 ts 走 ChatMini bubble ts copy (iter #223)。
- **appendAssistant 软反馈**：与 mute / 倒计时 / ping LLM 等 pet 右键操作同 ChatMini 反馈渠道，UX 一致。

## 不做

- **不写测试**：纯 IPC + clipboard；视觉验证（pet 右键 → 复制 → 粘贴看格式）足够。
- **不显历史 mood 也复制**：MoodWidget 双击已有 7 天浮窗；想批量 export 走那条路径。
- **不绑键盘快捷**：右键菜单已近 reach。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 1.19s
- 改动 ~35 行（menu 按钮 + H 调 + 注释）。既有 mute / 倒计时 / 主题 / ping LLM / 重启 menu 路径完全不动。

## TODO 状态

剩 5 条留池（含上轮替换补的 5 新条 - 本 iter 实现 1）：
- PanelMemory items 长 description 行级折叠 + "展开 (N 字)" 按钮
- ChatMini bubble 双击 ref token 时 + audio bell ping
- PanelTasks 顶 chip 行加 "今日已完成 N" green chip
- detail.md 编辑器 toolbar 加 "🧠 ask LLM about selection" 按钮
- detail.md 编辑器 ⌘K 唤起 task quick-find palette

## 后续

- ⌥+click 复制 mood + history 段 (最近 5 条)。
- pet ctx menu 加 "🎲 让宠物随机变 mood" 让 owner 主动调情绪。
