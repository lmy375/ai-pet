use crate::tools::{required_str, tool_error, Tool, ToolContext};

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

    crate::impl_execute!(read_file_impl);
}

async fn read_file_impl(arguments: &str, ctx: &ToolContext) -> String {
    let args = super::parse_args(arguments);
    let file_path = match required_str(&args, "file_path") {
        Ok(v) => v,
        Err(e) => return e,
    };

    // Read file once as bytes, then detect binary and convert to string
    let bytes = match std::fs::read(&file_path) {
        Ok(b) => b,
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                return tool_error(format!("file not found: {}", file_path));
            }
            return tool_error(format!("failed to read file: {}", e));
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
        Err(e) => return tool_error(format!("file is not valid UTF-8: {}", e)),
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
                "description": "Create a new file or completely overwrite an existing file with the given content. Parent directories are created automatically.\n\nIMPORTANT:\n- This tool OVERWRITES the entire file. For modifying existing files, prefer edit_file — it only changes the specific part you need.\n- Only use write_file to create new files or for complete rewrites.\n- Always prefer this over running echo/cat heredoc in bash to create files.\n- Do NOT proactively create documentation files (README, *.md) — only create them when the user explicitly asks.",
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

    crate::impl_execute!(write_file_impl);
}

async fn write_file_impl(arguments: &str, ctx: &ToolContext) -> String {
    let args = super::parse_args(arguments);
    let file_path = match required_str(&args, "file_path") {
        Ok(v) => v,
        Err(e) => return e,
    };
    // `content` may legitimately be "", so require a string rather than non-empty.
    if !args.get("content").is_some_and(|v| v.is_string()) {
        return tool_error("missing 'content' parameter");
    }
    let content = args["content"].as_str().unwrap_or("").to_string();

    // Create parent directories
    if let Some(parent) = std::path::Path::new(&file_path).parent() {
        if !parent.exists() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                return tool_error(format!("failed to create directories: {}", e));
            }
        }
    }

    let bytes_written = content.len();
    if let Err(e) = std::fs::write(&file_path, &content) {
        return tool_error(format!("failed to write file: {}", e));
    }

    ctx.log(&format!("write_file: {} ({} bytes)", file_path, bytes_written));

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

    crate::impl_execute!(edit_file_impl);
}

