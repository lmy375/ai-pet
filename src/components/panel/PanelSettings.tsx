import { useState, useEffect, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { AppSettings, AgentConfig, McpServerConfig, McpStatus, TelegramStatus } from "../../hooks/useSettings";
import { defaultAgent } from "../../hooks/useSettings";
import { Card } from "../ui/Card";
import { Button } from "../ui/Button";
import { Badge } from "../ui/Badge";
import { Label, TextInput, TextArea, Select } from "../ui/fields";
import { StatusText } from "../ui/StatusText";
import { ChevronDown, ChevronRight, PlusIcon, TrashIcon, ImageIcon, ExternalLinkIcon } from "../Icons";
import { AgentMemory } from "./PanelMemory";
import { open } from "@tauri-apps/plugin-dialog";
import { toneText, toneDot, connTone } from "../../utils/tone";
import { useI18n } from "../../i18n";

// Common model context windows, offered as one-tap presets next to the free
// numeric input (gpt-4o ~128K, Claude ~200K, Gemini ~1M).
const CONTEXT_PRESETS: { label: string; value: number }[] = [
  { label: "32K", value: 32000 },
  { label: "128K", value: 128000 },
  { label: "200K", value: 200000 },
  { label: "1M", value: 1000000 },
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

const blankSettings: AppSettings = {
  live_2d_model_path: "",
  language: "zh",
  gallery_dir: "",
  gallery_enabled: false,
  gallery_interval: 10,
  search_api_key: "",
  active_agent: "default",
  agents: [defaultAgent()],
};

export function PanelSettings() {
  const { t } = useI18n();
  const [form, setForm] = useState<AppSettings>(blankSettings);
  // Top-level tab: "raw" (config file), "global", or an agent id.
  const [tab, setTab] = useState<string>("global");
  const [loaded, setLoaded] = useState(false);
  const [testing, setTesting] = useState(false);
  // Status line under the form. `ok` drives the color — derived from the action,
  // not by sniffing the message text (which breaks once it's translated).
  const [message, setMessage] = useState<{ text: string; ok: boolean } | null>(null);
  const ok = (text: string) => setMessage({ text, ok: true });
  const fail = (text: string) => setMessage({ text, ok: false });
  const [mcpStatuses, setMcpStatuses] = useState<McpStatus[]>([]);
  const [reconnecting, setReconnecting] = useState(false);
  const [newServerName, setNewServerName] = useState("");
  const [telegramStatus, setTelegramStatus] = useState<TelegramStatus>({ running: false, error: null });
  const [telegramReconnecting, setTelegramReconnecting] = useState(false);
  const [rawYaml, setRawYaml] = useState("");
  const [models, setModels] = useState<string[]>([]);
  const [loadingModels, setLoadingModels] = useState(false);
  const [testResult, setTestResult] = useState<{ ok: boolean; text: string } | null>(null);

  // The agent shown in the active agent tab (falls back to the first agent).
  const isAgentTab = tab !== "raw" && tab !== "global";
  const editingAgentId = isAgentTab ? tab : (form.agents[0]?.id ?? "default");
  const agentIdx = Math.max(0, form.agents.findIndex((a) => a.id === editingAgentId));
  const agent = form.agents[agentIdx] ?? form.agents[0];

  useEffect(() => {
    invoke<AppSettings>("get_settings")
      .then((s) => {
        setForm(s);
        setLoaded(true);
      })
      .catch((e) => {
        console.error("Failed to load settings:", e);
        setLoaded(true);
      });
  }, []);

  // Switch the top-level tab. Loads the raw YAML when entering "config file", and
  // reloads settings from disk when leaving it (raw edits may have changed them).
  const selectTab = async (next: string) => {
    if (next === tab) return;
    setMessage(null);
    setTestResult(null);
    if (next === "raw") {
      try {
        setRawYaml(await invoke<string>("get_config_raw"));
        setTab("raw");
      } catch (e: any) {
        fail(t("settings.rawLoadFailed", { error: e }));
      }
      return;
    }
    let s = form;
    if (tab === "raw") {
      try { s = await invoke<AppSettings>("get_settings"); setForm(s); } catch {}
    }
    setTab(next);
    if (next !== "global") refreshAgentStatuses(next, s);
  };

  // Fetch the editing agent's MCP/Telegram status and model list. Called on load
  // and whenever the edited agent changes.
  const refreshAgentStatuses = (agentId: string, s: AppSettings) => {
    const a = s.agents.find((x) => x.id === agentId);
    invoke<McpStatus[]>("get_mcp_status", { agentId }).then(setMcpStatuses).catch(() => setMcpStatuses([]));
    invoke<TelegramStatus>("get_telegram_status", { agentId })
      .then(setTelegramStatus)
      .catch(() => setTelegramStatus({ running: false, error: null }));
    if (a?.api_base?.trim()) loadModels(a.api_base, a.api_key, true);
    else setModels([]);
  };

  // Auto-save current form settings (on blur / Enter). `next` lets callers persist
  // an updated value immediately without waiting for a state flush.
  const saveSettings = async (next?: AppSettings) => {
    try {
      await invoke("save_settings", { settings: next ?? form });
      ok(t("common.saved"));
    } catch (e: any) {
      fail(t("common.saveFailed", { error: e }));
    }
  };

  /* ---------- Editing-agent helpers ---------- */

  // Update the edited agent in-memory only (used while typing); persisted on blur.
  const updateAgent = (updates: Partial<AgentConfig>) => {
    setForm((prev) => {
      const agents = [...prev.agents];
      agents[agentIdx] = { ...agents[agentIdx], ...updates };
      return { ...prev, agents };
    });
  };

  // Update the edited agent and persist immediately (for discrete controls).
  const commitAgent = (updates: Partial<AgentConfig>) => {
    const agents = [...form.agents];
    agents[agentIdx] = { ...agents[agentIdx], ...updates };
    const next = { ...form, agents };
    setForm(next);
    saveSettings(next);
  };

  const addAgent = () => {
    const id = crypto.randomUUID();
    const next = {
      ...form,
      agents: [...form.agents, defaultAgent(id, t("settings.agent.newName"))],
    };
    setForm(next);
    setTab(id);
    setMcpStatuses([]);
    setTelegramStatus({ running: false, error: null });
    setModels([]);
    setMessage(null);
    setTestResult(null);
    saveSettings(next);
  };

  const removeAgent = (id: string) => {
    if (form.agents.length <= 1) return;
    const agents = form.agents.filter((a) => a.id !== id);
    const active_agent = form.active_agent === id ? agents[0].id : form.active_agent;
    const next = { ...form, agents, active_agent };
    setForm(next);
    if (tab === id) setTab("global");
    saveSettings(next);
  };

  const setActiveAgent = (id: string) => {
    const next = { ...form, active_agent: id };
    setForm(next);
    saveSettings(next);
  };

  /* ---------- MCP helpers (scoped to the edited agent) ---------- */

  const handleReconnectMcp = async () => {
    setReconnecting(true);
    setMessage(null);
    try {
      await invoke("save_settings", { settings: form });
      const statuses = await invoke<McpStatus[]>("reconnect_mcp", { agentId: agent.id });
      setMcpStatuses(statuses);
      const connected = statuses.filter((s) => s.connected).length;
      const total = statuses.length;
      ok(t("settings.mcp.reconnected", { connected, total }));
    } catch (e: any) {
      fail(t("settings.mcp.reconnectFailed", { error: e }));
    } finally {
      setReconnecting(false);
    }
  };

  const updateMcpServer = (name: string, updates: Partial<McpServerConfig>) => {
    updateAgent({ mcp_servers: { ...agent.mcp_servers, [name]: { ...agent.mcp_servers[name], ...updates } } });
  };

  const commitMcpServer = (name: string, updates: Partial<McpServerConfig>) => {
    commitAgent({ mcp_servers: { ...agent.mcp_servers, [name]: { ...agent.mcp_servers[name], ...updates } } });
  };

  const removeMcpServer = (name: string) => {
    const { [name]: _, ...rest } = agent.mcp_servers;
    commitAgent({ mcp_servers: rest });
  };

  const addMcpServer = () => {
    const name = newServerName.trim();
    if (!name || agent.mcp_servers[name]) return;
    commitAgent({ mcp_servers: { ...agent.mcp_servers, [name]: emptyMcpServer() } });
    setNewServerName("");
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
        ok(list.length === 0 ? t("settings.llm.modelsNone") : t("settings.llm.modelsLoaded", { count: list.length }));
      }
    } catch (e: any) {
      setModels([]);
      if (!silent) fail(t("settings.llm.modelsFailed", { error: e }));
    } finally {
      setLoadingModels(false);
    }
  };

  const handleTestModel = async () => {
    setTesting(true);
    setTestResult(null);
    try {
      await invoke("test_model", { apiBase: agent.api_base, apiKey: agent.api_key, model: agent.model });
      setTestResult({ ok: true, text: t("settings.llm.testOk") });
    } catch (e: any) {
      setTestResult({ ok: false, text: t("settings.llm.testFailed", { error: e }) });
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
      fail(t("settings.pickDirFailed", { error: e }));
    }
  };

  const handleOpenGalleryDir = async () => {
    if (!form.gallery_dir) return;
    try {
      await invoke("open_path", { path: form.gallery_dir });
    } catch (e: any) {
      fail(t("settings.openGalleryDirFailed", { error: e }));
    }
  };

  const handleOpenConfigDir = async () => {
    try {
      await invoke("open_config_dir");
    } catch (e: any) {
      fail(t("settings.openConfigDirFailed", { error: e }));
    }
  };

  const saveRaw = async () => {
    try {
      await invoke("save_config_raw", { content: rawYaml });
      ok(t("common.saved"));
    } catch (e: any) {
      fail(t("common.saveFailed", { error: e }));
    }
  };

  if (!loaded) {
    return <div className="flex h-full items-center justify-center text-[14px] text-slate-400">{t("common.loading")}</div>;
  }

  const serverEntries = Object.entries(agent.mcp_servers);
  const connectedCount = mcpStatuses.filter((s) => s.connected).length;
  const totalToolCount = mcpStatuses.reduce((sum, s) => sum + s.tool_count, 0);

  const messageLine = message && (
    <StatusText ok={message.ok} className="mt-1 text-[13px]">{message.text}</StatusText>
  );

  const setLanguage = (language: string) => {
    const next = { ...form, language };
    setForm(next);
    saveSettings(next);
  };

  return (
    <div className="flex h-full flex-col">
      {/* Tab bar: config file | global | per-agent... | + add | open folder */}
      <div className="flex shrink-0 items-center gap-1 overflow-x-auto border-b border-slate-200/70 bg-white/80 px-3 py-2 backdrop-blur">
        <TabBtn active={tab === "raw"} onClick={() => selectTab("raw")}>{t("settings.tab.file")}</TabBtn>
        <TabBtn active={tab === "global"} onClick={() => selectTab("global")}>{t("settings.tab.global")}</TabBtn>
        {form.agents.map((a) => (
          <TabBtn key={a.id} active={tab === a.id} onClick={() => selectTab(a.id)} dot={a.id === form.active_agent}>
            {a.name}
          </TabBtn>
        ))}
        <button
          onClick={addAgent}
          title={t("settings.agent.add")}
          className="flex shrink-0 items-center gap-1 rounded-lg px-2.5 py-1.5 text-[13px] font-medium text-accent transition-colors hover:bg-accent/10"
        >
          <PlusIcon className="h-4 w-4" />
          {t("settings.agent.add")}
        </button>
        <Button variant="ghost" size="sm" className="ml-auto shrink-0" onClick={handleOpenConfigDir} title={t("settings.openConfigDirTitle")}>
          {t("settings.openConfigDir")}
        </Button>
      </div>

      <div className="flex-1 overflow-y-auto px-5 py-5">
      {tab === "raw" ? (
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
        </>
      ) : tab === "global" ? (
        <>
          {/* Language */}
          <Card title={t("settings.language")}>
            <Select value={form.language === "en" ? "en" : "zh"} onChange={(e) => setLanguage(e.target.value)}>
              <option value="zh">中文</option>
              <option value="en">English</option>
            </Select>
          </Card>

          {/* Default agent */}
          <Card title={t("settings.agent.defaultTitle")}>
            <Select value={form.active_agent} onChange={(e) => setActiveAgent(e.target.value)}>
              {form.agents.map((a) => (
                <option key={a.id} value={a.id}>{a.name}</option>
              ))}
            </Select>
            <p className="mt-1 text-[11px] text-slate-400">{t("settings.agent.defaultNote")}</p>
          </Card>

          {/* Live2D */}
          <Card title={t("settings.live2d.title")}>
            <Label>{t("settings.live2d.path")}</Label>
            <TextInput
              value={form.live_2d_model_path}
              onChange={(e) => setForm({ ...form, live_2d_model_path: e.target.value })}
              onBlur={() => saveSettings()}
              onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
              placeholder="/models/miku/miku.model3.json"
            />
          </Card>

          {/* Gallery slideshow */}
          <Card title={t("settings.gallery.title")}>
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
              {t("settings.gallery.enable")}
            </label>

            <Label>{t("settings.gallery.dir")}</Label>
            <div className="flex gap-2">
              <TextInput value={form.gallery_dir} readOnly className="flex-1" placeholder={t("settings.gallery.noDir")} />
              <Button variant="secondary" onClick={handlePickGalleryDir}>
                <ImageIcon className="h-4 w-4" />
                {t("settings.gallery.pick")}
              </Button>
              <Button variant="secondary" onClick={handleOpenGalleryDir} disabled={!form.gallery_dir} title={t("settings.gallery.openDirTitle")}>
                <ExternalLinkIcon className="h-4 w-4" />
                {t("common.open")}
              </Button>
            </div>

            <Label className="mt-3">{t("settings.gallery.interval")}</Label>
            <TextInput
              type="number"
              min={1}
              value={form.gallery_interval}
              onChange={(e) => setForm({ ...form, gallery_interval: Number(e.target.value) || 0 })}
              onBlur={() => {
                const next = { ...form, gallery_interval: Math.max(1, form.gallery_interval || 10) };
                setForm(next);
                saveSettings(next);
              }}
              onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
              placeholder="10"
            />
            <p className="mt-1 text-[11px] text-slate-400">{t("settings.gallery.intervalNote")}</p>
          </Card>

          {/* Web Search (shared by all agents) */}
          <Card title={t("settings.search.title")}>
            <Label>{t("settings.search.apiKey")}</Label>
            <TextInput
              type="password"
              value={form.search_api_key}
              onChange={(e) => setForm({ ...form, search_api_key: e.target.value })}
              onBlur={() => saveSettings()}
              onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
              placeholder="tvly-..."
            />
            <p className="mt-1 text-[11px] text-slate-400">{t("settings.search.apiKeyNote")}</p>
          </Card>

        </>
      ) : (
        <>
          {/* Agent identity: name + delete. Selecting/adding agents is done via
              the tab bar; the default agent is chosen in the Global tab. */}
          <Card title={agent.name || t("settings.agent.newName")}>
            <Label>{t("settings.agent.name")}</Label>
            <div className="flex gap-2">
              <TextInput
                value={agent.name}
                onChange={(e) => updateAgent({ name: e.target.value })}
                onBlur={() => saveSettings()}
                onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
                className="flex-1"
                placeholder={t("settings.agent.newName")}
              />
              <Button
                variant="secondary"
                onClick={() => removeAgent(agent.id)}
                disabled={form.agents.length <= 1}
                title={t("settings.agent.remove")}
              >
                <TrashIcon className="h-4 w-4" />
                {t("settings.agent.remove")}
              </Button>
            </div>
            <div className="mt-1 flex items-center justify-between gap-2">
              <p className="text-[11px] text-slate-400">{t("settings.agent.idNote", { id: agent.id })}</p>
              {agent.id === form.active_agent ? (
                <span className="shrink-0 text-[11px] font-medium text-accent">{t("settings.agent.isDefault")}</span>
              ) : (
                <button onClick={() => setActiveAgent(agent.id)} className="shrink-0 text-[11px] font-medium text-accent hover:underline">
                  {t("settings.agent.setDefault")}
                </button>
              )}
            </div>
          </Card>

          {/* LLM Config */}
          <Card title={t("settings.llm.title")}>
            <Label>API Base URL</Label>
            <TextInput
              value={agent.api_base}
              onChange={(e) => updateAgent({ api_base: e.target.value })}
              onBlur={() => { saveSettings(); loadModels(agent.api_base, agent.api_key, true); }}
              onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
              placeholder="https://api.openai.com/v1"
            />
            <Label className="mt-3">API Key</Label>
            <TextInput
              type="password"
              value={agent.api_key}
              onChange={(e) => updateAgent({ api_key: e.target.value })}
              onBlur={() => { saveSettings(); loadModels(agent.api_base, agent.api_key, true); }}
              onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
              placeholder="sk-..."
            />
            <Label className="mt-3 flex items-center gap-2">
              <span>Model</span>
              {loadingModels && <span className="font-normal text-slate-400">{t("common.loading")}</span>}
            </Label>
            <div className="flex gap-2">
              <Select
                value={models.includes(agent.model) ? agent.model : ""}
                onChange={(e) => { commitAgent({ model: e.target.value }); setTestResult(null); }}
                disabled={models.length === 0}
                className="flex-1"
              >
                {models.length === 0 ? (
                  <option value="">{agent.api_base.trim() ? t("settings.llm.noModelsHint") : t("settings.llm.fillBaseFirst")}</option>
                ) : (
                  <>
                    <option value="" disabled>{t("settings.llm.selectFromN", { count: models.length })}</option>
                    {models.map((m) => (
                      <option key={m} value={m}>{m}</option>
                    ))}
                  </>
                )}
              </Select>
              <Button onClick={handleTestModel} disabled={testing || !agent.model.trim()}>
                {testing ? t("settings.llm.testing") : t("settings.llm.test")}
              </Button>
            </div>
            {testResult && (
              <StatusText ok={testResult.ok} className="mt-1.5 text-[12px]">{testResult.text}</StatusText>
            )}

            <Label className="mt-3">{t("settings.llm.contextWindow")}</Label>
            <div className="mb-2 flex gap-1.5">
              {CONTEXT_PRESETS.map((p) => {
                const active = agent.context_window === p.value;
                return (
                  <button
                    key={p.value}
                    type="button"
                    onClick={() => commitAgent({ context_window: p.value })}
                    className={`rounded-lg px-2.5 py-1 text-[12px] font-medium transition-colors ${
                      active ? "bg-accent text-white" : "bg-slate-100 text-slate-600 hover:bg-slate-200"
                    }`}
                  >
                    {p.label}
                  </button>
                );
              })}
            </div>
            <TextInput
              type="number"
              min={1}
              value={agent.context_window}
              onChange={(e) => updateAgent({ context_window: Number(e.target.value) || 0 })}
              onBlur={() => commitAgent({ context_window: Math.max(1, agent.context_window || 128000) })}
              onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
              placeholder="128000"
            />
            <p className="mt-1 text-[11px] text-slate-400">{t("settings.llm.contextWindowNote")}</p>
          </Card>

          {/* MCP Servers */}
          <Card
            title={
              <span>
                MCP Servers
                {serverEntries.length > 0 && (
                  <span className="ml-2 font-normal text-[12px] text-slate-500">
                    {t("settings.mcp.connectedTools", { connected: connectedCount, total: serverEntries.length, tools: totalToolCount })}
                  </span>
                )}
              </span>
            }
            action={
              <Button size="sm" onClick={handleReconnectMcp} disabled={reconnecting}>
                {reconnecting ? t("settings.connecting") : t("settings.saveConnect")}
              </Button>
            }
          >
            {serverEntries.length === 0 && (
              <div className="rounded-xl border border-dashed border-slate-200 py-4 text-center text-[13px] text-slate-400">
                {t("settings.mcp.empty")}
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
                placeholder={t("settings.mcp.newName")}
              />
              <Button variant="secondary" onClick={addMcpServer} disabled={!newServerName.trim() || !!agent.mcp_servers[newServerName.trim()]}>
                <PlusIcon className="h-4 w-4" />
                {t("common.add")}
              </Button>
            </div>
          </Card>

          {/* Telegram Bot */}
          <Card
            title={
              <span>
                Telegram Bot
                <span className={`ml-2 font-normal text-[11px] ${toneText(connTone(telegramStatus.running, telegramStatus.error))}`}>
                  {telegramStatus.running ? t("settings.tg.running") : telegramStatus.error ? t("settings.tg.connFailed") : t("settings.tg.stopped")}
                </span>
              </span>
            }
            action={
              <Button
                size="sm"
                disabled={telegramReconnecting}
                onClick={async () => {
                  setTelegramReconnecting(true);
                  setMessage(null);
                  try {
                    await invoke("save_settings", { settings: form });
                    await invoke("reconnect_telegram");
                    const status = await invoke<TelegramStatus>("get_telegram_status", { agentId: agent.id });
                    setTelegramStatus(status);
                    if (status.running) ok(t("settings.tg.connected"));
                    else ok(t("settings.tg.stoppedMsg"));
                  } catch (e: any) {
                    fail(t("settings.tg.opFailed", { error: e }));
                  } finally {
                    setTelegramReconnecting(false);
                  }
                }}
              >
                {telegramReconnecting ? t("settings.connecting") : t("settings.saveConnect")}
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
                checked={agent.telegram?.enabled ?? false}
                onChange={(e) => commitAgent({ telegram: { ...agent.telegram, enabled: e.target.checked } })}
              />
              {t("settings.tg.enable")}
            </label>

            <Label>Bot Token</Label>
            <TextInput
              type="password"
              value={agent.telegram?.bot_token ?? ""}
              onChange={(e) => updateAgent({ telegram: { ...agent.telegram, bot_token: e.target.value } })}
              onBlur={() => saveSettings()}
              onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
              className="mb-2 font-mono !text-[12px]"
              placeholder="123456789:ABCdefGhI..."
            />

            <Label>{t("settings.tg.allowedUser")}</Label>
            <TextInput
              value={agent.telegram?.allowed_username ?? ""}
              onChange={(e) => updateAgent({ telegram: { ...agent.telegram, allowed_username: e.target.value } })}
              onBlur={() => saveSettings()}
              onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
              className="font-mono !text-[12px]"
              placeholder={t("settings.tg.allowedUserPlaceholder")}
            />
          </Card>

          {/* Scheduled heartbeat */}
          <Card title={t("settings.hb.title")}>
            <label className="mb-3 flex items-center gap-1.5 text-[12px] font-medium text-slate-600">
              <input
                type="checkbox"
                className="accent-accent"
                checked={agent.heartbeat_enabled}
                onChange={(e) => commitAgent({ heartbeat_enabled: e.target.checked })}
              />
              {t("settings.hb.enable")}
            </label>

            <Label>{t("settings.hb.interval")}</Label>
            <TextInput
              type="number"
              min={1}
              value={agent.heartbeat_interval}
              onChange={(e) => updateAgent({ heartbeat_interval: Number(e.target.value) || 0 })}
              onBlur={() => commitAgent({ heartbeat_interval: Math.max(1, agent.heartbeat_interval || 60) })}
              onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
              placeholder="60"
            />
            <p className="mt-1 text-[11px] text-slate-400">{t("settings.hb.note")}</p>

            <Label className="mt-3">{t("settings.hb.contextTurns")}</Label>
            <TextInput
              type="number"
              min={0}
              value={agent.heartbeat_context_turns}
              onChange={(e) => updateAgent({ heartbeat_context_turns: Number(e.target.value) || 0 })}
              onBlur={() => commitAgent({ heartbeat_context_turns: Math.max(0, agent.heartbeat_context_turns || 0) })}
              onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
              placeholder="10"
            />
            <p className="mt-1 text-[11px] text-slate-400">{t("settings.hb.contextTurnsNote")}</p>
          </Card>

          {/* Per-agent memory */}
          <Card title={t("settings.agent.memoryTitle")}>
            <AgentMemory key={agent.id} agentId={agent.id} />
          </Card>
        </>
      )}
      {messageLine}
      </div>
    </div>
  );
}

/* ---------- Tab button ---------- */

function TabBtn({
  active,
  onClick,
  children,
  dot,
}: {
  active: boolean;
  onClick: () => void;
  children: ReactNode;
  dot?: boolean;
}) {
  return (
    <button
      onClick={onClick}
      className={`flex shrink-0 items-center gap-1.5 rounded-lg px-3 py-1.5 text-[13px] font-medium transition-colors ${
        active ? "bg-accent text-white" : "text-slate-600 hover:bg-slate-100"
      }`}
    >
      {dot && <span className={`h-1.5 w-1.5 rounded-full ${active ? "bg-white" : "bg-accent"}`} />}
      {children}
    </button>
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
  const { t } = useI18n();
  const [expanded, setExpanded] = useState(true);

  const hasError = !!status?.error && status.error !== "Disabled";
  const dotClass = toneDot(connTone(status?.connected, status?.error));
  const statusLabel = status?.connected
    ? t("settings.mcp.connected")
    : status?.error === "Disabled"
      ? t("settings.mcp.disabled")
      : status?.error
        ? t("settings.mcp.connFailed")
        : t("settings.mcp.disconnected");
  const statusColor = toneText(connTone(status?.connected, hasError));

  return (
    <div className={`rounded-xl border bg-slate-50 ${hasError ? "border-red-300" : "border-slate-200"}`}>
      {/* Header row */}
      <div className="flex cursor-pointer items-center gap-2 px-3 py-2.5" onClick={() => setExpanded(!expanded)}>
        <span className={`h-2 w-2 shrink-0 rounded-full ${dotClass}`} />
        <span className="flex-1 text-[13px] font-semibold text-slate-800">
          {name}
          <span className="ml-2 font-normal text-[11px] text-slate-400">{config.transport.toUpperCase()}</span>
          <span className={`ml-1.5 font-normal text-[11px] ${statusColor}`}>
            {statusLabel}
            {status?.connected && ` · ${t("settings.mcp.toolsSuffix", { count: status.tool_count })}`}
          </span>
        </span>

        <label className="flex items-center gap-1 text-[12px] text-slate-500" onClick={(e) => e.stopPropagation()}>
          <input
            type="checkbox"
            className="accent-accent"
            checked={config.enabled}
            onChange={(e) => onCommitChange({ enabled: e.target.checked })}
          />
          {t("common.enable")}
        </label>

        <button
          onClick={(e) => { e.stopPropagation(); onRemove(); }}
          title={t("common.delete")}
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

          <Label>{t("settings.mcp.transport")}</Label>
          <Select
            value={config.transport}
            onChange={(e) => onCommitChange({ transport: e.target.value as McpServerConfig["transport"] })}
            className="mb-2"
          >
            <option value="stdio">{t("settings.mcp.transport.stdio")}</option>
            <option value="sse">{t("settings.mcp.transport.sse")}</option>
            <option value="http">{t("settings.mcp.transport.http")}</option>
          </Select>

          {config.transport === "stdio" ? (
            <>
              <Label>{t("settings.mcp.command")}</Label>
              <TextInput
                value={config.command}
                onChange={(e) => onChange({ command: e.target.value })}
                onBlur={onCommit}
                onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
                className="mb-1.5 font-mono !text-[12px]"
                placeholder="npx"
              />
              <Label>{t("settings.mcp.args")}</Label>
              <TextArea
                value={config.args.join("\n")}
                onChange={(e) => onChange({ args: e.target.value.split("\n") })}
                onBlur={onCommit}
                rows={3}
                className="mb-1.5 font-mono !text-[12px]"
                placeholder={"-y\n@modelcontextprotocol/server-filesystem\n/tmp"}
              />
              <Label>{t("settings.mcp.env")}</Label>
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
              <Label>{t("settings.mcp.headers")}</Label>
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
              <Label>{t("settings.mcp.registeredTools", { count: status.tool_count })}</Label>
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
