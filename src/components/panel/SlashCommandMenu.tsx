import { useEffect, useRef } from "react";
import type { SlashCommand } from "./slashCommands";

interface Props {
  commands: SlashCommand[];
  selectedIdx: number;
  onSelect: (cmd: SlashCommand) => void;
}

/// 输入框上方的浮窗 —— 列出当前 prefix 下的可选命令。键盘上下 / Enter 由父组件
/// 处理（与 input 的 onKeyDown 共享同一事件源），本组件只负责渲染列表 + 鼠标
/// 点击触发 onSelect。selectedIdx 越界（commands 减少）由父组件 clamp 后传入。
export function SlashCommandMenu({ commands, selectedIdx, onSelect }: Props) {
  const listRef = useRef<HTMLDivElement>(null);

  // 选中行变化时确保它在可视区域内（键盘连按下导致选中走出视窗时滚动跟随）
  useEffect(() => {
    const list = listRef.current;
    if (!list) return;
    const target = list.querySelector<HTMLDivElement>(
      `[data-slash-idx="${selectedIdx}"]`,
    );
    if (target) {
      target.scrollIntoView({ block: "nearest" });
    }
  }, [selectedIdx]);

  if (commands.length === 0) {
    return (
      <div style={menuContainerStyle}>
        <div style={{ padding: "10px 12px", fontSize: "12px", color: "var(--pet-color-muted)" }}>
          没有匹配的命令；输入 /help 查看全部
        </div>
      </div>
    );
  }
  return (
    <div ref={listRef} style={menuContainerStyle}>
      {/* R144: 非 selected 行 hover overlay。selected 行 inline 蓝 bg 优先
          级 winning，rgba 不需 !important —— 选中态自然保留。 */}
      <style>{`
        .pet-slash-row {
          transition: background-color 0.12s ease;
        }
        .pet-slash-row:hover {
          background: rgba(0, 0, 0, 0.04);
        }
      `}</style>
      {commands.map((cmd, i) => {
        const selected = i === selectedIdx;
        return (
          <div
            key={cmd.name}
            className="pet-slash-row"
            data-slash-idx={i}
            onMouseDown={(e) => {
              // mousedown 而非 click，确保在 input blur 之前先响应
              e.preventDefault();
              onSelect(cmd);
            }}
            style={{
              padding: "6px 12px",
              cursor: "pointer",
              background: selected ? "var(--pet-tint-blue-bg)" : "transparent",
              borderLeft: selected
                ? "2px solid var(--pet-color-accent)"
                : "2px solid transparent",
              display: "flex",
              alignItems: "baseline",
              gap: "10px",
              fontSize: "13px",
            }}
          >
            <span
              style={{
                fontFamily: "'SF Mono', 'Menlo', monospace",
                color: selected
                  ? "var(--pet-tint-blue-fg)"
                  : "var(--pet-color-fg)",
                fontWeight: 600,
                minWidth: "70px",
              }}
            >
              /{cmd.name}
              {cmd.parametric && (
                <span style={{ color: "var(--pet-color-muted)", fontWeight: 400 }}> &lt;参数&gt;</span>
              )}
            </span>
            <span style={{ color: "var(--pet-color-fg)", fontSize: "12px", flex: 1 }}>
              {cmd.description}
            </span>
          </div>
        );
      })}
    </div>
  );
}

const menuContainerStyle: React.CSSProperties = {
  position: "absolute",
  bottom: "100%",
  left: 0,
  right: 0,
  marginBottom: "6px",
  maxHeight: "200px",
  overflowY: "auto",
  background: "var(--pet-color-card)",
  border: "1px solid var(--pet-color-border)",
  borderRadius: "6px",
  boxShadow: "0 4px 12px rgba(0,0,0,0.08)",
  zIndex: 20,
};
