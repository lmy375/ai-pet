import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { AppSettings } from "../../hooks/useSettings";

export function PanelSettings() {
  const [form, setForm] = useState<AppSettings>({
    live_2d_model_path: "",
    api_base: "",
    api_key: "",
    model: "",
  });
  const [soul, setSoul] = useState("");
  const [loaded, setLoaded] = useState(false);
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState("");

  useEffect(() => {
    Promise.all([
      invoke<AppSettings>("get_settings"),
      invoke<string>("get_soul"),
    ]).then(([s, soulContent]) => {
      setForm(s);
      setSoul(soulContent);
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

  if (!loaded) return <div style={containerStyle}>加载中...</div>;

  return (
    <div style={containerStyle}>
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
    </div>
  );
}

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
