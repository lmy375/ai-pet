import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { AppSettings, McpServerConfig, McpStatus, TelegramStatus } from "../../hooks/useSettings";
import { Card } from "../ui/Card";
import { Button } from "../ui/Button";
import { Segmented } from "../ui/Segmented";
import { Badge } from "../ui/Badge";
import { Label, TextInput, TextArea, Select } from "../ui/fields";
import { StatusText } from "../ui/StatusText";
import { ChevronDown, ChevronRight, PlusIcon, TrashIcon, ImageIcon } from "../Icons";
import { open } from "@tauri-apps/plugin-dialog";
import { toneText, toneDot, connTone } from "../../utils/tone";

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
    telegram: { bot_token: "", allowed_username: "", enabled: false },
    gallery_dir: "",
    gallery_enabled: false,
    gallery_interval: 10,
  });
  const [loaded, setLoaded] = useState(false);
  const [testing, setTesting] = useState(false);
  const [message, setMessage] = useState("");
  const [mcpStatuses, setMcpStatuses] = useState<McpStatus[]>([]);
  const [reconnecting, setReconnecting] = useState(false);
  const [newServerName, setNewServerName] = useState("");
  const [telegramStatus, setTelegramStatus] = useState<TelegramStatus>({ running: false, error: null });
  const [telegramReconnecting, setTelegramReconnecting] = useState(false);
  const [viewMode, setViewMode] = useState<"表单" | "源码">("表单");
  const [rawYaml, setRawYaml] = useState("");
  const [models, setModels] = useState<string[]>([]);
  const [loadingModels, setLoadingModels] = useState(false);
  const [testResult, setTestResult] = useState<{ ok: boolean; text: string } | null>(null);

  useEffect(() => {
    Promise.all([
      invoke<AppSettings>("get_settings"),
      invoke<McpStatus[]>("get_mcp_status"),
      invoke<TelegramStatus>("get_telegram_status").catch(() => ({ running: false, error: null }) as TelegramStatus),
    ]).then(([s, statuses, tgStatus]) => {
      setForm(s);
      setMcpStatuses(statuses);
      setTelegramStatus(tgStatus);
      setLoaded(true);
      // Pre-populate the model dropdown if the endpoint is already configured.
      if (s.api_base?.trim()) loadModels(s.api_base, s.api_key, true);
    }).catch((e) => {
      console.error("Failed to load settings:", e);
      setLoaded(true);
    });
  }, []);

  // Auto-save current form settings (on blur / Enter). `next` lets callers persist
  // an updated value immediately without waiting for a state flush.
  const saveSettings = async (next?: AppSettings) => {
    try {
      await invoke("save_settings", { settings: next ?? form });
      setMessage("已保存");
    } catch (e: any) {
      setMessage(`保存失败: ${e}`);
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

  // Update an MCP field in-memory only (used while typing); persisted on blur.
  const updateMcpServer = (name: string, updates: Partial<McpServerConfig>) => {
    setForm((prev) => ({
      ...prev,
      mcp_servers: {
        ...prev.mcp_servers,
        [name]: { ...prev.mcp_servers[name], ...updates },
      },
    }));
  };

  // Update an MCP field and persist immediately (used for discrete controls
  // like the transport select and the enable toggle).
  const commitMcpServer = (name: string, updates: Partial<McpServerConfig>) => {
    const next = {
      ...form,
      mcp_servers: {
        ...form.mcp_servers,
        [name]: { ...form.mcp_servers[name], ...updates },
      },
    };
    setForm(next);
    saveSettings(next);
  };

  const removeMcpServer = (name: string) => {
    const { [name]: _, ...rest } = form.mcp_servers;
    const next = { ...form, mcp_servers: rest };
    setForm(next);
    saveSettings(next);
  };

  const addMcpServer = () => {
    const name = newServerName.trim();
    if (!name || form.mcp_servers[name]) return;
    const next = {
      ...form,
      mcp_servers: { ...form.mcp_servers, [name]: emptyMcpServer() },
    };
    setForm(next);
    setNewServerName("");
    saveSettings(next);
  };

  const switchToRaw = async () => {
    try {
      const raw = await invoke<string>("get_config_raw");
      setRawYaml(raw);
      setViewMode("源码");
      setMessage("");
    } catch (e: any) {
      setMessage(`加载配置文件失败: ${e}`);
    }
  };

  const switchToForm = async () => {
    try {
      const s = await invoke<AppSettings>("get_settings");
      setForm(s);
      setViewMode("表单");
      setMessage("");
    } catch (e: any) {
      setMessage(`加载配置失败: ${e}`);
    }
  };

  // Load available models for the given base/key. Triggered automatically when
  // API Base / API Key lose focus. `silent` suppresses the success message.
  const loadModels = async (apiBase: string, apiKey: string, silent = false) => {
    if (!apiBase.trim()) return;
    setLoadingModels(true);
    try {
      const list = await invoke<string[]>("list_models", { apiBase, apiKey });
      setModels(list);
      if (!silent) {
        setMessage(list.length === 0 ? "未获取到可用模型" : `已加载 ${list.length} 个模型`);
      }
    } catch (e: any) {
      setModels([]);
      if (!silent) setMessage(`加载模型失败: ${e}`);
    } finally {
      setLoadingModels(false);
    }
  };

  const handleTestModel = async () => {
    setTesting(true);
    setTestResult(null);
    try {
      await invoke("test_model", {
        apiBase: form.api_base,
        apiKey: form.api_key,
        model: form.model,
      });
      setTestResult({ ok: true, text: "模型可用" });
    } catch (e: any) {
      setTestResult({ ok: false, text: `测试失败: ${e}` });
    } finally {
      setTesting(false);
    }
  };

  // Pick the gallery folder via the native dialog, defaulting to the OS Pictures
  // directory, then persist immediately.
  const handlePickGalleryDir = async () => {
    try {
      const defaultPath = await invoke<string | null>("default_gallery_dir").catch(() => null);
      const picked = await open({
        directory: true,
        multiple: false,
        defaultPath: form.gallery_dir || defaultPath || undefined,
      });
      if (typeof picked === "string") {
        const next = { ...form, gallery_dir: picked };
        setForm(next);
        saveSettings(next);
      }
    } catch (e: any) {
      setMessage(`选择目录失败: ${e}`);
    }
  };

  const handleOpenConfigDir = async () => {
    try {
      await invoke("open_config_dir");
    } catch (e: any) {
      setMessage(`打开配置文件夹失败: ${e}`);
    }
  };

  const saveRaw = async () => {
    try {
      await invoke("save_config_raw", { content: rawYaml });
      setMessage("已保存");
    } catch (e: any) {
      setMessage(`保存失败: ${e}`);
    }
  };

  const onViewChange = (v: "表单" | "源码") => {
    if (v === viewMode) return;
    v === "源码" ? switchToRaw() : switchToForm();
  };

  if (!loaded) {
    return <div className="flex h-full items-center justify-center text-[14px] text-slate-400">加载中...</div>;
  }

  const serverEntries = Object.entries(form.mcp_servers);
  const connectedCount = mcpStatuses.filter((s) => s.connected).length;
  const totalToolCount = mcpStatuses.reduce((sum, s) => sum + s.tool_count, 0);

  const messageLine = message && (
    <StatusText ok={!message.includes("失败")} className="mt-1 text-[13px]">{message}</StatusText>
  );

  return (
    <div className="h-full overflow-y-auto px-5 py-5">
      {/* Top bar: view mode toggle + open config folder */}
      <div className="mb-4 flex items-center justify-between">
        <Segmented value={viewMode} options={["表单", "源码"] as const} onChange={onViewChange} />
        <Button variant="ghost" size="sm" onClick={handleOpenConfigDir} title="在系统文件管理器中打开配置文件夹">
          打开配置文件夹
        </Button>
      </div>

      {viewMode === "源码" ? (
        <>
          <Card title="config.yaml">
            <TextArea
              value={rawYaml}
              onChange={(e) => setRawYaml(e.target.value)}
              onBlur={saveRaw}
              spellCheck={false}
              className="min-h-[300px] whitespace-pre font-mono !text-[12px] leading-relaxed"
            />
          </Card>

          {messageLine}
        </>
      ) : (
        <>
          {/* Live2D */}
          <Card title="Live2D 模型">
            <Label>模型路径</Label>
            <TextInput
              value={form.live_2d_model_path}
              onChange={(e) => setForm({ ...form, live_2d_model_path: e.target.value })}
              onBlur={() => saveSettings()}
              onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
              placeholder="/models/miku/miku.model3.json"
            />
          </Card>

          {/* Gallery slideshow */}
          <Card title="图库轮播">
            <label className="mb-3 flex items-center gap-1.5 text-[12px] font-medium text-slate-600">
              <input
                type="checkbox"
                className="accent-accent"
                checked={form.gallery_enabled}
                onChange={(e) => {
                  const next = { ...form, gallery_enabled: e.target.checked };
                  setForm(next);
                  saveSettings(next);
                }}
              />
              开启图库轮播（主窗口显示图片/视频，不再显示 Live2D，每 10 秒切换）
            </label>

            <Label>图库目录</Label>
            <div className="flex gap-2">
              <TextInput
                value={form.gallery_dir}
                readOnly
                className="flex-1"
                placeholder="尚未选择目录"
              />
              <Button variant="secondary" onClick={handlePickGalleryDir}>
                <ImageIcon className="h-4 w-4" />
                选择目录
              </Button>
            </div>

            <Label className="mt-3">轮播间隔（秒）</Label>
            <TextInput
              type="number"
              min={1}
              value={form.gallery_interval}
              onChange={(e) =>
                setForm({ ...form, gallery_interval: Number(e.target.value) || 0 })
              }
              onBlur={() => {
                const next = { ...form, gallery_interval: Math.max(1, form.gallery_interval || 10) };
                setForm(next);
                saveSettings(next);
              }}
              onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
              placeholder="10"
            />
            <p className="mt-1 text-[11px] text-slate-400">仅作用于图片；视频会完整播放后再切换。</p>
          </Card>

          {/* LLM Config */}
          <Card title="LLM 配置">
            <Label>API Base URL</Label>
            <TextInput
              value={form.api_base}
              onChange={(e) => setForm({ ...form, api_base: e.target.value })}
              onBlur={() => { saveSettings(); loadModels(form.api_base, form.api_key, true); }}
              onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
              placeholder="https://api.openai.com/v1"
            />
            <Label className="mt-3">API Key</Label>
            <TextInput
              type="password"
              value={form.api_key}
              onChange={(e) => setForm({ ...form, api_key: e.target.value })}
              onBlur={() => { saveSettings(); loadModels(form.api_base, form.api_key, true); }}
              onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
              placeholder="sk-..."
            />
            <Label className="mt-3 flex items-center gap-2">
              <span>Model</span>
              {loadingModels && <span className="font-normal text-slate-400">加载中...</span>}
            </Label>
            <div className="flex gap-2">
              <Select
                value={models.includes(form.model) ? form.model : ""}
                onChange={(e) => {
                  const next = { ...form, model: e.target.value };
                  setForm(next);
                  setTestResult(null);
                  saveSettings(next);
                }}
                disabled={models.length === 0}
                className="flex-1"
              >
                {models.length === 0 ? (
                  <option value="">{form.api_base.trim() ? "无可用模型，请检查 URL / API Key" : "请先填写 API Base URL"}</option>
                ) : (
                  <>
                    <option value="" disabled>从 {models.length} 个可用模型中选择...</option>
                    {models.map((m) => (
                      <option key={m} value={m}>{m}</option>
                    ))}
                  </>
                )}
              </Select>
              <Button onClick={handleTestModel} disabled={testing || !form.model.trim()}>
                {testing ? "测试中..." : "测试"}
              </Button>
            </div>
            {testResult && (
              <StatusText ok={testResult.ok} className="mt-1.5 text-[12px]">{testResult.text}</StatusText>
            )}
          </Card>

          {/* MCP Servers */}
          <Card
            title={
              <span>
                MCP Servers
                {serverEntries.length > 0 && (
                  <span className="ml-2 font-normal text-[12px] text-slate-500">
                    {connectedCount}/{serverEntries.length} 已连接 · {totalToolCount} 工具
                  </span>
                )}
              </span>
            }
            action={
              <Button size="sm" onClick={handleReconnectMcp} disabled={reconnecting}>
                {reconnecting ? "连接中..." : "保存并连接"}
              </Button>
            }
          >
            {serverEntries.length === 0 && (
              <div className="rounded-xl border border-dashed border-slate-200 py-4 text-center text-[13px] text-slate-400">
                尚未配置 MCP 服务器，在下方添加
              </div>
            )}

            <div className="flex flex-col gap-2">
              {serverEntries.map(([name, config]) => {
                const status = mcpStatuses.find((s) => s.name === name);
                return (
                  <McpServerEntry
                    key={name}
                    name={name}
                    config={config}
                    status={status}
                    onChange={(updates) => updateMcpServer(name, updates)}
                    onCommit={() => saveSettings()}
                    onCommitChange={(updates) => commitMcpServer(name, updates)}
                    onRemove={() => removeMcpServer(name)}
                  />
                );
              })}
            </div>

            {/* Add server */}
            <div className="mt-3 flex gap-2">
              <TextInput
                value={newServerName}
                onChange={(e) => setNewServerName(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && addMcpServer()}
                className="flex-1"
                placeholder="新服务器名称..."
              />
              <Button
                variant="secondary"
                onClick={addMcpServer}
                disabled={!newServerName.trim() || !!form.mcp_servers[newServerName.trim()]}
              >
                <PlusIcon className="h-4 w-4" />
                添加
              </Button>
            </div>
          </Card>

          {/* Telegram Bot */}
          <Card
            title={
              <span>
                Telegram Bot
                <span
                  className={`ml-2 font-normal text-[11px] ${toneText(connTone(telegramStatus.running, telegramStatus.error))}`}
                >
                  {telegramStatus.running ? "运行中" : telegramStatus.error ? "连接失败" : "未启动"}
                </span>
              </span>
            }
            action={
              <Button
                size="sm"
                disabled={telegramReconnecting}
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
              >
                {telegramReconnecting ? "连接中..." : "保存并连接"}
              </Button>
            }
          >
            {telegramStatus.error && (
              <div className="mb-2 rounded-lg border border-red-300 bg-red-50 px-2.5 py-1.5 text-[12px] text-red-600">
                {telegramStatus.error}
              </div>
            )}

            <label className="mb-2 flex items-center gap-1.5 text-[12px] font-medium text-slate-600">
              <input
                type="checkbox"
                className="accent-accent"
                checked={form.telegram?.enabled ?? false}
                onChange={(e) => {
                  const next = { ...form, telegram: { ...form.telegram, bot_token: form.telegram?.bot_token ?? "", allowed_username: form.telegram?.allowed_username ?? "", enabled: e.target.checked } };
                  setForm(next);
                  saveSettings(next);
                }}
              />
              启用 Telegram Bot
            </label>

            <Label>Bot Token</Label>
            <TextInput
              type="password"
              value={form.telegram?.bot_token ?? ""}
              onChange={(e) => setForm({ ...form, telegram: { ...form.telegram, enabled: form.telegram?.enabled ?? false, allowed_username: form.telegram?.allowed_username ?? "", bot_token: e.target.value } })}
              onBlur={() => saveSettings()}
              onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
              className="mb-2 font-mono !text-[12px]"
              placeholder="123456789:ABCdefGhI..."
            />

            <Label>允许的用户名</Label>
            <TextInput
              value={form.telegram?.allowed_username ?? ""}
              onChange={(e) => setForm({ ...form, telegram: { ...form.telegram, enabled: form.telegram?.enabled ?? false, bot_token: form.telegram?.bot_token ?? "", allowed_username: e.target.value } })}
              onBlur={() => saveSettings()}
              onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
              className="font-mono !text-[12px]"
              placeholder="@username (留空则允许所有人)"
            />
          </Card>

          {messageLine}
        </>
      )}
    </div>
  );
}

/* ---------- MCP Server Card ---------- */

function McpServerEntry({
  name,
  config,
  status,
  onChange,
  onCommit,
  onCommitChange,
  onRemove,
}: {
  name: string;
  config: McpServerConfig;
  status?: McpStatus;
  onChange: (updates: Partial<McpServerConfig>) => void;
  onCommit: () => void;
  onCommitChange: (updates: Partial<McpServerConfig>) => void;
  onRemove: () => void;
}) {
  const [expanded, setExpanded] = useState(true);

  const hasError = !!status?.error && status.error !== "Disabled";
  const dotClass = toneDot(connTone(status?.connected, status?.error));
  const statusLabel = status?.connected
    ? "已连接"
    : status?.error === "Disabled"
      ? "已禁用"
      : status?.error
        ? "连接失败"
        : "未连接";
  const statusColor = toneText(connTone(status?.connected, hasError));

  return (
    <div className={`rounded-xl border bg-slate-50 ${hasError ? "border-red-300" : "border-slate-200"}`}>
      {/* Header row */}
      <div
        className="flex cursor-pointer items-center gap-2 px-3 py-2.5"
        onClick={() => setExpanded(!expanded)}
      >
        <span className={`h-2 w-2 shrink-0 rounded-full ${dotClass}`} />
        <span className="flex-1 text-[13px] font-semibold text-slate-800">
          {name}
          <span className="ml-2 font-normal text-[11px] text-slate-400">{config.transport.toUpperCase()}</span>
          <span className={`ml-1.5 font-normal text-[11px] ${statusColor}`}>
            {statusLabel}
            {status?.connected && ` · ${status.tool_count} 工具`}
          </span>
        </span>

        <label
          className="flex items-center gap-1 text-[12px] text-slate-500"
          onClick={(e) => e.stopPropagation()}
        >
          <input
            type="checkbox"
            className="accent-accent"
            checked={config.enabled}
            onChange={(e) => onCommitChange({ enabled: e.target.checked })}
          />
          启用
        </label>

        <button
          onClick={(e) => { e.stopPropagation(); onRemove(); }}
          title="删除"
          className="flex h-6 w-6 items-center justify-center rounded-md text-slate-400 transition-colors hover:bg-red-50 hover:text-red-500"
        >
          <TrashIcon className="h-4 w-4" />
        </button>

        {expanded ? <ChevronDown className="h-4 w-4 text-slate-400" /> : <ChevronRight className="h-4 w-4 text-slate-400" />}
      </div>

      {/* Expanded: config fields + tool list */}
      {expanded && (
        <div className="border-t border-slate-200 px-3 pb-3 pt-3">
          {hasError && (
            <div className="mb-2 rounded-lg border border-red-300 bg-red-50 px-2.5 py-1.5 text-[12px] text-red-600">
              {status!.error}
            </div>
          )}

          <Label>传输方式</Label>
          <Select
            value={config.transport}
            onChange={(e) => onCommitChange({ transport: e.target.value as McpServerConfig["transport"] })}
            className="mb-2"
          >
            <option value="stdio">stdio (本地进程)</option>
            <option value="sse">SSE (远程)</option>
            <option value="http">HTTP (远程)</option>
          </Select>

          {config.transport === "stdio" ? (
            <>
              <Label>命令</Label>
              <TextInput
                value={config.command}
                onChange={(e) => onChange({ command: e.target.value })}
                onBlur={onCommit}
                onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
                className="mb-1.5 font-mono !text-[12px]"
                placeholder="npx"
              />
              <Label>参数 (每行一个)</Label>
              <TextArea
                value={config.args.join("\n")}
                onChange={(e) => onChange({ args: e.target.value.split("\n") })}
                onBlur={onCommit}
                rows={3}
                className="mb-1.5 font-mono !text-[12px]"
                placeholder={"-y\n@modelcontextprotocol/server-filesystem\n/tmp"}
              />
              <Label>环境变量 (KEY=VALUE，每行一个)</Label>
              <TextArea
                value={Object.entries(config.env || {}).map(([k, v]) => `${k}=${v}`).join("\n")}
                onChange={(e) => {
                  const env: Record<string, string> = {};
                  e.target.value.split("\n").forEach((line) => {
                    const idx = line.indexOf("=");
                    if (idx > 0) env[line.slice(0, idx)] = line.slice(idx + 1);
                  });
                  onChange({ env });
                }}
                onBlur={onCommit}
                rows={2}
                className="font-mono !text-[12px]"
                placeholder="GITHUB_TOKEN=ghp_xxx"
              />
            </>
          ) : (
            <>
              <Label>URL</Label>
              <TextInput
                value={config.url}
                onChange={(e) => onChange({ url: e.target.value })}
                onBlur={onCommit}
                onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
                className="mb-1.5 font-mono !text-[12px]"
                placeholder="http://localhost:3000/mcp"
              />
              <Label>自定义 Headers (KEY: VALUE，每行一个)</Label>
              <TextArea
                value={Object.entries(config.headers || {}).map(([k, v]) => `${k}: ${v}`).join("\n")}
                onChange={(e) => {
                  const headers: Record<string, string> = {};
                  e.target.value.split("\n").forEach((line) => {
                    const idx = line.indexOf(":");
                    if (idx > 0) headers[line.slice(0, idx).trim()] = line.slice(idx + 1).trim();
                  });
                  onChange({ headers });
                }}
                onBlur={onCommit}
                rows={2}
                className="font-mono !text-[12px]"
                placeholder="Authorization: Bearer xxx"
              />
            </>
          )}

          {status?.connected && status.tool_names.length > 0 && (
            <div className="mt-2">
              <Label>已注册工具 ({status.tool_count})</Label>
              <div className="flex flex-wrap gap-1">
                {status.tool_names.map((t) => (
                  <Badge key={t} color="sky" className="font-mono">{t}</Badge>
                ))}
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
