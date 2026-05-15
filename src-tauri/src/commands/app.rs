//! App-level meta commands（version / build-time 等）。与 commands/window.rs
//! 同层；不归到 db.rs 因为不依赖 SQLite。

/// 返回 Cargo.toml 里声明的 app 版本号（如 "0.1.0"）。前端 PanelSettings 显在
/// SQLite stats chip 行最前面，让用户一眼自检"我跑的是哪个版本"。
///
/// 用 `env!()` 编译期取值 —— 与 build 输出一致，不可能 drift。
#[tauri::command]
pub fn app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_version_returns_cargo_pkg_version() {
        // 至少非空 + 符合 semver 字符集（数字 + 点）；具体值随 release bump，
        // 不写死避免每次升版改测试。
        let v = app_version();
        assert!(!v.is_empty(), "version should not be empty");
        assert!(
            v.chars().all(|c| c.is_ascii_digit() || c == '.' || c.is_ascii_alphabetic() || c == '-'),
            "version unexpected charset: {v}"
        );
    }
}
