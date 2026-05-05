import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { AppSettings, McpServerConfig, TgCustomCommand } from "../../hooks/useSettings";
import { NumberField as SharedNumberField } from "../common/NumberField";

interface McpStatus {
  name: string;
  connected: boolean;
  tool_count: number;
  tool_names: string[];
  error: string | null;
}

interface TelegramStatus {
  running: boolean;
  error: string | null;
}

/** 常见 LLM 模型预设，给 settings 的 model 字段做 datalist 建议。
 * 任意 OpenAI-compatible 后端都可以手输自定义值，这里只是减少常用模型
 * 的拼写错误（典型 footgun：`gpt4o-mini` / `gpt-4o-mini`）。
 *
 * 维护节奏：每季度按主流厂商当前 representative 模型刷一遍即可，不需
 * 要保持"完整模型目录"。 */
const MODEL_PRESETS: string[] = [
  // OpenAI
  "gpt-4o",
  "gpt-4o-mini",
  "gpt-4-turbo",
  "o3-mini",
  // Anthropic
  "claude-opus-4-7",
  "claude-sonnet-4-6",
  "claude-haiku-4-5",
  // DeepSeek
  "deepseek-chat",
  "deepseek-reasoner",
  // 其它
  "gemini-2.0-flash",
  "qwen2.5-72b-instruct",
];

const emptyMcpServer = (transport: McpServerConfig["transport"] = "stdio"): McpServerConfig => ({
  transport,
  command: "",
  args: [],
  url: "",
  headers: {},
  env: {},
  enabled: true,
});

