# /image 历史 prompt 召回

## 需求

R129 历史召回（input 空 + ↑）只覆盖完整 user 消息，slash 命令不入。但 `/image` 的 prompt 经常会复用 —— "a watercolor painting of a rabbit" 这种长描述用户不想重打。在 `/image ` 触发态弹一个历史菜单（最近 5 条），让用户↑↓选 / Enter / Tab 填进 input，再加 / 改细节也行。

## 实现

### 存储

`src/components/panel/slashCommands.ts`：

- localStorage key `pet-image-prompt-history` → string[]，新 prompt 在数组头
- 单条 cap 200 字（防长 prompt 撑爆 localStorage）
- 总条数 cap 5（最旧的自动 slice 掉）
- `recordImagePrompt(prompt)`：trim 空白 → cap 长度 → dedupe（同 prompt 用过 → 移除旧位置、置顶）→ slice(0, 5) → write
- `readImagePrompts()`：读 + 类型守 + 解析失败兜底空

### 触发态

PanelChat：

```ts
const imagePromptTriggerActive = /^\/image\s*$/i.test(input) && !slashMenuVisible;
```

- 输入精确等于 `/image` 或 `/image ` / `/image   `（任意尾空白）
- slashMenuVisible 优先 —— 用户敲 `/im` 还在 slash 命令补全模式时，prompt 菜单不弹
- 用户一旦敲字符进 prompt（`/image cat`）→ regex 失效 → 菜单关，不影响 compose 体验

### UI

`src/components/panel/ImagePromptHistoryMenu.tsx`（新文件）：

- 与 SlashCommandMenu 同构：absolute bottom:100% 浮在 input 上方
- 顶上一行 muted hint「最近 prompt（↑↓ 选中，Enter / Tab 填入）」
- 每条 `🎨 <prompt>`，单行 ellipsis；tooltip 显完整 prompt 给溢出用户
- selected 行走 tint-blue-bg + 左侧 accent 边

### 键盘

`handleInputKeyDown` 在 slashMenuVisible 检查之前先处理 imagePromptMenuVisible：
- ↑↓ 选 + clamp 边界
- Enter / Tab → 填 input 为 `/image <picked>`（不自动 submit，让用户能进一步编辑）
- Esc → 清空 input 关菜单（input 是 `/image ` 这种东西，清掉等价"我不发了"）
- 其它键透传 → 用户敲字符就让 regex 自然失效关菜单

### 记录

executeSlash 的 `case "image"` 顶部加 `recordImagePrompt(action.prompt)`，每次成功执行 /image 时记一笔。重试按钮路径不走 executeSlash，但也不需要重复记 —— prompt 已经是历史里那条了。

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 第一次发 `/image dragon` → 历史空，菜单不弹
  - 后续敲 `/image ` → 弹菜单显 ["dragon"]
  - 用过 5 个不同 prompt → 菜单显 5 条
  - 再发同 prompt → dedupe 后置顶，不会变成 6 条
  - hover row → hover bg；click → 填 `/image <picked>` 到 input
  - localStorage 禁用 → readImagePrompts 兜底空，菜单不弹，主流程不崩

## 不在本轮范围

- 没做"prompt 模糊搜索"：用户敲 `/image cat` 时不再过滤历史菜单 —— 触发条件就是 arg 为空。简洁优先；要模糊搜索就放手让用户用 panel 跨会话搜
- 没把 prompt 与 `-n` flag 绑：历史只存 prompt 文本，重用时用户得自己重打 `-n N`。多数复用场景是单图，绑入 -n 反而成累赘
