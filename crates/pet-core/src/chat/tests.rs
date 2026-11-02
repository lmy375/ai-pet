    use std::collections::HashMap;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use genai::chat::ToolCall;
    use tokio::sync::{Barrier, Notify};

    use super::agent_loop::execute_tool_calls;
    use super::ImageCollectingSink;
    use crate::config::AiConfig;
    use crate::logging::LogStore;
    use crate::shell::ShellStore;
    use crate::tools::{Tool, ToolContext, ToolRegistry};

    struct BarrierTool {
        name: &'static str,
        barrier: Arc<Barrier>,
    }

    struct ImageTool {
        name: &'static str,
        image: &'static str,
        wait_for: Option<Arc<Notify>>,
        notify: Option<Arc<Notify>>,
    }

    impl Tool for BarrierTool {
        fn name(&self) -> &str {
            self.name
        }

        fn definition(&self) -> serde_json::Value {
            serde_json::json!({})
        }

        fn execute<'a>(
            &'a self,
            _arguments: &'a str,
            _ctx: &'a ToolContext,
        ) -> Pin<Box<dyn Future<Output = String> + Send + 'a>> {
            let name = self.name.to_string();
            let barrier = self.barrier.clone();
            Box::pin(async move {
                barrier.wait().await;
                name
            })
        }
    }

    impl Tool for ImageTool {
        fn name(&self) -> &str {
            self.name
        }

        fn definition(&self) -> serde_json::Value {
            serde_json::json!({})
        }

        fn execute<'a>(
            &'a self,
            _arguments: &'a str,
            ctx: &'a ToolContext,
        ) -> Pin<Box<dyn Future<Output = String> + Send + 'a>> {
            let name = self.name.to_string();
            let image = self.image.to_string();
            let wait_for = self.wait_for.clone();
            let notify = self.notify.clone();
            Box::pin(async move {
                if let Some(wait_for) = wait_for {
                    wait_for.notified().await;
                }
                ctx.emit_image(image);
                if let Some(notify) = notify {
                    notify.notify_one();
                }
                name
            })
        }
    }

    fn tool_call(call_id: &str, name: &str) -> ToolCall {
        ToolCall {
            call_id: call_id.to_string(),
            fn_name: name.to_string(),
            fn_arguments: serde_json::json!({}),
            thought_signatures: None,
        }
    }

    fn test_context() -> (AiConfig, ToolContext) {
        let config = AiConfig {
            agent_id: "test".to_string(),
            api_key: String::new(),
            base_url: String::new(),
            model: String::new(),
            provider: String::new(),
            context_window: 0,
            search_api_key: String::new(),
            mcp_servers: Vec::new(),
            reasoning: String::new(),
        };
        let ctx = ToolContext::new(
            LogStore(Arc::new(Mutex::new(Vec::new()))),
            ShellStore(Arc::new(Mutex::new(HashMap::new()))),
            config.clone(),
            crate::mcp::new_mcp_store(),
            "test-session".to_string(),
            None,
            None,
            false,
        );
        (config, ctx)
    }

    /// Both tools must enter `execute` before either can finish. A serial loop
    /// would park on the first barrier forever and hit the timeout.
    #[tokio::test]
    async fn tool_calls_in_one_round_run_concurrently() {
        let barrier = Arc::new(Barrier::new(2));
        let registry = ToolRegistry::for_test(vec![
            Box::new(BarrierTool { name: "first", barrier: barrier.clone() }),
            Box::new(BarrierTool { name: "second", barrier }),
        ]);
        let calls = vec![tool_call("call-1", "first"), tool_call("call-2", "second")];
        let (config, ctx) = test_context();
        let mcp_store = ctx.mcp_store.clone();
        let sink = ImageCollectingSink::new();

        let outputs = tokio::time::timeout(
            Duration::from_secs(2),
            execute_tool_calls(&calls, &sink, &config, &mcp_store, &registry, &ctx),
        )
        .await
        .expect("tool calls did not reach the barrier concurrently");

        assert_eq!(outputs, vec!["first", "second"]);
    }

    /// The second call finishes (and emits its image) first. The parent context
    /// must still receive images in call order, matching the tool results.
    #[tokio::test]
    async fn images_are_merged_in_tool_call_order() {
        let notify = Arc::new(Notify::new());
        let registry = ToolRegistry::for_test(vec![
            Box::new(ImageTool {
                name: "first",
                image: "data:first",
                wait_for: Some(notify.clone()),
                notify: None,
            }),
            Box::new(ImageTool {
                name: "second",
                image: "data:second",
                wait_for: None,
                notify: Some(notify),
            }),
        ]);
        let calls = vec![tool_call("call-1", "first"), tool_call("call-2", "second")];
        let (config, ctx) = test_context();
        let mcp_store = ctx.mcp_store.clone();
        let sink = ImageCollectingSink::new();

        let outputs = execute_tool_calls(&calls, &sink, &config, &mcp_store, &registry, &ctx).await;

        assert_eq!(outputs, vec!["first", "second"]);
        assert_eq!(ctx.take_images(), vec!["data:first", "data:second"]);
    }
