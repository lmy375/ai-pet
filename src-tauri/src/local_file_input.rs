//! GOAL 031：本地文件输入（005 URL fetch 的本地对偶）。pet 接收 text 文件
//! 时把内容入 prompt，让 LLM 直接对内容下评论而不是凭文件名猜。
//!
//! 边界 / 不做：
//! - 不动 PDF / docx / 图片 / 二进制——MIME 不在白名单一律拒，回退一句
//!   「我打不开这个格式」（GOAL：不静默吞）。PDF 单独留作 follow-up
//!   （要额外 crate + page slicing 决策，超出本刀）。
//! - 不持久化（与 001 photo / 005 url-fetch 同 contract）——本模块只产
//!   in-flight prompt text；不写 PanelMemory，不存原文件。
//! - 单文件字节上限 [`MAX_BYTES`]（与 url_fetch 对齐 1MB），prompt 字符上限
//!   [`MAX_PROMPT_CHARS`] 第二档（防 token 飙）。两道闸都触发时，文末加
//!   「已截断 X 字节 / Y 字符」注。
//!
//! 本模块全是 pure helpers——调用方（TG document handler、未来 ChatMini
//! drop）拿到 `read_text_bytes_capped` 结果后自行决定如何拼 prompt。

/// 单文件字节上限。与 url_fetch::MAX_BYTES 同 1MB，让两个输入入口的
/// 「大致允许多大」直觉一致。再大基本是 PDF / 数据库 dump，token 成本失控。
pub const MAX_BYTES: usize = 1_000_000;

/// 入 prompt 的字符上限。1MB 文本可能是 ~300k 字符，太大稀释指令。20k 字符
/// 够覆盖大部分单篇文档（一本短小说 ~50k 字），同时 token 量级在 5–8k 之内。
pub const MAX_PROMPT_CHARS: usize = 20_000;

/// 检测二进制时扫前多少字节。4KB 经验值——多数文本前 1KB 已能确认（编码
/// 头 / BOM / 纯 ASCII），4KB 留 buffer 给少数 UTF-8 BOM 在头几百字节是
/// 二进制字段的 corner case（如 `.ini` with mixed encoding）。
const BINARY_PROBE_BYTES: usize = 4096;

/// 可读文本扩展白名单（不含点）。覆盖：纯文本 / 配置 / 主流源码语言。
/// PDF / docx / image / archive 故意不在内。新增前先确认 LLM 处理该格式
/// 内容时不需要额外解码（已是 UTF-8 纯文本流）。
pub const TEXT_EXTENSIONS: &[&str] = &[
    "md", "markdown", "txt", "text", "log",
    "json", "yaml", "yml", "toml", "ini", "conf", "cfg",
    "csv", "tsv",
    "py", "rs", "ts", "tsx", "js", "jsx", "mjs", "cjs",
    "go", "java", "kt", "swift", "c", "cpp", "h", "hpp", "rb", "php",
    "html", "htm", "xml", "css", "scss", "sass",
    "sh", "bash", "zsh", "fish",
    "sql", "graphql", "proto",
    "env",
];

/// Pure：从文件名提取小写扩展（不含点）。无扩展 / 路径中无 `.` 返回空串。
pub fn extension_lower(name: &str) -> String {
    name.rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_default()
}

/// Pure：扩展是否在白名单内。
pub fn is_text_extension(ext: &str) -> bool {
    TEXT_EXTENSIONS.contains(&ext)
}

/// Pure：前 4KB 含 NUL byte → 视为二进制。文本文件极少含 \0；二进制几乎
/// 总在前几百字节出现。这条 heuristic 比"看是否全 ASCII"更稳——后者会把
/// UTF-8 中文文本误判。
pub fn looks_binary(bytes: &[u8]) -> bool {
    let probe = &bytes[..bytes.len().min(BINARY_PROBE_BYTES)];
    probe.iter().any(|&b| b == 0)
}

/// 读取后的结果。`content` 已经按字符 cap 截过；`truncated_bytes` /
/// `truncated_chars` 给 caller 决定要不要在 prompt 末尾加「已截断」注。
pub struct FileReadOutcome {
    pub content: String,
    pub total_bytes: usize,
    /// true = 输入字节超过 [`MAX_BYTES`]，content 是字节 slice 后再解码的产物。
    pub byte_capped: bool,
    /// true = 解码后字符数仍超过 [`MAX_PROMPT_CHARS`]，content 在 char 边界
    /// 上又被裁过一次。
    pub char_capped: bool,
}

/// 错误分类——caller 据此回退「我打不开这个格式 / 太大了」类反馈。
#[derive(Debug, PartialEq, Eq)]
pub enum FileReadErr {
    /// 扩展不在白名单（PDF / docx / image / archive 等）。
    UnsupportedExtension(String),
    /// 前 4KB 探到 NUL → 视为二进制流。
    BinaryDetected,
    /// 空文件——零信息量也无法 prompt。
    Empty,
}

