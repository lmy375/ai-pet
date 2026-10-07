import { useState, useEffect, type ComponentType, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { AppSettings, AgentConfig, McpStatus, TelegramStatus, SkillsInfo } from "../../hooks/useSettings";
import { EMPTY_SETTINGS, newAgent } from "../../hooks/useSettings";
import { Button } from "../ui/Button";
import { ErrorBox, LoadingScreen, HintText } from "../ui/feedback";
import { TextInput, TextArea, Select, SavedTextInput, NumberField } from "../ui/fields";
import { StatusText } from "../ui/StatusText";
import { SettingsSection, SettingsRow, Switch } from "../ui/settings";
import {
  PlusIcon,
  TrashIcon,
  ImageIcon,
  ExternalLinkIcon,
  DownloadIcon,
  SpinnerIcon,
  GearIcon,
  WrenchIcon,
  FileTextIcon,
  TerminalIcon,
  GlobeIcon,
  SearchIcon,
  AgentIcon,

} from "../Icons";
import { AgentMemory } from "./PanelMemory";
import { ModelsCard } from "./settings/ModelsCard";
import { PromptsCard } from "./settings/PromptsCard";
import { ToolsCard } from "./settings/ToolsCard";
import { McpCard } from "./settings/McpCard";
import { open } from "@tauri-apps/plugin-dialog";
import { toneText, toneDot, connTone } from "../../utils/tone";
import { useI18n, type TKey } from "../../i18n";

/** Agent tabs are keyed by id so every agent gets its own rail entry. */
const AGENT_PREFIX = "agent:";

/**
 * The left rail's tabs. Two top-level groups (Pet / AI); the agent group is
 * built from the agent list (one tab per agent); the general group (language,
 * raw config) follows the others.
 */
type TabId =
  | "pet-visual"
  | "ai-models"
  | "ai-mcp"
  | "ai-prompts"
  | "ai-tools"
  | "ai-search"
  | "ai-skills"
  | "general-language"
  | "raw"
  | `agent:${string}`;

interface NavEntry {
  id: TabId;
  label: TKey;
  icon: ComponentType<{ className?: string }>;
}

const NAV_GROUPS: { title: TKey; items: NavEntry[] }[] = [
  {
    title: "settings.group.pet",
    items: [{ id: "pet-visual", label: "settings.pet.title", icon: ImageIcon }],
  },
  {
    title: "settings.group.ai",
    items: [
      { id: "ai-models", label: "settings.models.title", icon: GearIcon },
      { id: "ai-mcp", label: "settings.mcp.title", icon: WrenchIcon },
      { id: "ai-prompts", label: "settings.prompts.title", icon: FileTextIcon },
      { id: "ai-tools", label: "settings.tools.title", icon: TerminalIcon },
      { id: "ai-search", label: "settings.search.title", icon: GlobeIcon },
      { id: "ai-skills", label: "settings.skills.title", icon: SearchIcon },
    ],
  },
];

/** General settings (language, raw config), rendered after the agent group. */
const GENERAL_ITEMS: NavEntry[] = [
  { id: "general-language", label: "settings.language", icon: GlobeIcon },
  { id: "raw", label: "settings.tab.file", icon: FileTextIcon },
];

export function PanelSettings() {
  const { t } = useI18n();
  const [form, setForm] = useState<AppSettings>(EMPTY_SETTINGS);
  const [tab, setTab] = useState<TabId>("ai-models");
  // Which agent the agent tabs edit (derived from the active agent tab).
  const [agentTab, setAgentTab] = useState<string>("default");
  const [loaded, setLoaded] = useState(false);
  // Status line under the form. `ok` drives the color — derived from the action,
  // not by sniffing the message text (which breaks once it's translated).
  const [message, setMessage] = useState<{ text: string; ok: boolean } | null>(null);
  const ok = (text: string) => setMessage({ text, ok: true });
  const fail = (text: string) => setMessage({ text, ok: false });
  const [mcpStatuses, setMcpStatuses] = useState<McpStatus[]>([]);
  // One busy flag for the MCP card: both the reconnect button and the per-server
  // switch talk to the same connection pool.
  const [mcpBusy, setMcpBusy] = useState(false);
  const [telegramStatus, setTelegramStatus] = useState<TelegramStatus>({ running: false, error: null });
  const [telegramReconnecting, setTelegramReconnecting] = useState(false);
  const [rawYaml, setRawYaml] = useState("");
  const [skillsInfo, setSkillsInfo] = useState<SkillsInfo | null>(null);
  // "Use sample model" download in flight (one button, so one flag).
  const [exampleBusy, setExampleBusy] = useState(false);

  // The agent shown across the agent group (falls back to the first agent, e.g.
  // after the edited one is deleted).
  const agentIdx = Math.max(0, form.agents.findIndex((a) => a.id === agentTab));
  const agent = form.agents[agentIdx] ?? form.agents[0];

  // Re-scan the skills dir. Cheap (one read_dir), so it runs on load, after the
  // directory changes, and on the card's refresh button.
  const loadSkills = () => {
    invoke<SkillsInfo>("list_skills").then(setSkillsInfo).catch(() => setSkillsInfo(null));
  };

  // MCP connections are global (one process per server, shared by every agent
  // that lists it), so there is one status list for the whole settings page.
  const loadMcpStatuses = () => {
    invoke<McpStatus[]>("get_mcp_status").then(setMcpStatuses).catch(() => setMcpStatuses([]));
  };

  useEffect(() => {
    invoke<AppSettings>("get_settings")
      .then((s) => {
        setForm(s);
        setAgentTab(s.agents[0]?.id ?? "default");
        setLoaded(true);
      })
      .catch((e) => {
        console.error("Failed to load settings:", e);
        setLoaded(true);
      });
    loadSkills();
    loadMcpStatuses();
  }, []);

  // Switch the left-rail tab. Loads the raw YAML when entering "config file",
  // and reloads settings from disk when leaving it (raw edits may have changed
  // them).
  const selectTab = async (next: TabId) => {
    if (next === tab) return;
    setMessage(null);
    if (next === "raw") {
      try {
        setRawYaml(await invoke<string>("get_config_raw"));
        setTab("raw");
      } catch (e: any) {
        fail(t("settings.rawLoadFailed", { error: e }));
      }
      return;
    }
    if (tab === "raw") {
      try { setForm(await invoke<AppSettings>("get_settings")); } catch {}
    }
    setTab(next);
    if (next.startsWith(AGENT_PREFIX)) {
      const id = next.slice(AGENT_PREFIX.length);
      setAgentTab(id);
      loadTelegramStatus(id);
    }
  };

  /** Jump from an agent's reference control to the AI pool it points at. */
  const goToPool = (id: "ai-models" | "ai-mcp") => {
    setMessage(null);
    setTab(id);
  };

  const loadTelegramStatus = (agentId: string) => {
    invoke<TelegramStatus>("get_telegram_status", { agentId })
      .then(setTelegramStatus)
      .catch(() => setTelegramStatus({ running: false, error: null }));
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

  /** Persist a settings object produced by one of the pool cards. */
  const commitSettings = (next: AppSettings) => {
    setForm(next);
    saveSettings(next);
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
    commitSettings({ ...form, agents });
  };

  const addAgent = async () => {
    const id = crypto.randomUUID();
    const next = {
      ...form,
      agents: [...form.agents, await newAgent(id, t("settings.agent.newName"))],
    };
    setForm(next);
    setTab(`${AGENT_PREFIX}${id}`);
    setAgentTab(id);
    setTelegramStatus({ running: false, error: null });
    setMessage(null);
    saveSettings(next);
  };

  const removeAgent = (id: string) => {
    if (form.agents.length <= 1) return;
    const agents = form.agents.filter((a) => a.id !== id);
    const active_agent = form.active_agent === id ? agents[0].id : form.active_agent;
    const next = { ...form, agents, active_agent };
    setForm(next);
    if (agentTab === id) {
      setAgentTab(agents[0].id);
      setTab(`${AGENT_PREFIX}${agents[0].id}`);
    }
    saveSettings(next);
  };

  const setActiveAgent = (id: string) => {
    commitSettings({ ...form, active_agent: id });
  };

  /** Save first, then reconnect every referenced server (connections are global). */
  const handleReconnectMcp = async () => {
    setMcpBusy(true);
    setMessage(null);
    try {
      await invoke("save_settings", { settings: form });
      const statuses = await invoke<McpStatus[]>("reconnect_mcp");
      setMcpStatuses(statuses);
      const connected = statuses.filter((s) => s.connected).length;
      ok(t("settings.mcp.reconnected", { connected, total: statuses.length }));
    } catch (e: any) {
      fail(t("settings.mcp.reconnectFailed", { error: e }));
    } finally {
      setMcpBusy(false);
    }
  };

  /** Flip one server's switch and apply it to the running pool immediately:
   *  persist the flag, then let the backend start or stop just that server. */
  const toggleMcpServer = async (name: string, enabled: boolean) => {
    const next = {
      ...form,
      mcp_servers: { ...form.mcp_servers, [name]: { ...form.mcp_servers[name], enabled } },
    };
    setForm(next);
    setMcpBusy(true);
    setMessage(null);
    try {
      await invoke("save_settings", { settings: next });
      setMcpStatuses(await invoke<McpStatus[]>("sync_mcp_server", { name }));
      ok(enabled ? t("settings.mcp.started", { name }) : t("settings.mcp.stopped", { name }));
    } catch (e: any) {
      fail(t("settings.mcp.reconnectFailed", { error: e }));
    } finally {
      setMcpBusy(false);
    }
  };

  /** Pick a directory via the native dialog, defaulting to `current` (or the
   *  OS Pictures folder), then persist immediately. */
  const pickDirectory = async (current: string, onPicked: (path: string) => void) => {
    try {
      const defaultPath = current || (await invoke<string | null>("default_gallery_dir").catch(() => null)) || undefined;
      const picked = await open({ directory: true, multiple: false, defaultPath });
      if (typeof picked === "string") onPicked(picked);
    } catch (e: any) {
      fail(t("settings.pickDirFailed", { error: e }));
    }
  };

  const handleOpenPath = async (path: string) => {
    if (!path) return;
    try {
      await invoke("open_path", { path });
    } catch (e: any) {
      fail(t("settings.openGalleryDirFailed", { error: e }));
    }
  };

  /** Pick the image-pet directory and validate it before committing: every
   *  emotion (idle / thinking / happy) must have an image file, otherwise the
   *  choice is rejected — no silent fallback to the built-in art. */
  const handlePickPetImageDir = async () => {
    try {
      const defaultPath =
        form.pet_image_dir ||
        (await invoke<string | null>("default_gallery_dir").catch(() => null)) ||
        undefined;
      const picked = await open({ directory: true, multiple: false, defaultPath });
      if (typeof picked !== "string") return;

      const items = await invoke<{ path: string; kind: string }[]>("list_gallery_media", { dir: picked });
      const stems = new Set(
        items
          .filter((i) => i.kind === "image")
          .map((i) => i.path.split(/[\\/]/).pop()!.replace(/\.[^.]+$/, "").toLowerCase()),
      );
      const missing = (["idle", "thinking", "happy"] as const).filter((e) => !stems.has(e));
      if (missing.length > 0) {
        fail(t("settings.pet.imageIncomplete", { missing: missing.join(", ") }));
        return;
      }
      commitSettings({ ...form, pet_image_dir: picked });
    } catch (e: any) {
      fail(t("settings.pickDirFailed", { error: e }));
    }
  };

  // Persist a new skills dir and immediately re-scan it, so the list below the
  // input always reflects the directory shown in it.
  const commitSkillsDir = async (skills_dir: string) => {
    const next = { ...form, skills_dir };
    setForm(next);
    await saveSettings(next);
    loadSkills();
  };

  const handlePickSkillsDir = async () => {
    try {
      const picked = await open({
        directory: true,
        multiple: false,
        defaultPath: skillsInfo?.dir || undefined,
      });
      if (typeof picked === "string") commitSkillsDir(picked);
    } catch (e: any) {
      fail(t("settings.pickDirFailed", { error: e }));
    }
  };

  const handleOpenConfigDir = async () => {
    try {
      await invoke("open_config_dir");
    } catch (e: any) {
      fail(t("settings.openConfigDirFailed", { error: e }));
    }
  };

  // Pick the Live2D core JS file (live2dcubismcore.min.js).
  // Empty = use the SDK bundled into the app.
  const handlePickLive2DCore = async () => {
    try {
      const defaultDir = await invoke<string>("default_live2d_dir").catch(() => null);
      const picked = await open({
        multiple: false,
        filters: [{ name: "JavaScript", extensions: ["js"] }],
        defaultPath: form.live_2d_core_path || defaultDir || undefined,
      });
      if (typeof picked === "string") commitSettings({ ...form, live_2d_core_path: picked });
    } catch (e: any) {
      fail(t("settings.pickDirFailed", { error: e }));
    }
  };

  // Pick the Live2D model file (.model3.json).
  const handlePickLive2DModel = async () => {
    try {
      const defaultDir = await invoke<string>("default_live2d_dir").catch(() => null);
      const picked = await open({
        multiple: false,
        filters: [{ name: "Live2D Model", extensions: ["json"] }],
        defaultPath: form.live_2d_model_path || defaultDir || undefined,
      });
      if (typeof picked === "string") commitSettings({ ...form, live_2d_model_path: picked });
    } catch (e: any) {
      fail(t("settings.pickDirFailed", { error: e }));
    }
  };

  // "Use sample model": download Live2D's official sample SDK + wanko model
  // (© Live2D Inc.) into <config>/live2d/ and wire both paths up in one go.
  // Rerunning re-downloads and overwrites, so it doubles as a repair.
  const handleDownloadExample = async () => {
    setExampleBusy(true);
    try {
      const paths = await invoke<{ core_path: string; model_path: string }>(
        "download_example_live2d",
      );
      commitSettings({
        ...form,
        live_2d_core_path: paths.core_path,
        live_2d_model_path: paths.model_path,
      });
      ok(t("settings.live2d.exampleDone"));
    } catch (e: any) {
      fail(t("settings.live2d.exampleFailed", { error: e }));
    } finally {
      setExampleBusy(false);
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
    return <LoadingScreen />;
  }

  const messageLine = message && (
    <StatusText ok={message.ok} className="mt-3 text-note">{message.text}</StatusText>
  );

  const setLanguage = (language: string) => commitSettings({ ...form, language });

  const modelNames = Object.keys(form.models);
  const serverNames = Object.keys(form.mcp_servers);
  // A reference that no longer resolves: show it (so it can be seen and fixed)
  // rather than silently selecting something else.
  const danglingModel = !!agent?.model && !form.models[agent.model];

  // Prerequisites for the two path-backed pet kinds. The pet-kind picker
  // disables those options until met, so the choice can never strand the pet
  // window on a kind with nothing to render.
  const live2dReady = !!form.live_2d_model_path.trim() && !!form.live_2d_core_path.trim();
  const galleryReady = !!form.gallery_dir.trim();

  const toggleAgentServer = (name: string, on: boolean) => {
    const mcp = on ? [...agent.mcp, name] : agent.mcp.filter((m) => m !== name);
    commitAgent({ mcp });
  };

  /* ---------- Left rail / page-title helpers ---------- */

  const isAgentTab = tab.startsWith(AGENT_PREFIX);
  const activeAgent = isAgentTab ? form.agents.find((a) => a.id === tab.slice(AGENT_PREFIX.length)) : undefined;
  const allEntries = [...NAV_GROUPS.flatMap((g) => g.items), ...GENERAL_ITEMS];
  const pageTitle = isAgentTab
    ? activeAgent?.name || t("settings.agent.newName")
    : t(allEntries.find((e) => e.id === tab)?.label ?? "settings.tab.file");
  const currentGroupKey: TKey = isAgentTab
    ? "settings.group.agents"
    : tab.startsWith("ai-")
      ? "settings.group.ai"
      : tab.startsWith("pet-")
        ? "settings.group.pet"
        : "settings.group.general";

  /* ---------- Per-tab content ---------- */

  const renderAgentPage = () => (
    <>
      {/* Identity */}
      <SettingsSection title={t("settings.agent.identityTitle")}>
        <SettingsRow label={t("settings.agent.name")}>
          <div className="flex gap-2">
            <SavedTextInput
              value={agent.name}
              onChange={(e) => updateAgent({ name: e.target.value })}
              onCommit={() => saveSettings()}
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
        </SettingsRow>
        <SettingsRow
          label={<span className="font-normal text-ink-faint">{t("settings.agent.idNote", { id: agent.id })}</span>}
          control={
            agent.id === form.active_agent ? (
              <span className="text-note font-medium text-accent">{t("settings.agent.isDefault")}</span>
            ) : (
              <button onClick={() => setActiveAgent(agent.id)} className="text-note font-medium text-accent hover:underline">
                {t("settings.agent.setDefault")}
              </button>
            )
          }
        />
      </SettingsSection>

      {/* Model */}
      <SettingsSection
        title={t("settings.agent.modelTitle")}
        action={
          <button onClick={() => goToPool("ai-models")} className="text-note font-medium text-accent hover:underline">
            {t("settings.agent.goConfigure")}
          </button>
        }
      >
        {modelNames.length === 0 ? (
          <SettingsRow><HintText className="mt-0">{t("settings.agent.modelEmpty")}</HintText></SettingsRow>
        ) : (
          <SettingsRow
            label={t("settings.agent.modelTitle")}
            description={danglingModel ? undefined : t("settings.agent.modelNote")}
            control={
              <Select className="w-56" value={agent.model} onChange={(e) => commitAgent({ model: e.target.value })}>
                <option value="">{t("settings.agent.modelNone")}</option>
                {danglingModel && <option value={agent.model}>{agent.model}</option>}
                {modelNames.map((name) => (
                  <option key={name} value={name}>{name}</option>
                ))}
              </Select>
            }
          />
        )}
        {danglingModel && (
          <SettingsRow>
            <StatusText ok={false} className="text-note">{t("settings.agent.modelMissing", { model: agent.model })}</StatusText>
          </SettingsRow>
        )}
      </SettingsSection>

      {/* MCP */}
      <SettingsSection
        title={t("settings.agent.mcpTitle")}
        description={t("settings.agent.mcpNote")}
        action={
          <button onClick={() => goToPool("ai-mcp")} className="text-note font-medium text-accent hover:underline">
            {t("settings.agent.goConfigure")}
          </button>
        }
      >
        {serverNames.length === 0 ? (
          <SettingsRow><HintText className="mt-0">{t("settings.agent.mcpEmpty")}</HintText></SettingsRow>
        ) : (
          serverNames.map((name) => {
            const status = mcpStatuses.find((s) => s.name === name);
            const off = !form.mcp_servers[name].enabled;
            return (
              <SettingsRow
                key={name}
                label={
                  <span className="flex items-center gap-2">
                    <span className={`h-1.5 w-1.5 shrink-0 rounded-full ${toneDot(connTone(status?.connected, status?.error))}`} />
                    {name}
                  </span>
                }
                description={
                  off
                    ? t("settings.mcp.disabled")
                    : status?.connected
                      ? t("settings.mcp.toolsSuffix", { count: status.tool_count })
                      : undefined
                }
                control={
                  <Switch checked={agent.mcp.includes(name)} onChange={(on) => toggleAgentServer(name, on)} />
                }
              />
            );
          })
        )}
      </SettingsSection>

      {/* Telegram */}
      <SettingsSection
        title={t("settings.agent.tgTitle")}
        description={
          <span className={toneText(connTone(telegramStatus.running, telegramStatus.error))}>
            {telegramStatus.running ? t("settings.tg.running") : telegramStatus.error ? t("settings.tg.connFailed") : t("settings.tg.stopped")}
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
          <SettingsRow><ErrorBox className="mt-0">{telegramStatus.error}</ErrorBox></SettingsRow>
        )}
        <SettingsRow
          label={t("settings.tg.enable")}
          control={
            <Switch
              checked={agent.telegram?.enabled ?? false}
              onChange={(on) => commitAgent({ telegram: { ...agent.telegram, enabled: on } })}
            />
          }
        />
        <SettingsRow label="Bot Token">
          <SavedTextInput
            type="password"
            value={agent.telegram?.bot_token ?? ""}
            onChange={(e) => updateAgent({ telegram: { ...agent.telegram, bot_token: e.target.value } })}
            onCommit={() => saveSettings()}
            className="font-mono !text-[12px]"
            placeholder="123456789:ABCdefGhI..."
          />
        </SettingsRow>
        <SettingsRow label={t("settings.tg.allowedUser")}>
          <SavedTextInput
            value={agent.telegram?.allowed_username ?? ""}
            onChange={(e) => updateAgent({ telegram: { ...agent.telegram, allowed_username: e.target.value } })}
            onCommit={() => saveSettings()}
            className="font-mono !text-[12px]"
            placeholder={t("settings.tg.allowedUserPlaceholder")}
          />
        </SettingsRow>
      </SettingsSection>

      {/* Scheduled heartbeat */}
      <SettingsSection title={t("settings.hb.title")}>
        <SettingsRow
          label={t("settings.hb.enable")}
          description={t("settings.hb.note")}
          align="start"
          control={
            <Switch
              checked={agent.heartbeat_enabled}
              onChange={(on) => commitAgent({ heartbeat_enabled: on })}
            />
          }
        />
        <SettingsRow
          label={t("settings.hb.interval")}
          control={
            <NumberField
              className="w-24"
              value={agent.heartbeat_interval}
              fallback={60}
              onChange={(v) => updateAgent({ heartbeat_interval: v })}
              onCommit={(v) => commitAgent({ heartbeat_interval: v })}
              placeholder="60"
            />
          }
        />
        <SettingsRow
          label={t("settings.hb.contextTurns")}
          description={t("settings.hb.contextTurnsNote")}
          align="start"
          control={
            <NumberField
              className="w-24"
              value={agent.heartbeat_context_turns}
              min={0}
              fallback={0}
              onChange={(v) => updateAgent({ heartbeat_context_turns: v })}
              onCommit={(v) => commitAgent({ heartbeat_context_turns: v })}
              placeholder="10"
            />
          }
        />
      </SettingsSection>

      {/* Memory */}
      <AgentMemory key={agent.id} agentId={agent.id} />
    </>
  );

  const renderTab = (): ReactNode => {
    switch (tab) {
      case "raw":
        return (
          <SettingsSection
            title="config.yaml"
            action={
              <Button variant="ghost" size="sm" onClick={handleOpenConfigDir} title={t("settings.openConfigDirTitle")}>
                {t("settings.openConfigDir")}
              </Button>
            }
          >
            <SettingsRow>
              <TextArea
                autoGrow
                value={rawYaml}
                onChange={(e) => setRawYaml(e.target.value)}
                onBlur={saveRaw}
                spellCheck={false}
                className="min-h-[360px] whitespace-pre font-mono !text-[12px] leading-relaxed"
              />
            </SettingsRow>
          </SettingsSection>
        );

      case "ai-models":
        return <ModelsCard settings={form} onDraft={setForm} onCommit={commitSettings} notify={ok} />;

      case "ai-mcp":
        return (
          <McpCard
            settings={form}
            onDraft={setForm}
            onCommit={commitSettings}
            statuses={mcpStatuses}
            onReconnect={handleReconnectMcp}
            onToggle={toggleMcpServer}
            busy={mcpBusy}
          />
        );

      case "ai-prompts":
        return <PromptsCard notify={ok} fail={fail} />;

      case "ai-tools":
        return <ToolsCard notify={ok} fail={fail} />;

      case "ai-search":
        return (
          <SettingsSection description={t("settings.search.apiKeyNote")}>
            <SettingsRow label={t("settings.search.apiKey")}>
              <SavedTextInput
                type="password"
                value={form.search_api_key}
                onChange={(e) => setForm({ ...form, search_api_key: e.target.value })}
                onCommit={() => saveSettings()}
                placeholder="tvly-..."
              />
            </SettingsRow>
          </SettingsSection>
        );

      case "ai-skills":
        return (
          <>
            <SettingsSection
              title={t("settings.skills.title")}
              action={
                <button onClick={loadSkills} className="text-note font-medium text-accent hover:underline">
                  {t("common.refresh")}
                </button>
              }
            >
              <SettingsRow label={t("settings.skills.dir")} description={t("settings.skills.dirNote", { dir: skillsInfo?.dir ?? "" })}>
                <div className="flex gap-2">
                  <SavedTextInput
                    value={form.skills_dir}
                    onChange={(e) => setForm({ ...form, skills_dir: e.target.value })}
                    onCommit={() => commitSkillsDir(form.skills_dir)}
                    className="flex-1"
                    placeholder={skillsInfo?.dir ?? "~/.agents/skills"}
                  />
                  <Button variant="secondary" onClick={handlePickSkillsDir}>
                    {t("settings.skills.pick")}
                  </Button>
                  <Button variant="secondary" onClick={() => invoke("open_skills_dir").catch((e: any) => fail(t("settings.skills.openDirFailed", { error: e })))} title={t("settings.skills.openDirTitle")}>
                    <ExternalLinkIcon className="h-4 w-4" />
                    {t("common.open")}
                  </Button>
                </div>
                <div className="mt-2 flex flex-wrap gap-1.5">
                  {(skillsInfo?.presets ?? []).map((p) => (
                    <button
                      key={p}
                      onClick={() => commitSkillsDir(p)}
                      className="rounded-lg bg-surface-soft px-2 py-1 font-mono text-[11px] text-ink-soft transition-colors hover:bg-hover"
                    >
                      {p}
                    </button>
                  ))}
                </div>
              </SettingsRow>
            </SettingsSection>

            <SettingsSection title={t("settings.skills.title")}>
              {skillsInfo?.skills.length ? (
                skillsInfo.skills.map((s) => (
                  <SettingsRow
                    key={s.path}
                    label={<span className="flex items-baseline gap-2">{s.name}<span className="font-mono text-[11px] font-normal text-accent">/skill:{s.slug}</span></span>}
                    description={s.error ? undefined : s.description}
                  >
                    {s.error ? (
                      <ErrorBox className="mt-0">{s.error}</ErrorBox>
                    ) : (
                      <p className="truncate font-mono text-[10px] text-ink-faint">{s.path}</p>
                    )}
                  </SettingsRow>
                ))
              ) : (
                <SettingsRow><HintText className="mt-0">{t("settings.skills.empty")}</HintText></SettingsRow>
              )}
            </SettingsSection>
          </>
        );

      case "pet-visual":
        return (
          <>
            <SettingsSection description={(!live2dReady || !galleryReady) ? t("settings.pet.gateHint") : undefined}>
              <SettingsRow>
                <div className="flex flex-wrap gap-1.5">
                  <PetKindBtn
                    active={form.pet_kind === "live2d"}
                    disabled={!live2dReady}
                    title={live2dReady ? "" : t("settings.pet.needLive2d")}
                    onClick={() => commitSettings({ ...form, pet_kind: "live2d" })}
                  >
                    {t("settings.pet.kind.live2d")}
                  </PetKindBtn>
                  <PetKindBtn
                    active={form.pet_kind === "gallery"}
                    disabled={!galleryReady}
                    title={galleryReady ? "" : t("settings.pet.needGallery")}
                    onClick={() => commitSettings({ ...form, pet_kind: "gallery" })}
                  >
                    {t("settings.pet.kind.gallery")}
                  </PetKindBtn>
                  <PetKindBtn
                    active={form.pet_kind === "image"}
                    onClick={() => commitSettings({ ...form, pet_kind: "image" })}
                  >
                    {t("settings.pet.kind.image")}
                  </PetKindBtn>
                </div>
              </SettingsRow>
              {form.pet_kind === "live2d" && !live2dReady && (
                <SettingsRow><StatusText ok={false} className="text-note">{t("settings.pet.needLive2d")}</StatusText></SettingsRow>
              )}
              {form.pet_kind === "gallery" && !galleryReady && (
                <SettingsRow><StatusText ok={false} className="text-note">{t("settings.pet.needGallery")}</StatusText></SettingsRow>
              )}
            </SettingsSection>

            <SettingsSection title={t("settings.live2d.title")}>
              <SettingsRow label={t("settings.live2d.corePath")} description={t("settings.live2d.corePathNote")}>
                <div className="flex gap-2">
                  <SavedTextInput
                    value={form.live_2d_core_path}
                    onChange={(e) => setForm({ ...form, live_2d_core_path: e.target.value })}
                    onCommit={() => saveSettings()}
                    className="flex-1"
                    placeholder={t("settings.live2d.corePlaceholder")}
                  />
                  <Button variant="secondary" onClick={handlePickLive2DCore}>
                    <ImageIcon className="h-4 w-4" />
                    {t("settings.gallery.pick")}
                  </Button>
                </div>
              </SettingsRow>
              <SettingsRow label={t("settings.live2d.modelPath")} description={t("settings.live2d.modelPathNote")}>
                <div className="flex gap-2">
                  <SavedTextInput
                    value={form.live_2d_model_path}
                    onChange={(e) => setForm({ ...form, live_2d_model_path: e.target.value })}
                    onCommit={() => saveSettings()}
                    className="flex-1"
                    placeholder={t("settings.live2d.modelPathPlaceholder")}
                  />
                  <Button variant="secondary" onClick={handlePickLive2DModel}>
                    <ImageIcon className="h-4 w-4" />
                    {t("settings.gallery.pick")}
                  </Button>
                </div>
              </SettingsRow>
              <SettingsRow
                label={t("settings.live2d.useExample")}
                align="start"
                control={
                  <div className="group relative inline-flex">
                    <Button variant="secondary" disabled={exampleBusy} onClick={handleDownloadExample}>
                      {exampleBusy ? (
                        <SpinnerIcon className="h-4 w-4 animate-spin" />
                      ) : (
                        <DownloadIcon className="h-4 w-4" />
                      )}
                      {exampleBusy
                        ? t("settings.live2d.exampleDownloading")
                        : t("settings.live2d.useExample")}
                    </Button>
                    <div className="pointer-events-none invisible absolute bottom-full right-0 z-50 mb-2 w-80 rounded-field border border-line bg-surface p-3 text-note leading-relaxed text-ink-soft opacity-0 shadow-card transition-opacity duration-150 group-hover:visible group-hover:opacity-100">
                      <div className="whitespace-pre-line [overflow-wrap:anywhere]">
                        {t("settings.live2d.exampleTooltip")}
                      </div>
                    </div>
                  </div>
                }
              />
            </SettingsSection>

            <SettingsSection title={t("settings.gallery.title")}>
              <SettingsRow label={t("settings.gallery.dir")}>
                <div className="flex gap-2">
                  <TextInput value={form.gallery_dir} readOnly className="flex-1" placeholder={t("settings.gallery.noDir")} />
                  <Button variant="secondary" onClick={() => pickDirectory(form.gallery_dir, (gallery_dir) => commitSettings({ ...form, gallery_dir }))}>
                    <ImageIcon className="h-4 w-4" />
                    {t("settings.gallery.pick")}
                  </Button>
                  <Button variant="secondary" onClick={() => handleOpenPath(form.gallery_dir)} disabled={!form.gallery_dir} title={t("settings.gallery.openDirTitle")}>
                    <ExternalLinkIcon className="h-4 w-4" />
                    {t("common.open")}
                  </Button>
                </div>
              </SettingsRow>
              <SettingsRow
                label={t("settings.gallery.interval")}
                description={t("settings.gallery.intervalNote")}
                control={
                  <NumberField
                    className="w-24"
                    value={form.gallery_interval}
                    fallback={10}
                    onChange={(v) => setForm({ ...form, gallery_interval: v })}
                    onCommit={(v) => commitSettings({ ...form, gallery_interval: v })}
                    placeholder="10"
                  />
                }
              />
            </SettingsSection>

            <SettingsSection title={t("settings.pet.imageTitle")}>
              <SettingsRow label={t("settings.pet.imageDir")} description={t("settings.pet.imageDirNote")}>
                <div className="flex gap-2">
                  <TextInput value={form.pet_image_dir} readOnly className="flex-1" placeholder={t("settings.pet.imageNoDir")} />
                  <Button variant="secondary" onClick={handlePickPetImageDir}>
                    <ImageIcon className="h-4 w-4" />
                    {t("settings.gallery.pick")}
                  </Button>
                  <Button variant="secondary" onClick={() => handleOpenPath(form.pet_image_dir)} disabled={!form.pet_image_dir} title={t("settings.gallery.openDirTitle")}>
                    <ExternalLinkIcon className="h-4 w-4" />
                    {t("common.open")}
                  </Button>
                </div>
              </SettingsRow>
            </SettingsSection>
          </>
        );

      case "general-language":
        return (
          <SettingsSection>
            <SettingsRow
              label={t("settings.language")}
              control={
                <Select className="w-40" value={form.language === "en" ? "en" : "zh"} onChange={(e) => setLanguage(e.target.value)}>
                  <option value="zh">中文</option>
                  <option value="en">English</option>
                </Select>
              }
            />
          </SettingsSection>
        );

      default:
        return renderAgentPage();
    }
  };

  return (
    <div className="flex h-full min-h-0">
      {/* Left rail: grouped tabs. */}
      <aside className="flex w-56 shrink-0 flex-col overflow-y-auto border-r border-line bg-canvas px-2.5 py-3">
        {NAV_GROUPS.map((group, gi) => (
          <div key={group.title} className={gi === 0 ? "" : "mt-4"}>
            <div className="px-3 pb-1.5 text-meta font-semibold text-ink-faint">{t(group.title)}</div>
            <div className="flex flex-col gap-0.5">
              {group.items.map((entry) => (
                <NavItem key={entry.id} active={tab === entry.id} icon={entry.icon} onClick={() => selectTab(entry.id)}>
                  {t(entry.label)}
                </NavItem>
              ))}
            </div>
          </div>
        ))}

        {/* Agents: one tab per agent; "+" adds one. */}
        <div className="mt-4">
          <div className="flex items-center justify-between px-3 pb-1.5">
            <span className="text-meta font-semibold text-ink-faint">{t("settings.group.agents")}</span>
            <button
              onClick={addAgent}
              title={t("settings.agent.add")}
              className="flex h-5 w-5 items-center justify-center rounded-md text-ink-faint transition-colors hover:bg-hover hover:text-ink"
            >
              <PlusIcon className="h-3.5 w-3.5" />
            </button>
          </div>
          <div className="flex flex-col gap-0.5">
            {form.agents.map((a) => (
              <NavItem
                key={a.id}
                active={tab === `${AGENT_PREFIX}${a.id}`}
                icon={AgentIcon}
                onClick={() => selectTab(`${AGENT_PREFIX}${a.id}`)}
                dot={a.id === form.active_agent}
              >
                {a.name}
              </NavItem>
            ))}
          </div>
        </div>

        <div className="mt-4">
          <div className="px-3 pb-1.5 text-meta font-semibold text-ink-faint">{t("settings.group.general")}</div>
          <div className="flex flex-col gap-0.5">
            {GENERAL_ITEMS.map((entry) => (
              <NavItem key={entry.id} active={tab === entry.id} icon={entry.icon} onClick={() => selectTab(entry.id)}>
                {t(entry.label)}
              </NavItem>
            ))}
          </div>
        </div>
      </aside>

      {/* Right content: page title + the active tab's sections. */}
      <div className="flex min-h-0 flex-1 flex-col bg-surface">
        <header className="shrink-0 border-b border-line/70 px-8 pb-4 pt-6">
          <p className="text-meta font-medium text-ink-faint">{t(currentGroupKey)}</p>
          <h1 className="mt-0.5 truncate text-heading font-semibold tracking-tight text-ink">{pageTitle}</h1>
        </header>
        <div className="min-h-0 flex-1 overflow-y-auto px-8 py-6">
          <div className="mx-auto max-w-2xl">
            {renderTab()}
            {messageLine}
          </div>
        </div>
      </div>
    </div>
  );
}

/* ---------- Left-rail tab button ---------- */

function NavItem({
  active,
  icon: Icon,
  onClick,
  children,
  dot = false,
}: {
  active: boolean;
  icon: ComponentType<{ className?: string }>;
  onClick: () => void;
  children: ReactNode;
  dot?: boolean;
}) {
  return (
    <button
      onClick={onClick}
      className={`flex w-full items-center gap-2.5 rounded-lg px-3 py-1.5 text-left text-body transition-colors ${
        active ? "bg-surface font-medium text-ink shadow-card" : "text-ink-soft hover:bg-hover"
      }`}
    >
      <Icon className={`h-4 w-4 shrink-0 ${active ? "text-accent" : "text-ink-faint"}`} />
      <span className="min-w-0 flex-1 truncate">{children}</span>
      {dot && <span className="h-1.5 w-1.5 shrink-0 rounded-full bg-accent" />}
    </button>
  );
}

/** One of the three pet-kind choices. Disabled (with the reason as `title`)
 *  while its prerequisite paths are unconfigured. */
function PetKindBtn({
  active,
  disabled,
  title,
  onClick,
  children,
}: {
  active: boolean;
  disabled?: boolean;
  title?: string;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      title={title}
      className={`rounded-full px-3.5 py-1.5 text-note font-medium transition-colors ${
        active
          ? "bg-accent text-white"
          : disabled
            ? "cursor-not-allowed bg-surface-soft text-ink-faint"
            : "bg-surface-soft text-ink-soft hover:bg-hover"
      }`}
    >
      {children}
    </button>
  );
}