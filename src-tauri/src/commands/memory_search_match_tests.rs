    use super::*;

    fn item(title: &str, desc: &str) -> MemoryItem {
        MemoryItem {
            title: title.to_string(),
            description: desc.to_string(),
            detail_path: String::new(),
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    #[test]
    fn matches_when_keyword_in_item_title() {
        assert!(item_matches_query("notes", "笔记", &item("妈妈生日", ""), "妈妈"));
    }

    #[test]
    fn matches_when_keyword_in_description() {
        assert!(item_matches_query(
            "notes",
            "笔记",
            &item("x", "[source: distill] 周末购物清单"),
            "distill",
        ));
    }

    #[test]
    fn matches_when_keyword_in_cat_name() {
        // 056-part1 新行为：cat_name 命中即使 item title/desc 不含 kw 也出现。
        assert!(item_matches_query(
            "butler_tasks",
            "管家任务",
            &item("morning_briefing", "[every: 09:00]"),
            "butler",
        ));
    }

    #[test]
    fn matches_when_keyword_in_cat_label_chinese() {
        // cat_name 走英文 slug，label 走中文展示名 —— 用户搜「管家」也要命中。
        assert!(item_matches_query(
            "butler_tasks",
            "管家任务",
            &item("morning_briefing", "..."),
            "管家",
        ));
    }

    #[test]
    fn case_insensitive_via_lowercase_input() {
        // memory_search caller 必须先 to_lowercase；helper 只对 kw_lc 工作。
        // 这里测「kw_lc 已小写」时大写英文 cat name 也能命中。
        assert!(item_matches_query("Notes", "笔记", &item("X", "y"), "notes"));
    }

    #[test]
    fn no_match_when_unrelated() {
        assert!(!item_matches_query(
            "notes",
            "笔记",
            &item("妈妈生日", "[source: distill]"),
            "telegram",
        ));
    }
