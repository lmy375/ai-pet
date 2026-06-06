# 050 · monolith 拆分

rust ≤ 1000 / tsx ≤ 600。测试 `#[cfg(test)] #[path] mod tests;`；prod
`#[path] mod foo; pub use foo::*;`。`pnpm check:file-cap` 防回归。

- ✅ proactive 7376→96。17..22 测试 sibling 抽 10 文件（详见 git log）。
- ✅ 050-24/25 prod 拆：tg/commands utility → command_utils.rs（224 行）+
  due_preset 簇 → due_preset.rs（173 行）。tg/commands 8277→7890。
- ⏳ ≥ 1000 剩：tg/commands 7890 / tg/bot 3726 / ChatMini 3607 / chat 1449
  / db 1324 / task 1218 / memory 1195 / debug 1055。
