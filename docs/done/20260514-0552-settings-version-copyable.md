# Settings `pet v0.1.0` chip 点击复制

## 背景

Settings 顶部 chip 行已经有 "pet v0.1.0" 显示 + "schema v4" 同行，但都是只读 span。用户写 bug report 想贴版本时还得人工 copy。

把 `pet v` chip 改成可点击，点一次复制 `"pet v0.1.0 · schema v4 · platform"` 简短字符串到剪贴板，配置 1.5s 绿色 "已复制" 反馈（与同一 section 的「在 Finder 中打开」+「复制路径」按钮的反馈样式一致）。

## 改动

`src/components/panel/PanelSettings.tsx`：

### 新增 versionCopied 状态

```ts
const [versionCopied, setVersionCopied] = useState(false);
```

### chip 改成 button

```tsx
{appVersion && (
  <button
    type="button"
    onClick={async () => {
      const plat =
        typeof navigator !== "undefined" ? navigator.platform : "";
      const parts = [`pet v${appVersion}`];
      if (dbStats?.schema_version)
        parts.push(`schema v${dbStats.schema_version}`);
      if (plat) parts.push(plat);
      try {
        await navigator.clipboard.writeText(parts.join(" · "));
        setVersionCopied(true);
        setTimeout(() => setVersionCopied(false), 1500);
      } catch {
        // 剪贴板权限错误 / 隐私模式：静默；用户看不到反馈即可
      }
    }}
    style={{
      ...padding: "0 4px", border: "none", background: "transparent",
      color: versionCopied ? "var(--pet-tint-green-fg)" : "var(--pet-color-fg)",
      fontWeight: 600,
      fontFamily: "inherit",
      fontSize: 11,
      cursor: "pointer",
    }}
    title="点击复制 pet v / schema v / 平台 一行（贴 bug report 用）"
  >
    {versionCopied ? "✓ 已复制" : `pet v${appVersion}`}
  </button>
)}
```

## 不做

- 不持久化复制偏好：1.5s 反馈足够
- 不复制全部 dbStats（butler_tasks count 等）：那是数据规模，不属版本身份
- 不动 schema / pet.db / table count chips 的可点性：版本是 bug report 高频项，其它 chip 是辅助信息

## 验收

- `npx tsc --noEmit` ✅
- 切「设置」→「本地数据目录」section → 鼠标移到 "pet v0.1.0" → cursor:pointer
- 点击 → chip 文案变绿 "✓ 已复制"，1.5s 后回原文；剪贴板里是 "pet v0.1.0 · schema v4 · MacIntel"

## 完成

- [x] PanelSettings.tsx: versionCopied state + chip 改 button
- [x] `npx tsc --noEmit` 通过
- [x] 移到 docs/done/
