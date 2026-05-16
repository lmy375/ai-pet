# 桌面 pet 右键菜单加「📡 ping LLM 测延迟」

## 背景

owner 偶尔遇到"宠物不回应 / chat 没反应"现象，排查链路：网络？api_base 错？api_key 错？model 配置错？目前必须开 PanelSettings 看 raw config + 手动调试。

加 pet 右键 「📡 ping LLM」一键测：现行 settings 的 api_base / api_key 对 `/models` 端点发 cheap GET（不消耗 token），计 RTT，返回 ChatMini 显结果。

## 改动

### Backend `src-tauri/src/commands/app.rs`

#### 新 `ping_llm` Tauri 命令

```rust
#[tauri::command]
pub async fn ping_llm() -> Result<PingLlmResult, String> {
    let settings = crate::commands::settings::get_settings()?;
    let base = settings.api_base.trim_end_matches('/').to_string();
    if base.is_empty() {
        return Err("api_base 未配置".to_string());
    }
    let url = format!("{}/models", base);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("HTTP client init failed: {e}"))?;
    let started = std::time::Instant::now();
    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", settings.api_key))
        .send()
        .await
        .map_err(|e| format!("请求 {} 失败：{}", url, e))?;
    let elapsed_ms = started.elapsed().as_millis() as u64;
    let status_code = resp.status().as_u16();
    let ok = resp.status().is_success();
    Ok(PingLlmResult { ok, elapsed_ms, status_code, api_base: base, model: settings.model })
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PingLlmResult {
    pub ok: bool,
    pub elapsed_ms: u64,
    pub status_code: u16,
    pub api_base: String,
    pub model: String,
}
```

- 调 `/models` 端点（OpenAI compat 通用 cheap path，0 token 消耗）
- 10s timeout 防卡死
- HTTP 非 2xx 仍 Ok 返结构体（让 owner 看 status_code 区分 401/404/500）
- Network / connection 错 → Err 透传原因

#### `src-tauri/src/lib.rs` 注册

```rust
commands::app::ping_llm,
```

### Frontend `src/App.tsx`

pet ctx menu 加 📡 ping LLM 按钮：

```tsx
<button
  onClick={async () => {
    setPetCtxMenu(null);
    appendAssistant("📡 ping LLM...");
    try {
      const r = await invoke<{ok, elapsed_ms, status_code, api_base, model}>("ping_llm");
      const icon = r.ok ? "✅" : "⚠️";
      appendAssistant(`${icon} ping ${r.api_base} → HTTP ${r.status_code} · ${r.elapsed_ms}ms · model=${r.model}`);
    } catch (e) {
      appendAssistant(`❌ ping LLM 失败：${e}`);
    }
  }}
  title="...排查 '宠物不回应' 第一步..."
>
  📡 ping LLM
</button>
```

立即 push "📡 ping LLM..." ack 让 owner 知道开始；返回时 push 结果（含 ✅/⚠️/❌ icon + 含 RTT + status + model name）。

#### menu 高度 H 调到 400

11 个 button + 5 个 separator ≈ 339，+ 余量到 400。

## 关键设计

- **调 /models 而非 /chat/completions**：/models 是 list 端点，0 token 消耗 + 大多数 provider 都实现；/chat/completions 即便 max_tokens=1 也耗费 quota。
- **HTTP 非 2xx 仍 Ok**：401 / 404 / 500 都是"通了但是错"信号，应返结构让 owner 看 status；只有 connection refused / timeout 等 transport 失败才 Err。
- **不读 response body**：判 status 就够；list models 可能 verbose，避免 IO 浪费。
- **结果走 appendAssistant 软消息**：owner 在 ChatMini 看到结果（保持记录可以回溯比较），不必 toast / modal。
- **echo api_base + model**：让 owner 看到"我在 ping 谁 + 用哪个 model"，确认配置无误。
- **timeout 10s**：足够慢网通也能拿到 RTT，避免长 hang。

## 不做

- **不实测 chat/completions**：要消耗 quota；本 iter 仅"通了 / 没通"信号已够 owner 排查。
- **不持久化 ping 历史**：每次 ping 独立 ad-hoc 操作；不引 DB / log 表。
- **不写 frontend test**：纯 invoke + appendAssistant；视觉验证（点 📡 → ChatMini 应见两条消息：开始 + 结果）足够。
- **不写 backend test**：reqwest 网络调用集成测试需要 mock server，相对此 iter 价值低；本质是裸 HTTP GET + 计时。

## 验证

- `cargo check` ✓
- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 1.20s
- 改动 ~80 行（backend ping_llm command + struct 50 + lib.rs 注册 1 + frontend ctx menu button + H 调整 30）。既有 ctx menu 其它 entries / 倒计时 / mute / 主题 / 重启 路径完全不动。

## TODO 状态

剩 0 条 —— TODO 池清空。下个 cron tick 进 auto-propose 分支。

## 后续

- 加 PanelSettings 内一行"📡 ping LLM" 按钮 + 结果显示，让设置时也能测。
- ping_llm 加 verbose mode 返 response body 头 200 字符让 owner 看 "你 provider 返了啥"。
- 历史 ping 结果 chart：sparkline 显示过去 1 小时的 RTT 趋势，发现"晚上 9 点延迟翻倍"等模式。
- ping_llm 失败时给具体 "可能原因 + 自动建议"：connection refused → 网络/防火墙；401 → api_key 错；404 → api_base 路径错。
