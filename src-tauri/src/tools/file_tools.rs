use crate::tools::{Tool, ToolContext};

const MAX_LINE_DISPLAY: usize = 2000;

// ---- read_file ----

pub struct ReadFileTool;

impl Tool for ReadFileTool {
    fn name(&self) -> &str {
        "read_file"
    }

    fn definition(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "read_file",
                "description": "Read the contents of a text file. Returns content with line numbers in cat -n format (line_number + tab + content).\n\nUsage:\n- By default reads up to 2000 lines from the beginning of the file.\n- Use offset (1-based line number) and limit to read specific portions of large files.\n- Auto-detects binary files and returns an error instead of garbled content.\n- Always prefer this tool over running cat/head/tail in bash.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "file_path": {
                            "type": "string",
                            "description": "Absolute path to the file to read"
                        },
                        "offset": {
                            "type": "integer",
                            "description": "Line number to start reading from (1-based). Defaults to 1."
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum number of lines to read. Defaults to 2000."
                        }
                    },
                    "required": ["file_path"]
                }
            }
        })
    }

    fn execute<'a>(
        &'a self,
        arguments: &'a str,
        ctx: &'a ToolContext,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = String> + Send + 'a>> {
        Box::pin(read_file_impl(arguments, ctx))
    }
}

async fn read_file_impl(arguments: &str, ctx: &ToolContext) -> String {
    let args: serde_json::Value = serde_json::from_str(arguments).unwrap_or_default();
    let file_path = args["file_path"].as_str().unwrap_or("").to_string();
    if file_path.is_empty() {
        return r#"{"error": "missing 'file_path' parameter"}"#.to_string();
    }

    // Read file once as bytes, then detect binary and convert to string
    let bytes = match std::fs::read(&file_path) {
        Ok(b) => b,
        Err(e) => {
            let kind = e.kind();
            if kind == std::io::ErrorKind::NotFound {
                return format!(r#"{{"error": "file not found: {}"}}"#, file_path);
            }
            return format!(r#"{{"error": "failed to read file: {}"}}"#, e);
        }
    };

    // Binary detection: scan first 8KB for null bytes
    let check_len = bytes.len().min(8192);
    if bytes[..check_len].contains(&0) {
        return serde_json::json!({
            "error": format!("binary file, cannot display: {}", file_path),
            "file_size": bytes.len(),
        })
        .to_string();
    }

    let content = match String::from_utf8(bytes) {
        Ok(s) => s,
        Err(e) => return format!(r#"{{"error": "file is not valid UTF-8: {}"}}"#, e),
    };

    let offset = args["offset"].as_u64().unwrap_or(1).max(1) as usize;
    let limit = args["limit"].as_u64().unwrap_or(MAX_LINE_DISPLAY as u64) as usize;

    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();
    let start = (offset - 1).min(total_lines);
    let end = (start + limit).min(total_lines);
    let selected = &lines[start..end];

    let mut numbered = String::new();
    for (i, line) in selected.iter().enumerate() {
        let line_num = start + i + 1;
        numbered.push_str(&format!("{}\t{}\n", line_num, line));
    }

    if end < total_lines {
        numbered.push_str(&format!(
            "--- truncated (showing lines {}-{} of {}) ---\n",
            start + 1,
            end,
            total_lines
        ));
    }

    ctx.log(&format!(
        "read_file: {} (lines {}-{} of {})",
        file_path,
        start + 1,
        end,
        total_lines
    ));

    serde_json::json!({
        "file_path": file_path,
        "content": numbered,
        "lines_shown": end - start,
        "total_lines": total_lines,
    })
    .to_string()
}

// ---- write_file ----

pub struct WriteFileTool;

impl Tool for WriteFileTool {
    fn name(&self) -> &str {
        "write_file"
    }

    fn definition(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "write_file",
                "description": "Create a new file or completely overwrite an existing file with the given content. Parent directories are created automatically.\n\nIMPORTANT:\n- This tool OVERWRITES the entire file. For modifying existing files, prefer edit_file — it only changes the specific part you need.\n- Only use write_file to create new files or for complete rewrites.\n- Always prefer this over running echo/cat heredoc in bash to create files.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "file_path": {
                            "type": "string",
                            "description": "Absolute path to the file to write"
                        },
                        "content": {
                            "type": "string",
                            "description": "The content to write to the file"
                        }
                    },
                    "required": ["file_path", "content"]
                }
            }
        })
    }

    fn execute<'a>(
        &'a self,
        arguments: &'a str,
        ctx: &'a ToolContext,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = String> + Send + 'a>> {
        Box::pin(write_file_impl(arguments, ctx))
    }
}

