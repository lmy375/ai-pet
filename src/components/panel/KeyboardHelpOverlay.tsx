import { Modal } from "./Modal";
import { SectionTitle } from "./SectionTitle";

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
    scope: "任意 tab（input/textarea 聚焦时让出键位）",
    items: [
      { keys: ["?"], description: "唤起本帮助层" },
      { keys: ["Esc"], description: "关闭弹窗 / 帮助层 / 高风险工具审核（拒绝最上面一条）" },
      {
        keys: ["⌘1", "⌘2", "⌘3", "⌘4", "⌘5"],
        description: "跳到对应 tab（设置 / 聊天 / 任务 / 记忆 / 人格；Ctrl 等价）",
      },
    ],
  },
  {
    title: "任务 tab",
    scope: "焦点不在 input/textarea/button 时",
    items: [
      { keys: ["⌘F", "⌘K", "Ctrl+F", "Ctrl+K"], description: "聚焦搜索框（⌘F 同浏览器/Notion；⌘K 同 Slack/Linear/Cursor）" },
      { keys: ["/"], description: "聚焦搜索框（GitHub/Linear 习惯）" },
      { keys: ["n"], description: "展开新建任务表单 + focus 标题输入" },
      { keys: ["↑", "↓"], description: "上下移动键盘焦点行" },
      { keys: ["Home", "End"], description: "跳到第一条 / 最后一条" },
      { keys: ["空格"], description: "勾选 / 取消勾选当前焦点行（多选）" },
      { keys: ["Enter"], description: "展开 / 折叠当前焦点行的详情" },
      { keys: ["d"], description: "把当前焦点行标 done（pending / error 才响应）" },
      { keys: ["r"], description: "重试当前焦点行（仅 error）" },
      { keys: ["p"], description: "切换当前焦点行 pinned（与右键菜单「📌 钉住」对偶；所有 status 都响应）" },
      { keys: ["Delete", "Backspace"], description: "打开当前焦点行的「取消任务」原因输入" },
    ],
  },
  {
    title: "搜索输入框",
    scope: "记忆 / 任务 / 跨会话搜索三处共享同模式",
    items: [
      { keys: ["Esc"], description: "非空时清掉 query（保持焦点继续敲）；空 input 让出键位走全局 Esc" },
      { keys: ["Enter"], description: "把当前 query 入历史（datalist 浮自动补全可选）" },
    ],
  },
  {
    title: "聊天输入框（PanelChat / 桌面 ChatPanel）",
    scope: "两个聊天输入框共享 shell 风历史栈（cap 20 · localStorage 持久 · 跨窗口）",
    items: [
      { keys: ["↑"], description: "空输入或正在浏览历史 → 拉上一条；多按继续往前翻" },
      { keys: ["↓"], description: "历史浏览中 → 反向；超过最新一条退出 + 清空" },
      {
        keys: ["⌥↑", "Alt+↑"],
        description:
          "PanelChat：空 input 时 IM 风召回最近一条 user bubble 进 inline 编辑模式（直接到 textarea，Enter 重发）",
      },
      {
        keys: ["双击 user 气泡"],
        description: "PanelChat：进入 inline 编辑；Enter 重发（截断 items[i:] + messagesRef）",
      },
    ],
  },
  {
    title: "桌面气泡 / ChatMini",
    scope: "桌面宠物窗口（不在 panel）",
    items: [
      { keys: ["Esc"], description: "streaming 中：取消生成（已写出的内容保留 + [已取消] 标记）" },
      { keys: ["Esc"], description: "空闲 + 焦点在 ChatPanel textarea：清空草稿（ChatPanel 本地 handle）" },
      { keys: ["⌘C", "Ctrl+C"], description: "选区为空时复制最近 assistant 一条；有选区走原生复制" },
      { keys: ["Shift+G"], description: "vim 风格跳到 mini chat 末尾 + 重启 follow-tail" },
      { keys: ["双击气泡"], description: "打开 Panel chat 页（与右上角 ⛶ 等价）" },
    ],
  },
];

/// 单个按键 chip 样式：mac-style 双层 box-shadow（外层 inset 模拟键帽边缘
/// 高光，下方一道实阴影模拟键帽厚度），让 chip 看起来"立体可按"。
const KEY_CHIP_STYLE: React.CSSProperties = {
  fontFamily: "'SF Mono', 'Menlo', monospace",
  fontSize: 11,
  padding: "2px 8px",
  borderRadius: 5,
  background: "var(--pet-color-card)",
  border: "1px solid var(--pet-color-border)",
  color: "var(--pet-color-fg)",
  marginRight: 4,
  marginBottom: 2,
  whiteSpace: "nowrap",
  fontWeight: 500,
  letterSpacing: 0.2,
  boxShadow:
    "inset 0 -1px 0 color-mix(in srgb, var(--pet-color-border) 60%, transparent), 0 1px 1px color-mix(in srgb, var(--pet-color-fg) 6%, transparent)",
  display: "inline-block",
};

interface Props {
  visible: boolean;
  onClose: () => void;
}

export function KeyboardHelpOverlay({ visible, onClose }: Props) {
  return (
    <Modal open={visible} onClose={onClose} maxWidth={560} zIndex={2500}>
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "baseline",
          marginBottom: 16,
        }}
      >
        <h2 style={{ margin: 0, fontSize: 16, fontWeight: 600 }}>键盘快捷键</h2>
        <span style={{ fontSize: 11, color: "var(--pet-color-muted)" }}>
          点背景或按 Esc 关闭
        </span>
      </div>
      {GROUPS.map((g) => (
        <div key={g.title} style={{ marginBottom: 18 }}>
          <SectionTitle subtitle={g.scope}>{g.title}</SectionTitle>
          <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
            {g.items.map((s, i) => (
              <div
                key={i}
                style={{
                  display: "flex",
                  alignItems: "baseline",
                  gap: 10,
                }}
              >
                <div style={{ flexShrink: 0, minWidth: 130 }}>
                  {s.keys.map((k) => (
                    <span key={k} style={KEY_CHIP_STYLE}>
                      {k}
                    </span>
                  ))}
                </div>
                <span style={{ fontSize: 12, color: "var(--pet-color-fg)", lineHeight: 1.55 }}>
                  {s.description}
                </span>
              </div>
            ))}
          </div>
        </div>
      ))}
    </Modal>
  );
}
