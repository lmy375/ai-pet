import { useState } from "react";
import type { AppSettings } from "../hooks/useSettings";
import { NumberField as SharedNumberField } from "./common/NumberField";

interface Props {
  settings: AppSettings;
  soul: string;
  onSave: (settings: AppSettings, soul: string) => void;
  onClose: () => void;
}

export function SettingsPanel({ settings, soul, onSave, onClose }: Props) {
  const [form, setForm] = useState<AppSettings>({ ...settings });
  const [soulText, setSoulText] = useState(soul);
  const [saving, setSaving] = useState(false);

  const handleSave = async () => {
    setSaving(true);
    try {
      onSave(form, soulText);
    } finally {
      setSaving(false);
    }
  };

  return (
    <div
      onMouseDown={(e) => e.stopPropagation()}
      onMouseMove={(e) => e.stopPropagation()}
      onClick={(e) => e.stopPropagation()}
      style={{
        position: "absolute",
        inset: 0,
        background: "rgba(0,0,0,0.5)",
        zIndex: 50,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        backdropFilter: "blur(4px)",
      }}
    >
      <div
        style={{
          background: "#fff",
          borderRadius: "12px",
          padding: "20px",
          width: "300px",
          maxHeight: "560px",
          overflowY: "auto",
          boxShadow: "0 4px 24px rgba(0,0,0,0.2)",
        }}
      >
        <h3 style={{ margin: "0 0 14px", fontSize: "15px", color: "#333", fontWeight: 600 }}>
          设置
        </h3>

        <label style={labelStyle}>Live2D 模型路径</label>
        <input
          value={form.live_2d_model_path}
          onChange={(e) => setForm({ ...form, live_2d_model_path: e.target.value })}
          placeholder="/models/miku/miku.model3.json"
          style={inputStyle}
        />

        <label style={sectionLabel}>LLM API Base</label>
        <input
          value={form.api_base}
          onChange={(e) => setForm({ ...form, api_base: e.target.value })}
          placeholder="https://api.openai.com/v1"
          style={inputStyle}
        />

        <label style={sectionLabel}>API Key</label>
        <input
          type="password"
          value={form.api_key}
          onChange={(e) => setForm({ ...form, api_key: e.target.value })}
          placeholder="sk-..."
          style={inputStyle}
        />

        <label style={sectionLabel}>Model</label>
        <input
          value={form.model}
          onChange={(e) => setForm({ ...form, model: e.target.value })}
          placeholder="gpt-4o-mini"
          style={inputStyle}
        />

        <label style={sectionLabel}>系统提示词 (SOUL.md)</label>
        <textarea
          value={soulText}
          onChange={(e) => setSoulText(e.target.value)}
          placeholder="输入 AI 角色设定..."
          rows={4}
          style={{ ...inputStyle, resize: "vertical", fontFamily: "inherit" }}
        />

        <div style={groupHeaderStyle}>主动开口 (Proactive)</div>

        <label style={checkboxRow}>
          <input
            type="checkbox"
            checked={form.proactive.enabled}
            onChange={(e) =>
              setForm({ ...form, proactive: { ...form.proactive, enabled: e.target.checked } })
            }
          />
          <span>启用宠物主动跟我说话</span>
        </label>

        <div style={twoColRow}>
          <NumberField
            label="检查间隔 (秒)"
            value={form.proactive.interval_seconds}
            min={60}
            onChange={(v) =>
              setForm({ ...form, proactive: { ...form.proactive, interval_seconds: v } })
            }
          />
          <NumberField
            label="冷却 (秒)"
            value={form.proactive.cooldown_seconds}
            min={0}
            onChange={(v) =>
              setForm({ ...form, proactive: { ...form.proactive, cooldown_seconds: v } })
            }
          />
        </div>
        <div style={twoColRow}>
          <NumberField
            label="最少静默 (秒)"
            value={form.proactive.idle_threshold_seconds}
            min={60}
            onChange={(v) =>
              setForm({ ...form, proactive: { ...form.proactive, idle_threshold_seconds: v } })
            }
          />
          <NumberField
            label="键鼠空闲 (秒)"
            value={form.proactive.input_idle_seconds}
            min={0}
            onChange={(v) =>
              setForm({ ...form, proactive: { ...form.proactive, input_idle_seconds: v } })
            }
          />
        </div>
        <div style={twoColRow}>
          <NumberField
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
          <NumberField
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
        <label style={checkboxRow}>
          <input
            type="checkbox"
            checked={form.proactive.respect_focus_mode}
            onChange={(e) =>
              setForm({
                ...form,
                proactive: { ...form.proactive, respect_focus_mode: e.target.checked },
              })
            }
          />
          <span>开启 macOS 勿扰/Focus 时不打扰</span>
        </label>

        <div style={groupHeaderStyle}>记忆整理 (Consolidate)</div>

        <label style={checkboxRow}>
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
          <span>启用后台记忆整理</span>
        </label>
        <div style={twoColRow}>
          <NumberField
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
          <NumberField
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
          <NumberField
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
          <NumberField
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
        <div style={twoColRow}>
          <NumberField
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
        </div>

        <div style={groupHeaderStyle}>对话上下文 (Chat)</div>
        <div style={twoColRow}>
          <NumberField
            label="历史保留条数 (0=不限)"
            value={form.chat.max_context_messages}
            min={0}
            onChange={(v) =>
              setForm({ ...form, chat: { ...form.chat, max_context_messages: v } })
            }
          />
          <div style={{ flex: 1 }} />
        </div>

        <div style={{ display: "flex", gap: "8px", marginTop: "16px", justifyContent: "flex-end" }}>
          <button onClick={onClose} style={btnSecondaryStyle}>取消</button>
          <button onClick={handleSave} disabled={saving} style={btnPrimaryStyle}>
            {saving ? "保存中..." : "保存"}
          </button>
        </div>
      </div>
    </div>
  );
}