export function PanelSettings() {
  const [form, setForm] = useState<AppSettings>({
    live_2d_model_path: "",
    api_base: "",
    api_key: "",
    model: "",
    mcp_servers: {},
    telegram: { bot_token: "", allowed_username: "", enabled: false, persona_layer_enabled: true, custom_commands: [], command_lang: "zh" },
    proactive: {
      enabled: false,
      interval_seconds: 300,
      idle_threshold_seconds: 900,
      input_idle_seconds: 60,
      cooldown_seconds: 1800,
      quiet_hours_start: 23,
      quiet_hours_end: 7,
      respect_focus_mode: true,
      chatty_day_threshold: 5,
      companion_mode: "balanced",
      task_heartbeat_minutes: 30,
    },
    morning_briefing: {
      enabled: true,
      hour: 8,
      minute: 30,
    },
    memory_consolidate: {
      enabled: false,
      interval_hours: 6,
      min_total_items: 12,
      stale_reminder_hours: 24,
      stale_plan_hours: 24,
      stale_once_butler_hours: 48,
      stale_daily_review_days: 30,
      weekly_summary_closing_hour: 20,
    },
    chat: {
      max_context_messages: 50,
    },
    privacy: {
      redaction_patterns: [],
      regex_patterns: [],
    },
    user_name: "",
    tool_review_overrides: {},
    motion_mapping: {},
  });
  const [soul, setSoul] = useState("");
  const [loaded, setLoaded] = useState(false);
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState("");
  const [mcpStatuses, setMcpStatuses] = useState<McpStatus[]>([]);
  const [reconnecting, setReconnecting] = useState(false);
  const [newServerName, setNewServerName] = useState("");
  const [telegramStatus, setTelegramStatus] = useState<TelegramStatus>({ running: false, error: null });
  const [telegramReconnecting, setTelegramReconnecting] = useState(false);
  // 清空 TG 命令补全 ack：让按钮按下时显 "清空中…"，避免重复点击。
  const [telegramResetting, setTelegramResetting] = useState(false);
  const [viewMode, setViewMode] = useState<"form" | "raw">("form");
  const [rawYaml, setRawYaml] = useState("");
  // 搜索框状态：仅在 form 模式生效。空 query = 全展；非空 = 按标题 + 关键字
  // 子串（大小写不敏感）过滤 section。
  const [searchQuery, setSearchQuery] = useState("");
  // 工具风险面板：name + level + note 是后端静态 metadata（一次性加载）；
  // mode 反映 form.tool_review_overrides 当前编辑状态（不存盘也即时生效）。
  const [toolRiskRows, setToolRiskRows] = useState<{ name: string; level: string; note: string }[]>([]);

  useEffect(() => {
    Promise.all([
      invoke<AppSettings>("get_settings"),
      invoke<string>("get_soul"),
      invoke<McpStatus[]>("get_mcp_status"),
      invoke<TelegramStatus>("get_telegram_status").catch(() => ({ running: false, error: null }) as TelegramStatus),
      invoke<{ name: string; level: string; note: string; mode: string }[]>("get_tool_risk_overview").catch(() => []),
    ]).then(([s, soulContent, statuses, tgStatus, riskOverview]) => {
      setForm(s);
      setSoul(soulContent);
      setMcpStatuses(statuses);
      setTelegramStatus(tgStatus);
      setToolRiskRows(riskOverview.map(({ name, level, note }) => ({ name, level, note })));
      setLoaded(true);
    }).catch((e) => {
      console.error("Failed to load settings:", e);
      setLoaded(true);
    });
  }, []);

  const handleSave = async () => {
    setSaving(true);
    setMessage("");
    try {
      await invoke("save_settings", { settings: form });
      await invoke("save_soul", { content: soul });
      setMessage("保存成功！重启宠物窗口后生效。");
    } catch (e: any) {
      setMessage(`保存失败: ${e}`);
    } finally {
      setSaving(false);
    }
  };

  const handleReconnectMcp = async () => {
    setReconnecting(true);
    setMessage("");
    try {
      await invoke("save_settings", { settings: form });
      const statuses = await invoke<McpStatus[]>("reconnect_mcp");
      setMcpStatuses(statuses);
      const connected = statuses.filter((s) => s.connected).length;
      const total = statuses.length;
      setMessage(`MCP 已重连: ${connected}/${total} 个服务器连接成功`);
    } catch (e: any) {
      setMessage(`MCP 重连失败: ${e}`);
    } finally {
      setReconnecting(false);
    }
  };

  const updateMcpServer = (name: string, updates: Partial<McpServerConfig>) => {
    setForm((prev) => ({
      ...prev,
      mcp_servers: {
        ...prev.mcp_servers,
        [name]: { ...prev.mcp_servers[name], ...updates },
      },
    }));
  };

  const removeMcpServer = (name: string) => {
    setForm((prev) => {
      const { [name]: _, ...rest } = prev.mcp_servers;
      return { ...prev, mcp_servers: rest };
    });
  };

  const addMcpServer = () => {
    const name = newServerName.trim();
    if (!name || form.mcp_servers[name]) return;
    setForm((prev) => ({
      ...prev,
      mcp_servers: { ...prev.mcp_servers, [name]: emptyMcpServer() },
    }));
    setNewServerName("");
  };

  const switchToRaw = async () => {
    try {
      const raw = await invoke<string>("get_config_raw");
      setRawYaml(raw);
      setViewMode("raw");
      setMessage("");
    } catch (e: any) {
      setMessage(`加载配置文件失败: ${e}`);
    }
  };

  const switchToForm = async () => {
    try {
      const s = await invoke<AppSettings>("get_settings");
      setForm(s);
      setViewMode("form");
      setMessage("");
    } catch (e: any) {
      setMessage(`加载配置失败: ${e}`);
    }
  };

  const handleSaveRaw = async () => {
    setSaving(true);
    setMessage("");
    try {
      await invoke("save_config_raw", { content: rawYaml });
      await invoke("save_soul", { content: soul });
      setMessage("保存成功！重启宠物窗口后生效。");
    } catch (e: any) {
      setMessage(`保存失败: ${e}`);
    } finally {
      setSaving(false);
    }
  };

  if (!loaded) return <div style={containerStyle}>加载中...</div>;

  const serverEntries = Object.entries(form.mcp_servers);
  const connectedCount = mcpStatuses.filter((s) => s.connected).length;
  const totalToolCount = mcpStatuses.reduce((sum, s) => sum + s.tool_count, 0);

  return (
    <div className="pet-settings-root" style={containerStyle}>
      {/* Iter R47: focus ring audit — inputStyle had `outline: none` with
          no replacement, same accessibility hole R46 fixed in ChatPanel.
          Scoped descendant selector here covers every input/textarea/select
          inside this panel without touching each call site. */}
      <style>{`
        .pet-settings-root input:focus,
        .pet-settings-root textarea:focus,
        .pet-settings-root select:focus {
          border-color: #38bdf8;
          box-shadow: 0 0 0 2px rgba(56,189,248,0.18);
          transition: border-color 150ms ease-out, box-shadow 150ms ease-out;
        }
      `}</style>
      {/* View mode toggle */}
      <div style={{ display: "flex", gap: "4px", marginBottom: "16px", background: "var(--pet-color-border)", borderRadius: "8px", padding: "3px" }}>
        <button
          onClick={viewMode === "raw" ? switchToForm : undefined}
          style={{
            flex: 1,
            padding: "6px 0",
            borderRadius: "6px",
            border: "none",
            background: viewMode === "form" ? "var(--pet-color-card)" : "transparent",
            color: viewMode === "form" ? "var(--pet-color-fg)" : "var(--pet-color-muted)",
            fontWeight: viewMode === "form" ? 600 : 400,
            fontSize: "13px",
            cursor: viewMode === "form" ? "default" : "pointer",
            transition: "all 0.2s",
          }}
        >
          表单
        </button>
        <button
          onClick={viewMode === "form" ? switchToRaw : undefined}
          style={{
            flex: 1,
            padding: "6px 0",
            borderRadius: "6px",
            border: "none",
            background: viewMode === "raw" ? "var(--pet-color-card)" : "transparent",
            color: viewMode === "raw" ? "var(--pet-color-fg)" : "var(--pet-color-muted)",
            fontWeight: viewMode === "raw" ? 600 : 400,
            fontSize: "13px",
            cursor: viewMode === "raw" ? "default" : "pointer",
            transition: "all 0.2s",
          }}
        >
          源码
        </button>
      </div>

      {viewMode === "raw" ? (
        <>
          {/* Raw YAML editor */}
          <div style={sectionStyle}>
            <h4 style={sectionTitle}>config.yaml</h4>
            <textarea
              value={rawYaml}
              onChange={(e) => setRawYaml(e.target.value)}
              style={{
                ...inputStyle,
                fontFamily: "monospace",
                fontSize: "12px",
                lineHeight: "1.6",
                resize: "vertical",
                minHeight: "300px",
                whiteSpace: "pre",
                overflowWrap: "normal",
                overflowX: "auto",
              }}
              spellCheck={false}
            />
          </div>

          {/* SOUL */}
          <div style={sectionStyle}>
            <h4 style={sectionTitle}>系统提示词 (SOUL.md)</h4>
            <textarea
              value={soul}
              onChange={(e) => setSoul(e.target.value)}
              rows={6}
              style={{ ...inputStyle, resize: "vertical", fontFamily: "inherit", lineHeight: "1.5" }}
              placeholder="输入 AI 角色设定..."
            />
          </div>

          {/* Save */}
          <div style={{ display: "flex", alignItems: "center", gap: "12px", marginTop: "8px" }}>
            <button onClick={handleSaveRaw} disabled={saving} style={btnStyle}>
              {saving ? "保存中..." : "保存"}
            </button>
            {message && (
              <span style={{ fontSize: "13px", color: message.includes("失败") ? "#ef4444" : "#22c55e" }}>
                {message}
              </span>
            )}
          </div>
        </>
      ) : (
      <>
      {/* 设置内容很长（11 个 section），加搜索框按标题 / 关键字过滤。
          只在 form 模式下渲染；raw 模式是单 YAML textarea 不适用。 */}
      <div style={{ display: "flex", gap: 6, marginBottom: 12 }}>
        <input
          type="text"
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
          onKeyDown={(e) => {
            // Esc 清空搜索 —— 与 PanelChat 跨会话搜索面板的 Esc 行为统一，
            // 让"按 Esc 退出过滤态"成为整个 panel 的肌肉记忆。
            if (e.key === "Escape") setSearchQuery("");
          }}
          placeholder="搜索设置（按标题或关键字过滤；如 api / mute / regex / 工具）"
          style={{ ...inputStyle, flex: 1 }}
        />
        {searchQuery && (
          <button
            type="button"
            onClick={() => setSearchQuery("")}
            style={{
              padding: "0 12px",
              border: "1px solid var(--pet-color-border)",
              borderRadius: 4,
              background: "var(--pet-color-card)",
              color: "var(--pet-color-muted)",
              cursor: "pointer",
              fontSize: 12,
              flexShrink: 0,
            }}
            title="清空搜索"
          >
            ✕
          </button>
        )}
      </div>
      {/* Live2D */}
      <SearchableSection
        title="Live2D 模型"
        keywords={["live2d", "model", "motion", "miku", "映射", "动作"]}
        query={searchQuery}
      >
      <div style={sectionStyle}>
        <h4 style={sectionTitle}><HighlightedText text="Live2D 模型" query={searchQuery} /></h4>
        <label style={labelStyle}>模型路径</label>
        <input
          value={form.live_2d_model_path}
          onChange={(e) => setForm({ ...form, live_2d_model_path: e.target.value })}
          style={inputStyle}
          placeholder="/models/miku/miku.model3.json"
        />

        {/* Motion 映射：把 4 个语义键映射到当前模型的实际 motion group 名。
            空 = 直接用语义键名（与内置 miku 行为一致）。 */}
        <div style={{ marginTop: "12px" }}>
          <label style={labelStyle}>Motion 映射</label>
          <p style={{ fontSize: "11px", color: "var(--pet-color-muted)", margin: "0 0 8px 0", lineHeight: 1.5 }}>
            把 LLM 写的 4 个语义键翻译到你 model 的实际 motion group 名。留空 =
            直接用左侧键名。改完保存即时生效。
          </p>
          {(
            [
              { key: "Tap", hint: "开心 / 活泼" },
              { key: "Flick", hint: "想分享 / 兴致" },
              { key: "Flick3", hint: "烦躁 / 焦虑" },
              { key: "Idle", hint: "平静 / 沉静" },
            ] as const
          ).map(({ key, hint }) => (
            <div
              key={key}
              style={{ display: "flex", alignItems: "center", gap: "8px", marginBottom: "6px" }}
            >
              <span
                style={{
                  fontFamily: "'SF Mono', 'Menlo', monospace",
                  fontSize: "12px",
                  color: "var(--pet-color-fg)",
                  minWidth: "70px",
                }}
              >
                {key}
              </span>
              <span style={{ fontSize: "11px", color: "var(--pet-color-muted)", minWidth: "100px" }}>
                {hint}
              </span>
              <input
                value={form.motion_mapping?.[key] ?? ""}
                onChange={(e) => {
                  const v = e.target.value;
                  setForm((prev) => {
                    const next = { ...(prev.motion_mapping ?? {}) };
                    if (v.trim().length === 0) {
                      delete next[key]; // 空值不存键，避免脏数据
                    } else {
                      next[key] = v;
                    }
                    return { ...prev, motion_mapping: next };
                  });
                }}
                placeholder={key}
                style={{ ...inputStyle, flex: 1 }}
              />
            </div>
          ))}
        </div>
      </div>
      </SearchableSection>

      {/* LLM Config */}
      <SearchableSection
        title="LLM 配置"
        keywords={["llm", "api", "key", "model", "openai", "base", "url", "gpt"]}
        query={searchQuery}
      >
      <div style={sectionStyle}>
        <h4 style={sectionTitle}><HighlightedText text="LLM 配置" query={searchQuery} /></h4>
        <label style={labelStyle}>API Base URL</label>
        <input
          value={form.api_base}
          onChange={(e) => setForm({ ...form, api_base: e.target.value })}
          style={inputStyle}
          placeholder="https://api.openai.com/v1"
        />
        <label style={{ ...labelStyle, marginTop: "8px" }}>API Key</label>
        <input
          type="password"
          value={form.api_key}
          onChange={(e) => setForm({ ...form, api_key: e.target.value })}
          style={inputStyle}
          placeholder="sk-..."
        />
        <label style={{ ...labelStyle, marginTop: "8px" }}>Model</label>
        <input
          list="model-presets"
          value={form.model}
          onChange={(e) => setForm({ ...form, model: e.target.value })}
          style={inputStyle}
          placeholder="gpt-4o-mini"
        />
        <datalist id="model-presets">
          {MODEL_PRESETS.map((m) => (
            <option key={m} value={m} />
          ))}
        </datalist>
      </div>
      </SearchableSection>

      {/* MCP Servers */}
      <SearchableSection
        title="MCP Servers"
        keywords={["mcp", "server", "tool", "工具", "服务器"]}
        query={searchQuery}
      >
      <div style={sectionStyle}>
        <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: "10px" }}>
          <h4 style={{ ...sectionTitle, margin: 0 }}>
            <HighlightedText text="MCP Servers" query={searchQuery} />
            {serverEntries.length > 0 && (
              <span style={{ fontWeight: 400, fontSize: "12px", color: "var(--pet-color-muted)", marginLeft: "8px" }}>
                {connectedCount}/{serverEntries.length} 已连接 · {totalToolCount} 工具
              </span>
            )}
          </h4>
          <button
            onClick={handleReconnectMcp}
            disabled={reconnecting}
            style={{
              ...btnSmallStyle,
              background: reconnecting ? "#94a3b8" : "#8b5cf6",
            }}
          >
            {reconnecting ? "连接中..." : "保存并连接"}
          </button>
        </div>

        {serverEntries.length === 0 && (
          <div style={{ padding: "16px", textAlign: "center", color: "var(--pet-color-muted)", fontSize: "13px", border: "1px dashed var(--pet-color-border)", borderRadius: "8px" }}>
            尚未配置 MCP 服务器，在下方添加
          </div>
        )}

        {serverEntries.map(([name, config]) => {
          const status = mcpStatuses.find((s) => s.name === name);
          return (
            <McpServerEntry
              key={name}
              name={name}
              config={config}
              status={status}
              onChange={(updates) => updateMcpServer(name, updates)}
              onRemove={() => removeMcpServer(name)}
            />
          );
        })}

        {/* Add server */}
        <div style={{ display: "flex", gap: "8px", marginTop: serverEntries.length > 0 ? "8px" : "12px" }}>
          <input
            value={newServerName}
            onChange={(e) => setNewServerName(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && addMcpServer()}
            style={{ ...inputStyle, flex: 1 }}
            placeholder="新服务器名称..."
          />
          <button
            onClick={addMcpServer}
            disabled={!newServerName.trim() || !!form.mcp_servers[newServerName.trim()]}
            style={{
              ...btnSmallStyle,
              background: !newServerName.trim() ? "#94a3b8" : "#22c55e",
            }}
          >
            + 添加
          </button>
        </div>
      </div>
      </SearchableSection>

      {/* Telegram Bot */}
      <SearchableSection
        title="Telegram Bot"
        keywords={["telegram", "tg", "bot", "token", "username", "机器人"]}
        query={searchQuery}
      >
      <div style={sectionStyle}>
        <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: "10px" }}>
          <h4 style={{ ...sectionTitle, margin: 0 }}>
            <HighlightedText text="Telegram Bot" query={searchQuery} />
            <span style={{
              fontWeight: 400,
              fontSize: "11px",
              marginLeft: "8px",
              color: telegramStatus.running ? "#22c55e" : telegramStatus.error ? "#ef4444" : "#94a3b8",
            }}>
              {telegramStatus.running ? "运行中" : telegramStatus.error ? "连接失败" : "未启动"}
            </span>
          </h4>
          <div style={{ display: "flex", gap: 6, alignItems: "center" }}>
            <button
              onClick={async () => {
                setTelegramResetting(true);
                setMessage("");
                try {
                  await invoke("reset_tg_commands");
                  setMessage(
                    "TG 命令补全已清空。点 保存并连接 重新注册以让客户端拿到最新命令。",
                  );
                } catch (e: any) {
                  setMessage(`清空命令补全失败: ${e}`);
                } finally {
                  setTelegramResetting(false);
                }
              }}
              disabled={telegramResetting || telegramReconnecting}
              title="把 TG 客户端的命令补全表清空（set_my_commands(vec![])）。重命名 / 删命令后用一次，下次重连重注册新名。"
              style={{
                ...btnSmallStyle,
                background:
                  telegramResetting || telegramReconnecting ? "#94a3b8" : "#64748b",
              }}
            >
              {telegramResetting ? "清空中..." : "清空命令补全"}
            </button>
            <button
              onClick={async () => {
                setTelegramReconnecting(true);
                setMessage("");
                try {
                  await invoke("save_settings", { settings: form });
                  const status = await invoke<TelegramStatus>("reconnect_telegram");
                  setTelegramStatus(status);
                  if (status.error) {
                    setMessage(`Telegram 连接失败: ${status.error}`);
                  } else if (status.running) {
                    setMessage("Telegram Bot 已连接");
                  } else {
                    setMessage("Telegram Bot 已停止");
                  }
                } catch (e: any) {
                  setMessage(`Telegram 操作失败: ${e}`);
                } finally {
                  setTelegramReconnecting(false);
                }
              }}
              disabled={telegramReconnecting || telegramResetting}
              style={{
                ...btnSmallStyle,
                background:
                  telegramReconnecting || telegramResetting ? "#94a3b8" : "#0ea5e9",
              }}
            >
              {telegramReconnecting ? "连接中..." : "保存并连接"}
            </button>
          </div>
        </div>

        {telegramStatus.error && (
          <div style={{ background: "var(--pet-tint-orange-bg)", border: "1px solid var(--pet-tint-orange-fg)", borderRadius: "6px", padding: "6px 10px", marginBottom: "8px", fontSize: "12px", color: "var(--pet-tint-orange-fg)" }}>
            {telegramStatus.error}
          </div>
        )}

        <label style={{ ...labelStyle, display: "flex", alignItems: "center", gap: "6px", marginBottom: "8px" }}>
          <input
            type="checkbox"
            checked={form.telegram?.enabled ?? false}
            onChange={(e) => setForm({ ...form, telegram: { ...form.telegram, bot_token: form.telegram?.bot_token ?? "", allowed_username: form.telegram?.allowed_username ?? "", enabled: e.target.checked } })}
          />
          启用 Telegram Bot
        </label>

        <label style={labelStyle}>Bot Token</label>
        <input
          type="password"
          value={form.telegram?.bot_token ?? ""}
          onChange={(e) => setForm({ ...form, telegram: { ...form.telegram, enabled: form.telegram?.enabled ?? false, allowed_username: form.telegram?.allowed_username ?? "", bot_token: e.target.value } })}
          style={{ ...inputStyle, marginBottom: "8px", fontFamily: "monospace", fontSize: "12px" }}
          placeholder="123456789:ABCdefGhI..."
        />

        <label style={labelStyle}>
          允许的用户名
          <span style={{ marginLeft: 6, fontSize: 11, color: "var(--pet-color-muted)", fontWeight: 400 }}>
            多个用 `,` 分隔
          </span>
        </label>
        <input
          value={form.telegram?.allowed_username ?? ""}
          onChange={(e) => setForm({ ...form, telegram: { ...form.telegram, enabled: form.telegram?.enabled ?? false, bot_token: form.telegram?.bot_token ?? "", allowed_username: e.target.value } })}
          style={{ ...inputStyle, fontFamily: "monospace", fontSize: "12px" }}
          placeholder="@alice, @bob (留空则允许所有人)"
        />

        <label style={{ ...labelStyle, display: "flex", alignItems: "center", gap: "6px", marginTop: "8px" }}>
          <input
            type="checkbox"
            checked={form.telegram?.persona_layer_enabled ?? true}
            onChange={(e) =>
              setForm({
                ...form,
                telegram: {
                  ...form.telegram,
                  enabled: form.telegram?.enabled ?? false,
                  bot_token: form.telegram?.bot_token ?? "",
                  allowed_username: form.telegram?.allowed_username ?? "",
                  persona_layer_enabled: e.target.checked,
                },
              })
            }
          />
          注入长期人格层（陪伴天数 + 自我画像 + 心情谱）
        </label>

        {/* TG 客户端补全表里 hardcoded 命令描述的语种切换。custom 不翻译。 */}
        <label style={{ ...labelStyle, marginTop: 8, display: "flex", alignItems: "center", gap: 6 }}>
          命令描述语种
          <select
            value={form.telegram?.command_lang ?? "zh"}
            onChange={(e) =>
              setForm({
                ...form,
                telegram: {
                  ...form.telegram,
                  enabled: form.telegram?.enabled ?? false,
                  bot_token: form.telegram?.bot_token ?? "",
                  allowed_username: form.telegram?.allowed_username ?? "",
                  persona_layer_enabled:
                    form.telegram?.persona_layer_enabled ?? true,
                  custom_commands: form.telegram?.custom_commands ?? [],
                  command_lang: e.target.value,
                },
              })
            }
            style={{
              ...inputStyle,
              flex: "0 0 auto",
              padding: "2px 8px",
              fontSize: 12,
            }}
            title="切换 TG 客户端命令补全表里 hardcoded 命令的描述语种。改完点 保存并连接 重连生效。"
          >
            <option value="zh">中文</option>
            <option value="en">English</option>
          </select>
          <span style={{ fontSize: 11, color: "var(--pet-color-muted)", fontWeight: 400 }}>
            自定义命令不翻译；运行时反馈仍中文
          </span>
        </label>

        {/* 自定义命令矩阵 — textarea 每行 `name: description`。bot 启动
            注册到 TG 客户端补全表；调用时不走 dispatch，fall through 到
            chat pipeline 让 LLM 自由选 tool（不绑定具体 1:1 映射）。 */}
        <label style={{ ...labelStyle, marginTop: 12 }}>
          <HighlightedText text="自定义命令" query={searchQuery} />
          <span style={{ marginLeft: 6, fontSize: 11, color: "var(--pet-color-muted)", fontWeight: 400 }}>
            每行 `name: description`；name 必须 lowercase + 数字 / `_`
          </span>
        </label>
        <textarea
          style={{
            width: "100%",
            minHeight: 80,
            padding: "6px 10px",
            fontSize: 12,
            fontFamily: "'SF Mono', 'Menlo', monospace",
            border: "1px solid var(--pet-color-border)",
            borderRadius: 4,
            resize: "vertical",
            boxSizing: "border-box",
            lineHeight: 1.5,
            background: "var(--pet-color-card)",
            color: "var(--pet-color-fg)",
          }}
          placeholder="timer: 设置一个提醒&#10;translate: 翻译为英文&#10;weather: 查询本地天气"
          value={(form.telegram?.custom_commands ?? [])
            .map((c) => `${c.name}: ${c.description}`)
            .join("\n")}
          onChange={(e) => {
            // 解析每行 `name: description`；缺 `:` / 字段空 → 静默丢弃。
            // 不在前端做 lowercase / 字符校验 — 后端 `merged_command_registry`
            // 已统一过滤；前端校严反而让用户看不见自己输错了什么。
            const parsed: TgCustomCommand[] = [];
            for (const line of e.target.value.split("\n")) {
              const trimmed = line.trim();
              if (trimmed === "") continue;
              const idx = trimmed.indexOf(":");
              if (idx <= 0) continue;
              const name = trimmed.slice(0, idx).trim();
              const description = trimmed.slice(idx + 1).trim();
              if (name === "" || description === "") continue;
              parsed.push({ name, description });
            }
            setForm({
              ...form,
              telegram: {
                ...form.telegram,
                enabled: form.telegram?.enabled ?? false,
                bot_token: form.telegram?.bot_token ?? "",
                allowed_username: form.telegram?.allowed_username ?? "",
                persona_layer_enabled:
                  form.telegram?.persona_layer_enabled ?? true,
                custom_commands: parsed,
              },
            });
          }}
        />
        <p style={{ marginTop: 4, fontSize: 11, color: "var(--pet-color-muted)" }}>
          用户调用自定义命令 → bot 把消息当文本走 chat pipeline，LLM 自由
          选 tool（不绑定具体 tool 映射）。改完点 <b>保存并连接</b> 重连生效。
        </p>
      </div>
      </SearchableSection>

      {/* Proactive */}
      <SearchableSection
        title="主动开口"
        keywords={["proactive", "主动", "cooldown", "idle", "quiet", "chatty", "companion", "mute", "heartbeat", "心跳"]}
        query={searchQuery}
      >
      <div style={sectionStyle}>
        <h4 style={sectionTitle}><HighlightedText text="主动开口" query={searchQuery} /></h4>
        <label style={{ ...labelStyle, display: "flex", alignItems: "center", gap: "6px", marginBottom: "8px" }}>
          <input
            type="checkbox"
            checked={form.proactive.enabled}
            onChange={(e) =>
              setForm({ ...form, proactive: { ...form.proactive, enabled: e.target.checked } })
            }
          />
          启用宠物主动跟我说话
        </label>

        <div style={twoColRow}>
          <PanelNumberField
            label="检查间隔 (秒)"
            value={form.proactive.interval_seconds}
            min={60}
            onChange={(v) => setForm({ ...form, proactive: { ...form.proactive, interval_seconds: v } })}
          />
          <PanelNumberField
            label="冷却 (秒)"
            value={form.proactive.cooldown_seconds}
            min={0}
            onChange={(v) => setForm({ ...form, proactive: { ...form.proactive, cooldown_seconds: v } })}
          />
        </div>
        <div style={twoColRow}>
          <PanelNumberField
            label="最少静默 (秒)"
            value={form.proactive.idle_threshold_seconds}
            min={60}
            onChange={(v) => setForm({ ...form, proactive: { ...form.proactive, idle_threshold_seconds: v } })}
          />
          <PanelNumberField
            label="键鼠空闲 (秒)"
            value={form.proactive.input_idle_seconds}
            min={0}
            onChange={(v) => setForm({ ...form, proactive: { ...form.proactive, input_idle_seconds: v } })}
          />
        </div>
        <div style={twoColRow}>
          <PanelNumberField
            label="安静时段开始 (时)"
            value={form.proactive.quiet_hours_start}
            min={0}
            onChange={(v) =>
              setForm({
                ...form,
                proactive: { ...form.proactive, quiet_hours_start: Math.max(0, Math.min(23, v)) },
              })
            }
          />
          <PanelNumberField
            label="安静时段结束 (时)"
            value={form.proactive.quiet_hours_end}
            min={0}
            onChange={(v) =>
              setForm({
                ...form,
                proactive: { ...form.proactive, quiet_hours_end: Math.max(0, Math.min(23, v)) },
              })
            }
          />
        </div>
        <label style={{ ...labelStyle, display: "flex", alignItems: "center", gap: "6px", marginTop: "8px" }}>
          <input
            type="checkbox"
            checked={form.proactive.respect_focus_mode}
            onChange={(e) =>
              setForm({ ...form, proactive: { ...form.proactive, respect_focus_mode: e.target.checked } })
            }
          />
          开启 macOS 勿扰/Focus 时不打扰
        </label>
        <div style={{ marginTop: "8px" }}>
          <PanelNumberField
            label="今天主动开口达到此数后变克制（0 = 关闭）"
            value={form.proactive.chatty_day_threshold}
            min={0}
            onChange={(v) =>
              setForm({
                ...form,
                proactive: { ...form.proactive, chatty_day_threshold: Math.max(0, v) },
              })
            }
          />
        </div>
        {/* Iter R29: companion_mode dropdown — high-level dial layered on
            cooldown + chatty_threshold. R13 added the backend field but the
            UI was deferred ("R13b"); R29 ships it. Three options map to
            apply_companion_mode multipliers (×1 / ×0.5cooldown+×2chatty /
            ×2cooldown+×0.5chatty). */}
        <div style={{ marginTop: "10px" }}>
          <label style={{ display: "block", fontSize: "12px", color: "var(--pet-color-fg)", marginBottom: "4px" }}>
            高层级"陪伴模式"（叠加 cooldown / chatty 后的实际效果）
          </label>
          <select
            value={form.proactive.companion_mode}
            onChange={(e) =>
              setForm({
                ...form,
                proactive: { ...form.proactive, companion_mode: e.target.value },
              })
            }
            style={{
              width: "100%",
              padding: "6px 8px",
              fontSize: "13px",
              border: "1px solid var(--pet-color-border)",
              borderRadius: "6px",
              background: "var(--pet-color-card)",
              color: "var(--pet-color-fg)",
            }}
          >
            <option value="balanced">balanced — 默认（不改 base）</option>
            <option value="chatty">chatty — ×0.5 cooldown · ×2 chatty 阈值（多说）</option>
            <option value="quiet">quiet — ×2 cooldown · ×0.5 chatty 阈值（少说）</option>
          </select>
          <div style={{ fontSize: "11px", color: "var(--pet-color-muted)", marginTop: "4px", lineHeight: 1.5 }}>
            base cooldown=0 时三档都返 0，保留"显式关闭" 语义。R7 反馈适配器在此模式之上再叠加
            ratio 调整（&gt;0.6 ×2 / &lt;0.2 ×0.7）。
          </div>
        </div>
      </div>
      </SearchableSection>

      {/* Morning Briefing */}
      <SearchableSection
        title="早安简报"
        keywords={["morning", "briefing", "早安", "简报", "天气", "日历"]}
        query={searchQuery}
      >
      <div style={sectionStyle}>
        <h4 style={sectionTitle}><HighlightedText text="早安简报" query={searchQuery} /></h4>
        <label style={{ ...labelStyle, display: "flex", alignItems: "center", gap: "6px", marginBottom: "8px" }}>
          <input
            type="checkbox"
            checked={form.morning_briefing?.enabled ?? true}
            onChange={(e) =>
              setForm({
                ...form,
                morning_briefing: {
                  enabled: e.target.checked,
                  hour: form.morning_briefing?.hour ?? 8,
                  minute: form.morning_briefing?.minute ?? 30,
                },
              })
            }
          />
          每天早晨让宠物主动播报天气 / 日程 / 昨日回顾
        </label>
        <div style={twoColRow}>
          <PanelNumberField
            label="触发小时 (0-23)"
            value={form.morning_briefing?.hour ?? 8}
            min={0}
            onChange={(v) =>
              setForm({
                ...form,
                morning_briefing: {
                  enabled: form.morning_briefing?.enabled ?? true,
                  hour: Math.max(0, Math.min(23, v)),
                  minute: form.morning_briefing?.minute ?? 30,
                },
              })
            }
          />
          <PanelNumberField
            label="触发分钟 (0-59)"
            value={form.morning_briefing?.minute ?? 30}
            min={0}
            onChange={(v) =>
              setForm({
                ...form,
                morning_briefing: {
                  enabled: form.morning_briefing?.enabled ?? true,
                  hour: form.morning_briefing?.hour ?? 8,
                  minute: Math.max(0, Math.min(59, v)),
                },
              })
            }
          />
        </div>
        <div style={{ fontSize: "11px", color: "var(--pet-color-muted)", marginTop: "4px", lineHeight: 1.5 }}>
          到点后 1 小时内的第一个 proactive tick 触发；mute / Focus 期间安静顺延到次日。
        </div>
      </div>
      </SearchableSection>

      {/* Tool Risk Overrides */}
      <SearchableSection
        title="工具风险"
        keywords={["tool", "risk", "审核", "review", "approve", "deny", "风险"]}
        query={searchQuery}
      >
      <div style={sectionStyle}>
        <h4 style={sectionTitle}><HighlightedText text="工具风险" query={searchQuery} /></h4>
        <div style={{ fontSize: "11px", color: "var(--pet-color-muted)", marginBottom: 8, lineHeight: 1.5 }}>
          每次工具调用都会先经分类器自动定级。这里按工具单独覆盖：「自动」跟分类器走（高危才弹审核）；「总是审核」无论什么情况都让你先确认；「总是放行」哪怕高危也直接执行。MCP 工具不在列表里，按默认 medium 处理。
        </div>
        {toolRiskRows.length === 0 ? (
          <div style={{ fontSize: 12, color: "var(--pet-color-muted)" }}>加载中…</div>
        ) : (
          <div style={{ display: "grid", gridTemplateColumns: "auto 1fr auto", gap: "6px 10px", alignItems: "center" }}>
            {toolRiskRows.map((row) => {
              const mode = form.tool_review_overrides?.[row.name] ?? "auto";
              const levelColor =
                row.level === "high"
                  ? { bg: "#fee2e2", fg: "#b91c1c" }
                  : row.level === "medium"
                    ? { bg: "#fef3c7", fg: "#92400e" }
                    : { bg: "#dcfce7", fg: "#166534" };
              return (
                <div key={row.name} style={{ display: "contents" }}>
                  <div style={{ display: "flex", alignItems: "center", gap: 6, fontSize: 12 }}>
                    <code style={{ background: "#f1f5f9", padding: "2px 6px", borderRadius: 4, fontSize: 11 }}>{row.name}</code>
                    <span style={{ fontSize: 10, padding: "1px 6px", borderRadius: 8, background: levelColor.bg, color: levelColor.fg }}>{row.level}</span>
                  </div>
                  <div style={{ fontSize: 11, color: "var(--pet-color-muted)", lineHeight: 1.4 }}>{row.note}</div>
                  <select
                    value={mode}
                    onChange={(e) => {
                      const v = e.target.value;
                      setForm((prev) => {
                        const next = { ...(prev.tool_review_overrides ?? {}) };
                        if (v === "auto") {
                          delete next[row.name];
                        } else {
                          next[row.name] = v;
                        }
                        return { ...prev, tool_review_overrides: next };
                      });
                    }}
                    style={{ padding: "3px 6px", fontSize: 12, border: "1px solid var(--pet-color-border)", borderRadius: 4, background: "var(--pet-color-card)", color: "var(--pet-color-fg)" }}
                  >
                    <option value="auto">自动</option>
                    <option value="always_review">总是审核</option>
                    <option value="always_approve">总是放行</option>
                  </select>
                </div>
              );
            })}
          </div>
        )}
      </div>
      </SearchableSection>

      {/* Memory Consolidate */}
      <SearchableSection
        title="记忆整理"
        keywords={["consolidate", "memory", "整理", "记忆", "stale", "weekly"]}
        query={searchQuery}
      >
      <div style={sectionStyle}>
        <h4 style={sectionTitle}><HighlightedText text="记忆整理" query={searchQuery} /></h4>
        <label style={{ ...labelStyle, display: "flex", alignItems: "center", gap: "6px", marginBottom: "8px" }}>
          <input
            type="checkbox"
            checked={form.memory_consolidate.enabled}
            onChange={(e) =>
              setForm({
                ...form,
                memory_consolidate: { ...form.memory_consolidate, enabled: e.target.checked },
              })
            }
          />
          启用后台记忆整理
        </label>
        <div style={twoColRow}>
          <PanelNumberField
            label="间隔 (小时)"
            value={form.memory_consolidate.interval_hours}
            min={1}
            onChange={(v) =>
              setForm({
                ...form,
                memory_consolidate: { ...form.memory_consolidate, interval_hours: v },
              })
            }
          />
          <PanelNumberField
            label="触发条目数"
            value={form.memory_consolidate.min_total_items}
            min={0}
            onChange={(v) =>
              setForm({
                ...form,
                memory_consolidate: { ...form.memory_consolidate, min_total_items: v },
              })
            }
          />
        </div>
        <div style={twoColRow}>
          <PanelNumberField
            label="清理过期 reminder (小时)"
            value={form.memory_consolidate.stale_reminder_hours}
            min={1}
            onChange={(v) =>
              setForm({
                ...form,
                memory_consolidate: { ...form.memory_consolidate, stale_reminder_hours: v },
              })
            }
          />
          <PanelNumberField
            label="清理过期 plan (小时)"
            value={form.memory_consolidate.stale_plan_hours}
            min={1}
            onChange={(v) =>
              setForm({
                ...form,
                memory_consolidate: { ...form.memory_consolidate, stale_plan_hours: v },
              })
            }
          />
        </div>
        {/* Iter R30: surface the two settings that were yaml-only debt —
            stale_once_butler_hours (Cλ) and stale_daily_review_days (R17). */}
        <div style={twoColRow}>
          <PanelNumberField
            label="清理已完成 [once] butler 任务 (小时)"
            value={form.memory_consolidate.stale_once_butler_hours}
            min={1}
            onChange={(v) =>
              setForm({
                ...form,
                memory_consolidate: { ...form.memory_consolidate, stale_once_butler_hours: v },
              })
            }
          />
          <PanelNumberField
            label="清理过期 daily_review (天，0=关闭)"
            value={form.memory_consolidate.stale_daily_review_days}
            min={0}
            onChange={(v) =>
              setForm({
                ...form,
                memory_consolidate: {
                  ...form.memory_consolidate,
                  stale_daily_review_days: Math.max(0, v),
                },
              })
            }
          />
        </div>
        <div style={{ fontSize: "11px", color: "var(--pet-color-muted)", marginTop: "4px" }}>
          reminder：consolidate 跑时删超过该时长的过期 [remind: YYYY-MM-DD HH:MM]。plan：daily_plan 条目 updated_at 超过该时长就清空。butler：完成的 [once] 任务过该时长后被自动清掉。daily_review：保留最近 N 天的 22:00 写入的 ai_insights/daily_review_YYYY-MM-DD 条目；0 = 永不清理。
        </div>
      </div>
      </SearchableSection>

      {/* Chat context */}
      <SearchableSection
        title="对话上下文"
        keywords={["chat", "context", "message", "上下文"]}
        query={searchQuery}
      >
      <div style={sectionStyle}>
        <h4 style={sectionTitle}><HighlightedText text="对话上下文" query={searchQuery} /></h4>
        <div style={twoColRow}>
          <PanelNumberField
            label="历史保留条数 (0=不限)"
            value={form.chat.max_context_messages}
            min={0}
            onChange={(v) =>
              setForm({ ...form, chat: { ...form.chat, max_context_messages: v } })
            }
          />
          <div style={{ flex: 1 }} />
        </div>
        <div style={{ fontSize: "11px", color: "var(--pet-color-muted)", marginTop: "4px" }}>
          桌面 chat 和 Telegram 都按此上限裁剪。前端仍展示全部消息，仅发给 LLM 时裁。
        </div>
      </div>
      </SearchableSection>

      {/* Privacy redaction */}
      <SearchableSection
        title="隐私过滤"
        keywords={["privacy", "redaction", "regex", "pattern", "私人", "隐私"]}
        query={searchQuery}
      >
      <div style={sectionStyle}>
        <h4 style={sectionTitle}><HighlightedText text="隐私过滤" query={searchQuery} /></h4>
        <label style={labelStyle}>
          子串关键词（一行一个，大小写不敏感；匹配位置替换为 `(私人)`）
        </label>
        <textarea
          value={(form.privacy?.redaction_patterns ?? []).join("\n")}
          onChange={(e) =>
            setForm({
              ...form,
              privacy: {
                redaction_patterns: e.target.value
                  .split("\n")
                  .map((s) => s.trim())
                  .filter((s) => s.length > 0),
                regex_patterns: form.privacy?.regex_patterns ?? [],
              },
            })
          }
          rows={4}
          style={{ ...inputStyle, resize: "vertical", fontFamily: "monospace", fontSize: "12px" }}
          placeholder={"Slack\n某客户公司名\n项目代号"}
        />

        <label style={{ ...labelStyle, marginTop: "10px" }}>
          正则模式（一行一个；匹配命中也替换为 `(私人)`；非法语法自动跳过）
        </label>
        <textarea
          value={(form.privacy?.regex_patterns ?? []).join("\n")}
          onChange={(e) =>
            setForm({
              ...form,
              privacy: {
                redaction_patterns: form.privacy?.redaction_patterns ?? [],
                regex_patterns: e.target.value
                  .split("\n")
                  .map((s) => s.trim())
                  .filter((s) => s.length > 0),
              },
            })
          }
          rows={3}
          style={{ ...inputStyle, resize: "vertical", fontFamily: "monospace", fontSize: "12px" }}
          placeholder={String.raw`\b\d{4}-\d{4}-\d{4}-\d{4}\b
[\w.+-]+@[\w-]+\.[\w.-]+`}
        />
        <div style={{ fontSize: "11px", color: "var(--pet-color-muted)", marginTop: "4px" }}>
          覆盖 5 个 prompt 注入通道：active_window 工具、calendar 工具、mood note、speech_history 反哺、persona_summary 反哺。
          子串先于正则应用。Rust regex 引擎线性时间，不支持反向引用——天然 ReDoS 安全。修改即时生效。
        </div>
      </div>
      </SearchableSection>

      {/* SOUL */}
      <SearchableSection
        title="系统提示词 (SOUL.md)"
        keywords={["soul", "prompt", "persona", "人格", "设定", "系统提示词"]}
        query={searchQuery}
      >
      <div style={sectionStyle}>
        <h4 style={sectionTitle}><HighlightedText text="系统提示词 (SOUL.md)" query={searchQuery} /></h4>
        <textarea
          value={soul}
          onChange={(e) => setSoul(e.target.value)}
          rows={6}
          style={{ ...inputStyle, resize: "vertical", fontFamily: "inherit", lineHeight: "1.5" }}
          placeholder="输入 AI 角色设定..."
        />
      </div>
      </SearchableSection>

      {/* 全部 section 都被搜索过滤掉时的 empty-state。Save 按钮始终可见。 */}
      {searchQuery.trim().length > 0 && SETTINGS_SECTION_INDEX.every(
        ([title, keywords]) => !matchSection(title, keywords, searchQuery),
      ) && (
        <div style={{ padding: "16px", textAlign: "center", color: "var(--pet-color-muted)", fontSize: "13px", border: "1px dashed var(--pet-color-border)", borderRadius: "8px", marginBottom: "12px" }}>
          没有匹配「{searchQuery}」的设置项；试试其它关键字（如 api / mute / regex / 工具）
        </div>
      )}

      {/* Save */}
      <div style={{ display: "flex", alignItems: "center", gap: "12px", marginTop: "8px" }}>
        <button onClick={handleSave} disabled={saving} style={btnStyle}>
          {saving ? "保存中..." : "保存"}
        </button>
        {message && (
          <span style={{ fontSize: "13px", color: message.includes("失败") ? "#ef4444" : "#22c55e" }}>
            {message}
          </span>
        )}
      </div>
      </>
      )}
    </div>
  );
}

