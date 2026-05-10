import { useEffect } from "react";

/// 集中列出整个 panel 内分散在各 tab 的快捷键。`?` 唤起，Esc / 点背景关闭。
/// 各组按 tab 范围分（panel-wide / 任务 / 调试），用户能直接按位置定位。
///
/// 维护：新增快捷键时回填本表；尽量保持与代码层一致（事实源于代码，本表
/// 是镜像）。空类用 None 跳过整段，免出现"任务: 暂无"这种空行。

interface Shortcut {
  keys: string[];
  description: string;
}

interface ShortcutGroup {
  title: string;
  scope: string;
  items: Shortcut[];
}

const GROUPS: ShortcutGroup[] = [
  {
    title: "Panel 全局",
    scope: "任意 tab",
    items: [
      { keys: ["?"], description: "唤起本帮助层" },
      { keys: ["Esc"], description: "关闭弹窗 / 帮助层 / 高风险工具审核（拒绝最上面一条）" },
    ],
  },
  {
    title: "任务 tab",
    scope: "焦点不在 input/textarea/button 时",
    items: [
      { keys: ["⌘F", "Ctrl+F"], description: "聚焦搜索框（与浏览器/Notion 习惯一致）" },
      { keys: ["/"], description: "聚焦搜索框（GitHub/Linear 习惯）" },
      { keys: ["n"], description: "展开新建任务表单 + focus 标题输入" },
      { keys: ["↑", "↓"], description: "上下移动键盘焦点行" },
      { keys: ["Home", "End"], description: "跳到第一条 / 最后一条" },
      { keys: ["空格"], description: "勾选 / 取消勾选当前焦点行（多选）" },
      { keys: ["Enter"], description: "展开 / 折叠当前焦点行的详情" },
      { keys: ["d"], description: "把当前焦点行标 done（pending / error 才响应）" },
      { keys: ["r"], description: "重试当前焦点行（仅 error）" },
      { keys: ["Delete", "Backspace"], description: "打开当前焦点行的「取消任务」原因输入" },
    ],
  },
];

const KEY_CHIP_STYLE: React.CSSProperties = {
  fontFamily: "'SF Mono', 'Menlo', monospace",
  fontSize: 11,
  padding: "2px 6px",
  borderRadius: 4,
  background: "var(--pet-color-bg)",
  border: "1px solid var(--pet-color-border)",
  color: "var(--pet-color-fg)",
  marginRight: 4,
  whiteSpace: "nowrap",
};

interface Props {
  visible: boolean;
  onClose: () => void;
}

export function KeyboardHelpOverlay({ visible, onClose }: Props) {
  // Esc 关闭。仅 visible 时挂监听，避免无业务时占全局快捷键位。
  useEffect(() => {
    if (!visible) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [visible, onClose]);

  if (!visible) return null;

  return (
    <div
      onClick={onClose}
      style={{
        position: "fixed",
        inset: 0,
        background: "rgba(0,0,0,0.4)",
        zIndex: 2500,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        padding: 32,
      }}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        style={{
          background: "var(--pet-color-card)",
          borderRadius: 10,
          maxWidth: 560,
          width: "100%",
          maxHeight: "80vh",
          overflowY: "auto",
          padding: "20px 24px",
          color: "var(--pet-color-fg)",
          fontFamily: "system-ui, sans-serif",
          fontSize: 13,
          lineHeight: 1.6,
          boxShadow: "0 10px 40px rgba(0,0,0,0.25)",
        }}
      >
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline", marginBottom: 12 }}>
          <h2 style={{ margin: 0, fontSize: 16, fontWeight: 600 }}>键盘快捷键</h2>
          <span style={{ fontSize: 11, color: "var(--pet-color-muted)" }}>
            点背景或按 Esc 关闭
          </span>
        </div>
        {GROUPS.map((g) => (
          <div key={g.title} style={{ marginBottom: 18 }}>
            <div style={{ display: "flex", alignItems: "baseline", gap: 8, marginBottom: 6 }}>
              <span style={{ fontSize: 13, fontWeight: 600 }}>{g.title}</span>
              <span style={{ fontSize: 11, color: "var(--pet-color-muted)" }}>
                {g.scope}
              </span>
            </div>
            <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
              {g.items.map((s, i) => (
                <div key={i} style={{ display: "flex", alignItems: "baseline", gap: 8 }}>
                  <div style={{ flexShrink: 0, minWidth: 110 }}>
                    {s.keys.map((k) => (
                      <span key={k} style={KEY_CHIP_STYLE}>
                        {k}
                      </span>
                    ))}
                  </div>
                  <span style={{ fontSize: 12, color: "var(--pet-color-fg)" }}>{s.description}</span>
                </div>
              ))}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