// Bind the shared NumberField to this panel's local styles. Call sites stay free of
// style boilerplate; the shared component still owns the input-handling logic.
function NumberField(props: {
  label: string;
  value: number;
  min?: number;
  onChange: (v: number) => void;
}) {
  return <SharedNumberField {...props} labelStyle={labelStyle} inputStyle={inputStyle} />;
}

const labelStyle: React.CSSProperties = {
  display: "block",
  fontSize: "12px",
  color: "#666",
  marginBottom: "4px",
  fontWeight: 500,
};

const sectionLabel: React.CSSProperties = {
  ...labelStyle,
  marginTop: "10px",
};

const inputStyle: React.CSSProperties = {
  width: "100%",
  padding: "8px 10px",
  borderRadius: "8px",
  border: "1px solid #ddd",
  fontSize: "13px",
  outline: "none",
  color: "#333",
  boxSizing: "border-box",
};

const btnPrimaryStyle: React.CSSProperties = {
  padding: "6px 16px",
  borderRadius: "8px",
  border: "none",
  background: "#0ea5e9",
  color: "#fff",
  fontSize: "13px",
  cursor: "pointer",
  fontWeight: 500,
};

const btnSecondaryStyle: React.CSSProperties = {
  padding: "6px 16px",
  borderRadius: "8px",
  border: "1px solid #ddd",
  background: "#fff",
  color: "#666",
  fontSize: "13px",
  cursor: "pointer",
};

const groupHeaderStyle: React.CSSProperties = {
  marginTop: "18px",
  marginBottom: "8px",
  fontSize: "12px",
  fontWeight: 600,
  color: "#0ea5e9",
  textTransform: "uppercase",
  letterSpacing: "0.5px",
  borderTop: "1px solid #eee",
  paddingTop: "12px",
};

const checkboxRow: React.CSSProperties = {
  display: "flex",
  alignItems: "center",
  gap: "8px",
  fontSize: "13px",
  color: "#333",
  marginBottom: "8px",
  cursor: "pointer",
};

const twoColRow: React.CSSProperties = {
  display: "flex",
  gap: "8px",
  marginBottom: "6px",
};