/// 设置面板各 section 的标题与关键字索引——给 empty-state 用（"全部过滤掉了
/// 吗？"判定）。SearchableSection 内联标题/关键字 props 是真相源；这里复制一
/// 份是因为 React component 渲染顺序和"是否有任意命中"是两个独立查询，不复
/// 用一份内存又没好办法。新增 section 时同步加一行即可（漏加只影响 empty-
/// state 表现，不影响主流程渲染）。
const SETTINGS_SECTION_INDEX: ReadonlyArray<readonly [string, readonly string[]]> = [
  ["Live2D 模型", ["live2d", "model", "motion", "miku", "映射", "动作"]],
  ["LLM 配置", ["llm", "api", "key", "model", "openai", "base", "url", "gpt"]],
  ["MCP Servers", ["mcp", "server", "tool", "工具", "服务器"]],
  ["Telegram Bot", ["telegram", "tg", "bot", "token", "username", "机器人"]],
  ["主动开口", ["proactive", "主动", "cooldown", "idle", "quiet", "chatty", "companion", "mute", "heartbeat", "心跳"]],
  ["早安简报", ["morning", "briefing", "早安", "简报", "天气", "日历"]],
  ["工具风险", ["tool", "risk", "审核", "review", "approve", "deny", "风险"]],
  ["记忆整理", ["consolidate", "memory", "整理", "记忆", "stale", "weekly"]],
  ["对话上下文", ["chat", "context", "message", "上下文"]],
  ["隐私过滤", ["privacy", "redaction", "regex", "pattern", "私人", "隐私"]],
  ["系统提示词 (SOUL.md)", ["soul", "prompt", "persona", "人格", "设定", "系统提示词"]],
];

