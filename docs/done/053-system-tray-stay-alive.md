# 053 · system tray + minimize-to-tray

后台陪伴前提：close window 不 quit，proactive / TG / notification 要 pet 活着。

- ✅ part1：`tauri` 加 `tray-icon` feature；tray + menu（显/mute30m/2h/解除/退出），
  左键 toggle main；main `CloseRequested` → prevent + hide。
- ✅ part2：`unread_tray.rs` 静态计数；3 emit 路径 main 隐时 bump tooltip
  「Pet · N 条未读」；main focus / tray-show / 左键展开 → clear。
- ✅ part3：`window_state.rs` 持久化主窗可见性；hide/show 三入口写文件，
  启动读回 restore（首启动 / 文件损坏退化到默认显）。
- Deferred 视觉 polish：mute 灰显 tray / custom pet 头像 icon / 「今晚」
  自然语言 mute — 均需 icon 资源 + 视觉迭代，不阻塞 core。
