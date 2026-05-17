# PanelDebug 日志 tab「🔍 过滤 keyword」substring filter（iter #369）

## Background

PanelDebug 日志 tab 当前已有 ERROR / WARN / INFO 三级 chip 多选过
滤，但 owner 想"找 task title X 是啥时跑的 / 找 error message 含
keyword Y" 等子串查询时仍需肉眼扫读千行日志。本 iter 加 substring
实时 filter，与 level chips AND 叠加。

## Changes

### `src/components/panel/PanelDebugLogs.tsx`

#### 1. state

```ts
const [keyword, setKeyword] = useState("");
```

不持久化 — debug 临时操作，关 tab 复位。

#### 2. `filteredLogs` 加 keyword filter

```ts
const filteredLogs = useMemo(() => {
  const kwLower = keyword.trim().toLowerCase();
  const levelActive = logLevels.size > 0;
  if (!levelActive && kwLower.length === 0) return logs;
  return logs.filter((line) => {
    if (levelActive) { /* level check */ }
    if (kwLower.length > 0 && !line.toLowerCase().includes(kwLower)) {
      return false;
    }
    return true;
  });
}, [logs, logLevels, keyword]);
```

case-insensitive substring；与 level chips AND 关系。

#### 3. 输入框 UI（紧贴 level chips 右侧）

- `🔍` prefix + 180px `<input>` + 右侧 ✕ 清空按钮（仅 keyword 非空
  时显）
- placeholder "过滤关键字（substring）…"
- Esc 键清空（与 ✕ 等价）
- monospace 字体（与日志体内字体一致 — 调键字时易感受 spacing）

#### 4. 显示计数 + empty state 更新

- "显示 N / M" 计数 chip 现在在 `logLevels.size > 0 || keyword.trim().length > 0` 时显
- followTail 按钮 marginLeft 同步条件
- 空匹配 empty state 三态分流：
  - 仅 level → "当前 level 过滤无匹配日志"
  - 仅 keyword → "「kw」无匹配日志"
  - level + keyword → "「kw」+ 当前 level 过滤无匹配日志"
  让 owner 知道是哪一层过滤导致空集，方便快速 backtrack。

## Key design decisions

- **substring 而非 regex**：owner 多数场景是 "我记得有个 task title
  里含 'Downloads'" / "刚才那条 ERROR 含 'parse failed'"，substring
  覆盖 95% 用例。regex 需 escape 经验 + 错写 regex 时炸 — 不值得。
- **case-insensitive**：log 体内 ERROR / WARN / INFO / task title /
  user 输入 mix-case，case-sensitive 会让 owner 反复尝试大小写。
- **AND 叠加 level chips**：两过滤维度都是 "narrow down"，并存合
  乎直觉（VSCode / Chrome devtools log 也是同模式）。
- **不 debounce**：log 体量 ring buffer 通常 < 5000 行；每键击重 filter
  在现代 JS engine < 1ms 不卡。debounce 反而引入打字 → 视觉延迟的
  错觉感。
- **不持久化 localStorage**：debug 关键字是非常临时的查询，关 tab
  / 重启都应清空。level chips 同样不持久化，保一致心智。
- **✕ 清空按钮位置内嵌 input padding-right**：紧凑布局，避免外置按
  钮占额外 chip 行宽度。Esc 等价键盘党友好。
- **跟随 followTail 行为不变**：过滤后 filteredLogs 仍触发自动滚到
  底（既有 effect 依赖 logs / followTail，已隐式覆盖 filtered 变化
  — 等价行为）。

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.25s)
- 后端无改动
