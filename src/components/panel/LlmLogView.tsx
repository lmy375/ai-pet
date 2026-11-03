import { useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "../ui/Button";
import { ImageLightbox } from "../ui/ImageLightbox";
import { useI18n } from "../../i18n";
import { RefreshIcon } from "../Icons";
import { usePolling } from "../../hooks/usePolling";
import { LogList } from "./llmLog/LogList";
import type { LlmEntry, LlmMeta } from "./llmLog/types";

export function LlmLogView() {
  const { t } = useI18n();
  const [metas, setMetas] = useState<LlmMeta[]>([]);
  const [expandedId, setExpandedId] = useState<string | null>(null);
  // Bodies are fetched on demand and cached; `null` means the file is gone
  // (compaction dropped it after the index line was read).
  const [bodies, setBodies] = useState<Record<string, LlmEntry | null>>({});
  const [zoomed, setZoomed] = useState<string | null>(null);

  // The index is one small record per conversation, so polling it is cheap —
  // the messages stay on disk until a row is actually opened.
  const fetchIndex = async () => {
    try {
      setMetas(await invoke<LlmMeta[]>("get_llm_index"));
    } catch (e) {
      console.error("Failed to fetch LLM log index:", e);
    }
  };

  usePolling(fetchIndex, 2000);

  const toggle = useCallback(
    async (id: string) => {
      if (expandedId === id) {
        setExpandedId(null);
        return;
      }
      setExpandedId(id);
      if (id in bodies) return;
      try {
        const entry = await invoke<LlmEntry | null>("get_llm_entry", { id });
        setBodies((prev) => ({ ...prev, [id]: entry }));
      } catch (e) {
        console.error("Failed to fetch LLM log entry:", e);
        setBodies((prev) => ({ ...prev, [id]: null }));
      }
    },
    [expandedId, bodies],
  );

  return (
    <div className="flex h-full flex-col bg-surface-soft">
      {/* Toolbar */}
      <div className="flex shrink-0 items-center gap-2 border-b border-line/70 bg-surface px-4 py-2.5">
        <Button variant="ghost" size="sm" onClick={fetchIndex}>
          <RefreshIcon className="h-4 w-4" />
          {t("common.refresh")}
        </Button>
        <span className="flex-1" />
        <span className="text-[12px] text-ink-faint">{t("llm.recordCount", { count: metas.length })}</span>
      </div>

      {/* Log entries */}
      <LogList
        metas={metas}
        expandedId={expandedId}
        bodies={bodies}
        onToggle={toggle}
        onZoom={setZoomed}
      />
      {zoomed && <ImageLightbox src={zoomed} onClose={() => setZoomed(null)} />}
    </div>
  );
}