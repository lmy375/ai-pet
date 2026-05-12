use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

/// `~/.config/pet/memories`. Pub(crate) so sibling commands（如 task 详情页读
/// detail.md）能拼出绝对路径而不必 hard-code 路径模板。
pub(crate) fn memories_dir() -> Result<PathBuf, String> {
    let dir = dirs::config_dir()
        .ok_or_else(|| "Cannot determine config directory".to_string())?
        .join("pet")
        .join("memories");
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create memories dir: {e}"))?;
    Ok(dir)
}

fn index_path() -> Result<PathBuf, String> {
    Ok(memories_dir()?.join("index.yaml"))
}

#[derive(Serialize)]
pub struct MemoryDiskUsage {
    pub total_bytes: u64,
    pub file_count: u64,
}

/// 递归扫 memories dir 加总字节数 + 文件计数。给 PanelMemory 头部显存储占用
/// 用，让用户感知"该 consolidate 了"。出错（dir 不存在 / 没权限）→ Err 透传；
/// 实践中 memories_dir() 上面 create_dir_all 已建好。
#[tauri::command]
pub fn memory_disk_usage() -> Result<MemoryDiskUsage, String> {
    let dir = memories_dir()?;
    let mut total_bytes = 0u64;
    let mut file_count = 0u64;
    // 显式 stack 模拟递归（memories 子目录可能套到 categorized 目录或 detail.md
    // 子级；防误用真递归栈打爆）。实际深度通常 ≤ 2 但循环防御。
    let mut stack = vec![dir];
    while let Some(d) = stack.pop() {
        let Ok(entries) = fs::read_dir(&d) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            if meta.is_dir() {
                stack.push(path);
            } else if meta.is_file() {
                total_bytes += meta.len();
                file_count += 1;
            }
        }
    }
    Ok(MemoryDiskUsage {
        total_bytes,
        file_count,
    })
}

fn now_iso() -> String {
    chrono::Local::now()
        .format("%Y-%m-%dT%H:%M:%S%:z")
        .to_string()
}

