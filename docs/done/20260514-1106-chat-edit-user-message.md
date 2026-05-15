# 双击编辑 user 消息并重新生成（IM 风）

## 背景

TODO 最后一项：

> 聊天 user 消息编辑/重发：双击历史 user bubble 进 inline 编辑，Enter 后丢弃后续 messages 重新生成（IM 习惯）。

当前 PanelChat 输入是 forward-only：发错一句 / 想换一个问法 → 只能再发新消息覆盖，旧轮的 assistant 噪声还在历史里继续被 LLM 看到。IM 类应用（iMessage / Telegram / 微信电脑端）都支持"长按编辑"或"双击编辑"，让用户能 in-place 调整发问后立即拿到新一轮的回答。

## 改动

### `src/components/panel/PanelChat.tsx`

**1. 纯函数 helper（顶层 export，将来 vitest 一接入直接 unit test）**

```ts
export function findMessageIndexForUserItem(
  items: ChatItem[],
  messages: Array<{ role?: string }>,
  userItemIdx: number,
): number | null
```

为什么需要：items 与 messagesRef 不是 1:1 对齐 —— 一个 chat turn 在 items 里可能写出多条 assistant（toolStart 中转写 accumulated），但 messagesRef 只在 done 时 push 一次最终 assistant；items 还混着 tool / error / systemNote 等非 LLM-facing 条目。**但 user 是 1:1 的**：每次 sendMessage 同时给 items 与 messagesRef 各 push 一条 user。所以"items 第 K 个 user"刚好对应"messagesRef 第 K 个 role==='user'"。算法两次扫描即可（前扫数 K，再扫找下标）。

**2. sendMessage 加 `opts?: { baseItems?: ChatItem[] }`**

```ts
const sendMessage = useCallback(
  async (content: string, images?: string[], opts?: { baseItems?: ChatItem[] }) => {
    ...
    const base = opts?.baseItems ?? items;
    const newItems = [...base, { type: "user", content, ... }];
    ...
  },
  [items, saveCurrentSession],
);
```

为什么不直接 `setItems(truncated)` 再调 sendMessage：sendMessage 闭包里读到的仍是旧 items（React 18 调度不保证立即可读）。显式 base 参数让 caller 把"截断后的起点"直接交给 sendMessage，避免 closure-stale 问题。不传时回到原行为，不影响 18 处既有 sendMessage / setItems 调用站。

**3. 编辑态状态机**

```ts
const [editingItemIdx, setEditingItemIdx] = useState<number | null>(null);
const [editingDraft, setEditingDraft] = useState("");

const enterEditMode = (idx: number) => {
  if (isLoading) return;             // 流式中拒绝
  const it = items[idx];
  if (it?.type !== "user") return;
  if (it.images?.length) return;     // 多模态消息暂不支持编辑
  setEditingItemIdx(idx);
  setEditingDraft(it.content);
};

const commitMessageEdit = () => {
  if (editingItemIdx === null) return;
  const trimmed = editingDraft.trim();
  if (!trimmed) return;              // 空文本 noop（与 submitInput 一致）
  if (isLoading) return;
  const msgIdx = findMessageIndexForUserItem(items, messagesRef.current, editingItemIdx);
  if (msgIdx === null) { cancelEditMode(); return; }   // 不一致 → 退出（理论不会）
  const newItems = items.slice(0, editingItemIdx);    // 砍掉自己 + 后续
  messagesRef.current = messagesRef.current.slice(0, msgIdx);
  setItems(newItems);
  setEditingItemIdx(null);
  setEditingDraft("");
  void sendMessage(trimmed, undefined, { baseItems: newItems });
};
```

**4. UI：双击 user bubble 进编辑态**

- 在 user 分支的 CopyableMessage 外包一层 `<div onDoubleClick={enterEditMode}>`。bubble 内的 task-ref token 已在自己 onDoubleClick 里 stopPropagation，双击 ref 跳任务面板的语义不被这里抢走。
- 编辑态渲染一个 textarea + 「保存并重发 / 取消」按钮 + 「Enter 重发 · Shift+Enter 换行 · Esc 取消」提示文案。
- textarea autoFocus；focus halo 跟 panel-global focus 双层呼应（accent 50% border + 18% halo）。
- 含 image 的 bubble 仍走原 CopyableMessage 路径，title tooltip 在原值；双击 noop。
- IME composing 期间 Enter 不提交（用 `nativeEvent.isComposing` —— React SyntheticEvent 没暴露此字段）。

**截断后的持久化路径**

sendMessage 自带的 done / error 分支会调 saveCurrentSession(finalItems) —— 写回去的 items 已经是新发送之后的全集，messagesRef 也是新的（commit 前已截断 + sendMessage 内部又 push 了新 user 与新 assistant）。所以编辑后等于自动写盘新版本，不需要额外 explicit save。

## 不做

- **多模态消息（含 images 的 user msg）不进入编辑**：编辑文本同时保留 / 删图的语义边界复杂（删第几张？新增图？），先把 95% 场景（纯文本）做好；含图的消息保持原 bubble 可复制/可挂 mark 行为不变。
- **不暴露 settings 开关**。双击行为不挡键盘 / 鼠标交互别的路径；用户不双击不会发生任何变化。
- **不重写 sendMessage 内部流式逻辑**。截断逻辑全在 caller 侧，sendMessage 仅多了 baseItems 参数；既有 toolStart / toolResult / done / error 全部流程不动。
- **不撤销/版本历史**。编辑就是丢弃后续，与 IM 行为对齐；想"看原版"靠 session 备份（snapshot 导出已有路径）。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.17s
- 改动是叠加 + 一处签名扩展（默认参数）—— 18 处既有 sendMessage / setItems 调用站行为不变。

## 后续

- 多模态消息编辑（处理删图 / 加图的 UX）。
- 编辑历史轨迹（哪些消息被改过、原版长什么样）—— 当前编辑等同删除，IM 通常也不留 trace。
- `↑` 召回到最近一条 user 自动进入编辑（更键盘党 ergo）。