async fn edit_file_impl(arguments: &str, ctx: &ToolContext) -> String {
    let args = super::parse_args(arguments);
    let file_path = match required_str(&args, "file_path") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let old_string = args["old_string"].as_str().unwrap_or("").to_string();
    let new_string = args["new_string"].as_str().unwrap_or("").to_string();
    let replace_all = args["replace_all"].as_bool().unwrap_or(false);

    if old_string.is_empty() {
        return tool_error("old_string must not be empty");
    }

    let content = match std::fs::read_to_string(&file_path) {
        Ok(c) => c,
        Err(e) => return tool_error(format!("failed to read file: {}", e)),
    };

    let count = content.matches(&old_string).count();

    if count == 0 {
        // 精确匹配失败的第一大原因是模型抄错了缩进（行内内容对、前导空白错）。
        // 退而做「逐行 trim 后唯一匹配」，用文件的真实缩进重建替换文本。
        return match flexible_replace(&content, &old_string, &new_string) {
            FlexibleOutcome::Replaced(new_content) => {
                if let Err(e) = std::fs::write(&file_path, &new_content) {
                    return tool_error(format!("failed to write file: {}", e));
                }
                ctx.log(&format!("edit_file: {} (1 replacement, indent-normalized)", file_path));
                serde_json::json!({
                    "file_path": file_path,
                    "status": "ok",
                    "replacements": 1,
                    "matched": "indent_normalized",
                })
                .to_string()
            }
            FlexibleOutcome::Ambiguous(n) => serde_json::json!({
                "error": format!(
                    "old_string not found exactly, and matches {} locations ignoring \
                     leading/trailing whitespace; provide more surrounding context",
                    n
                ),
            })
            .to_string(),
            FlexibleOutcome::NotFound => {
                // 给出最接近的一行，让模型下一轮能对着真实内容自纠，
                // 而不是拿着同一个错的 old_string 盲试。
                match closest_line(&content, &old_string) {
                    Some((line_no, text)) => tool_error(format!(
                        "old_string not found in file. Closest match is line {}: {:?} — \
                         read_file that region and retry with the exact current content",
                        line_no, text
                    )),
                    None => tool_error("old_string not found in file"),
                }
            }
        };
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
        return tool_error(format!("failed to write file: {}", e));
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

enum FlexibleOutcome {
    Replaced(String),
    Ambiguous(usize),
    NotFound,
}

fn leading_ws(s: &str) -> &str {
    &s[..s.len() - s.trim_start().len()]
}

/// 逐行 trim 后在文件里找 old_string 的唯一匹配窗口；命中则把 new_string 按
/// 「模型写的缩进 → 文件真实缩进」的映射重排后拼回去。多处命中或没命中都不动文件。
fn flexible_replace(content: &str, old_string: &str, new_string: &str) -> FlexibleOutcome {
    let file_lines: Vec<&str> = content.split('\n').collect();
    let old_lines: Vec<&str> = old_string.split('\n').collect();
    if old_lines.iter().all(|l| l.trim().is_empty()) || old_lines.len() > file_lines.len() {
        return FlexibleOutcome::NotFound;
    }

    let matches: Vec<usize> = (0..=file_lines.len() - old_lines.len())
        .filter(|&i| {
            old_lines
                .iter()
                .enumerate()
                .all(|(j, ol)| file_lines[i + j].trim() == ol.trim())
        })
        .collect();

    match matches.len() {
        0 => FlexibleOutcome::NotFound,
        1 => {
            let start = matches[0];
            // 模型声称的缩进 → 该窗口里文件的真实缩进（首见为准）
            let mut indent_map: std::collections::HashMap<&str, &str> =
                std::collections::HashMap::new();
            for (j, ol) in old_lines.iter().enumerate() {
                if !ol.trim().is_empty() {
                    indent_map.entry(leading_ws(ol)).or_insert(leading_ws(file_lines[start + j]));
                }
            }
            let replacement: Vec<String> = new_string
                .split('\n')
                .map(|line| {
                    if line.trim().is_empty() {
                        return line.to_string();
                    }
                    let claimed = leading_ws(line);
                    match indent_map.get(claimed) {
                        Some(actual) => format!("{}{}", actual, &line[claimed.len()..]),
                        None => line.to_string(),
                    }
                })
                .collect();

            let mut out: Vec<String> = Vec::with_capacity(
                file_lines.len() - old_lines.len() + replacement.len(),
            );
            out.extend(file_lines[..start].iter().map(|s| s.to_string()));
            out.extend(replacement);
            out.extend(file_lines[start + old_lines.len()..].iter().map(|s| s.to_string()));
            FlexibleOutcome::Replaced(out.join("\n"))
        }
        n => FlexibleOutcome::Ambiguous(n),
    }
}

/// old_string 第一行内容在文件里最相似的一行（编辑距离），供报错时定位。
fn closest_line(content: &str, old_string: &str) -> Option<(usize, String)> {
    let target = old_string.lines().find(|l| !l.trim().is_empty())?.trim();
    let target: String = target.chars().take(200).collect();
    content
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty())
        .map(|(i, l)| {
            let line: String = l.trim().chars().take(200).collect();
            (levenshtein(&target, &line), i + 1, line)
        })
        .min_by_key(|(d, _, _)| *d)
        // 距离超过目标长度一半就谈不上「接近」，不给误导性提示
        .filter(|(d, _, _)| *d <= target.chars().count().div_ceil(2))
        .map(|(_, line_no, line)| (line_no, line.chars().take(120).collect()))
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0usize; b.len() + 1];
    for i in 1..=a.len() {
        cur[0] = i;
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    // DeepSWE anko 题实测：18 次 edit_file 失败里 15 次是行内内容全对、
    // 只有前导缩进抄错（比如三个 TAB 写成两个）。这类必须能匹配上，
    // 且替换文本要跟着文件的真实缩进走，不能把错误缩进写进文件。
    #[test]
    fn flexible_replace_fixes_wrong_leading_indent() {
        let content = "func f() {\n\t\t\tx := 1\n\t\t\ty := 2\n}\n";
        let old = "func f() {\n\tx := 1\n\ty := 2\n}"; // 缩进错：1 个 TAB
        let new = "func f() {\n\tx := 10\n\ty := 2\n}";
        match flexible_replace(content, old, new) {
            FlexibleOutcome::Replaced(out) => {
                assert_eq!(out, "func f() {\n\t\t\tx := 10\n\t\t\ty := 2\n}\n");
            }
            _ => panic!("expected unique flexible match"),
        }
    }

    // 宽松匹配多处命中时绝不能乱改——报歧义，让模型加上下文。
    #[test]
    fn flexible_replace_refuses_ambiguous_windows() {
        let content = "  a\n  b\n\n  a\n  b\n";
        match flexible_replace(content, "a\nb", "a\nc") {
            FlexibleOutcome::Ambiguous(2) => {}
            _ => panic!("expected ambiguity on two windows"),
        }
    }

    // 内容真不存在时给「最接近的一行」定位；差太远则不给误导性提示。
    #[test]
    fn closest_line_hints_near_miss_only() {
        let content = "let count = compute_total(items);\nreturn count;\n";
        let (line_no, text) =
            closest_line(content, "let count = compute_totals(items);").unwrap();
        assert_eq!(line_no, 1);
        assert!(text.contains("compute_total"));

        assert!(closest_line(content, "完全无关的一行内容啊").is_none());
    }
}
