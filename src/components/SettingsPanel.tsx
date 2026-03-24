import { useState } from "react";
import type { AppSettings } from "../hooks/useSettings";

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
          width: "260px",
          maxHeight: "420px",
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
