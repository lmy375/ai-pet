# macOS 自动化：读取与操作应用

宠物没有把 AX 自动化做成工具 —— 它通过 `bash` 工具跑 `osascript`（AppleScript /
JXA）来读取和操作你的 App。能力是现成的，需要的只是 **OS 权限**。

仓库另带一个独立的 macOS Accessibility CLI：`axcu`（`crates/ax-computer-use`），
用于手工查看/操作 App。它是可选的调试工具，宠物运行时不依赖它 —— 见文末
[可选的 axcu CLI](#可选的-axcu-cli)。

## 两条路径

- **可脚本化的 App**（Terminal、Finder、备忘录、邮件、Safari 等）：用 AppleScript
  直接驱动，最稳。
  ```
  osascript -e 'tell application "Terminal" to do script "pwd; whoami"'
  ```
- **不可脚本化的 App**（如微信）：用 System Events 做 GUI 自动化 —— `activate` 唤起、
  `keystroke` / `key code` 模拟键入、`click` 点按钮。脆但通吃。
  例：唤起微信 → 打开「文件传输助手」会话 → 键入内容 → `key code 36`（回车）发送。

## 读取窗口文字

优先用 System Events 读 UI 元素的结构化文本（`value` / `title`），纯文本、便宜、可
逐字精确。**截图（`screenshot` 工具）走视觉模型、较贵**，仅在 AX 树太稀疏（微信、部分
Electron）或确实需要看视觉布局时才用。

## 截某个 App 的窗口

`screenshot` 工具可传 `app` 参数，只截该 App 的窗口（按窗口 ID 截，复用屏幕录制权限、
不受遮挡影响）；不传则截整屏。

## 权限（System Settings › Privacy & Security）

| 权限 | 用途 |
| --- | --- |
| 屏幕录制 | 截图 |
| 辅助功能 | System Events 模拟按键/点击、读 AX |
| 自动化 | 驱动可脚本化 App（每个目标 App 首次单独弹窗） |

> 开发版每次重新编译可能丢失/重弹授权（签名身份不稳定）；正式打包后一劳永逸。命令若报
> 权限错误，是需要在系统设置里授权，而非脚本有误。
## 可选的 axcu CLI

`crates/ax-computer-use` 把 Accessibility API 封装成一个独立 CLI（bin 名 `axcu`），
与宠物的 `bash`/osascript 路径互不影响，适合手工排查 AX 树或写脚本。

```bash
cargo run -p ax-computer-use -- list-apps          # 按前台顺序列出 App
cargo run -p ax-computer-use -- state Safari       # 打印前台窗口的元素树（带 [index]）
cargo run -p ax-computer-use -- click Safari 3     # 按上一步的 index 点击
cargo run -p ax-computer-use -- type Safari 3 "hi" # 按 index 输入文本
cargo run -p ax-computer-use -- key Safari "cmd+c" # 发送按键/组合键
cargo run -p ax-computer-use -- scroll Safari 7 down
```

`state` 输出的索引在同一次界面状态下稳定；App 的 UI 变化后需重新 `state`。它只在
macOS 上编译/运行，同样需要「辅助功能」权限。
