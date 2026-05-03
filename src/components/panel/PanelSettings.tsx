import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { AppSettings, McpServerConfig } from "../../hooks/useSettings";
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
    telegram: { bot_token: "", allowed_username: "", enabled: false, persona_layer_enabled: true },
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
    },
    memory_consolidate: {
      enabled: false,
      interval_hours: 6,
      min_total_items: 12,
      stale_reminder_hours: 24,
      stale_plan_hours: 24,
    },
    chat: {
      max_context_messages: 50,
    },
    privacy: {
      redaction_patterns: [],
    },
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
  const [viewMode, setViewMode] = useState<"form" | "raw">("form");
  const [rawYaml, setRawYaml] = useState("");

  useEffect(() => {
    Promise.all([
      invoke<AppSettings>("get_settings"),
      invoke<string>("get_soul"),
      invoke<McpStatus[]>("get_mcp_status"),
      invoke<TelegramStatus>("get_telegram_status").catch(() => ({ running: false, error: null }) as TelegramStatus),
    ]).then(([s, soulContent, statuses, tgStatus]) => {
      setForm(s);
      setSoul(soulContent);
      setMcpStatuses(statuses);
      setTelegramStatus(tgStatus);
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
    <div style={containerStyle}>
      {/* View mode toggle */}
      <div style={{ display: "flex", gap: "4px", marginBottom: "16px", background: "#e2e8f0", borderRadius: "8px", padding: "3px" }}>
        <button
          onClick={viewMode === "raw" ? switchToForm : undefined}
          style={{
            flex: 1,
            padding: "6px 0",
            borderRadius: "6px",
            border: "none",
            background: viewMode === "form" ? "#fff" : "transparent",
            color: viewMode === "form" ? "#1e293b" : "#64748b",
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
            background: viewMode === "raw" ? "#fff" : "transparent",
            color: viewMode === "raw" ? "#1e293b" : "#64748b",
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
      {/* Live2D */}
      <div style={sectionStyle}>
        <h4 style={sectionTitle}>Live2D 模型</h4>
        <label style={labelStyle}>模型路径</label>
        <input
          value={form.live_2d_model_path}
          onChange={(e) => setForm({ ...form, live_2d_model_path: e.target.value })}
          style={inputStyle}
          placeholder="/models/miku/miku.model3.json"
        />
      </div>

      {/* LLM Config */}
      <div style={sectionStyle}>
        <h4 style={sectionTitle}>LLM 配置</h4>
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
          value={form.model}
          onChange={(e) => setForm({ ...form, model: e.target.value })}
          style={inputStyle}
          placeholder="gpt-4o-mini"
        />
      </div>

      {/* MCP Servers */}
      <div style={sectionStyle}>
        <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: "10px" }}>
          <h4 style={{ ...sectionTitle, margin: 0 }}>
            MCP Servers
            {serverEntries.length > 0 && (
              <span style={{ fontWeight: 400, fontSize: "12px", color: "#64748b", marginLeft: "8px" }}>
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
          <div style={{ padding: "16px", textAlign: "center", color: "#94a3b8", fontSize: "13px", border: "1px dashed #e2e8f0", borderRadius: "8px" }}>
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

      {/* Telegram Bot */}
      <div style={sectionStyle}>
        <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: "10px" }}>
          <h4 style={{ ...sectionTitle, margin: 0 }}>
            Telegram Bot
            <span style={{
              fontWeight: 400,
              fontSize: "11px",
              marginLeft: "8px",
              color: telegramStatus.running ? "#22c55e" : telegramStatus.error ? "#ef4444" : "#94a3b8",
            }}>
              {telegramStatus.running ? "运行中" : telegramStatus.error ? "连接失败" : "未启动"}
            </span>
          </h4>
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
            disabled={telegramReconnecting}
            style={{
              ...btnSmallStyle,
              background: telegramReconnecting ? "#94a3b8" : "#0ea5e9",
            }}
          >
            {telegramReconnecting ? "连接中..." : "保存并连接"}
          </button>
        </div>

        {telegramStatus.error && (
          <div style={{ background: "#fef2f2", border: "1px solid #fca5a5", borderRadius: "6px", padding: "6px 10px", marginBottom: "8px", fontSize: "12px", color: "#dc2626" }}>
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

        <label style={labelStyle}>允许的用户名</label>
        <input
          value={form.telegram?.allowed_username ?? ""}
          onChange={(e) => setForm({ ...form, telegram: { ...form.telegram, enabled: form.telegram?.enabled ?? false, bot_token: form.telegram?.bot_token ?? "", allowed_username: e.target.value } })}
          style={{ ...inputStyle, fontFamily: "monospace", fontSize: "12px" }}
          placeholder="@username (留空则允许所有人)"
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
      </div>

      {/* Proactive */}
      <div style={sectionStyle}>
        <h4 style={sectionTitle}>主动开口</h4>
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
      </div>

      {/* Memory Consolidate */}
      <div style={sectionStyle}>
        <h4 style={sectionTitle}>记忆整理</h4>
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
        <div style={{ fontSize: "11px", color: "#94a3b8", marginTop: "4px" }}>
          reminder：consolidate 跑时删超过该时长的过期 [remind: YYYY-MM-DD HH:MM]。plan：daily_plan 条目 updated_at 超过该时长就清空。
        </div>
      </div>

      {/* Chat context */}
      <div style={sectionStyle}>
        <h4 style={sectionTitle}>对话上下文</h4>
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
        <div style={{ fontSize: "11px", color: "#94a3b8", marginTop: "4px" }}>
          桌面 chat 和 Telegram 都按此上限裁剪。前端仍展示全部消息，仅发给 LLM 时裁。
        </div>
      </div>

      {/* Privacy redaction */}
      <div style={sectionStyle}>
        <h4 style={sectionTitle}>隐私过滤</h4>
        <label style={labelStyle}>
          要从环境工具结果里隐去的关键词（一行一个，大小写不敏感；匹配位置替换为 `(私人)`）
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
              },
            })
          }
          rows={4}
          style={{ ...inputStyle, resize: "vertical", fontFamily: "monospace", fontSize: "12px" }}
          placeholder={"Slack\n某客户公司名\n项目代号"}
        />
        <div style={{ fontSize: "11px", color: "#94a3b8", marginTop: "4px" }}>
          应用于 `get_active_window`（app + 标题）和 `get_upcoming_events`（标题 + 地点）。
          每次工具调用时实时套用，留空则不过滤。修改即时生效（下一次工具调用读最新设置）。
        </div>
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
    <div style={{ ...mcpCardStyle, borderColor: status?.error && status.error !== "Disabled" ? "#fca5a5" : "#e2e8f0" }}>
      {/* Header row */}
      <div
        style={{ display: "flex", alignItems: "center", gap: "8px", cursor: "pointer" }}
        onClick={() => setExpanded(!expanded)}
      >
        {/* Status dot */}
        <span style={{ width: 8, height: 8, borderRadius: "50%", background: statusDot, flexShrink: 0 }} />

        {/* Name + status */}
        <span style={{ fontWeight: 600, fontSize: "13px", flex: 1, color: "#1e293b" }}>
          {name}
          <span style={{ fontWeight: 400, fontSize: "11px", color: "#94a3b8", marginLeft: "8px" }}>
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
          style={{ fontSize: "12px", color: "#64748b", display: "flex", alignItems: "center", gap: "4px" }}
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
        <span style={{ fontSize: "10px", color: "#94a3b8", userSelect: "none" }}>
          {expanded ? "▲" : "▼"}
        </span>
      </div>

      {/* Expanded: config fields + tool list */}
      {expanded && (
        <div style={{ marginTop: "10px", borderTop: "1px solid #e2e8f0", paddingTop: "10px" }}>
          {/* Error message */}
          {status?.error && status.error !== "Disabled" && (
            <div style={{ background: "#fef2f2", border: "1px solid #fca5a5", borderRadius: "6px", padding: "6px 10px", marginBottom: "8px", fontSize: "12px", color: "#dc2626" }}>
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
  color: "#1e293b",
};

const labelStyle: React.CSSProperties = {
  display: "block",
  fontSize: "12px",
  color: "#64748b",
  marginBottom: "4px",
  fontWeight: 500,
};

const inputStyle: React.CSSProperties = {
  width: "100%",
  padding: "8px 12px",
  borderRadius: "8px",
  border: "1px solid #e2e8f0",
  fontSize: "13px",
  outline: "none",
  color: "#1e293b",
  boxSizing: "border-box",
  background: "#fff",
};

const btnStyle: React.CSSProperties = {
  padding: "8px 24px",
  borderRadius: "8px",
  border: "none",
  background: "#0ea5e9",
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
  border: "1px solid #e2e8f0",
  borderRadius: "8px",
  padding: "10px 12px",
  marginBottom: "8px",
  background: "#f8fafc",
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
