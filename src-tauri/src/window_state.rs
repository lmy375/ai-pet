//! 053-part3: persist main window visibility across launches.
//!
//! 写一个轻量 plain-text 单 bool 文件到 pet 数据目录。被 lib.rs setup
//! 在启动时读、各 hide / show 路径写。文件缺失 / 解析失败 / IO 失败一律
//! 退化到「显示主窗口」语义 —— 出错也不让 pet 隐身把用户吓懵。

use std::fs;
use std::path::PathBuf;

const FILE_NAME: &str = "main_window_visible";

fn state_path() -> Option<PathBuf> {
    let dir = dirs::config_dir()?.join("pet");
    Some(dir.join(FILE_NAME))
}

/// 读上次保存的主窗口可见性。文件不存在 → None（首次启动 → 默认显）。
/// 文件存在但内容非「true」/「false」一律 None（被损坏的状态文件不应
/// 让 pet 隐身）。
pub fn load_main_visible() -> Option<bool> {
    let p = state_path()?;
    let content = fs::read_to_string(&p).ok()?;
    match content.trim() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

/// 写当前主窗口可见性。失败静默吞 —— 状态持久化是 UX 增强，不该把
/// IO 异常上抛影响主流程。父目录不存在时尝试创建。
pub fn save_main_visible(visible: bool) {
    let Some(p) = state_path() else { return };
    if let Some(parent) = p.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(&p, if visible { "true" } else { "false" });
}
