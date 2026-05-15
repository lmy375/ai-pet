import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { AppSettings, McpServerConfig, TgCustomCommand } from "../../hooks/useSettings";
import { NumberField as SharedNumberField } from "../common/NumberField";
import { ImageLightbox } from "../common/ImageLightbox";
import { LoadingState } from "./LoadingState";
import { SectionTitle } from "./SectionTitle";
import { MaskedSecretField } from "./MaskedSecretField";
import { formatBytes } from "../../utils/formatBytes";

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

/** 图像生成 model 预设。与 chat model 不同源 —— 一般是各家专门的图像端点
 * 模型名（如 OpenAI 的 dall-e-3、Stability 的 sd-xl、Black Forest 的 flux）。
 * 空串 = 禁用 /image 命令。 */
const IMAGE_MODEL_PRESETS: string[] = [
  "dall-e-3",
  "dall-e-2",
  "stable-diffusion-xl",
  "flux-1.1-pro",
  "flux-schnell",
];

/** 图像尺寸 preset。三档主流 aspect：方 / 竖（手机壁纸 / 海报）/ 横（桌面壁纸
 * / 封面）。各 provider 也支持 256x256 / 512x512 等老 dall-e-2 尺寸，用户可
 * 手输自定义值，datalist 仅做引导。 */