/// Sanitize a title into a safe filename (lowercase, replace non-alnum with _)
fn title_to_filename(title: &str) -> String {
    let s: String = title
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = s.trim_matches('_').to_string();
    if trimmed.is_empty() {
        "untitled".to_string()
    } else {
        trimmed
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryItem {
    pub title: String,
    pub description: String,
    #[serde(default)]
    pub detail_path: String,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryData {
    pub label: String,
    pub items: Vec<MemoryItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryIndex {
    pub version: u32,
    pub categories: BTreeMap<String, CategoryData>,
}

/// Iter R18: shared helper for the "look up a single item by title in
/// `ai_insights`" pattern that was duplicated 7+ times across proactive.rs
/// and consolidate.rs (find persona_summary / daily_plan / daily_review_*).
/// Returns a cloned `MemoryItem` so callers can take description /
/// updated_at / detail_path without holding a borrow into the index.
///
/// Returns `None` for any failure mode (memory_list error, missing
/// category, missing title) — callers that want to distinguish these
/// rare failure shapes can still call `memory_list` directly. So far no
/// caller has needed that level of detail.
pub fn read_ai_insights_item(title: &str) -> Option<MemoryItem> {
    // v10: kv_state 优先（mirror 双写让它与 yaml 同步；快路径单 SELECT）。
    // 不存在 → fallback yaml（升级用户首次启动 + backfill 之前能读到）。
    if let Some((value, updated_at)) = crate::db::kv_get_with_updated_at(title) {
        // detail_path / created_at 不在 kv_state；ai_insights 实践中
        // 这两个字段都是空 / 不被 caller 使用（grep 验证），构造空串。
        return Some(MemoryItem {
            title: title.to_string(),
            description: value,
            detail_path: String::new(),
            created_at: updated_at.clone(),
            updated_at,
        });
    }
    let index = memory_list(Some("ai_insights".to_string())).ok()?;
    let cat = index.categories.get("ai_insights")?;
    cat.items.iter().find(|i| i.title == title).cloned()
}

impl Default for MemoryIndex {
    fn default() -> Self {
        let mut categories = BTreeMap::new();
        categories.insert(
            "ai_insights".to_string(),
            CategoryData {
                label: "AI 思考与经验".to_string(),
                items: vec![],
            },
        );
        categories.insert(
            "user_profile".to_string(),
            CategoryData {
                label: "用户习惯".to_string(),
                items: vec![],
            },
        );
        categories.insert(
            "todo".to_string(),
            CategoryData {
                label: "当前任务".to_string(),
                items: vec![],
            },
        );
        categories.insert(
            "butler_tasks".to_string(),
            CategoryData {
                label: "管家任务".to_string(),
                items: vec![],
            },
        );
        categories.insert(
            "task_archive".to_string(),
            CategoryData {
                label: "任务归档".to_string(),
                items: vec![],
            },
        );
        categories.insert(
            "general".to_string(),
            CategoryData {
                label: "其他".to_string(),
                items: vec![],
            },
        );
        Self {
            version: 1,
            categories,
        }
    }
}

fn read_index() -> MemoryIndex {
    let path = match index_path() {
        Ok(p) => p,
        Err(_) => return MemoryIndex::default(),
    };
    let mut index: MemoryIndex = match fs::read_to_string(&path) {
        Ok(content) => serde_yaml::from_str(&content).unwrap_or_default(),
        Err(_) => MemoryIndex::default(),
    };
    // 老 index 文件可能缺少新引入的默认 category（例如 task_archive 是
    // 后加入的归档类目）。每次读盘时把 default 里有但本地没有的 category
    // 补回来，保证 memory_edit("create", "task_archive", ...) 不会被
    // "Unknown category" 拒绝。已存在的同名 category 不动，避免覆盖用户
    // 手动改过的 label / items。
    let defaults = MemoryIndex::default();
    for (key, data) in defaults.categories {
        index.categories.entry(key).or_insert(data);
    }
    index
}

fn write_index(index: &MemoryIndex) -> Result<(), String> {
    let path = index_path()?;
    let yaml =
        serde_yaml::to_string(index).map_err(|e| format!("Failed to serialize index: {e}"))?;
    fs::write(&path, yaml).map_err(|e| format!("Failed to write index: {e}"))
}

// ---- Tauri commands ----

#[tauri::command]
pub fn memory_list(category: Option<String>) -> Result<MemoryIndex, String> {
    let mut index = read_index();
    // v6 / v7 / v8: butler_tasks / todo / task_archive 段从 SQLite 取代 yaml。
    // yaml 仍保留段（by v4/v7/v8 mirror 双写早已与 SQLite 同步；orphan
    // 不删，让回滚到旧版本仍能工作）。这里在 read 时**覆盖** items 列
    // 表 —— caller（含 LLM memory_list 工具）拿到的永远是 SQLite 真相。
    if let Some(cat) = index.categories.get_mut("butler_tasks") {
        cat.items = crate::db::butler_tasks_as_memory_items();
    }
    if let Some(cat) = index.categories.get_mut("todo") {
        cat.items = crate::db::todos_as_memory_items();
    }
    if let Some(cat) = index.categories.get_mut("task_archive") {
        cat.items = crate::db::task_archive_as_memory_items();
    }
    if let Some(cat) = category {
        // Return only the requested category
        let mut filtered = MemoryIndex {
            version: index.version,
            categories: BTreeMap::new(),
        };
        if let Some(data) = index.categories.get(&cat) {
            filtered.categories.insert(cat, data.clone());
        }
        Ok(filtered)
    } else {
        Ok(index)
    }
}

#[tauri::command]
pub fn memory_search(keyword: String) -> Result<Vec<(String, MemoryItem)>, String> {
    let mut index = read_index();
    // v6 / v7 / v8：butler_tasks / todo / task_archive 段从 SQLite 取
    // （与 memory_list 同步），让 search 看到的也是 SQLite 真相。orphan
    // yaml items 在此被覆盖，不会出现在结果里。
    if let Some(cat) = index.categories.get_mut("butler_tasks") {
        cat.items = crate::db::butler_tasks_as_memory_items();
    }
    if let Some(cat) = index.categories.get_mut("todo") {
        cat.items = crate::db::todos_as_memory_items();
    }
    if let Some(cat) = index.categories.get_mut("task_archive") {
        cat.items = crate::db::task_archive_as_memory_items();
    }
    let kw = keyword.to_lowercase();
    let mut results = Vec::new();
    for (cat_name, cat_data) in &index.categories {
        for item in &cat_data.items {
            if item.title.to_lowercase().contains(&kw)
                || item.description.to_lowercase().contains(&kw)
            {
                results.push((cat_name.clone(), item.clone()));
            }
        }
    }
    Ok(results)
}

#[tauri::command]
pub fn memory_edit(
    action: String,
    category: String,
    title: String,
    description: Option<String>,
    detail_content: Option<String>,
) -> Result<String, String> {
    // 拦截 ai_insights/current_mood：心情已迁出 memory，由 mood_state_path()
    // 单独存。LLM 仍习惯通过 memory_edit 写心情，所以本拦截把 LLM 的
    // create / update / delete 透明转写到文件，而不真的进 memory index。
    // 这样 PanelMemory 列表里不会出现 current_mood 条目（不可编辑/删除），
    // 但 LLM 端的 prompt 不需要改。
    if category == crate::mood::MOOD_CATEGORY && title == crate::mood::MOOD_TITLE {
        match action.as_str() {
            "create" | "update" => {
                let desc = description.unwrap_or_default();
                crate::mood::record_current_mood(&desc);
                return Ok("Mood updated.".to_string());
            }
            "delete" => {
                crate::mood::clear_current_mood();
                return Ok("Mood cleared.".to_string());
            }
            _ => {}
        }
    }

    let mut index = read_index();
    let now = now_iso();
    let mem_dir = memories_dir()?;

    // Ensure category exists
    if !index.categories.contains_key(&category) {
        return Err(format!("Unknown category: {category}"));
    }

    match action.as_str() {
        "create" => {
            let desc = description.unwrap_or_default();
            let filename = title_to_filename(&title);

            // Ensure category subdirectory exists
            let cat_dir = mem_dir.join(&category);
            fs::create_dir_all(&cat_dir)
                .map_err(|e| format!("Failed to create category dir: {e}"))?;

            // Generate unique filename
            let mut detail_path = format!("{}/{}.md", category, filename);
            let mut full_path = mem_dir.join(&detail_path);
            let mut counter = 1u32;
            while full_path.exists() {
                detail_path = format!("{}/{}_{}.md", category, filename, counter);
                full_path = mem_dir.join(&detail_path);
                counter += 1;
            }

            // Write detail md if provided
            if let Some(content) = detail_content {
                fs::write(&full_path, &content)
                    .map_err(|e| format!("Failed to write detail file: {e}"))?;
            } else {
                fs::write(&full_path, "")
                    .map_err(|e| format!("Failed to write detail file: {e}"))?;
            }

            let item = MemoryItem {
                title,
                description: desc,
                detail_path: detail_path.clone(),
                created_at: now.clone(),
                updated_at: now,
            };

            index
                .categories
                .get_mut(&category)
                .unwrap()
                .items
                .push(item.clone());
            write_index(&index)?;

            // SQLite v4 / v7 / v8 / v10 双写：业务态 best-effort 镜像到 db。
            // ai_insights 用 kv_state（单值条目 schema）。
            match category.as_str() {
                "butler_tasks" => crate::db::mirror_butler_create(&item),
                "todo" => crate::db::mirror_todo_create(&item),
                "task_archive" => crate::db::mirror_archive_create(&item),
                "ai_insights" => crate::db::mirror_ai_insights_create(&item),
                _ => {}
            }

            Ok(format!("Created. Detail path: {detail_path}"))
        }

        "update" => {
            let cat_data = index.categories.get_mut(&category).unwrap();
            let item = cat_data
                .items
                .iter_mut()
                .find(|i| i.title == title)
                .ok_or_else(|| format!("Memory not found: '{title}' in {category}"))?;

            if let Some(desc) = description {
                item.description = desc;
            }
            item.updated_at = now;

            // Update detail file content if provided
            if let Some(content) = detail_content {
                let full_path = mem_dir.join(&item.detail_path);
                fs::write(&full_path, &content)
                    .map_err(|e| format!("Failed to write detail file: {e}"))?;
            }

            // Snapshot for SQLite mirror（避免在 write_index 后再借 index）
            let mirror_kind: Option<&str> = match category.as_str() {
                "butler_tasks" => Some("butler"),
                "todo" => Some("todo"),
                "task_archive" => Some("archive"),
                "ai_insights" => Some("ai_insights"),
                _ => None,
            };
            let mirror_item = mirror_kind.map(|_| item.clone());

            write_index(&index)?;
            match (mirror_kind, mirror_item) {
                (Some("butler"), Some(snapshot)) => crate::db::mirror_butler_update(&snapshot),
                (Some("todo"), Some(snapshot)) => crate::db::mirror_todo_update(&snapshot),
                (Some("archive"), Some(snapshot)) => crate::db::mirror_archive_update(&snapshot),
                (Some("ai_insights"), Some(snapshot)) => {
                    crate::db::mirror_ai_insights_update(&snapshot)
                }
                _ => {}
            }
            Ok("Updated.".to_string())
        }

        "delete" => {
            let cat_data = index.categories.get_mut(&category).unwrap();
            let pos = cat_data
                .items
                .iter()
                .position(|i| i.title == title)
                .ok_or_else(|| format!("Memory not found: '{title}' in {category}"))?;

            let removed = cat_data.items.remove(pos);
            let removed_title = removed.title.clone();

            // Delete detail file
            let full_path = mem_dir.join(&removed.detail_path);
            if full_path.exists() {
                let _ = fs::remove_file(&full_path);
            }

            write_index(&index)?;
            match category.as_str() {
                "butler_tasks" => crate::db::mirror_butler_delete(&removed_title),
                "todo" => crate::db::mirror_todo_delete(&removed_title),
                "task_archive" => crate::db::mirror_archive_delete(&removed_title),
                "ai_insights" => crate::db::mirror_ai_insights_delete(&removed_title),
                _ => {}
            }
            Ok("Deleted.".to_string())
        }

        _ => Err(format!(
            "Unknown action: {action}. Use create/update/delete."
        )),
    }
}

/// 给 memory item 改名：移 detail.md 文件 + 更新 index 里的 title / detail_path。
/// 命中 ai_insights/current_mood 拒绝（心情不可改名）；目标 new_title 与
/// 同 category 其它 item 重名拒绝（避免 detail.md 文件覆盖）。trim 空值
/// 当 noop。
///
/// 给 PanelTasks task title 双击 inline 改名用；理论上 memory tab 也能复用。
#[tauri::command]
pub fn memory_rename(
    category: String,
    old_title: String,
    new_title: String,
) -> Result<String, String> {
    let new_trimmed = new_title.trim().to_string();
    if new_trimmed.is_empty() {
        return Err("new title must not be empty".to_string());
    }
    if new_trimmed == old_title {
        // noop
        return Ok("No change.".to_string());
    }
    // 心情类不可改名（迁出 memory 后没有真实 index 项）
    if category == crate::mood::MOOD_CATEGORY && old_title == crate::mood::MOOD_TITLE {
        return Err("current_mood is not renameable".to_string());
    }

    let mut index = read_index();
    let mem_dir = memories_dir()?;
    let cat_data = index
        .categories
        .get_mut(&category)
        .ok_or_else(|| format!("Unknown category: {category}"))?;
    // 重名检查：同 category 内 new_title 已被占用 → 拒
    if cat_data.items.iter().any(|i| i.title == new_trimmed) {
        return Err(format!(
            "Title already exists in {category}: '{new_trimmed}'"
        ));
    }
    let pos = cat_data
        .items
        .iter()
        .position(|i| i.title == old_title)
        .ok_or_else(|| format!("Memory not found: '{old_title}' in {category}"))?;

    // 算新 detail_path（同 create 路径用 title_to_filename，碰撞时加 _N 后缀）
    let new_filename = title_to_filename(&new_trimmed);
    let cat_dir = mem_dir.join(&category);
    fs::create_dir_all(&cat_dir).map_err(|e| format!("Failed to create category dir: {e}"))?;
    let mut new_detail_path = format!("{}/{}.md", category, new_filename);
    let mut new_full_path = mem_dir.join(&new_detail_path);
    let mut counter = 1u32;
    while new_full_path.exists() {
        new_detail_path = format!("{}/{}_{}.md", category, new_filename, counter);
        new_full_path = mem_dir.join(&new_detail_path);
        counter += 1;
    }

    // 移文件：旧路径存在才移；不存在视为"detail 从未写过"，直接建空文件
    // 让新 path 落地（保持与 create 路径一致："index 有项 = 文件应存在"）。
    let item = &mut cat_data.items[pos];
    let old_full_path = mem_dir.join(&item.detail_path);
    if old_full_path.exists() {
        fs::rename(&old_full_path, &new_full_path).map_err(|e| {
            format!(
                "Failed to move detail file from {} to {}: {}",
                item.detail_path, new_detail_path, e
            )
        })?;
    } else {
        fs::write(&new_full_path, "")
            .map_err(|e| format!("Failed to create new detail file: {e}"))?;
    }

    item.title = new_trimmed.clone();
    item.detail_path = new_detail_path.clone();
    item.updated_at = now_iso();
    // Snapshot before write_index so we can mirror to SQLite without
    // re-borrowing index.
    let mirror_kind: Option<&str> = match category.as_str() {
        "butler_tasks" => Some("butler"),
        "todo" => Some("todo"),
        "task_archive" => Some("archive"),
        "ai_insights" => Some("ai_insights"),
        _ => None,
    };
    let mirror_item = mirror_kind.map(|_| item.clone());
    write_index(&index)?;
    match (mirror_kind, mirror_item) {
        (Some("butler"), Some(snapshot)) => crate::db::mirror_butler_rename(&old_title, &snapshot),
        (Some("todo"), Some(snapshot)) => crate::db::mirror_todo_rename(&old_title, &snapshot),
        (Some("archive"), Some(snapshot)) => crate::db::mirror_archive_rename(&old_title, &snapshot),
        (Some("ai_insights"), Some(snapshot)) => {
            crate::db::mirror_ai_insights_rename(&old_title, &snapshot)
        }
        _ => {}
    }

    Ok(format!("Renamed to '{new_trimmed}'."))
}

/// 读 memory item 的 detail.md 内容前缀（默认 600 字符），供 PanelMemory
/// PanelMemory item 列表行的 "detail X 字" 小灰字指示用 —— 一次性扫
/// 所有 detail.md 算 Unicode code-point 数（与编辑态 counter 同方法，
/// 对中文 / emoji 正确）。返回 `detail_path → char_count` map。
///
/// 失败容忍：单个文件读不到（NotFound / 权限）→ 该 path 不进 map（前
/// 端按"无数据"渲染：不显字数）。一次 panel mount 调一次，跨 < 100 文
/// 件读 + char iter 在 ms 量级。不返 byte size：UTF-8 中文每字 3B，
/// byte 与 char 数差 3x，与编辑态 "X 字" 信号会不一致。
///
/// 安全：与 memory_read_detail 同 path traversal 防御（canonicalize +
/// starts_with check），任何越界 detail_path 段静默跳过不进 map。
#[tauri::command]
pub fn memory_detail_sizes() -> Result<std::collections::HashMap<String, usize>, String> {
    let mem_dir = memories_dir()?;
    let mem_canon = match fs::canonicalize(&mem_dir) {
        Ok(p) => p,
        Err(_) => return Ok(std::collections::HashMap::new()),
    };
    let index = read_index();
    let mut out: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for cat in index.categories.values() {
        for item in &cat.items {
            let p = item.detail_path.trim();
            if p.is_empty() || p.contains("..") || p.starts_with('/') {
                continue;
            }
            let full = mem_dir.join(p);
            let Ok(full_canon) = fs::canonicalize(&full) else {
                continue;
            };
            if !full_canon.starts_with(&mem_canon) {
                continue;
            }
            let Ok(content) = fs::read_to_string(&full_canon) else {
                continue;
            };
            out.insert(p.to_string(), content.chars().count());
        }
    }
    Ok(out)
}

/// hover preview 用。安全：相对路径必须落在 memories_dir 之内；包含 `..`
/// 段直接拒绝防 path traversal。文件不存在 / 太长都返 ""（空字符串作"无
/// 预览可显"语义），不抛 error 让前端 hover UX 平稳。
#[tauri::command]
pub fn memory_read_detail(detail_path: String) -> Result<String, String> {
    const PREVIEW_MAX: usize = 600;
    let trimmed = detail_path.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    // path traversal 守门：不允许 `..` 段或绝对路径
    if trimmed.contains("..") || trimmed.starts_with('/') {
        return Err("invalid detail_path".to_string());
    }
    let mem_dir = memories_dir()?;
    let full = mem_dir.join(trimmed);
    // canonicalize 后再检查是否落在 mem_dir 下面（更稳的安全检查）；
    // 文件不存在时 canonicalize 失败，直接返空（无预览）。
    let mem_canon = match fs::canonicalize(&mem_dir) {
        Ok(p) => p,
        Err(_) => return Ok(String::new()),
    };
    let full_canon = match fs::canonicalize(&full) {
        Ok(p) => p,
        Err(_) => return Ok(String::new()), // 文件不存在 = 没有 detail
    };
    if !full_canon.starts_with(&mem_canon) {
        return Err("detail_path escaped memories_dir".to_string());
    }
    let content = match fs::read_to_string(&full_canon) {
        Ok(s) => s,
        Err(_) => return Ok(String::new()),
    };
    // 按 char（非 byte）截断，避免切到多字节 emoji / 中文中间
    let chars: Vec<char> = content.chars().collect();
    if chars.len() <= PREVIEW_MAX {
        Ok(content)
    } else {
        let head: String = chars.iter().take(PREVIEW_MAX).collect();
        Ok(format!("{head}…"))
    }
}

/// 同 `memory_read_detail` 的 path traversal 防御 + 同结构 fast-path，但
/// 不截断 —— 给"复制 detail.md 全文"路径用。文件不存在 / 读失败仍返空字
/// 符串（与 read_detail 同语义，保证前端 clipboard.writeText 不抛）。
#[tauri::command]
pub fn memory_read_detail_full(detail_path: String) -> Result<String, String> {
    let trimmed = detail_path.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    if trimmed.contains("..") || trimmed.starts_with('/') {
        return Err("invalid detail_path".to_string());
    }
    let mem_dir = memories_dir()?;
    let full = mem_dir.join(trimmed);
    let mem_canon = match fs::canonicalize(&mem_dir) {
        Ok(p) => p,
        Err(_) => return Ok(String::new()),
    };
    let full_canon = match fs::canonicalize(&full) {
        Ok(p) => p,
        Err(_) => return Ok(String::new()),
    };
    if !full_canon.starts_with(&mem_canon) {
        return Err("detail_path escaped memories_dir".to_string());
    }
    fs::read_to_string(&full_canon).or(Ok(String::new()))
}