/// 设置面板搜索：判定一条 section 在当前 query 下是否应展示。空 query 即所有
/// section 全展示；非空 query 时，标题 + 关键字数组任一含 query 子串（大小写
/// 不敏感）即视为命中。pure 函数，让 SearchableSection 与 empty-state 共用。
function matchSection(title: string, keywords: readonly string[], query: string): boolean {
  const q = query.trim().toLowerCase();
  if (q.length === 0) return true;
  const haystacks = [title, ...keywords].map((s) => s.toLowerCase());
  return haystacks.some((s) => s.includes(q));
}

/// 简单 wrapper：query 不命中时返回 null（隐藏整个 section）。children 直接
/// 透传，不影响既有 sectionStyle / 内部布局。
/// 设置面板搜索结果高亮：把 query 子串在 text 里第一次出现的位置用 `<mark>`
/// 包起来。空 query 或未命中时原样输出（不显示 mark）。
/// 配色与 PanelChat SearchResultRow 一致（黄底深棕字），让"设置 / 聊天"两处
/// 搜索的视觉统一。
const HIGHLIGHT_MARK_STYLE: React.CSSProperties = {
  background: "#fef3c7",
  color: "#92400e",
  padding: "0 1px",
  borderRadius: 2,
};

function HighlightedText({ text, query }: { text: string; query: string }) {
  const q = query.trim();
  if (q.length === 0) return <>{text}</>;
  const idx = text.toLowerCase().indexOf(q.toLowerCase());
  if (idx < 0) return <>{text}</>;
  return (
    <>
      {text.slice(0, idx)}
      <mark style={HIGHLIGHT_MARK_STYLE}>{text.slice(idx, idx + q.length)}</mark>
      {text.slice(idx + q.length)}
    </>
  );
}