const IMAGE_SIZE_PRESETS: string[] = [
  "1024x1024",
  "1024x1792",
  "1792x1024",
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
    image_model: "",
    image_size: "",
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
      stale_butler_archive_days: 30,
      weekly_summary_closing_hour: 20,
    },
    chat: {
      max_context_messages: 50,
    },
    user_name: "",
    user_glyph: "",
    assistant_glyph: "",
    tool_review_overrides: {},
    motion_mapping: {},
  });
  const [soul, setSoul] = useState("");
  const [loaded, setLoaded] = useState(false);
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState("");
  // 长按 👁 时短暂解掩码。mouseUp / mouseLeave / touchEnd 都会立刻 setFalse，
  // 用户松手 / 把鼠标移出按钮，输入框立即变回 ••••。
  // motion mapping "全部演示一遍" 按钮的播放态。播放期间按钮 disabled +
  // 文案换"演示中…"，避免双击让 4 个 motion 错乱叠播。
  const [demoingMotions, setDemoingMotions] = useState(false);
  const [mcpStatuses, setMcpStatuses] = useState<McpStatus[]>([]);
  const [reconnecting, setReconnecting] = useState(false);
  const [newServerName, setNewServerName] = useState("");
  const [telegramStatus, setTelegramStatus] = useState<TelegramStatus>({ running: false, error: null });
  const [telegramReconnecting, setTelegramReconnecting] = useState(false);
  // 清空 TG 命令补全 ack：让按钮按下时显 "清空中…"，避免重复点击。
  const [telegramResetting, setTelegramResetting] = useState(false);
  const [viewMode, setViewMode] = useState<"form" | "raw">("form");
  const [rawYaml, setRawYaml] = useState("");
  // Raw YAML 模式下的 textarea ref —— YamlSearchBar 用它做 setSelectionRange
  // 跳转到匹配位置。
  const rawYamlRef = useRef<HTMLTextAreaElement>(null);
  // YAML lint state：debounce 500ms 调 validate_config_raw，错误透传 serde_yaml
  // 的 line/column 信息。null = 检查中或还没检查过；"" = 合法；非空 = 错误文案。
  // 用 null vs "" 两个空态区分让 UI 在 typing 期间不闪。
  const [yamlError, setYamlError] = useState<string | null>(null);
  useEffect(() => {
    if (viewMode !== "raw") return;
    if (!loaded) return;
    setYamlError(null);
    const id = window.setTimeout(() => {
      invoke<void>("validate_config_raw", { content: rawYaml })
        .then(() => setYamlError(""))
        .catch((e: unknown) => setYamlError(String(e)));
    }, 500);
    return () => window.clearTimeout(id);
  }, [rawYaml, viewMode, loaded]);
  // 搜索框状态：仅在 form 模式生效。空 query = 全展；非空 = 按标题 + 关键字
  // 子串（大小写不敏感）过滤 section。
  const [searchQuery, setSearchQuery] = useState("");
  // 工具风险面板：name + level + note 是后端静态 metadata（一次性加载）；
  // mode 反映 form.tool_review_overrides 当前编辑状态（不存盘也即时生效）。
  const [toolRiskRows, setToolRiskRows] = useState<{ name: string; level: string; note: string }[]>([]);
  // 本地数据目录绝对路径。挂载时一次性 fetch；后端 `config_dir()` 内部
  // 已 ensure dir 存在，前端拿到的路径即真实可见目录。空串表示后端报错
  // —— 对应 section 显错误 banner，避免 dump undefined 让 UI 看起来像没加载完。
  const [petDataDir, setPetDataDir] = useState<string>("");
  const [petDataDirError, setPetDataDirError] = useState<string>("");
  const [openingDataDir, setOpeningDataDir] = useState<boolean>(false);
  const [pathCopied, setPathCopied] = useState<boolean>(false);
  // SQLite db stats（v0-v12 migration 落地数据量）：挂载时一次性拉，让 owner
  // 看到 backfill 已运转 + 各表实际行数。失败 / 旧 backend 无此命令 → null。
  type DbStats = {
    size_bytes: number;
    /// SQLite _migrations 表里最大 version；显示让 owner 知道 schema 跑到哪一档。
    schema_version: number;
    butler_tasks_count: number;
    todo_count: number;
    task_archive_count: number;
    kv_state_count: number;
  };
  const [dbStats, setDbStats] = useState<DbStats | null>(null);
  /// App 版本号（Cargo.toml 编译期）。挂载时一次 fetch；老 backend / 失败 → null
  /// 不渲染段。chip 行没显 pet vX.Y.Z 时退化等价 v0 状态，不破坏其它字段。
  const [appVersion, setAppVersion] = useState<string | null>(null);
  /// 版本 chip 点击复制后的瞬时绿色"已复制"反馈，1.5s 自清。
  const [versionCopied, setVersionCopied] = useState(false);
  // image_model 测试态：调一次 image_generate 验真链路。testing = 进行中；
  // result 含 data URL + 耗时；error 透传后端错误（key 错 / quota / model 名错）。
  // 单次只跑一张 256x256 / 同 image_size 的实际配置 —— 测试就是要验真实路径。
  // chat model 测试态：调一次非流式 chat_test 验真链路。reply 截一段（前 80 字）
  // 显在按钮旁，让用户**看见**模型确实回了；不是 200 OK 就当成功。
  const [chatTesting, setChatTesting] = useState(false);
  const [chatTestReply, setChatTestReply] = useState<{
    text: string;
    elapsedMs: number;
  } | null>(null);
  const [chatTestError, setChatTestError] = useState<string>("");
  const handleTestChat = async () => {
    setChatTesting(true);
    setChatTestReply(null);
    setChatTestError("");
    const t0 = performance.now();
    try {
      const text = await invoke<string>("chat_test");
      const elapsedMs = Math.round(performance.now() - t0);
      setChatTestReply({ text, elapsedMs });
    } catch (e) {
      setChatTestError(String(e));
    } finally {
      setChatTesting(false);
    }
  };

  const [imageTesting, setImageTesting] = useState(false);
  const [imageTestResult, setImageTestResult] = useState<{
    url: string;
    elapsedMs: number;
  } | null>(null);
  const [imageTestError, setImageTestError] = useState<string>("");
  const handleTestImage = async () => {
    setImageTesting(true);
    setImageTestResult(null);
    setImageTestError("");
    const t0 = performance.now();
    try {
      // image_generate 现在返回 { urls, errors } 部分成功结构。测试只跑 n=1，
      // urls 空 = 全败 → 显第一条 error；非空 = 成功（取首张）。
      const result = await invoke<{ urls: string[]; errors: string[] }>(
        "image_generate",
        {
          prompt: "a tiny cute cat icon, simple flat illustration",
          n: 1,
        },
      );
      const elapsedMs = Math.round(performance.now() - t0);
      if (result.urls.length === 0) {
        setImageTestError(result.errors[0] ?? "API 返回空，但没报错。");
      } else {
        setImageTestResult({ url: result.urls[0], elapsedMs });
      }
    } catch (e) {
      setImageTestError(String(e));
    } finally {
      setImageTesting(false);
    }
  };

  // Live2D 模型可用 motion group 名列表。读 form.live_2d_model_path 指向的
  // model3.json，解析 FileReferences.Motions 的 keys → datalist 建议给
  // motion_mapping 输入用。失败（路径错 / 解析错）→ 空数组（仅没有建议，输入
  // 仍可手填）。
  const [availableMotionGroups, setAvailableMotionGroups] = useState<string[]>([]);
  useEffect(() => {
    const path = form.live_2d_model_path?.trim();
    if (!path) {
      setAvailableMotionGroups([]);
      return;
    }
    let cancelled = false;
    (async () => {
      try {
        const resp = await fetch(path);
        if (!resp.ok) {
          if (!cancelled) setAvailableMotionGroups([]);
          return;
        }
        const json = await resp.json();
        const motions = json?.FileReferences?.Motions;
        if (motions && typeof motions === "object") {
          const groups = Object.keys(motions);
          if (!cancelled) setAvailableMotionGroups(groups);
        } else {
          if (!cancelled) setAvailableMotionGroups([]);
        }
      } catch {
        // 不弹错误 banner —— 这只是 datalist 建议，失败时让用户继续手填即可
        if (!cancelled) setAvailableMotionGroups([]);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [form.live_2d_model_path]);

  // Model 字段旁的多模态 chip：null = 检查中 / 加载前；true / false = 后端结果。
  // 用 form.model 作输入；变化时 250ms debounce 调 check_multimodal_model_name，
  // 避免每按一键就一次 IPC。空名 → 直接 false 不调命令。
  const [modelMultimodal, setModelMultimodal] = useState<boolean | null>(null);
  useEffect(() => {
    const name = form.model.trim();
    if (!name) {
      setModelMultimodal(false);
      return;
    }
    const id = window.setTimeout(() => {
      invoke<boolean>("check_multimodal_model_name", { name })
        .then((v) => setModelMultimodal(v))
        .catch(() => setModelMultimodal(null));
    }, 250);
    return () => window.clearTimeout(id);
  }, [form.model]);

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
    // 本地数据目录单独拉一次（与上面 batch 解耦：失败也不该挡住主设置加载）。
    invoke<string>("get_pet_data_dir")
      .then(setPetDataDir)
      .catch((e) => setPetDataDirError(String(e)));
    // SQLite db stats 同样独立拉。命令未注册（旧 backend）→ 静默 null，
    // 显示侧渲染时不会出该块。
    invoke<DbStats>("get_db_stats")
      .then(setDbStats)
      .catch(() => setDbStats(null));
    // app_version：编译期 env!，永远成功；catch 仅防老 backend 缺命令时静默
    invoke<string>("app_version")
      .then(setAppVersion)
      .catch(() => setAppVersion(null));
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

  // 重启 pet 窗口：armed 二次确认。改完 live_2d_model_path / motion_mapping
  // 等需要重启窗口生效的字段后用；不动 panel / debug。
  const [restartArmed, setRestartArmed] = useState(false);
  const restartArmTimerRef = useRef<number | null>(null);

  // 设置页内 lightbox 大图 src。当前只服务 image_model 测试缩略图；将来若
  // 加其它图（如 motion preview）可复用同 state。
  const [settingsLightboxSrc, setSettingsLightboxSrc] = useState<string | null>(null);

  // 重置默认按钮：armed 二次确认（同 /clear 路径）。armed 5s 内再点 → 真重置 +
  // reload settings；超时自动撤回 armed。不删 SOUL.md / memory / sessions —— 只
  // 重置 settings 项。
  const [resetArmed, setResetArmed] = useState(false);
  const resetArmTimerRef = useRef<number | null>(null);
  // "重置 SOUL.md 为内置默认"二次确认：与 resetArmed 同模式，5s 自动 revert。
  // busy 在 invoke 期间防双触。
  const [soulResetArmed, setSoulResetArmed] = useState(false);
  const [soulResetBusy, setSoulResetBusy] = useState(false);
  const soulResetTimerRef = useRef<number | null>(null);
  const handleResetDefaults = async () => {
    if (!resetArmed) {
      setResetArmed(true);
      if (resetArmTimerRef.current !== null) {
        window.clearTimeout(resetArmTimerRef.current);
      }
      resetArmTimerRef.current = window.setTimeout(() => {
        setResetArmed(false);
        resetArmTimerRef.current = null;
      }, 5000);
      setMessage("⚠ 再点一次确认重置（5 秒内）。仅清设置，不删 SOUL.md / memory / sessions。");
      return;
    }
    if (resetArmTimerRef.current !== null) {
      window.clearTimeout(resetArmTimerRef.current);
      resetArmTimerRef.current = null;
    }
    setResetArmed(false);
    setSaving(true);
    setMessage("");
    try {
      await invoke("reset_config_to_defaults");
      const fresh = await invoke<AppSettings>("get_settings");
      setForm(fresh);
      // raw YAML 模式也要刷一下，避免用户切到 raw 看到旧内容
      try {
        const raw = await invoke<string>("get_config_raw");
        setRawYaml(raw);
      } catch {
        // raw 拉失败不影响主路径
      }
      setMessage("已重置为默认设置。");
    } catch (e: any) {
      setMessage(`重置失败: ${e}`);
    } finally {
      setSaving(false);
    }
  };

  /// 导出快照：调后端拿 base64 → 写剪贴板。snapshot 含 api_key 明文（base64
  /// 是编码不是加密），导出成功时单独弹红字提示，让用户在 IM 分享等场景前
  /// 警觉。8s 自清避免长期占视觉位。
  const [securityNotice, setSecurityNotice] = useState("");
  const securityNoticeTimerRef = useRef<number | null>(null);
  const handleExportSnapshot = async () => {
    setMessage("");
    if (securityNoticeTimerRef.current !== null) {
      window.clearTimeout(securityNoticeTimerRef.current);
    }
    setSecurityNotice("");
    try {
      const payload = await invoke<string>("export_settings_snapshot");
      await navigator.clipboard.writeText(payload);
      setMessage(`已复制 snapshot（${payload.length} 字符）到剪贴板`);
      setSecurityNotice(
        "⚠ snapshot 含 API key / Telegram token 明文（base64 只是编码不是加密）—— 贴到 IM / 公开 issue 前请审核。",
      );
      securityNoticeTimerRef.current = window.setTimeout(() => {
        setSecurityNotice("");
        securityNoticeTimerRef.current = null;
      }, 8000);
    } catch (e: any) {
      setMessage(`导出失败: ${e}`);
    }
  };

  /// 导出 markdown：把 config.yaml + SOUL.md 包成 fenced code blocks 拼一
  /// 段 markdown 写剪贴板。与"导出快照"（base64 roundtrip）目的不同 ——
  /// 这条是给人 / LLM 阅读的，提 issue / share / 跟 maintainer 讨论用。
  /// 仍然含 api_key 明文，与 snapshot 同款 8s security 警示。
  const handleExportSettingsMarkdown = async () => {
    setMessage("");
    if (securityNoticeTimerRef.current !== null) {
      window.clearTimeout(securityNoticeTimerRef.current);
    }
    setSecurityNotice("");
    try {
      const [configYaml, soul] = await Promise.all([
        invoke<string>("get_config_raw"),
        invoke<string>("get_soul"),
      ]);
      const ts = new Date().toLocaleString();
      const md = [
        `# Pet 配置快照 markdown（${ts}）`,
        "",
        "## config.yaml",
        "```yaml",
        configYaml || "（空）",
        "```",
        "",
        "## SOUL.md",
        "```markdown",
        soul || "（空）",
        "```",
      ].join("\n");
      await navigator.clipboard.writeText(md);
      setMessage(`已复制 settings markdown（${md.length} 字符）到剪贴板`);
      setSecurityNotice(
        "⚠ 导出内容含 api_key / Telegram token 等明文字段，贴到公开 issue 前请审核脱敏。",
      );
      securityNoticeTimerRef.current = window.setTimeout(() => {
        setSecurityNotice("");
        securityNoticeTimerRef.current = null;
      }, 8000);
    } catch (e: any) {
      setMessage(`导出失败: ${e}`);
    }
  };

  /// 导入快照：armed 二次确认（同 reset 路径，覆盖性写盘要防误触）。第一次点
  /// 读剪贴板预检（解 base64 + JSON）后弹"确认 import"；armed 5s 内再点真写。
  const [importArmed, setImportArmed] = useState(false);
  const importPayloadRef = useRef<string>("");
  const importArmTimerRef = useRef<number | null>(null);
  const handleImportSnapshot = async () => {
    setMessage("");
    if (!importArmed) {
      let payload = "";
      try {
        payload = await navigator.clipboard.readText();
      } catch (e: any) {
        setMessage(`读剪贴板失败: ${e}`);
        return;
      }
      if (!payload.trim()) {
        setMessage("剪贴板为空。先把 snapshot 字符串复制过来再点导入。");
        return;
      }
      importPayloadRef.current = payload;
      setImportArmed(true);
      if (importArmTimerRef.current !== null) {
        window.clearTimeout(importArmTimerRef.current);
      }
      importArmTimerRef.current = window.setTimeout(() => {
        setImportArmed(false);
        importPayloadRef.current = "";
        importArmTimerRef.current = null;
      }, 5000);
      setMessage(
        `⚠ 检测到 ${payload.length} 字符的 snapshot，再点一次确认覆盖当前 config.yaml + SOUL.md（5 秒内）。`,
      );
      return;
    }
    if (importArmTimerRef.current !== null) {
      window.clearTimeout(importArmTimerRef.current);
      importArmTimerRef.current = null;
    }
    setImportArmed(false);
    const payload = importPayloadRef.current;
    importPayloadRef.current = "";
    setSaving(true);
    try {
      await invoke("import_settings_snapshot", { payload });
      // 重新拉一次让 form / soul 刷成新值
      const fresh = await invoke<AppSettings>("get_settings");
      setForm(fresh);
      try {
        const raw = await invoke<string>("get_config_raw");
        setRawYaml(raw);
      } catch {
        // raw 拉失败不影响主路径
      }
      try {
        const freshSoul = await invoke<string>("get_soul");
        setSoul(freshSoul);
      } catch {
        // soul 拉失败不影响主路径
      }
      setMessage("已导入 snapshot。");
    } catch (e: any) {
      setMessage(`导入失败: ${e}`);
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

  if (!loaded) return <div style={containerStyle}><LoadingState /></div>;

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
            <SectionTitle>config.yaml</SectionTitle>
            <YamlSearchBar
              text={rawYaml}
              textareaRef={rawYamlRef}
            />
            <textarea
              ref={rawYamlRef}
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
                borderColor: yamlError
                  ? "var(--pet-tint-red-fg)"
                  : undefined,
              }}
              spellCheck={false}
            />
            {/* Lint 反馈：debounce 500ms 后显。空串 = 合法（不渲染）；非空 =
                serde_yaml 错误透传，通常含行号 / 列号让用户跳过去修。 */}
            {yamlError && (
              <div
                style={{
                  marginTop: 6,
                  padding: "6px 10px",
                  fontSize: 11,
                  color: "var(--pet-tint-red-fg)",
                  background: "var(--pet-tint-red-bg)",
                  border: "1px solid var(--pet-tint-red-fg)",
                  borderRadius: 4,
                  fontFamily: "'SF Mono', 'Menlo', monospace",
                  wordBreak: "break-word",
                  whiteSpace: "pre-wrap",
                }}
              >
                ⚠ {yamlError}
              </div>
            )}
            {yamlError === "" && (
              <div
                style={{
                  marginTop: 6,
                  fontSize: 11,
                  color: "var(--pet-tint-green-fg)",
                }}
              >
                ✓ YAML 合法
              </div>
            )}
          </div>

          {/* SOUL */}
          <div style={sectionStyle}>
            <div
              style={{
                display: "flex",
                alignItems: "center",
                justifyContent: "space-between",
                marginBottom: 8,
              }}
            >
              <SectionTitle noMargin>系统提示词 (SOUL.md)</SectionTitle>
              {/* "重置为内置默认" 按钮：实验时改坏了想复位用。armed 二次
                  确认（首点变红 + 5s 内再点真重置，超时自动 revert）。后端
                  reset_soul_to_default 直接覆盖 SOUL.md 并返新内容，前端 sync
                  textarea state；其它字段（config.yaml）不动。 */}
              {(() => {
                const armed = soulResetArmed;
                return (
                  <button
                    type="button"
                    onClick={async () => {
                      if (soulResetBusy) return;
                      if (!armed) {
                        setSoulResetArmed(true);
                        if (soulResetTimerRef.current !== null) {
                          window.clearTimeout(soulResetTimerRef.current);
                        }
                        soulResetTimerRef.current = window.setTimeout(() => {
                          setSoulResetArmed(false);
                          soulResetTimerRef.current = null;
                        }, 5000);
                        return;
                      }
                      setSoulResetArmed(false);
                      if (soulResetTimerRef.current !== null) {
                        window.clearTimeout(soulResetTimerRef.current);
                        soulResetTimerRef.current = null;
                      }
                      setSoulResetBusy(true);
                      try {
                        const fresh = await invoke<string>("reset_soul_to_default");
                        setSoul(fresh);
                        setMessage("SOUL.md 已重置为内置默认");
                      } catch (e: any) {
                        setMessage(`重置失败：${e}`);
                      } finally {
                        setSoulResetBusy(false);
                      }
                    }}
                    disabled={soulResetBusy}
                    style={{
                      fontSize: 11,
                      padding: "3px 10px",
                      borderRadius: 4,
                      border: armed
                        ? "1px solid #dc2626"
                        : "1px solid var(--pet-color-border)",
                      background: soulResetBusy
                        ? "var(--pet-color-bg)"
                        : armed
                          ? "var(--pet-tint-red-bg)"
                          : "var(--pet-color-card)",
                      color: armed
                        ? "var(--pet-tint-red-fg)"
                        : "var(--pet-color-muted)",
                      cursor: soulResetBusy ? "default" : "pointer",
                      fontWeight: armed ? 600 : 400,
                    }}
                    title={
                      armed
                        ? "再次点击确认（5s 内有效；覆盖当前 SOUL.md，不可恢复）"
                        : "把 SOUL.md 重置为内置默认提示词。改坏了 / 想从零开始时用。两次点击确认。"
                    }
                  >
                    {soulResetBusy
                      ? "重置中…"
                      : armed
                        ? "再点确认 (5s)"
                        : "↺ 重置默认"}
                  </button>
                );
              })()}
            </div>
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
            <button
              onClick={handleSaveRaw}
              disabled={saving || (yamlError !== "" && yamlError !== null)}
              style={{
                ...btnStyle,
                // YAML 报错时灰态防"保存了又被后端 422"循环。lint pending（null）
                // 不 disable，让用户在 typing 期间仍能确认（500ms debounce 后才有
                // 结果，期间按 save 走老路径让后端拦）。
                opacity: yamlError !== "" && yamlError !== null ? 0.5 : 1,
                cursor:
                  yamlError !== "" && yamlError !== null
                    ? "not-allowed"
                    : "pointer",
              }}
              title={
                yamlError !== "" && yamlError !== null
                  ? "YAML 解析失败，修正后才能保存"
                  : undefined
              }
            >
              {saving ? "保存中..." : "保存"}
            </button>
            {message && (
              <span style={{ fontSize: "13px", color: message.includes("失败") ? "var(--pet-tint-red-fg)" : "var(--pet-tint-green-fg)" }}>
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
      {/* 外观：主题切换。顶部 tab bar 已经有 🌙/☀️ 一键 toggle，但藏在右上角；
          这里把控件再露一次，并解释跨窗同步行为。 */}
      <SearchableSection
        title="外观"
        keywords={["theme", "外观", "主题", "深色", "浅色", "dark", "light"]}
        query={searchQuery}
      >
      <div style={sectionStyle}>
        <SectionTitle><HighlightedText text="外观" query={searchQuery} /></SectionTitle>
        <ThemeToggleRow />
        <div style={{ marginTop: 12 }}>
          <AccentPickerRow />
        </div>
        {/* 复制对话历史时的角色前缀。默认 🧑 / 🐾，让用户自定义"我"/"猫娘"等。
            ChatMini 顶部 📋 复制 N 条 / 跨会话搜索导出 markdown 都走这俩字段。 */}
        <div
          style={{
            marginTop: 12,
            display: "grid",
            gridTemplateColumns: "120px 1fr",
            gap: 6,
            alignItems: "center",
          }}
        >
          <label
            style={labelStyle}
            title="复制对话时用户那一段前缀。默认 🧑；改成「我:」/「我」/「主人」都行。空串走默认。"
          >
            User glyph
          </label>
          <input
            value={form.user_glyph}
            onChange={(e) => setForm({ ...form, user_glyph: e.target.value })}
            placeholder="🧑"
            style={{ ...inputStyle, maxWidth: 200 }}
          />
          <label
            style={labelStyle}
            title="复制对话时助手那一段前缀。默认 🐾；自定义 SOUL 是猫娘 / 阅读伙伴 / 翻译官时改这里更准确。空串走默认。"
          >
            Assistant glyph
          </label>
          <input
            value={form.assistant_glyph}
            onChange={(e) => setForm({ ...form, assistant_glyph: e.target.value })}
            placeholder="🐾"
            style={{ ...inputStyle, maxWidth: 200 }}
          />
        </div>
      </div>
      </SearchableSection>

      {/* 本地数据目录：让用户一眼看到宠物把 config / memory / sessions 落到哪儿，
          一键打开 Finder 备份 / 排查；路径下 ~/.config/pet/ 起。 */}
      <SearchableSection
        title="本地数据目录"
        keywords={["data", "dir", "path", "本地", "目录", "memory", "session", "config", "finder"]}
        query={searchQuery}
      >
      <div style={sectionStyle}>
        <SectionTitle><HighlightedText text="本地数据目录" query={searchQuery} /></SectionTitle>
        {petDataDirError ? (
          <div style={{ fontSize: "12px", color: "var(--pet-tint-red-fg)", padding: "6px 0" }}>
            读取失败：{petDataDirError}
          </div>
        ) : (
          <>
            <code
              style={{
                display: "block",
                fontFamily: "'SF Mono', 'Menlo', monospace",
                fontSize: "12px",
                background: "var(--pet-color-bg)",
                color: "var(--pet-color-fg)",
                padding: "6px 10px",
                borderRadius: 6,
                border: "1px solid var(--pet-color-border)",
                wordBreak: "break-all",
                userSelect: "all",
              }}
            >
              {petDataDir || "（加载中…）"}
            </code>
            <div style={{ display: "flex", gap: 8, marginTop: 8, flexWrap: "wrap" }}>
              <button
                onClick={async () => {
                  setOpeningDataDir(true);
                  try {
                    await invoke("open_pet_data_dir");
                  } catch (e) {
                    setPetDataDirError(String(e));
                  } finally {
                    setOpeningDataDir(false);
                  }
                }}
                disabled={openingDataDir || !petDataDir}
                style={{
                  padding: "6px 14px",
                  borderRadius: 6,
                  border: "1px solid var(--pet-color-accent)",
                  background: openingDataDir ? "var(--pet-color-muted)" : "var(--pet-color-accent)",
                  color: "#fff",
                  fontSize: 13,
                  fontWeight: 500,
                  cursor: openingDataDir || !petDataDir ? "default" : "pointer",
                }}
                title="在系统文件管理器（macOS Finder）里打开此目录，方便 inspect / 备份"
              >
                {openingDataDir ? "打开中…" : "在 Finder 中打开"}
              </button>
              <button
                onClick={async () => {
                  if (!petDataDir) return;
                  try {
                    await navigator.clipboard.writeText(petDataDir);
                    setPathCopied(true);
                    setTimeout(() => setPathCopied(false), 1500);
                  } catch (e) {
                    setPetDataDirError(String(e));
                  }
                }}
                disabled={!petDataDir}
                style={{
                  padding: "6px 14px",
                  borderRadius: 6,
                  border: "1px solid var(--pet-color-border)",
                  background: "var(--pet-color-card)",
                  color: pathCopied ? "var(--pet-tint-green-fg)" : "var(--pet-color-fg)",
                  fontSize: 13,
                  cursor: petDataDir ? "pointer" : "default",
                }}
                title="把绝对路径复制到剪贴板"
              >
                {pathCopied ? "已复制" : "复制路径"}
              </button>
            </div>
            <div style={{ fontSize: "11px", color: "var(--pet-color-muted)", marginTop: 6, lineHeight: 1.6 }}>
              此目录下：<code>config.yaml</code> 设置、<code>SOUL.md</code> 系统提示词、
              <code>memories/</code> 记忆库（含 task_archive 归档）、<code>sessions/</code> 对话存档、
              <code>pet.db</code> SQLite 业务数据（butler_tasks / todo / task_archive / kv_state）。
              复制 / 备份整个目录即可迁移到新机器。
            </div>
            {/* SQLite stats：让 owner 看到 v0-v12 migration 落地的实际数据
                量。dbStats === null（旧 backend / 失败）时不渲染该块。 */}
            {dbStats && (
              <div
                style={{
                  marginTop: 10,
                  padding: "8px 12px",
                  background: "var(--pet-color-bg)",
                  border: "1px solid var(--pet-color-border)",
                  borderRadius: 6,
                  fontSize: 11,
                  color: "var(--pet-color-muted)",
                  display: "flex",
                  flexWrap: "wrap",
                  gap: "4px 14px",
                  fontFamily: "'SF Mono', 'Menlo', monospace",
                }}
                title="pet.db 文件大小 + 各业务态表行数。SQLite 持久化分层（v0-v12）的落地状态，让你看到 backfill 已运转 + 数据规模"
              >
                {appVersion && (
                  <button
                    type="button"
                    onClick={async () => {
                      const plat =
                        typeof navigator !== "undefined" ? navigator.platform : "";
                      const parts = [`pet v${appVersion}`];
                      if (dbStats?.schema_version)
                        parts.push(`schema v${dbStats.schema_version}`);
                      if (plat) parts.push(plat);
                      try {
                        await navigator.clipboard.writeText(parts.join(" · "));
                        setVersionCopied(true);
                        setTimeout(() => setVersionCopied(false), 1500);
                      } catch {
                        // 剪贴板权限错误 / 隐私模式：静默；按钮反馈不出现即可
                      }
                    }}
                    title="点击复制 pet v / schema v / 平台 一行（贴 bug report 用）"
                    style={{
                      padding: "0 4px",
                      border: "none",
                      background: "transparent",
                      color: versionCopied
                        ? "var(--pet-tint-green-fg)"
                        : "var(--pet-color-fg)",
                      fontWeight: 600,
                      fontFamily: "inherit",
                      fontSize: 11,
                      cursor: "pointer",
                    }}
                  >
                    {versionCopied ? "✓ 已复制" : `pet v${appVersion}`}
                  </button>
                )}
                <span style={{ color: "var(--pet-color-fg)", fontWeight: 600 }}>
                  pet.db
                </span>
                <span>{formatBytes(dbStats.size_bytes)}</span>
                <span title="SQLite _migrations 表最大 version。当前最新 schema = 4（v9 加 kv_state 起）。低于此值说明 migration 没跑完，重启 app 让它补跑。">
                  schema v{dbStats.schema_version}
                </span>
                <span>butler_tasks: {dbStats.butler_tasks_count}</span>
                <span>todo: {dbStats.todo_count}</span>
                <span>task_archive: {dbStats.task_archive_count}</span>
                <span>kv_state: {dbStats.kv_state_count}</span>
              </div>
            )}
          </>
        )}
      </div>
      </SearchableSection>

      {/* Live2D */}
      <SearchableSection
        title="Live2D 模型"
        keywords={["live2d", "model", "motion", "miku", "映射", "动作"]}
        query={searchQuery}
      >
      <div style={sectionStyle}>
        <SectionTitle><HighlightedText text="Live2D 模型" query={searchQuery} /></SectionTitle>
        <label style={labelStyle}>模型路径</label>
        <input
          value={form.live_2d_model_path}
          onChange={(e) => setForm({ ...form, live_2d_model_path: e.target.value })}
          style={inputStyle}
          placeholder="/models/miku/miku.model3.json"
        />
        {/* 重启 pet 窗口：armed 二次确认。改了 model 路径 / motion_mapping 等
            字段后点这个，关 main + 重建让新设置生效，省 quit 整个 app。
            不影响 panel / debug 等其它窗口。 */}
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 8,
            marginTop: 8,
          }}
        >
          <button
            type="button"
            onClick={async () => {
              if (!restartArmed) {
                setRestartArmed(true);
                if (restartArmTimerRef.current !== null) {
                  window.clearTimeout(restartArmTimerRef.current);
                }
                restartArmTimerRef.current = window.setTimeout(() => {
                  setRestartArmed(false);
                  restartArmTimerRef.current = null;
                }, 5000);
                setMessage("⚠ 再点一次确认重启 pet 窗口（5 秒内）。先点最下方『保存』让配置落盘。");
                return;
              }
              if (restartArmTimerRef.current !== null) {
                window.clearTimeout(restartArmTimerRef.current);
                restartArmTimerRef.current = null;
              }
              setRestartArmed(false);
              try {
                await invoke("restart_pet_window");
                setMessage("已重启 pet 窗口。新配置生效。");
              } catch (e: any) {
                setMessage(`重启失败: ${e}`);
              }
            }}
            disabled={saving}
            title="关掉桌面 pet 窗口再用新配置（含 motion_mapping / minSize / 模型路径）重建。先保存设置才能让新值生效。panel / debug 窗口不动。"
            style={{
              ...btnSmallStyle,
              background: restartArmed ? "var(--pet-tint-red-fg)" : "var(--pet-color-bg)",
              color: restartArmed ? "#fff" : "var(--pet-color-muted)",
              border: `1px solid ${restartArmed ? "var(--pet-tint-red-fg)" : "var(--pet-color-border)"}`,
              fontWeight: restartArmed ? 700 : 400,
            }}
          >
            {restartArmed ? "⚠ 确认重启？" : "🔄 重启 pet 窗口"}
          </button>
          {/* Reload 当前 panel：不重建 native window，仅 location.reload 让
              JS 重读 settings / 重挂 listener。比重启 pet 窗口便宜得多 ——
              用户改了纯 panel 内字段（如 chat / mcp / proactive）想立刻看新
              值生效时用。不弹 armed —— 没保存的草稿会丢，tooltip 提醒。 */}
          <button
            type="button"
            onClick={() => window.location.reload()}
            disabled={saving}
            title="只刷当前 panel webview（不动 pet 窗口）让 JS 重读 settings。注意：未保存的 form 草稿会丢；先点最下方『保存』。"
            style={{
              ...btnSmallStyle,
              background: "var(--pet-color-bg)",
              color: "var(--pet-color-muted)",
              border: "1px solid var(--pet-color-border)",
              fontWeight: 400,
            }}
          >
            🔁 reload 此面板
          </button>
        </div>

        {/* Motion 映射：把 4 个语义键映射到当前模型的实际 motion group 名。
            空 = 直接用语义键名（与内置 miku 行为一致）。 */}
        <div style={{ marginTop: "12px" }}>
          <div
            style={{
              display: "flex",
              alignItems: "center",
              justifyContent: "space-between",
              gap: 8,
            }}
          >
            <label style={{ ...labelStyle, marginBottom: 0 }}>Motion 映射</label>
            {/* 全部演示一遍：按 1.6s 间隔挨个触发 Tap / Flick / Flick3 / Idle，
                让用户改完映射后用一次性"巡演"看到全套效果，不必发 4 次消息。
                间隔取 1.6s 让 motion 单次能播完（多数 motion < 1.5s）。
                disabled 由 demoingMotions state 控制，防双击叠播。 */}
            <button
              type="button"
              onClick={async () => {
                setDemoingMotions(true);
                const keys = ["Tap", "Flick", "Flick3", "Idle"] as const;
                for (const key of keys) {
                  try {
                    await invoke("trigger_motion", { semantic: key });
                  } catch (e) {
                    console.error("trigger_motion failed:", e);
                  }
                  await new Promise((r) => window.setTimeout(r, 1600));
                }
                setDemoingMotions(false);
              }}
              disabled={demoingMotions}
              style={{
                ...btnSmallStyle,
                background: demoingMotions
                  ? "var(--pet-color-bg)"
                  : "var(--pet-color-accent)",
                color: demoingMotions ? "var(--pet-color-muted)" : "#fff",
                cursor: demoingMotions ? "default" : "pointer",
              }}
              title="按 Tap → Flick → Flick3 → Idle 顺序播放每个 motion 一次（间隔 1.6s）。改完映射想一眼看完整套用这条；不必发 4 次消息。"
            >
              {demoingMotions ? "演示中…" : "▶ 全部演示一遍"}
            </button>
          </div>
          <p style={{ fontSize: "11px", color: "var(--pet-color-muted)", margin: "0 0 8px 0", lineHeight: 1.5 }}>
            把 LLM 写的 4 个语义键翻译到你 model 的实际 motion group 名。留空 =
            直接用左侧键名。改完保存即时生效。
            {availableMotionGroups.length > 0 && (
              <>
                {" "}
                <span style={{ color: "var(--pet-tint-blue-fg)" }}>
                  从 model3.json 检测到可用 group：{availableMotionGroups.join(" / ")}
                </span>
              </>
            )}
          </p>
          {/* 共享 datalist：所有 motion 映射输入用同一份建议。datalist option
              永远渲染（即便 0 group），让 input 的 list 引用始终有效。 */}
          <datalist id="motion-group-presets">
            {availableMotionGroups.map((g) => (
              <option key={g} value={g} />
            ))}
          </datalist>
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
                list="motion-group-presets"
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
              <button
                type="button"
                onClick={() => {
                  void invoke("trigger_motion", { semantic: key }).catch((e) =>
                    console.error("trigger_motion failed:", e),
                  );
                }}
                style={{
                  ...btnSmallStyle,
                  background: "var(--pet-color-accent)",
                  flex: "0 0 auto",
                }}
                title={`让桌面 Live2D 播一下 ${key} —— 走"语义键 → 已保存的 motion_mapping → 实际 group"翻译。改完字段先点最下方『保存』再测才能验最新映射。`}
              >
                ▶ 试一下
              </button>
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
        <SectionTitle><HighlightedText text="LLM 配置" query={searchQuery} /></SectionTitle>
        <label style={labelStyle}>API Base URL</label>
        <input
          value={form.api_base}
          onChange={(e) => setForm({ ...form, api_base: e.target.value })}
          style={inputStyle}
          placeholder="https://api.openai.com/v1"
        />
        <label style={{ ...labelStyle, marginTop: "8px" }}>API Key</label>
        <MaskedSecretField
          value={form.api_key}
          onChange={(v) => setForm({ ...form, api_key: v })}
          placeholder="sk-..."
          secretLabel="API key"
          inputStyle={{ ...inputStyle, marginTop: 0 }}
          onCopyFeedback={(m) => {
            setMessage(m);
            window.setTimeout(() => setMessage(""), 3000);
          }}
        />
        <label
          style={{
            ...labelStyle,
            marginTop: "8px",
            display: "flex",
            alignItems: "center",
            gap: 8,
          }}
        >
          Model
          {/* 多模态 chip：当前 model 名是否能识别图片输入。绿色 = 支持 / 灰色 = 纯文本。
              null（检测中或失败）不渲染避免误导。文案 + tooltip 解释来源 + 局限。 */}
          {modelMultimodal !== null && (
            <span
              title={
                modelMultimodal
                  ? "当前 model 名匹配多模态关键字（gpt-4o / claude-3 / gemini / vision / qwen-vl 等）—— 聊天页可粘贴图片走多模态识别。"
                  : "当前 model 名未匹配多模态关键字 —— 聊天页粘贴图片会被守门拒绝。如果你确定它支持多模态，把名字片段加到后端 MULTIMODAL_MARKERS。"
              }
              style={{
                fontSize: 10,
                padding: "1px 8px",
                borderRadius: 999,
                border: "1px solid",
                borderColor: modelMultimodal
                  ? "var(--pet-tint-green-fg, #16a34a)"
                  : "var(--pet-color-border)",
                background: modelMultimodal
                  ? "var(--pet-tint-green-bg, #dcfce7)"
                  : "var(--pet-color-bg)",
                color: modelMultimodal
                  ? "var(--pet-tint-green-fg, #16a34a)"
                  : "var(--pet-color-muted)",
                fontWeight: 500,
              }}
            >
              {modelMultimodal ? "多模态" : "纯文本"}
            </span>
          )}
        </label>
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
        {/* 测试 chat 按钮：与 image_model 测试按钮对称。一次非流式调用，让用户
            确认 model+key+base_url 真链路 OK。注意：测试用的是已保存的 settings，
            改完字段先点最下方"保存"再测。 */}
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 8,
            marginTop: 8,
          }}
        >
          <button
            type="button"
            onClick={handleTestChat}
            disabled={chatTesting || form.model.trim().length === 0}
            style={{
              ...btnSmallStyle,
              background: chatTesting ? "var(--pet-color-muted)" : "var(--pet-color-accent)",
            }}
            title="发一句简短消息验 chat model + key + base_url 真链路；走的是已保存的 settings，改完先点底部『保存』再测。"
          >
            {chatTesting ? "测试中…" : "🧪 测试 chat"}
          </button>
          {chatTestReply && (
            <span
              style={{
                fontSize: 11,
                color: "var(--pet-tint-green-fg)",
                wordBreak: "break-word",
                flex: 1,
              }}
            >
              ✓ {(chatTestReply.elapsedMs / 1000).toFixed(1)}s ·{" "}
              {chatTestReply.text.length > 80
                ? chatTestReply.text.slice(0, 80) + "…"
                : chatTestReply.text}
            </span>
          )}
          {chatTestError && (
            <span
              style={{
                fontSize: 11,
                color: "var(--pet-tint-red-fg)",
                wordBreak: "break-word",
                flex: 1,
              }}
            >
              ✗ {chatTestError}
            </span>
          )}
        </div>
        <label
          style={{ ...labelStyle, marginTop: "8px" }}
          title="图片生成 model（OpenAI compatible /images/generations 端点）。与 chat model 解耦 —— chat 是文本/多模态模型，images 一般要 dall-e-3 / sd-xl 这种独立模型。空串则 /image 命令会拒绝执行。"
        >
          Image Model
        </label>
        <input
          list="image-model-presets"
          value={form.image_model}
          onChange={(e) => setForm({ ...form, image_model: e.target.value })}
          style={inputStyle}
          placeholder="dall-e-3（留空 = 禁用 /image）"
        />
        <datalist id="image-model-presets">
          {IMAGE_MODEL_PRESETS.map((m) => (
            <option key={m} value={m} />
          ))}
        </datalist>
        <div
          style={{
            fontSize: 11,
            color: "var(--pet-color-muted)",
            marginTop: 4,
          }}
        >
          {form.image_model.trim().length === 0
            ? "未配置 — `/image` 命令会拒绝执行"
            : `\`/image <prompt>\` 会调用 ${form.api_base.replace(/\/$/, "")}/images/generations，model = ${form.image_model.trim()}`}
        </div>
        <label
          style={{ ...labelStyle, marginTop: "8px" }}
          title="图片尺寸 WxH。dall-e-3 支持 1024x1024 / 1024x1792 / 1792x1024；dall-e-2 还支持 256/512；SD/flux 一般 512 ~ 1024 的方/竖/横；空串则后端 fallback 1024x1024。"
        >
          Image Size
        </label>
        <input
          list="image-size-presets"
          value={form.image_size}
          onChange={(e) => setForm({ ...form, image_size: e.target.value })}
          style={inputStyle}
          placeholder="1024x1024（横/竖图改成 1792x1024 / 1024x1792）"
        />
        <datalist id="image-size-presets">
          {IMAGE_SIZE_PRESETS.map((s) => (
            <option key={s} value={s} />
          ))}
        </datalist>
        {/* 测试按钮：直接调 image_generate，验"model + key + size + base_url"
            真实链路。注意它走的是已保存的 settings —— 用户改了输入框但还没点
            "保存"时，测试用的还是上次保存的值；这点 placeholder 文案里说明。 */}
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 8,
            marginTop: 8,
          }}
        >
          <button
            type="button"
            onClick={handleTestImage}
            disabled={imageTesting || form.image_model.trim().length === 0}
            style={{
              ...btnSmallStyle,
              background: imageTesting ? "var(--pet-color-muted)" : "var(--pet-color-accent)",
            }}
            title="生成一张小猫测试图，验当前 model + key + size 是否真的能出图。注意走的是已保存的 settings —— 改完字段先点最下方『保存』再测。"
          >
            {imageTesting ? "测试中…" : "🧪 测试生图"}
          </button>
          {imageTestResult && (
            <span
              style={{
                fontSize: 11,
                color: "var(--pet-tint-green-fg)",
              }}
            >
              ✓ 成功，耗时 {(imageTestResult.elapsedMs / 1000).toFixed(1)} 秒
            </span>
          )}
          {imageTestError && (
            <span
              style={{
                fontSize: 11,
                color: "var(--pet-tint-red-fg)",
                wordBreak: "break-word",
                flex: 1,
              }}
            >
              ✗ {imageTestError}
            </span>
          )}
        </div>
        {imageTestResult && (
          <div style={{ marginTop: 6 }}>
            <img
              src={imageTestResult.url}
              alt="test result"
              onClick={() => setSettingsLightboxSrc(imageTestResult.url)}
              title="点击放大查看 / 复制 / 另存为"
              style={{
                maxWidth: 200,
                maxHeight: 200,
                borderRadius: 6,
                border: "1px solid var(--pet-color-border)",
                display: "block",
                cursor: "zoom-in",
              }}
            />
          </div>
        )}
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
          <SectionTitle
            noMargin
            subtitle={
              serverEntries.length > 0
                ? `${connectedCount}/${serverEntries.length} 已连接 · ${totalToolCount} 工具`
                : undefined
            }
          >
            <HighlightedText text="MCP Servers" query={searchQuery} />
          </SectionTitle>
          <button
            onClick={handleReconnectMcp}
            disabled={reconnecting}
            style={{
              ...btnSmallStyle,
              background: reconnecting ? "var(--pet-color-muted)" : "var(--pet-tint-purple-fg)",
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
              background: !newServerName.trim() ? "var(--pet-color-muted)" : "var(--pet-tint-green-fg)",
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
          <SectionTitle
            noMargin
            subtitle={
              <span
                style={{
                  color: telegramStatus.running
                    ? "var(--pet-tint-green-fg)"
                    : telegramStatus.error
                      ? "var(--pet-tint-red-fg)"
                      : "var(--pet-color-muted)",
                }}
              >
                {telegramStatus.running ? "运行中" : telegramStatus.error ? "连接失败" : "未启动"}
              </span>
            }
          >
            <HighlightedText text="Telegram Bot" query={searchQuery} />
          </SectionTitle>
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
                  telegramResetting || telegramReconnecting ? "var(--pet-color-muted)" : "var(--pet-color-muted)",
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
                  telegramReconnecting || telegramResetting ? "var(--pet-color-muted)" : "var(--pet-color-accent)",
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
        <MaskedSecretField
          value={form.telegram?.bot_token ?? ""}
          onChange={(v) =>
            setForm({
              ...form,
              telegram: {
                ...form.telegram,
                enabled: form.telegram?.enabled ?? false,
                allowed_username: form.telegram?.allowed_username ?? "",
                bot_token: v,
              },
            })
          }
          placeholder="123456789:ABCdefGhI..."
          secretLabel="Bot Token"
          inputStyle={{
            ...inputStyle,
            marginBottom: "8px",
            fontFamily: "monospace",
            fontSize: "12px",
          }}
          onCopyFeedback={(m) => {
            setMessage(m);
            window.setTimeout(() => setMessage(""), 3000);
          }}
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
        <SectionTitle><HighlightedText text="主动开口" query={searchQuery} /></SectionTitle>
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
        <SectionTitle><HighlightedText text="早安简报" query={searchQuery} /></SectionTitle>
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
        <SectionTitle><HighlightedText text="工具风险" query={searchQuery} /></SectionTitle>
        <div style={{ fontSize: "11px", color: "var(--pet-color-muted)", marginBottom: 8, lineHeight: 1.5 }}>
          每次工具调用都会先经分类器自动定级。这里按工具单独覆盖：「自动」跟分类器走（高危才弹审核）；「总是审核」无论什么情况都让你先确认；「总是放行」哪怕高危也直接执行。MCP 工具不在列表里，按默认 medium 处理。
        </div>
        {toolRiskRows.length === 0 ? (
          <LoadingState inline compact />
        ) : (
          <div style={{ display: "grid", gridTemplateColumns: "auto 1fr auto", gap: "6px 10px", alignItems: "center" }}>
            {toolRiskRows.map((row) => {
              const mode = form.tool_review_overrides?.[row.name] ?? "auto";
              const levelColor =
                row.level === "high"
                  ? { bg: "var(--pet-tint-red-bg)", fg: "var(--pet-tint-red-fg)" }
                  : row.level === "medium"
                    ? { bg: "var(--pet-tint-yellow-bg)", fg: "var(--pet-tint-yellow-fg)" }
                    : { bg: "var(--pet-tint-green-bg)", fg: "var(--pet-tint-green-fg)" };
              return (
                <div key={row.name} style={{ display: "contents" }}>
                  <div style={{ display: "flex", alignItems: "center", gap: 6, fontSize: 12 }}>
                    <code style={{ background: "var(--pet-color-bg)", padding: "2px 6px", borderRadius: 4, fontSize: 11 }}>{row.name}</code>
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
        <SectionTitle><HighlightedText text="记忆整理" query={searchQuery} /></SectionTitle>
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
          <PanelNumberField
            label="butler 任务归档 (天，0=关闭)"
            value={form.memory_consolidate.stale_butler_archive_days}
            min={0}
            onChange={(v) =>
              setForm({
                ...form,
                memory_consolidate: {
                  ...form.memory_consolidate,
                  stale_butler_archive_days: Math.max(0, v),
                },
              })
            }
          />
        </div>
        <div style={{ fontSize: "11px", color: "var(--pet-color-muted)", marginTop: "4px" }}>
          reminder：consolidate 跑时删超过该时长的过期 [remind: YYYY-MM-DD HH:MM]。plan：daily_plan 条目 updated_at 超过该时长就清空。butler：完成的 [once] 任务过该时长后被自动清掉。daily_review：保留最近 N 天的 22:00 写入的 ai_insights/daily_review_YYYY-MM-DD 条目；0 = 永不清理。归档：终态（done / cancelled）butler_tasks 超过该天数后自动挪到 task_archive 类目，活跃队列长期保持轻量；0 = 永不归档。
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
        <SectionTitle><HighlightedText text="对话上下文" query={searchQuery} /></SectionTitle>
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

      {/* SOUL */}
      <SearchableSection
        title="系统提示词 (SOUL.md)"
        keywords={["soul", "prompt", "persona", "人格", "设定", "系统提示词"]}
        query={searchQuery}
      >
      <div style={sectionStyle}>
        <SectionTitle><HighlightedText text="系统提示词 (SOUL.md)" query={searchQuery} /></SectionTitle>
        <textarea
          value={soul}
          onChange={(e) => setSoul(e.target.value)}
          rows={6}
          style={{ ...inputStyle, resize: "vertical", fontFamily: "inherit", lineHeight: "1.5" }}
          placeholder="输入 AI 角色设定..."
        />
        {/* 字数 counter：三档颜色（< 500 muted / 500-1000 amber / >= 1000 red）。
            每次 chat 都把 SOUL 作 system message 注入 —— 长 prompt × 每轮 × 多个
            session 累计成 token 大头。提醒用户掌握长度。 */}
        {(() => {
          const len = soul.length;
          const SOFT = 500;
          const HARD = 1000;
          const color =
            len >= HARD
              ? "var(--pet-tint-red-fg)"
              : len >= SOFT
                ? "var(--pet-tint-yellow-fg)"
                : "var(--pet-color-muted)";
          return (
            <div
              style={{
                marginTop: 4,
                fontSize: 11,
                color,
                fontFamily: "'SF Mono', 'Menlo', monospace",
                textAlign: "right",
              }}
              title={
                len >= HARD
                  ? "prompt 超过 1000 字：每次 chat 都注入这段 → token 成本累积明显，建议精炼"
                  : len >= SOFT
                    ? "prompt 偏长（500+），可以继续但留意 token 成本"
                    : "prompt 长度（按 Unicode code unit 计；含空白）"
              }
            >
              {len} 字
            </div>
          );
        })()}
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
        {/* 重置默认：armed 二次确认。armed 状态变红 + 文案"再点一次"；超时
            5s 自动撤回。tooltip 强调只清设置不动 SOUL / memory / sessions。 */}
        <button
          type="button"
          onClick={handleResetDefaults}
          disabled={saving}
          title="把 config.yaml 重置成出厂默认。SOUL.md / memory / sessions / butler_history 都不动 —— 只清设置项。"
          style={{
            ...btnStyle,
            background: resetArmed ? "var(--pet-tint-red-fg)" : "var(--pet-color-bg)",
            color: resetArmed ? "#fff" : "var(--pet-color-muted)",
            border: `1px solid ${resetArmed ? "var(--pet-tint-red-fg)" : "var(--pet-color-border)"}`,
            fontWeight: resetArmed ? 700 : 400,
          }}
        >
          {resetArmed ? "⚠ 确认重置？" : "重置默认"}
        </button>
        {/* 快照导出 / 导入：让用户把 config + SOUL 复制到剪贴板带到另一台机
            上恢复。本地剪贴板，不上云。 */}
        <button
          type="button"
          onClick={handleExportSnapshot}
          disabled={saving}
          title="把当前 config.yaml + SOUL.md 序列化成 base64 字符串复制到剪贴板。新机贴回点『导入快照』即恢复；不上云。"
          style={{
            ...btnStyle,
            background: "var(--pet-color-bg)",
            color: "var(--pet-color-muted)",
            border: "1px solid var(--pet-color-border)",
            fontWeight: 400,
          }}
        >
          导出快照
        </button>
        <button
          type="button"
          onClick={handleExportSettingsMarkdown}
          disabled={saving}
          title="把当前 config.yaml + SOUL.md 包成可读 markdown 复制到剪贴板（fenced code blocks）。提 issue / 跟 maintainer 讨论 / 喂给 LLM 自查用，比 base64 snapshot 直接阅读。"
          style={{
            ...btnStyle,
            background: "var(--pet-color-bg)",
            color: "var(--pet-color-muted)",
            border: "1px solid var(--pet-color-border)",
            fontWeight: 400,
          }}
        >
          📋 导出 md
        </button>
        <button
          type="button"
          onClick={handleImportSnapshot}
          disabled={saving}
          title="读剪贴板里的 snapshot 字符串覆盖当前 config.yaml + SOUL.md。第一次点弹确认；5 秒内再点真覆盖。"
          style={{
            ...btnStyle,
            background: importArmed ? "var(--pet-tint-red-fg)" : "var(--pet-color-bg)",
            color: importArmed ? "#fff" : "var(--pet-color-muted)",
            border: `1px solid ${importArmed ? "var(--pet-tint-red-fg)" : "var(--pet-color-border)"}`,
            fontWeight: importArmed ? 700 : 400,
          }}
        >
          {importArmed ? "⚠ 确认导入？" : "导入快照"}
        </button>
        {message && (
          <span style={{ fontSize: "13px", color: message.includes("失败") ? "var(--pet-tint-red-fg)" : "var(--pet-tint-green-fg)" }}>
            {message}
          </span>
        )}
      </div>
      {/* 导出 snapshot 后的 secret 警告。独立一行红字，8s 自清。 */}
      {securityNotice && (
        <div
          style={{
            marginTop: 6,
            padding: "6px 10px",
            fontSize: 12,
            color: "var(--pet-tint-red-fg)",
            background: "var(--pet-tint-red-bg)",
            border: "1px solid var(--pet-tint-red-fg)",
            borderRadius: 4,
            wordBreak: "break-word",
          }}
        >
          {securityNotice}
        </div>
      )}
      </>
      )}
      <ImageLightbox
        src={settingsLightboxSrc}
        onClose={() => setSettingsLightboxSrc(null)}
      />
    </div>
  );
}

/// 设置面板各 section 的标题与关键字索引——给 empty-state 用（"全部过滤掉了
/// 吗？"判定）。SearchableSection 内联标题/关键字 props 是真相源；这里复制一
/// 份是因为 React component 渲染顺序和"是否有任意命中"是两个独立查询，不复
/// 用一份内存又没好办法。新增 section 时同步加一行即可（漏加只影响 empty-
/// state 表现，不影响主流程渲染）。
const SETTINGS_SECTION_INDEX: ReadonlyArray<readonly [string, readonly string[]]> = [
  ["本地数据目录", ["data", "dir", "path", "本地", "目录", "memory", "session", "config", "finder"]],
  ["Live2D 模型", ["live2d", "model", "motion", "miku", "映射", "动作"]],
  ["LLM 配置", ["llm", "api", "key", "model", "openai", "base", "url", "gpt"]],
  ["MCP Servers", ["mcp", "server", "tool", "工具", "服务器"]],
  ["Telegram Bot", ["telegram", "tg", "bot", "token", "username", "机器人"]],
  ["主动开口", ["proactive", "主动", "cooldown", "idle", "quiet", "chatty", "companion", "mute", "heartbeat", "心跳"]],
  ["早安简报", ["morning", "briefing", "早安", "简报", "天气", "日历"]],
  ["工具风险", ["tool", "risk", "审核", "review", "approve", "deny", "风险"]],
  ["记忆整理", ["consolidate", "memory", "整理", "记忆", "stale", "weekly"]],
  ["对话上下文", ["chat", "context", "message", "上下文"]],
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
  background: "var(--pet-tint-yellow-bg)",
  color: "var(--pet-tint-yellow-fg)",
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

/**
 * Raw YAML textarea 上方的搜索条。仅在 raw 模式渲染。
 *
 * 不做 overlay 高亮（textarea 不支持 inline styling，套 absolute pre 既要同步
 * scroll 又要算字体度量）。改用 textarea 原生 selection：找到匹配位置 →
 * setSelectionRange + scrollIntoView，selection 高亮即天然结果反馈。Enter /
 * Ctrl+G 跳下一处；shift+Enter 跳上一处；显 N/M 计数。
 */
/**
 * 主题 pill 双键切换：浅色 / 深色。本地立刻 applyTheme + setStoredTheme，并
 * emit("theme-change") 让桌面 pet / 调试 webview 同步。PanelApp 顶部已经有个
 * 月亮图标快捷 toggle —— 这里把控件露在设置页，让用户更容易找到。
 */
function ThemeToggleRow() {
  // 用 module-level theme helpers；不和 PanelApp 共享 useState（跨组件树）。
  // 渲染态从 getStoredTheme 拉，切换后 setStoredTheme 持久化，PanelApp 通过
  // theme-change listener 同步它的 state。
  const [theme, setTheme] = useState<"light" | "dark">(() => {
    const t =
      typeof window !== "undefined" &&
      window.localStorage?.getItem("pet-theme") === "dark"
        ? "dark"
        : "light";
    return t;
  });
  const apply = async (next: "light" | "dark") => {
    setTheme(next);
    const themeMod = await import("../../theme");
    themeMod.applyTheme(next);
    themeMod.setStoredTheme(next);
    const eventMod = await import("@tauri-apps/api/event");
    void eventMod.emit("theme-change", next);
  };
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
      <div
        style={{
          display: "flex",
          gap: 0,
          padding: 2,
          background: "var(--pet-color-bg)",
          borderRadius: 6,
          border: "1px solid var(--pet-color-border)",
        }}
      >
        {(["light", "dark"] as const).map((t) => {
          const active = theme === t;
          return (
            <button
              key={t}
              type="button"
              onClick={() => apply(t)}
              style={{
                padding: "4px 12px",
                fontSize: 12,
                border: "none",
                borderRadius: 4,
                background: active ? "var(--pet-color-card)" : "transparent",
                color: active ? "var(--pet-color-fg)" : "var(--pet-color-muted)",
                cursor: active ? "default" : "pointer",
                fontWeight: active ? 600 : 400,
              }}
              title={t === "light" ? "浅色主题" : "深色主题"}
            >
              {t === "light" ? "☀️ 浅色" : "🌙 深色"}
            </button>
          );
        })}
      </div>
      <span style={{ fontSize: 11, color: "var(--pet-color-muted)" }}>
        切换立即同步到桌面宠物 / 调试窗口
      </span>
    </div>
  );
}

/**
 * Accent 调色板选择器：5 个色样按钮 + label。选中后立刻 applyTheme 覆盖
 * --pet-color-accent + setStoredAccent 持久化 + emit("accent-change") 让
 * 桌面 / 调试窗口同步。default 沿用既有 sky 蓝（兼容老用户视觉）。
 */
function AccentPickerRow() {
  const [accent, setAccent] = useState<import("../../theme").Accent>(() => {
    if (typeof window === "undefined") return "default";
    try {
      const raw = window.localStorage?.getItem("pet-accent");
      if (raw === "green" || raw === "purple" || raw === "orange" || raw === "rose")
        return raw;
    } catch {
      /* localStorage 不可用 → 默认 */
    }
    return "default";
  });
  const apply = async (next: import("../../theme").Accent) => {
    setAccent(next);
    const themeMod = await import("../../theme");
    themeMod.applyTheme(themeMod.getStoredTheme(), next);
    themeMod.setStoredAccent(next);
    const eventMod = await import("@tauri-apps/api/event");
    void eventMod.emit("accent-change", next);
  };
  // 选项列表 lazy import 避免顶部 import circular（PanelSettings 自顶向
  // 下被 PanelApp 引入，theme module 自己 import 链路无影响，但保留 dynamic
  // import 模式与 ThemeToggleRow 一致便于阅读）。
  const options: Array<{ key: import("../../theme").Accent; label: string; swatch: string }> = [
    { key: "default", label: "蓝", swatch: "#0ea5e9" },
    { key: "green", label: "绿", swatch: "#10b981" },
    { key: "purple", label: "紫", swatch: "#8b5cf6" },
    { key: "orange", label: "橙", swatch: "#f97316" },
    { key: "rose", label: "玫红", swatch: "#f43f5e" },
  ];
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
      <span
        style={{ fontSize: 12, color: "var(--pet-color-muted)", marginRight: 4 }}
        title="主品牌色（active tab / primary button / 高亮链接）。切换立即同步到桌面 / 调试窗口。"
      >
        Accent
      </span>
      <div style={{ display: "flex", gap: 6 }}>
        {options.map((opt) => {
          const active = accent === opt.key;
          return (
            <button
              key={opt.key}
              type="button"
              onClick={() => void apply(opt.key)}
              title={`选 ${opt.label} 作为主色`}
              aria-label={`accent ${opt.label}`}
              style={{
                width: 28,
                height: 28,
                borderRadius: 14,
                border: active ? "2px solid var(--pet-color-fg)" : "2px solid transparent",
                background: opt.swatch,
                cursor: active ? "default" : "pointer",
                padding: 0,
                position: "relative",
                boxShadow: active ? "0 0 0 2px var(--pet-color-card)" : "none",
                fontSize: 11,
                color: "#fff",
                lineHeight: 1,
              }}
            >
              {active ? "✓" : ""}
            </button>
          );
        })}
      </div>
      <span style={{ fontSize: 11, color: "var(--pet-color-muted)" }}>
        立即同步到桌面 / 调试窗口
      </span>
    </div>
  );
}

function YamlSearchBar({
  text,
  textareaRef,
}: {
  text: string;
  textareaRef: React.RefObject<HTMLTextAreaElement | null>;
}) {
  const [query, setQuery] = useState("");
  const [activeIdx, setActiveIdx] = useState(0);

  // 计算所有匹配位置；空 query 返回空数组。query 大小写不敏感。
  const matches = useMemo(() => {
    if (!query) return [];
    const q = query.toLowerCase();
    const lower = text.toLowerCase();
    const out: number[] = [];
    let i = lower.indexOf(q);
    while (i >= 0) {
      out.push(i);
      i = lower.indexOf(q, i + Math.max(1, q.length));
    }
    return out;
  }, [query, text]);

  // query 变化时 reset activeIdx=0 + 自动跳到首匹配（与浏览器原生 find 行为一致）。
  // 用 query 而非 matches.length 作 dep，避免 text 变化时也 reset 让用户失位。
  useEffect(() => {
    if (matches.length === 0) {
      setActiveIdx(0);
      return;
    }
    setActiveIdx(0);
    const ta = textareaRef.current;
    if (!ta) return;
    const start = matches[0];
    const end = start + query.length;
    ta.focus();
    ta.setSelectionRange(start, end);
    const linesBefore = text.slice(0, start).split("\n").length;
    const lineHeight = 19.2;
    const target = (linesBefore - 3) * lineHeight;
    if (target > 0 && target < ta.scrollHeight) {
      ta.scrollTop = target;
    }
    // 这条 effect 写 textareaRef 与 query/matches 联动，故只跟踪 query。
    // matches 是 query+text 派生，跟着 query 变；text 单变时不重跳避免抢光标。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [query]);

  const jumpTo = (idx: number) => {
    const ta = textareaRef.current;
    if (!ta) return;
    const start = matches[idx];
    if (start === undefined) return;
    const end = start + query.length;
    ta.focus();
    ta.setSelectionRange(start, end);
    // textarea selection 滚动靠 scrollTop 调整 —— 估算行高 + 行号定位。lineHeight
    // 约 19.2px（fontSize 12 × 1.6），跨平台略有差异，scrollTop 设到匹配行附近
    // 即可，selectionRange 自带 scrollIntoView 行为可补偿剩余偏差。
    const linesBefore = text.slice(0, start).split("\n").length;
    const lineHeight = 19.2;
    const target = (linesBefore - 3) * lineHeight; // 上面留 3 行 padding
    if (target > 0 && target < ta.scrollHeight) {
      ta.scrollTop = target;
    }
  };

  const goNext = () => {
    if (matches.length === 0) return;
    const next = (activeIdx + 1) % matches.length;
    setActiveIdx(next);
    jumpTo(next);
  };
  const goPrev = () => {
    if (matches.length === 0) return;
    const prev = (activeIdx - 1 + matches.length) % matches.length;
    setActiveIdx(prev);
    jumpTo(prev);
  };

  return (
    <div
      style={{
        display: "flex",
        gap: 6,
        alignItems: "center",
        marginBottom: 6,
      }}
    >
      <input
        type="text"
        value={query}
        onChange={(e) => {
          setQuery(e.target.value);
          setActiveIdx(0);
        }}
        onKeyDown={(e) => {
          // Enter / ⌘G → 下一处；Shift+Enter / ⌘⇧G → 上一处。query 变化已在
          // useEffect 中跳到首匹配，所以这里 Enter 直接前进即可。
          if (e.key === "Enter") {
            e.preventDefault();
            if (e.shiftKey) goPrev();
            else goNext();
          } else if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "g") {
            e.preventDefault();
            if (e.shiftKey) goPrev();
            else goNext();
          } else if (e.key === "Escape") {
            setQuery("");
          }
        }}
        placeholder="搜 YAML（Enter 跳首匹配 / 反复跳下一处；⌘G 同；Shift+Enter / ⌘⇧G 跳上一处）"
        style={{
          flex: 1,
          padding: "6px 10px",
          border: "1px solid var(--pet-color-border)",
          borderRadius: 4,
          fontSize: 12,
          background: "var(--pet-color-card)",
          color: "var(--pet-color-fg)",
          outline: "none",
        }}
      />
      <span
        style={{
          fontSize: 11,
          color: "var(--pet-color-muted)",
          minWidth: 60,
          textAlign: "right",
          fontFamily: "'SF Mono', 'Menlo', monospace",
        }}
      >
        {query.length === 0
          ? ""
          : matches.length === 0
            ? "0 匹配"
            : `${activeIdx + 1} / ${matches.length}`}
      </span>
      <button
        type="button"
        onClick={goPrev}
        disabled={matches.length === 0}
        title="跳上一处（Shift+Enter / ⌘⇧G）"
        style={{
          padding: "4px 8px",
          border: "1px solid var(--pet-color-border)",
          borderRadius: 4,
          background: "var(--pet-color-card)",
          color: "var(--pet-color-fg)",
          cursor: matches.length === 0 ? "default" : "pointer",
          fontSize: 12,
        }}
      >
        ↑
      </button>
      <button
        type="button"
        onClick={goNext}
        disabled={matches.length === 0}
        title="跳下一处（Enter / ⌘G）"
        style={{
          padding: "4px 8px",
          border: "1px solid var(--pet-color-border)",
          borderRadius: 4,
          background: "var(--pet-color-card)",
          color: "var(--pet-color-fg)",
          cursor: matches.length === 0 ? "default" : "pointer",
          fontSize: 12,
        }}
      >
        ↓
      </button>
    </div>
  );
}

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

  const statusDot = status?.connected ? "var(--pet-tint-green-fg)" : status?.error ? "var(--pet-tint-red-fg)" : "var(--pet-color-muted)";
  const statusLabel = status?.connected
    ? "已连接"
    : status?.error === "Disabled"
      ? "已禁用"
      : status?.error
        ? "连接失败"
        : "未连接";

  return (
    <div style={{ ...mcpCardStyle, borderColor: status?.error && status.error !== "Disabled" ? "color-mix(in srgb, var(--pet-tint-red-fg) 40%, transparent)" : "var(--pet-color-border)" }}>
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
              color: status?.connected ? "var(--pet-tint-green-fg)" : status?.error && status.error !== "Disabled" ? "var(--pet-tint-red-fg)" : "var(--pet-color-muted)",
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
  padding: "22px 24px 24px",
  height: "100%",
  overflowY: "auto",
};

const sectionStyle: React.CSSProperties = {
  marginBottom: "18px",
  padding: "18px 20px",
  // 与 .pet-card-elev 同语言：顶端 accent 极淡渐变，让每个 section 卡片有
  // 一点温度而非纯白板。border / shadow 保持。
  background:
    "linear-gradient(180deg, color-mix(in srgb, var(--pet-color-accent) 3%, var(--pet-color-card)) 0%, var(--pet-color-card) 55%)",
  border: "1px solid var(--pet-color-border)",
  borderRadius: "12px",
  boxShadow: "var(--pet-shadow-sm)",
};

const labelStyle: React.CSSProperties = {
  display: "block",
  fontSize: "12px",
  color: "var(--pet-color-muted)",
  marginBottom: "5px",
  fontWeight: 500,
  letterSpacing: 0.1,
};

const inputStyle: React.CSSProperties = {
  width: "100%",
  padding: "9px 12px",
  borderRadius: "8px",
  border: "1px solid var(--pet-color-border)",
  fontSize: "13px",
  outline: "none",
  color: "var(--pet-color-fg)",
  boxSizing: "border-box",
  background: "var(--pet-color-card)",
  transition: "border-color 140ms ease-out, box-shadow 140ms ease-out",
};

const btnStyle: React.CSSProperties = {
  padding: "9px 24px",
  borderRadius: "8px",
  border: "none",
  background: "var(--pet-color-accent)",
  color: "#fff",
  fontSize: "14px",
  fontWeight: 600,
  letterSpacing: 0.3,
  cursor: "pointer",
  boxShadow:
    "0 3px 10px color-mix(in srgb, var(--pet-color-accent) 28%, transparent)",
};

const btnSmallStyle: React.CSSProperties = {
  padding: "6px 14px",
  borderRadius: "6px",
  border: "none",
  color: "#fff",
  fontSize: "12px",
  fontWeight: 600,
  letterSpacing: 0.2,
  cursor: "pointer",
  whiteSpace: "nowrap",
};

const btnDangerStyle: React.CSSProperties = {
  padding: "2px 8px",
  borderRadius: "4px",
  border: "none",
  background: "var(--pet-tint-red-fg)",
  color: "#fff",
  fontSize: "11px",
  fontWeight: 600,
  cursor: "pointer",
};

const mcpCardStyle: React.CSSProperties = {
  border: "1px solid var(--pet-color-border)",
  borderRadius: "10px",
  padding: "12px 14px",
  marginBottom: "10px",
  background: "var(--pet-color-bg)",
  boxShadow: "var(--pet-shadow-sm)",
};

const toolBadgeStyle: React.CSSProperties = {
  display: "inline-block",
  padding: "2px 9px",
  borderRadius: 999,
  background: "var(--pet-tint-blue-bg)",
  color: "var(--pet-tint-blue-fg)",
  fontSize: "11px",
  fontFamily: "'SF Mono', 'Menlo', monospace",
  fontWeight: 600,
  letterSpacing: 0.2,
  border:
    "1px solid color-mix(in srgb, var(--pet-tint-blue-fg) 18%, transparent)",
};