impl FileReadErr {
    /// 给用户看的中文一句话。TG / 未来 ChatMini 直接 echo 这个。
    pub fn user_message(&self, name: &str) -> String {
        match self {
            FileReadErr::UnsupportedExtension(ext) => {
                if ext.is_empty() {
                    format!("「{}」没扩展名，我看不出怎么读，暂时打不开。", name)
                } else {
                    format!("我打不开 .{} 格式（{}）。文本类我才能直接看。", ext, name)
                }
            }
            FileReadErr::BinaryDetected => {
                format!("「{}」看起来是二进制文件，我没法当文本读。", name)
            }
            FileReadErr::Empty => format!("「{}」是空的，没东西可读。", name),
        }
    }
}

/// Pure 主入口：name 走扩展白名单，bytes 走二进制 heuristic + 字节 cap +
/// UTF-8 lossy 解码 + 字符 cap。两道 cap 都触发时同时置位。
pub fn read_text_bytes_capped(name: &str, bytes: &[u8]) -> Result<FileReadOutcome, FileReadErr> {
    let ext = extension_lower(name);
    if !is_text_extension(&ext) {
        return Err(FileReadErr::UnsupportedExtension(ext));
    }
    if bytes.is_empty() {
        return Err(FileReadErr::Empty);
    }
    if looks_binary(bytes) {
        return Err(FileReadErr::BinaryDetected);
    }
    let total_bytes = bytes.len();
    let byte_capped = total_bytes > MAX_BYTES;
    let slice = if byte_capped { &bytes[..MAX_BYTES] } else { bytes };
    // UTF-8 lossy：让坏字节替换为 U+FFFD 而不是 fail——TG 用户可能上传
    // GBK 文档等非 UTF-8 文本，lossy 保证 LLM 至少看到大部分字符。
    let decoded = String::from_utf8_lossy(slice).to_string();
    let (content, char_capped) = truncate_chars(&decoded, MAX_PROMPT_CHARS);
    Ok(FileReadOutcome {
        content,
        total_bytes,
        byte_capped,
        char_capped,
    })
}

/// Pure：按 char 数（不是字节）裁。返 (裁后, 是否真的裁过)。char 安全
/// 边界——string slice 上不会切到 UTF-8 sequence 中间。
///
/// 实现：char_indices 给出每个 char 的 *起始* byte index；当 count 已经
/// 等于 max 时，本次 idx 就是「第 max+1 个 char」的起点，slice 到 idx
/// 即恰好包含前 max 个 char。旧版用 `end = idx; count += 1` 顺序错乱，
/// 在最后一次未来得及更新 end 就 return，少裁 1 个 char。
pub fn truncate_chars(s: &str, max_chars: usize) -> (String, bool) {
    let mut count = 0;
    for (idx, _) in s.char_indices() {
        if count == max_chars {
            return (s[..idx].to_string(), true);
        }
        count += 1;
    }
    (s.to_string(), false)
}

