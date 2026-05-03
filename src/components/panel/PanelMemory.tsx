import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

interface MemoryItem {
  title: string;
  description: string;
  detail_path: string;
  created_at: string;
  updated_at: string;
}

interface CategoryData {
  label: string;
  items: MemoryItem[];
}

interface MemoryIndex {
  version: number;
  categories: Record<string, CategoryData>;
}

const CATEGORY_ORDER = ["butler_tasks", "todo", "ai_insights", "user_profile", "general"];

export function PanelMemory() {
  const [index, setIndex] = useState<MemoryIndex | null>(null);
  const [loading, setLoading] = useState(true);
  const [searchKeyword, setSearchKeyword] = useState("");
  const [searchResults, setSearchResults] = useState<
    { category: string; title: string; description: string; detail_path: string }[] | null
  >(null);
  const [editingItem, setEditingItem] = useState<{
    category: string;
    title: string;
    description: string;
    isNew: boolean;
  } | null>(null);
  const [message, setMessage] = useState("");
  const [consolidating, setConsolidating] = useState(false);

  const loadIndex = async () => {
    try {
      const data = await invoke<MemoryIndex>("memory_list", {});
      setIndex(data);
    } catch (e: any) {
      console.error("Failed to load memories:", e);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadIndex();
  }, []);

  const handleSearch = async () => {
    if (!searchKeyword.trim()) {
      setSearchResults(null);
      return;
    }
    try {
      const results = await invoke<
        { category: string; title: string; description: string; detail_path: string }[]
      >("memory_search", { keyword: searchKeyword });
      setSearchResults(results);
    } catch (e: any) {
      setMessage(`搜索失败: ${e}`);
    }
  };

  const handleConsolidate = async () => {
    setConsolidating(true);
    setMessage("正在整理记忆，请稍候…");
    try {
      const status = await invoke<string>("trigger_consolidate");
      setMessage(status);
      await loadIndex();
    } catch (e: any) {
      setMessage(`整理失败: ${e}`);
    } finally {
      setConsolidating(false);
    }
  };

  const handleDelete = async (category: string, title: string) => {
    if (!confirm(`确认删除 "${title}"？`)) return;
    try {
      await invoke("memory_edit", { action: "delete", category, title });
      setMessage("已删除");
      await loadIndex();
      setSearchResults(null);
    } catch (e: any) {
      setMessage(`删除失败: ${e}`);
    }
  };

  const handleSaveEdit = async () => {
    if (!editingItem) return;
    try {
      if (editingItem.isNew) {
        await invoke("memory_edit", {
          action: "create",
          category: editingItem.category,
          title: editingItem.title,
          description: editingItem.description,
        });
        setMessage("已创建");
      } else {
        await invoke("memory_edit", {
          action: "update",
          category: editingItem.category,
          title: editingItem.title,
          description: editingItem.description,
        });
        setMessage("已更新");
      }
      setEditingItem(null);
      await loadIndex();
    } catch (e: any) {
      setMessage(`保存失败: ${e}`);
    }
  };

  if (loading) {
    return <div style={{ padding: 20, color: "#64748b" }}>加载中...</div>;
  }

  const s = {
    container: { padding: 16, overflowY: "auto" as const, height: "100%", fontFamily: "system-ui, sans-serif" },
    section: { marginBottom: 20 },
    sectionTitle: { fontSize: 14, fontWeight: 600, color: "#334155", marginBottom: 8, display: "flex", alignItems: "center", gap: 8 },
    badge: { fontSize: 11, background: "#e2e8f0", color: "#64748b", borderRadius: 10, padding: "1px 8px" },
    item: { padding: "8px 12px", background: "#fff", border: "1px solid #e2e8f0", borderRadius: 6, marginBottom: 6, fontSize: 13 },
    itemTitle: { fontWeight: 600, color: "#1e293b", marginBottom: 2 },
    itemDesc: { color: "#64748b", fontSize: 12, lineHeight: 1.4 },
    itemMeta: { color: "#94a3b8", fontSize: 11, marginTop: 4 },
    btn: { padding: "4px 10px", border: "1px solid #e2e8f0", borderRadius: 4, background: "#fff", color: "#64748b", cursor: "pointer", fontSize: 12 },
    btnDanger: { padding: "4px 10px", border: "1px solid #fecaca", borderRadius: 4, background: "#fff", color: "#ef4444", cursor: "pointer", fontSize: 12 },
    btnPrimary: { padding: "6px 16px", border: "none", borderRadius: 4, background: "#0ea5e9", color: "#fff", cursor: "pointer", fontSize: 13 },
    input: { width: "100%", padding: "6px 10px", border: "1px solid #e2e8f0", borderRadius: 4, fontSize: 13, boxSizing: "border-box" as const },
    textarea: { width: "100%", padding: "6px 10px", border: "1px solid #e2e8f0", borderRadius: 4, fontSize: 13, resize: "vertical" as const, minHeight: 60, boxSizing: "border-box" as const },
    searchRow: { display: "flex", gap: 8, marginBottom: 16 },
    msg: { padding: "6px 12px", background: "#f0fdf4", color: "#166534", borderRadius: 4, fontSize: 12, marginBottom: 12 },
  };

  return (
    <div style={s.container}>
      {message && (
        <div style={s.msg} onClick={() => setMessage("")}>
          {message}
        </div>
      )}

      {/* Search */}
      <div style={s.searchRow}>
        <input
          style={{ ...s.input, flex: 1 }}
          placeholder="搜索记忆..."
          value={searchKeyword}
          onChange={(e) => setSearchKeyword(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && handleSearch()}
        />
        <button style={s.btn} onClick={handleSearch}>
          搜索
        </button>
        {searchResults !== null && (
          <button
            style={s.btn}
            onClick={() => {
              setSearchResults(null);
              setSearchKeyword("");
            }}
          >
            清除
          </button>
        )}
        <button
          style={{
            ...s.btn,
            background: consolidating ? "#94a3b8" : "#8b5cf6",
            color: "#fff",
          }}
          onClick={handleConsolidate}
          disabled={consolidating}
          title="立即让 LLM 检查并整理记忆（合并重复 / 删过期 todo / 清 stale reminder），不必等定时触发。"
        >
          {consolidating ? "整理中…" : "立即整理"}
        </button>
      </div>

      {/* Search results */}
      {searchResults !== null && (
        <div style={s.section}>
          <div style={s.sectionTitle}>
            搜索结果 <span style={s.badge}>{searchResults.length}</span>
          </div>
          {searchResults.length === 0 && (
            <div style={{ color: "#94a3b8", fontSize: 13 }}>未找到匹配项</div>
          )}
          {searchResults.map((r, i) => (
            <div key={i} style={s.item}>
              <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
                <div style={s.itemTitle}>{r.title}</div>
                <span style={s.badge}>{r.category}</span>
              </div>
              <div style={s.itemDesc}>{r.description}</div>
            </div>
          ))}
        </div>
      )}

      {/* Edit modal */}
      {editingItem && (
        <div
          style={{
            position: "fixed",
            inset: 0,
            background: "rgba(0,0,0,0.3)",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            zIndex: 100,
          }}
          onClick={() => setEditingItem(null)}
        >
          <div
            style={{ background: "#fff", borderRadius: 8, padding: 20, width: 400, maxWidth: "90%" }}
            onClick={(e) => e.stopPropagation()}
          >
            <div style={{ fontSize: 15, fontWeight: 600, marginBottom: 12 }}>
              {editingItem.isNew ? "新建记忆" : "编辑记忆"}
            </div>
            <div style={{ marginBottom: 8 }}>
              <label style={{ fontSize: 12, color: "#64748b" }}>分类</label>
              <select
                style={s.input}
                value={editingItem.category}
                onChange={(e) => setEditingItem({ ...editingItem, category: e.target.value })}
                disabled={!editingItem.isNew}
              >
                {CATEGORY_ORDER.map((k) => (
                  <option key={k} value={k}>
                    {index?.categories[k]?.label || k}
                  </option>
                ))}
              </select>
            </div>
            <div style={{ marginBottom: 8 }}>
              <label style={{ fontSize: 12, color: "#64748b" }}>标题</label>
              <input
                style={s.input}
                maxLength={20}
                value={editingItem.title}
                onChange={(e) => setEditingItem({ ...editingItem, title: e.target.value })}
                disabled={!editingItem.isNew}
              />
            </div>
            <div style={{ marginBottom: 12 }}>
              <label style={{ fontSize: 12, color: "#64748b" }}>描述</label>
              <textarea
                style={s.textarea}
                maxLength={300}
                value={editingItem.description}
                onChange={(e) => setEditingItem({ ...editingItem, description: e.target.value })}
              />
            </div>
            <div style={{ display: "flex", gap: 8, justifyContent: "flex-end" }}>
              <button style={s.btn} onClick={() => setEditingItem(null)}>
                取消
              </button>
              <button style={s.btnPrimary} onClick={handleSaveEdit}>
                保存
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Categories */}
      {searchResults === null &&
        index &&
        CATEGORY_ORDER.map((catKey) => {
          const cat = index.categories[catKey];
          if (!cat) return null;
          return (
            <div key={catKey} style={s.section}>
              <div style={s.sectionTitle}>
                {cat.label}
                <span style={s.badge}>{cat.items.length}</span>
                <button
                  style={{ ...s.btn, marginLeft: "auto" }}
                  onClick={() =>
                    setEditingItem({ category: catKey, title: "", description: "", isNew: true })
                  }
                >
                  + 新建
                </button>
              </div>
              {cat.items.length === 0 && (
                <div style={{ color: "#94a3b8", fontSize: 12, paddingLeft: 4 }}>暂无记忆</div>
              )}
              {cat.items.map((item, i) => (
                <div key={i} style={s.item}>
                  <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
                    <div style={s.itemTitle}>{item.title}</div>
                    <div style={{ display: "flex", gap: 4 }}>
                      <button
                        style={s.btn}
                        onClick={() =>
                          setEditingItem({
                            category: catKey,
                            title: item.title,
                            description: item.description,
                            isNew: false,
                          })
                        }
                      >
                        编辑
                      </button>
                      <button style={s.btnDanger} onClick={() => handleDelete(catKey, item.title)}>
                        删除
                      </button>
                    </div>
                  </div>
                  <div style={s.itemDesc}>{item.description}</div>
                  <div style={s.itemMeta}>
                    {item.detail_path} | 更新于 {item.updated_at?.slice(0, 16).replace("T", " ")}
                  </div>
                </div>
              ))}
            </div>
          );
        })}
    </div>
  );
}
