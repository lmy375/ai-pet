import { useState } from "react";

/// 密钥型 input 三件套：默认掩码（type=password）+ 👁 按住显示 + 📋 复制
/// 到剪贴板。给 OpenAI API key / Telegram Bot Token 等"必须显示但不能泄漏
/// 到截图"的字段统一形态。
///
/// 设计取舍：
/// - 默认 mask：每次 mount visible=false 重置，最大化防截图泄漏
/// - 按住显示而非 toggle：录屏 / 截图绝大多数在用户手没按时，"长按才显"
///   比"toggle 切换"不易忘按而泄漏
/// - 📋 复制走系统剪贴板：避开"全选输入框 + ⌘C"那一瞬间的明文可见
/// - 反馈走 onCopyFeedback callback：caller 决定如何展示（message bar / toast）
interface Props {
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  /// 用于 tooltip / 反馈文案的具名（如 "API key" / "Bot Token"）
  secretLabel: string;
  /// 复制成功 / 失败的反馈；caller 持 message state 决定如何展示
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
            onCopyFeedback?.(
              `已复制 ${secretLabel} 到剪贴板（${value.length} 字符）`,
            );
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
