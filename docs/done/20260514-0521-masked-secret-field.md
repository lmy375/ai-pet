# 抽 `MaskedSecretField` 组件（API key / Bot Token 共用）

## 背景

OpenAI api_key 字段已有完整三件套：
- `type="password"` 默认掩码
- 👁 按住显示 / 松开复盖（mouse + touch 双事件）
- 📋 复制到剪贴板（避开"全选→⌘C"明文可见瞬间）

而 TG **bot_token 字段只有 `type="password"`，缺 👁 与 📋**。两个字段的语义完全等价（secret string，必须显示但不能泄漏到截图），不平行是凑出来的。

抽 `MaskedSecretField` 组件，两处共用 + 让 bot_token 一并升级到三件套。

## 改动

### `src/components/panel/MaskedSecretField.tsx`（新）

```tsx
import { useState } from "react";

interface Props {
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  /// 用于 tooltip / 反馈文案的具名（如 "API key" / "Bot Token"）
  secretLabel: string;
  /// 复制成功 / 失败的 callback —— 由 caller 持 message state 决定如何显
  onCopyFeedback?: (msg: string) => void;
  /// 透传 input 的 style override（如 fontFamily monospace）
  inputStyle?: React.CSSProperties;
}

export function MaskedSecretField({
  value,
  onChange,
  placeholder,
  secretLabel,
  onCopyFeedback,
  inputStyle,
}: Props) {
  const [visible, setVisible] = useState(false);
  const disabled = !value;
  const btnBase: React.CSSProperties = {
    padding: "0 10px",
    border: "1px solid var(--pet-color-border)",
    borderRadius: 4,
    fontFamily: "inherit",
    fontSize: 13,
    background: "var(--pet-color-card)",
    color: "var(--pet-color-muted)",
    cursor: disabled ? "not-allowed" : "pointer",
    userSelect: "none",
  };
  return (
    <div style={{ display: "flex", gap: 6, alignItems: "stretch" }}>
      <input
        type={visible ? "text" : "password"}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        style={{ flex: 1, ...inputStyle }}
        placeholder={placeholder}
      />
      <button
        type="button"
        onMouseDown={() => setVisible(true)}
        onMouseUp={() => setVisible(false)}
        onMouseLeave={() => setVisible(false)}
        onTouchStart={() => setVisible(true)}
        onTouchEnd={() => setVisible(false)}
        title={`按住显示 ${secretLabel}（松开即重新掩码）`}
        aria-label={`按住显示 ${secretLabel}`}
        style={{
          ...btnBase,
          background: visible ? "var(--pet-tint-yellow-bg)" : btnBase.background,
          color: visible ? "var(--pet-tint-yellow-fg)" : btnBase.color,
        }}
        disabled={disabled}
      >
        {visible ? "👁" : "👁‍🗨"}
      </button>
      <button
        type="button"
        onClick={async () => {
          if (!value) return;
          try {
            await navigator.clipboard.writeText(value);
            onCopyFeedback?.(`已复制 ${secretLabel} 到剪贴板（${value.length} 字符）`);
          } catch (e) {
            onCopyFeedback?.(`复制失败：${e}`);
          }
        }}
        title={`复制 ${secretLabel} 到剪贴板（避开手动全选 → ⌘C 的明文可见瞬间）`}
        aria-label={`复制 ${secretLabel}`}
        style={btnBase}
        disabled={disabled}
      >
        📋
      </button>
    </div>
  );
}
```

视觉行为与既有 api_key 完全一致：button base 抽出 `btnBase` 减少重复内联 style。`inputStyle` 透传让 caller 仍能加 `fontFamily: 'monospace'`。

### `src/components/panel/PanelSettings.tsx`

- 删 `apiKeyVisible` state（移入 component 内部）
- api_key 那 50 行块换成：
  ```tsx
  <MaskedSecretField
    value={form.api_key}
    onChange={(v) => setForm({ ...form, api_key: v })}
    placeholder="sk-..."
    secretLabel="API key"
    onCopyFeedback={(m) => {
      setMessage(m);
      window.setTimeout(() => setMessage(""), 3000);
    }}
    inputStyle={{
      ...inputStyle,
      marginTop: 0,
    }}
  />
  ```
- bot_token 同模式替换（原 `<input type="password" ...>` 单行 → MaskedSecretField）

## 不做

- 不持久化 visible 状态：每次 mount 默认 false（mask 显），最大防泄漏
- 不让 onCopyFeedback 强制 throughable 进 form state：feedback 是 caller 决定（PanelSettings 走 message 通道，其它 caller 可能走 toast）
- 不写 unit test：纯 view + 标准事件转发；无 vitest 项目通则

## 验收

- `npx tsc --noEmit` ✅
- API key 行为不变（按住 👁 显字 / 📋 复制反馈）
- 「Telegram Bot」section 的 Bot Token 多出 👁 / 📋，行为与 api_key 一致
- 字段无值时 disabled cursor not-allowed

## 完成

- [x] MaskedSecretField.tsx 新建
- [x] PanelSettings.tsx: api_key 替换 + bot_token 替换
- [x] 删 apiKeyVisible state
- [x] `npx tsc --noEmit` 通过
- [x] 移到 docs/done/
