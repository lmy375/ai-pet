import { useState, useEffect, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { AppSettings, AgentConfig, McpStatus, TelegramStatus, SkillsInfo } from "../../hooks/useSettings";
import { defaultAgent } from "../../hooks/useSettings";
import { Card } from "../ui/Card";
import { Button } from "../ui/Button";
import { ErrorBox, LoadingScreen, HintText } from "../ui/feedback";
import { Label, TextInput, TextArea, Select, SavedTextInput, NumberField } from "../ui/fields";
import { StatusText } from "../ui/StatusText";
import { PlusIcon, TrashIcon, ImageIcon, ExternalLinkIcon, DownloadIcon, SpinnerIcon } from "../Icons";
import { AgentMemory } from "./PanelMemory";
import { ModelsCard } from "./settings/ModelsCard";
import { PromptsCard } from "./settings/PromptsCard";
import { ToolsCard } from "./settings/ToolsCard";
import { McpCard } from "./settings/McpCard";
import { open } from "@tauri-apps/plugin-dialog";
import { toneText, toneDot, connTone } from "../../utils/tone";
import { useI18n } from "../../i18n";

const blankSettings: AppSettings = {
  models: {},
  mcp_servers: {},
  live_2d_model_path: "",
  live_2d_core_path: "",
  language: "zh",
  pet_kind: "image",
  pet_image_dir: "",
  gallery_dir: "",
  gallery_interval: 10,
  search_api_key: "",
  skills_dir: "",
  tools: { disabled: [] },
  active_agent: "default",
  agents: [defaultAgent()],
};

/** The three top-level groups. Agents get their own sub-tab bar inside. */
type TopTab = "ai" | "pet" | "agents" | "raw";

export function PanelSettings() {
  const { t } = useI18n();
  const [form, setForm] = useState<AppSettings>(blankSettings);
  const [tab, setTab] = useState<TopTab>("ai");
  // Which agent is edited inside the "agents" group (its own sub-tab bar).
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

  // The agent shown in the agents group (falls back to the first agent, e.g.
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

  // Switch the top-level tab. Loads the raw YAML when entering "config file",
  // and reloads settings from disk when leaving it (raw edits may have changed
  // them).
  const selectTab = async (next: TopTab) => {
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
    if (next === "agents" && agent) loadTelegramStatus(agent.id);
  };

  // Switch the edited agent inside the agents group.
  const selectAgent = (id: string) => {
    setAgentTab(id);
    setMessage(null);
    loadTelegramStatus(id);
  };

  /** Jump to one of the AI pools from an agent's reference control. */
  const goToPool = (id: "pool-models" | "pool-mcp") => {
    setTab("ai");
    setMessage(null);
    requestAnimationFrame(() =>
      document.getElementById(id)?.scrollIntoView({ behavior: "smooth", block: "start" })
    );
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

  const addAgent = () => {
    const id = crypto.randomUUID();
    const next = {
      ...form,
      agents: [...form.agents, defaultAgent(id, t("settings.agent.newName"))],
    };
    setForm(next);
    setTab("agents");
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
    if (agentTab === id) setAgentTab(agents[0].id);
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
    <StatusText ok={message.ok} className="mt-1 text-[13px]">{message.text}</StatusText>
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

  return (
    <div className="flex h-full flex-col">
      {/* Top-level groups: AI 配置 | 宠物配置 | Agent 配置 ... 配置文件 (far right) */}
      <div className="flex shrink-0 items-center gap-1 overflow-x-auto border-b border-line/70 bg-surface/80 px-3 py-2 backdrop-blur">
        <TabBtn active={tab === "ai"} onClick={() => selectTab("ai")}>{t("settings.tab.ai")}</TabBtn>
        <TabBtn active={tab === "pet"} onClick={() => selectTab("pet")}>{t("settings.tab.pet")}</TabBtn>
        <TabBtn active={tab === "agents"} onClick={() => selectTab("agents")}>{t("settings.tab.agents")}</TabBtn>
        <div className="ml-auto shrink-0">
          <TabBtn active={tab === "raw"} onClick={() => selectTab("raw")}>{t("settings.tab.file")}</TabBtn>
        </div>
      </div>

      <div className="flex-1 overflow-y-auto px-5 py-5">
      {tab === "raw" ? (
        <>
          <Card
            title="config.yaml"
            action={
              <Button variant="ghost" size="sm" onClick={handleOpenConfigDir} title={t("settings.openConfigDirTitle")}>
                {t("settings.openConfigDir")}
              </Button>
            }
          >
            <TextArea
              autoGrow
              value={rawYaml}
              onChange={(e) => setRawYaml(e.target.value)}
              onBlur={saveRaw}
              spellCheck={false}
              className="min-h-[300px] whitespace-pre font-mono !text-[12px] leading-relaxed"
            />
          </Card>
        </>
      ) : tab === "ai" ? (
        <>
          {/* The two pools agents reference, first — everything below is chrome
              by comparison. */}
          <div id="pool-models">
            <ModelsCard settings={form} onDraft={setForm} onCommit={commitSettings} notify={ok} />
          </div>

          <div id="pool-mcp">
            <McpCard
              settings={form}
              onDraft={setForm}
              onCommit={commitSettings}
              statuses={mcpStatuses}
              onReconnect={handleReconnectMcp}
              onToggle={toggleMcpServer}
              busy={mcpBusy}
            />
          </div>

          {/* What the model reads before every turn: the system prompts, and
              the tool list with each tool's description. */}
          <PromptsCard notify={ok} fail={fail} />
          <ToolsCard notify={ok} fail={fail} />

          {/* Web Search (shared by all agents) */}
          <Card title={t("settings.search.title")}>
            <Label>{t("settings.search.apiKey")}</Label>
            <SavedTextInput
              type="password"
              value={form.search_api_key}
              onChange={(e) => setForm({ ...form, search_api_key: e.target.value })}
              onCommit={() => saveSettings()}
              placeholder="tvly-..."
            />
            <HintText>{t("settings.search.apiKeyNote")}</HintText>
          </Card>

          {/* Agent Skills (shared by all agents). Read-only: skills are authored
              on disk, this card only shows what was discovered there. */}
          <Card
            title={t("settings.skills.title")}
            action={
              <button onClick={loadSkills} className="text-[12px] font-medium text-accent hover:underline">
                {t("common.refresh")}
              </button>
            }
          >
            <Label>{t("settings.skills.dir")}</Label>
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
            <HintText>{t("settings.skills.dirNote", { dir: skillsInfo?.dir ?? "" })}</HintText>

            <div className="mt-3 space-y-2">
              {skillsInfo && skillsInfo.skills.length === 0 && (
                <HintText>{t("settings.skills.empty")}</HintText>
              )}
              {skillsInfo?.skills.map((s) => (
                <div key={s.path} className="rounded-xl border border-line/70 px-3 py-2">
                  <div className="flex items-baseline gap-2">
                    <span className="text-[13px] font-medium text-ink">{s.name}</span>
                    <span className="font-mono text-[11px] text-accent">/skill:{s.slug}</span>
                  </div>
                  {s.error ? (
                    <ErrorBox className="mt-1">{s.error}</ErrorBox>
                  ) : (
                    <p className="mt-1 line-clamp-3 text-[12px] leading-relaxed text-ink-soft">{s.description}</p>
                  )}
                  <p className="mt-1 truncate font-mono text-[10px] text-ink-faint">{s.path}</p>
                </div>
              ))}
            </div>
          </Card>
        </>
      ) : tab === "pet" ? (
        <>
          {/* The pet's visual: an explicit three-way choice. The two kinds that
              read from disk are disabled until their paths are configured in
              the cards below, so the pet window can never end up blank. */}
          <Card title={t("settings.pet.title")}>
            <div className="flex flex-wrap gap-2">
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
            {(!live2dReady || !galleryReady) && (
              <HintText>{t("settings.pet.gateHint")}</HintText>
            )}
            {/* Hand-edited config can still name an unready kind — say so
                instead of silently rendering something else. */}
            {form.pet_kind === "live2d" && !live2dReady && (
              <StatusText ok={false} className="mt-1.5 text-[12px]">{t("settings.pet.needLive2d")}</StatusText>
            )}
            {form.pet_kind === "gallery" && !galleryReady && (
              <StatusText ok={false} className="mt-1.5 text-[12px]">{t("settings.pet.needGallery")}</StatusText>
            )}
          </Card>

          {/* Live2D */}
          <Card title={t("settings.live2d.title")}>
            <Label>{t("settings.live2d.corePath")}</Label>
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
            <HintText>{t("settings.live2d.corePathNote")}</HintText>

            <Label>{t("settings.live2d.modelPath")}</Label>
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
            <HintText>{t("settings.live2d.modelPathNote")}</HintText>

            {/* Sample installer: hover shows exactly what it downloads and
                configures before anything happens; click runs the download and
                fills both paths above. */}
            <div className="mt-1">
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
                <div className="pointer-events-none invisible absolute bottom-full left-0 z-50 mb-2 w-80 rounded-field border border-line bg-surface p-3 text-note leading-relaxed text-ink-soft opacity-0 shadow-card transition-opacity duration-150 group-hover:visible group-hover:opacity-100">
                  <div className="whitespace-pre-line [overflow-wrap:anywhere]">
                    {t("settings.live2d.exampleTooltip")}
                  </div>
                </div>
              </div>
            </div>
          </Card>

          {/* Gallery slideshow */}
          <Card title={t("settings.gallery.title")}>
            <Label>{t("settings.gallery.dir")}</Label>
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

            <Label className="mt-3">{t("settings.gallery.interval")}</Label>
            <NumberField
              value={form.gallery_interval}
              fallback={10}
              onChange={(v) => setForm({ ...form, gallery_interval: v })}
              onCommit={(v) => commitSettings({ ...form, gallery_interval: v })}
              placeholder="10"
            />
            <HintText>{t("settings.gallery.intervalNote")}</HintText>
          </Card>

          {/* Image pet: bundled art by default, or a directory of the owner's
              own images (one file per emotion, missing ones fall back). */}
          <Card title={t("settings.pet.imageTitle")}>
            <Label>{t("settings.pet.imageDir")}</Label>
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
            <HintText>{t("settings.pet.imageDirNote")}</HintText>
          </Card>

          {/* Language */}
          <Card title={t("settings.language")}>
            <Select value={form.language === "en" ? "en" : "zh"} onChange={(e) => setLanguage(e.target.value)}>
              <option value="zh">中文</option>
              <option value="en">English</option>
            </Select>
          </Card>
        </>
      ) : (
        <>
          {/* Agent sub-tabs: one tab per agent, + add. */}
          <div className="mb-4 flex items-center gap-1 overflow-x-auto">
            {form.agents.map((a) => (
              <TabBtn key={a.id} active={a.id === agent?.id} onClick={() => selectAgent(a.id)} dot={a.id === form.active_agent}>
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
          </div>

          {/* Default agent */}
          <Card title={t("settings.agent.defaultTitle")}>
            <Select value={form.active_agent} onChange={(e) => setActiveAgent(e.target.value)}>
              {form.agents.map((a) => (
                <option key={a.id} value={a.id}>{a.name}</option>
              ))}
            </Select>
            <HintText>{t("settings.agent.defaultNote")}</HintText>
          </Card>

          {/* Agent identity: name + delete. Selecting/adding agents is done via
              the sub-tab bar; the default agent is chosen in the card above. */}
          <Card title={agent.name || t("settings.agent.newName")}>
            <Label>{t("settings.agent.name")}</Label>
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
            <div className="mt-1 flex items-center justify-between gap-2">
              <p className="text-[11px] text-ink-faint">{t("settings.agent.idNote", { id: agent.id })}</p>
              {agent.id === form.active_agent ? (
                <span className="shrink-0 text-[11px] font-medium text-accent">{t("settings.agent.isDefault")}</span>
              ) : (
                <button onClick={() => setActiveAgent(agent.id)} className="shrink-0 text-[11px] font-medium text-accent hover:underline">
                  {t("settings.agent.setDefault")}
                </button>
              )}
            </div>
          </Card>

          {/* Model: a reference into the global pool, not a copy of its settings. */}
          <Card
            title={t("settings.agent.modelTitle")}
            action={
              <button onClick={() => goToPool("pool-models")} className="text-[12px] font-medium text-accent hover:underline">
                {t("settings.agent.goConfigure")}
              </button>
            }
          >
            {modelNames.length === 0 ? (
              <HintText>{t("settings.agent.modelEmpty")}</HintText>
            ) : (
              <>
                <Select value={agent.model} onChange={(e) => commitAgent({ model: e.target.value })}>
                  <option value="">{t("settings.agent.modelNone")}</option>
                  {danglingModel && <option value={agent.model}>{agent.model}</option>}
                  {modelNames.map((name) => (
                    <option key={name} value={name}>{name}</option>
                  ))}
                </Select>
                {danglingModel ? (
                  <StatusText ok={false} className="mt-1.5 text-[12px]">
                    {t("settings.agent.modelMissing", { model: agent.model })}
                  </StatusText>
                ) : (
                  <HintText>{t("settings.agent.modelNote")}</HintText>
                )}
              </>
            )}
          </Card>

          {/* MCP: which of the global servers this agent may call. */}
          <Card
            title={t("settings.agent.mcpTitle")}
            action={
              <button onClick={() => goToPool("pool-mcp")} className="text-[12px] font-medium text-accent hover:underline">
                {t("settings.agent.goConfigure")}
              </button>
            }
          >
            {serverNames.length === 0 ? (
              <HintText>{t("settings.agent.mcpEmpty")}</HintText>
            ) : (
              <>
                <div className="flex flex-col gap-1.5">
                  {serverNames.map((name) => {
                    const status = mcpStatuses.find((s) => s.name === name);
                    const off = !form.mcp_servers[name].enabled;
                    return (
                      <label key={name} className="flex items-center gap-2 text-[13px] text-ink">
                        <input
                          type="checkbox"
                          className="accent-accent"
                          checked={agent.mcp.includes(name)}
                          onChange={(e) => toggleAgentServer(name, e.target.checked)}
                        />
                        <span className={`h-1.5 w-1.5 shrink-0 rounded-full ${toneDot(connTone(status?.connected, status?.error))}`} />
                        {name}
                        {/* A server switched off globally offers no tools here,
                            so say so instead of showing a silently dead entry. */}
                        <span className="text-[11px] text-ink-faint">
                          {off
                            ? t("settings.mcp.disabled")
                            : status?.connected
                              ? t("settings.mcp.toolsSuffix", { count: status.tool_count })
                              : ""}
                        </span>
                      </label>
                    );
                  })}
                </div>
                <HintText>{t("settings.agent.mcpNote")}</HintText>
              </>
            )}
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
            {telegramStatus.error && <ErrorBox className="mb-2">{telegramStatus.error}</ErrorBox>}

            <label className="mb-2 flex items-center gap-1.5 text-[12px] font-medium text-ink-soft">
              <input
                type="checkbox"
                className="accent-accent"
                checked={agent.telegram?.enabled ?? false}
                onChange={(e) => commitAgent({ telegram: { ...agent.telegram, enabled: e.target.checked } })}
              />
              {t("settings.tg.enable")}
            </label>

            <Label>Bot Token</Label>
            <SavedTextInput
              type="password"
              value={agent.telegram?.bot_token ?? ""}
              onChange={(e) => updateAgent({ telegram: { ...agent.telegram, bot_token: e.target.value } })}
              onCommit={() => saveSettings()}
              className="mb-2 font-mono !text-[12px]"
              placeholder="123456789:ABCdefGhI..."
            />

            <Label>{t("settings.tg.allowedUser")}</Label>
            <SavedTextInput
              value={agent.telegram?.allowed_username ?? ""}
              onChange={(e) => updateAgent({ telegram: { ...agent.telegram, allowed_username: e.target.value } })}
              onCommit={() => saveSettings()}
              className="font-mono !text-[12px]"
              placeholder={t("settings.tg.allowedUserPlaceholder")}
            />
          </Card>

          {/* Scheduled heartbeat */}
          <Card title={t("settings.hb.title")}>
            <label className="mb-3 flex items-center gap-1.5 text-[12px] font-medium text-ink-soft">
              <input
                type="checkbox"
                className="accent-accent"
                checked={agent.heartbeat_enabled}
                onChange={(e) => commitAgent({ heartbeat_enabled: e.target.checked })}
              />
              {t("settings.hb.enable")}
            </label>

            <Label>{t("settings.hb.interval")}</Label>
            <NumberField
              value={agent.heartbeat_interval}
              fallback={60}
              onChange={(v) => updateAgent({ heartbeat_interval: v })}
              onCommit={(v) => commitAgent({ heartbeat_interval: v })}
              placeholder="60"
            />
            <HintText>{t("settings.hb.note")}</HintText>

            <Label className="mt-3">{t("settings.hb.contextTurns")}</Label>
            <NumberField
              value={agent.heartbeat_context_turns}
              min={0}
              fallback={0}
              onChange={(v) => updateAgent({ heartbeat_context_turns: v })}
              onCommit={(v) => commitAgent({ heartbeat_context_turns: v })}
              placeholder="10"
            />
            <HintText>{t("settings.hb.contextTurnsNote")}</HintText>
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

/* ---------- Tab buttons ---------- */

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
        active ? "bg-accent text-white" : "text-ink-soft hover:bg-hover"
      }`}
    >
      {dot && <span className={`h-1.5 w-1.5 rounded-full ${active ? "bg-surface" : "bg-accent"}`} />}
      {children}
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
      className={`rounded-lg px-4 py-1.5 text-[13px] font-medium transition-colors ${
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
