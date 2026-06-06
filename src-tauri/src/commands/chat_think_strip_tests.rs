    use super::strip_think_blocks;

    #[test]
    fn no_think_tag_returns_input_unchanged() {
        let s = "hello world";
        let (visible, blocks) = strip_think_blocks(s);
        assert_eq!(visible, "hello world");
        assert!(blocks.is_empty());
    }

    #[test]
    fn single_think_block_stripped() {
        let s = "before<think>reasoning</think>after";
        let (visible, blocks) = strip_think_blocks(s);
        assert_eq!(visible, "beforeafter");
        assert_eq!(blocks, vec!["reasoning".to_string()]);
    }

    #[test]
    fn multiple_think_blocks_all_stripped() {
        let s = "a<think>x</think>b<think>y</think>c";
        let (visible, blocks) = strip_think_blocks(s);
        assert_eq!(visible, "abc");
        assert_eq!(blocks, vec!["x".to_string(), "y".to_string()]);
    }

    #[test]
    fn case_insensitive_open_close() {
        let s = "hi<Think>r</Think>bye";
        let (visible, blocks) = strip_think_blocks(s);
        assert_eq!(visible, "hibye");
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0], "r");
    }

    #[test]
    fn unclosed_think_drops_remainder_into_block() {
        // 未闭合：后续视作 think 块，visible 不再保留任何后续——防止 think
        // 内容混入 final（spec：「主气泡只渲染 think 标签之外的最终回答」）
        let s = "shown<think>unfinished reasoning continues...";
        let (visible, blocks) = strip_think_blocks(s);
        assert_eq!(visible, "shown");
        assert_eq!(blocks.len(), 1);
        assert!(blocks[0].contains("unfinished"));
    }

    #[test]
    fn trims_leading_newlines_left_by_strip() {
        // think 在最前面：剥完后 visible 不应以多余 \n 开头
        let s = "<think>r</think>\n\n  actual response";
        let (visible, _) = strip_think_blocks(s);
        assert!(visible.starts_with("\n") == false || visible.starts_with("  actual"));
        assert!(visible.contains("actual response"));
    }

    #[test]
    fn empty_think_block_handled() {
        let s = "<think></think>final";
        let (visible, blocks) = strip_think_blocks(s);
        assert_eq!(visible, "final");
        assert_eq!(blocks, vec!["".to_string()]);
    }

    #[test]
    fn chinese_content_unaffected_by_strip() {
        let s = "你好<think>思考中文</think>世界";
        let (visible, blocks) = strip_think_blocks(s);
        assert_eq!(visible, "你好世界");
        assert_eq!(blocks, vec!["思考中文".to_string()]);
    }

    #[test]
    fn newlines_inside_think_kept_in_block() {
        let s = "a<think>line1\nline2\nline3</think>b";
        let (visible, blocks) = strip_think_blocks(s);
        assert_eq!(visible, "ab");
        assert_eq!(blocks[0], "line1\nline2\nline3");
    }

    #[test]
    fn markdown_outside_think_preserved() {
        let s = "**bold**<think>r</think> [link](#)";
        let (visible, _) = strip_think_blocks(s);
        assert_eq!(visible, "**bold** [link](#)");
    }

    #[test]
    fn input_with_only_think_block_becomes_empty() {
        let s = "<think>only thinking</think>";
        let (visible, blocks) = strip_think_blocks(s);
        assert_eq!(visible, "");
        assert_eq!(blocks, vec!["only thinking".to_string()]);
    }