async fn write_file_impl(arguments: &str, ctx: &ToolContext) -> String {
    let args: serde_json::Value = serde_json::from_str(arguments).unwrap_or_default();
    let file_path = args["file_path"].as_str().unwrap_or("").to_string();
    let content = args["content"].as_str().unwrap_or("").to_string();

    if file_path.is_empty() {
        return r#"{"error": "missing 'file_path' parameter"}"#.to_string();
    }
    if !args.get("content").is_some_and(|v| v.is_string()) {
        return r#"{"error": "missing 'content' parameter"}"#.to_string();
    }

    // Create parent directories
    if let Some(parent) = std::path::Path::new(&file_path).parent() {
        if !parent.exists() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                return format!(r#"{{"error": "failed to create directories: {}"}}"#, e);
            }
        }
    }

    let bytes_written = content.len();
    if let Err(e) = std::fs::write(&file_path, &content) {
        return format!(r#"{{"error": "failed to write file: {}"}}"#, e);
    }

    ctx.log(&format!(
        "write_file: {} ({} bytes)",
        file_path, bytes_written
    ));

    serde_json::json!({
        "file_path": file_path,
        "status": "ok",
        "bytes_written": bytes_written,
    })
    .to_string()
}

// ---- edit_file ----

pub struct EditFileTool;

impl Tool for EditFileTool {
    fn name(&self) -> &str {
        "edit_file"
    }

    fn definition(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "edit_file",
                "description": "Make an exact string replacement in a file. This is the preferred tool for modifying existing files — it only sends the diff rather than rewriting the entire file.\n\nRules:\n- old_string must match EXACTLY, including whitespace, indentation, and line breaks.\n- The edit FAILS if old_string is not unique in the file (appears more than once). Provide more surrounding context to make it unique, or set replace_all: true.\n- Use replace_all: true for renaming variables/strings across the file.\n- Always read_file first before editing to ensure you have the correct content to match.\n- Prefer this tool over sed/awk in bash for all file modifications.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "file_path": {
                            "type": "string",
                            "description": "Absolute path to the file to edit"
                        },
                        "old_string": {
                            "type": "string",
                            "description": "The exact string to find and replace"
                        },
                        "new_string": {
                            "type": "string",
                            "description": "The replacement string"
                        },
                        "replace_all": {
                            "type": "boolean",
                            "description": "If true, replace all occurrences. If false (default), old_string must appear exactly once."
                        }
                    },
                    "required": ["file_path", "old_string", "new_string"]
                }
            }
        })
    }

    fn execute<'a>(
        &'a self,
        arguments: &'a str,
        ctx: &'a ToolContext,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = String> + Send + 'a>> {
        Box::pin(edit_file_impl(arguments, ctx))
    }
}

async fn edit_file_impl(arguments: &str, ctx: &ToolContext) -> String {
    let args: serde_json::Value = serde_json::from_str(arguments).unwrap_or_default();
    let file_path = args["file_path"].as_str().unwrap_or("").to_string();
    let old_string = args["old_string"].as_str().unwrap_or("").to_string();
    let new_string = args["new_string"].as_str().unwrap_or("").to_string();
    let replace_all = args["replace_all"].as_bool().unwrap_or(false);

    if file_path.is_empty() {
        return r#"{"error": "missing 'file_path' parameter"}"#.to_string();
    }
    if old_string.is_empty() {
        return r#"{"error": "old_string must not be empty"}"#.to_string();
    }

    let content = match std::fs::read_to_string(&file_path) {
        Ok(c) => c,
        Err(e) => return format!(r#"{{"error": "failed to read file: {}"}}"#, e),
    };

    let count = content.matches(&old_string).count();

    if count == 0 {
        return r#"{"error": "old_string not found in file"}"#.to_string();
    }

    if count > 1 && !replace_all {
        return serde_json::json!({
            "error": "old_string appears multiple times; set replace_all: true to replace all, or provide a more specific string",
            "occurrences": count,
        })
        .to_string();
    }

    let new_content = if replace_all {
        content.replace(&old_string, &new_string)
    } else {
        content.replacen(&old_string, &new_string, 1)
    };

    if let Err(e) = std::fs::write(&file_path, &new_content) {
        return format!(r#"{{"error": "failed to write file: {}"}}"#, e);
    }

    ctx.log(&format!(
        "edit_file: {} ({} replacement{})",
        file_path,
        count,
        if count > 1 { "s" } else { "" }
    ));

    serde_json::json!({
        "file_path": file_path,
        "status": "ok",
        "replacements": count,
    })
    .to_string()
}