function SearchableSection({
  title,
  keywords = [],
  query,
  children,
}: {
  title: string;
  keywords?: string[];
  query: string;
  children: React.ReactNode;
}) {
  if (!matchSection(title, keywords, query)) return null;
  return <>{children}</>;
}

const twoColRow: React.CSSProperties = {
  display: "flex",
  gap: "8px",
  marginBottom: "6px",
};

// Bind the shared NumberField to this panel's local styles. Call sites stay free of
// style boilerplate; the shared component owns the input-handling logic.
function PanelNumberField(props: {
  label: string;
  value: number;
  min?: number;
  onChange: (v: number) => void;
}) {
  return <SharedNumberField {...props} labelStyle={labelStyle} inputStyle={inputStyle} />;
}

/* ---------- MCP Server Card ---------- */

function McpServerEntry({
  name,
  config,
  status,
  onChange,
  onRemove,
}: {
  name: string;
  config: McpServerConfig;
  status?: McpStatus;
  onChange: (updates: Partial<McpServerConfig>) => void;
  onRemove: () => void;
}) {
  const [expanded, setExpanded] = useState(true);

  const statusDot = status?.connected ? "#22c55e" : status?.error ? "#ef4444" : "#94a3b8";
  const statusLabel = status?.connected
    ? "已连接"
    : status?.error === "Disabled"
      ? "已禁用"
      : status?.error
        ? "连接失败"
        : "未连接";

  return (
    <div style={{ ...mcpCardStyle, borderColor: status?.error && status.error !== "Disabled" ? "#fca5a5" : "var(--pet-color-border)" }}>
      {/* Header row */}
      <div
        style={{ display: "flex", alignItems: "center", gap: "8px", cursor: "pointer" }}
        onClick={() => setExpanded(!expanded)}
      >
        {/* Status dot */}
        <span style={{ width: 8, height: 8, borderRadius: "50%", background: statusDot, flexShrink: 0 }} />

        {/* Name + status */}
        <span style={{ fontWeight: 600, fontSize: "13px", flex: 1, color: "var(--pet-color-fg)" }}>
          {name}
          <span style={{ fontWeight: 400, fontSize: "11px", color: "var(--pet-color-muted)", marginLeft: "8px" }}>
            {config.transport.toUpperCase()}
          </span>
          <span
            style={{
              fontWeight: 400,
              fontSize: "11px",
              marginLeft: "6px",
              color: status?.connected ? "#22c55e" : status?.error && status.error !== "Disabled" ? "#ef4444" : "#94a3b8",
            }}
          >
            {statusLabel}
            {status?.connected && ` · ${status.tool_count} 工具`}
          </span>
        </span>

        {/* Enable toggle */}
        <label
          style={{ fontSize: "12px", color: "var(--pet-color-muted)", display: "flex", alignItems: "center", gap: "4px" }}
          onClick={(e) => e.stopPropagation()}
        >
          <input
            type="checkbox"
            checked={config.enabled}
            onChange={(e) => onChange({ enabled: e.target.checked })}
          />
          启用
        </label>

        {/* Delete */}
        <button onClick={(e) => { e.stopPropagation(); onRemove(); }} style={btnDangerStyle}>
          删除
        </button>

        {/* Collapse */}
        <span style={{ fontSize: "10px", color: "var(--pet-color-muted)", userSelect: "none" }}>
          {expanded ? "▲" : "▼"}
        </span>
      </div>

      {/* Expanded: config fields + tool list */}
      {expanded && (
        <div style={{ marginTop: "10px", borderTop: "1px solid var(--pet-color-border)", paddingTop: "10px" }}>
          {/* Error message */}
          {status?.error && status.error !== "Disabled" && (
            <div style={{ background: "var(--pet-tint-orange-bg)", border: "1px solid var(--pet-tint-orange-fg)", borderRadius: "6px", padding: "6px 10px", marginBottom: "8px", fontSize: "12px", color: "var(--pet-tint-orange-fg)" }}>
              {status.error}
            </div>
          )}

          {/* Transport */}
          <label style={labelStyle}>传输方式</label>
          <select
            value={config.transport}
            onChange={(e) => onChange({ transport: e.target.value as McpServerConfig["transport"] })}
            style={{ ...inputStyle, marginBottom: "8px" }}
          >
            <option value="stdio">stdio (本地进程)</option>
            <option value="sse">SSE (远程)</option>
            <option value="http">HTTP (远程)</option>
          </select>

          {/* stdio fields */}
          {config.transport === "stdio" ? (
            <>
              <label style={labelStyle}>命令</label>
              <input
                value={config.command}
                onChange={(e) => onChange({ command: e.target.value })}
                style={{ ...inputStyle, marginBottom: "6px", fontFamily: "monospace", fontSize: "12px" }}
                placeholder="npx"
              />
              <label style={labelStyle}>参数 (每行一个)</label>
              <textarea
                value={config.args.join("\n")}
                onChange={(e) => onChange({ args: e.target.value.split("\n") })}
                rows={3}
                style={{ ...inputStyle, resize: "vertical", fontFamily: "monospace", fontSize: "12px", marginBottom: "6px" }}
                placeholder={"-y\n@modelcontextprotocol/server-filesystem\n/tmp"}
              />
              <label style={labelStyle}>环境变量 (KEY=VALUE，每行一个)</label>
              <textarea
                value={Object.entries(config.env || {}).map(([k, v]) => `${k}=${v}`).join("\n")}
                onChange={(e) => {
                  const env: Record<string, string> = {};
                  e.target.value.split("\n").forEach((line) => {
                    const idx = line.indexOf("=");
                    if (idx > 0) env[line.slice(0, idx)] = line.slice(idx + 1);
                  });
                  onChange({ env });
                }}
                rows={2}
                style={{ ...inputStyle, resize: "vertical", fontFamily: "monospace", fontSize: "12px" }}
                placeholder="GITHUB_TOKEN=ghp_xxx"
              />
            </>
          ) : (
            /* sse / http fields */
            <>
              <label style={labelStyle}>URL</label>
              <input
                value={config.url}
                onChange={(e) => onChange({ url: e.target.value })}
                style={{ ...inputStyle, marginBottom: "6px", fontFamily: "monospace", fontSize: "12px" }}
                placeholder="http://localhost:3000/mcp"
              />
              <label style={labelStyle}>自定义 Headers (KEY: VALUE，每行一个)</label>
              <textarea
                value={Object.entries(config.headers || {}).map(([k, v]) => `${k}: ${v}`).join("\n")}
                onChange={(e) => {
                  const headers: Record<string, string> = {};
                  e.target.value.split("\n").forEach((line) => {
                    const idx = line.indexOf(":");
                    if (idx > 0) headers[line.slice(0, idx).trim()] = line.slice(idx + 1).trim();
                  });
                  onChange({ headers });
                }}
                rows={2}
                style={{ ...inputStyle, resize: "vertical", fontFamily: "monospace", fontSize: "12px" }}
                placeholder="Authorization: Bearer xxx"
              />
            </>
          )}

          {/* Connected tool list */}
          {status?.connected && status.tool_names.length > 0 && (
            <div style={{ marginTop: "8px" }}>
              <label style={labelStyle}>已注册工具 ({status.tool_count})</label>
              <div style={{ display: "flex", flexWrap: "wrap", gap: "4px" }}>
                {status.tool_names.map((t) => (
                  <span key={t} style={toolBadgeStyle}>{t}</span>
                ))}
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

/* ---------- Styles ---------- */

const containerStyle: React.CSSProperties = {
  padding: "20px 24px",
  height: "100%",
  overflowY: "auto",
};

const sectionStyle: React.CSSProperties = {
  marginBottom: "20px",
};

const sectionTitle: React.CSSProperties = {
  margin: "0 0 10px",
  fontSize: "14px",
  fontWeight: 600,
  color: "var(--pet-color-fg)",
};

const labelStyle: React.CSSProperties = {
  display: "block",
  fontSize: "12px",
  color: "var(--pet-color-muted)",
  marginBottom: "4px",
  fontWeight: 500,
};

const inputStyle: React.CSSProperties = {
  width: "100%",
  padding: "8px 12px",
  borderRadius: "8px",
  border: "1px solid var(--pet-color-border)",
  fontSize: "13px",
  outline: "none",
  color: "var(--pet-color-fg)",
  boxSizing: "border-box",
  background: "var(--pet-color-card)",
};

const btnStyle: React.CSSProperties = {
  padding: "8px 24px",
  borderRadius: "8px",
  border: "none",
  background: "var(--pet-color-accent)",
  color: "#fff",
  fontSize: "14px",
  fontWeight: 500,
  cursor: "pointer",
};

const btnSmallStyle: React.CSSProperties = {
  padding: "6px 14px",
  borderRadius: "6px",
  border: "none",
  color: "#fff",
  fontSize: "12px",
  fontWeight: 500,
  cursor: "pointer",
  whiteSpace: "nowrap",
};

const btnDangerStyle: React.CSSProperties = {
  padding: "2px 8px",
  borderRadius: "4px",
  border: "none",
  background: "#ef4444",
  color: "#fff",
  fontSize: "11px",
  cursor: "pointer",
};

const mcpCardStyle: React.CSSProperties = {
  border: "1px solid var(--pet-color-border)",
  borderRadius: "8px",
  padding: "10px 12px",
  marginBottom: "8px",
  background: "var(--pet-color-bg)",
};

const toolBadgeStyle: React.CSSProperties = {
  display: "inline-block",
  padding: "2px 8px",
  borderRadius: "4px",
  background: "#e0f2fe",
  color: "#0369a1",
  fontSize: "11px",
  fontFamily: "monospace",
};
