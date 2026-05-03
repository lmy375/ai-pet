use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

fn memories_dir() -> Result<PathBuf, String> {
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
    match fs::read_to_string(&path) {
        Ok(content) => serde_yaml::from_str(&content).unwrap_or_default(),
        Err(_) => MemoryIndex::default(),
    }
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
    let index = read_index();
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
    let index = read_index();
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
                .push(item);
            write_index(&index)?;

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

            write_index(&index)?;
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

            // Delete detail file
            let full_path = mem_dir.join(&removed.detail_path);
            if full_path.exists() {
                let _ = fs::remove_file(&full_path);
            }

            write_index(&index)?;
            Ok("Deleted.".to_string())
        }

        _ => Err(format!("Unknown action: {action}. Use create/update/delete.")),
    }
}