/// 拼 LLM 输入：`[文件: <name>]\n<caption_or_empty>\n--- 文件内容 ---\n<content>\n--- ... ---`
/// 末尾按 cap flag 加截断注。空 caption 不渲染空行。
pub fn format_for_prompt(name: &str, caption: &str, outcome: &FileReadOutcome) -> String {
    let mut s = String::new();
    s.push_str(&format!("[文件: {}]\n", name));
    let cap_trim = caption.trim();
    if !cap_trim.is_empty() {
        s.push_str(cap_trim);
        s.push('\n');
    }
    s.push_str("--- 文件内容 ---\n");
    s.push_str(&outcome.content);
    if !outcome.content.ends_with('\n') {
        s.push('\n');
    }
    s.push_str("--- 内容结束 ---");
    if outcome.byte_capped || outcome.char_capped {
        s.push_str(&format!(
            "\n（原文件 {} bytes；已截断{}）",
            outcome.total_bytes,
            match (outcome.byte_capped, outcome.char_capped) {
                (true, true) => "（字节上限 + 字符上限均触发）",
                (true, false) => "（超过字节上限）",
                (false, true) => "（超过字符上限）",
                _ => "",
            }
        ));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_lower_lower_cases_and_handles_no_dot() {
        assert_eq!(extension_lower("README.MD"), "md");
        assert_eq!(extension_lower("path/to/file.TXT"), "txt");
        assert_eq!(extension_lower("Makefile"), "");
        assert_eq!(extension_lower(""), "");
    }

    #[test]
    fn is_text_extension_recognizes_whitelist_and_rejects_other() {
        assert!(is_text_extension("md"));
        assert!(is_text_extension("rs"));
        assert!(is_text_extension("yaml"));
        assert!(!is_text_extension("pdf"));
        assert!(!is_text_extension("docx"));
        assert!(!is_text_extension("png"));
        assert!(!is_text_extension(""));
    }

    #[test]
    fn looks_binary_detects_nul_byte_and_passes_utf8_chinese() {
        let pdf_head: &[u8] = b"%PDF-1.4\x00\x00binary garbage";
        assert!(looks_binary(pdf_head));
        let chinese: Vec<u8> = "中文 markdown 内容\n第二行".as_bytes().to_vec();
        assert!(!looks_binary(&chinese));
        let plain_ascii = b"# Header\n\nbody";
        assert!(!looks_binary(plain_ascii));
    }

    #[test]
    fn read_unsupported_extension_returns_err() {
        let r = read_text_bytes_capped("doc.pdf", b"%PDF-1.4 stuff");
        assert_eq!(r.err(), Some(FileReadErr::UnsupportedExtension("pdf".to_string())));
    }

    #[test]
    fn read_binary_detected_for_whitelisted_ext_with_nul() {
        // 扩展是 .txt 但内容含 NUL → 仍判 BinaryDetected。用户偶尔会把
        // 真二进制 rename 成 .txt 试图绕过，heuristic 不让它过。
        let mut bytes = b"hi there".to_vec();
        bytes.push(0);
        let r = read_text_bytes_capped("a.txt", &bytes);
        assert_eq!(r.err(), Some(FileReadErr::BinaryDetected));
    }

    #[test]
    fn read_empty_returns_err() {
        let r = read_text_bytes_capped("a.md", b"");
        assert_eq!(r.err(), Some(FileReadErr::Empty));
    }

    #[test]
    fn read_caps_chars_for_long_input() {
        // 30k 字符 ascii → 字符 cap 触发；字节也超 (30000 > MAX_BYTES? no
        // 30000 < 1_000_000)，所以仅 char_capped。
        let big: String = "a".repeat(MAX_PROMPT_CHARS + 5_000);
        let r = read_text_bytes_capped("big.txt", big.as_bytes()).unwrap();
        assert!(r.char_capped);
        assert!(!r.byte_capped);
        assert_eq!(r.content.chars().count(), MAX_PROMPT_CHARS);
    }

    #[test]
    fn read_caps_bytes_for_huge_input() {
        let big: Vec<u8> = vec![b'a'; MAX_BYTES + 1_000];
        let r = read_text_bytes_capped("big.log", &big).unwrap();
        assert!(r.byte_capped);
        // 字节裁后还会触发 char cap（MAX_BYTES = 1M ascii char = 1M chars > MAX_PROMPT_CHARS）。
        assert!(r.char_capped);
        assert!(r.total_bytes > MAX_BYTES);
    }

    #[test]
    fn truncate_chars_safe_on_utf8_boundary() {
        let s = "中文测试内容";
        let (out, capped) = truncate_chars(s, 3);
        assert!(capped);
        assert_eq!(out.chars().count(), 3);
        assert_eq!(out, "中文测");
    }

    #[test]
    fn truncate_chars_no_op_when_under_cap() {
        let (out, capped) = truncate_chars("abc", 10);
        assert!(!capped);
        assert_eq!(out, "abc");
    }

    #[test]
    fn format_for_prompt_adds_truncation_note_when_capped() {
        let outcome = FileReadOutcome {
            content: "x".to_string(),
            total_bytes: 5_000_000,
            byte_capped: true,
            char_capped: true,
        };
        let s = format_for_prompt("big.md", "看看", &outcome);
        assert!(s.contains("[文件: big.md]"));
        assert!(s.contains("看看"));
        assert!(s.contains("--- 文件内容 ---"));
        assert!(s.contains("--- 内容结束 ---"));
        assert!(s.contains("5000000 bytes"));
        assert!(s.contains("已截断"));
    }

    #[test]
    fn format_for_prompt_no_caption_no_blank_line() {
        let outcome = FileReadOutcome {
            content: "body\n".to_string(),
            total_bytes: 5,
            byte_capped: false,
            char_capped: false,
        };
        let s = format_for_prompt("a.txt", "", &outcome);
        // caption 为空时不应出现连续两个换行的空行。
        assert!(!s.contains("[文件: a.txt]\n\n"));
        assert!(!s.contains("已截断"));
    }

    #[test]
    fn user_message_distinguishes_error_kinds() {
        let m1 = FileReadErr::UnsupportedExtension("pdf".to_string()).user_message("doc.pdf");
        assert!(m1.contains("pdf"));
        assert!(m1.contains("doc.pdf"));
        let m2 = FileReadErr::BinaryDetected.user_message("blob.txt");
        assert!(m2.contains("二进制"));
        let m3 = FileReadErr::Empty.user_message("e.md");
        assert!(m3.contains("空"));
    }
}
