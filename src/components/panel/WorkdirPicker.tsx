import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { homeDir } from "@tauri-apps/api/path";
import { open } from "@tauri-apps/plugin-dialog";
import { FolderIcon } from "../Icons";
import { useI18n } from "../../i18n";

/** `/Users/moon/code/pet` under `/Users/moon` → `~/code/pet`. */
function tilde(path: string, home: string): string {
  if (!home) return path;
  if (path === home) return "~";
  return path.startsWith(`${home}/`) ? `~${path.slice(home.length)}` : path;
}

/** Keep the tail (the part that identifies the directory) when the rail is too
 *  narrow, cutting at a separator: `~/a/b/very/deep/dir` → `…/deep/dir`. */
function shorten(path: string, max = 26): string {
  if (path.length <= max) return path;
  const cut = path.length - max;
  const at = path.indexOf("/", cut);
  return `…${path.slice(at === -1 ? cut : at)}`;
}

/**
 * The working directory the agent runs in, sitting under the session search.
 * It's process state, not a setting: the GUI always starts at `$HOME`, and
 * switching it here applies to every session from the next message on (it's
 * rebuilt into the system prompt each turn).
 */
export function WorkdirPicker() {
  const { t } = useI18n();
  const [dir, setDir] = useState("");
  const [home, setHome] = useState("");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    invoke<string>("get_workdir").then(setDir).catch(() => {});
    homeDir()
      .then((h) => setHome(h.replace(/\/+$/, "")))
      .catch(() => {});
  }, []);

  const pick = async () => {
    setError(null);
    try {
      const picked = await open({ directory: true, multiple: false, defaultPath: dir || undefined });
      if (typeof picked !== "string") return;
      setDir(await invoke<string>("set_workdir", { path: picked }));
    } catch (e: any) {
      setError(String(e));
    }
  };

  const shown = tilde(dir, home);

  return (
    <div className="shrink-0 px-3 pb-2">
      <button
        type="button"
        onClick={pick}
        title={t("chat.workdir.tooltip", { dir: dir || "—" })}
        className="flex w-full items-center gap-1.5 rounded-field border border-line bg-surface-soft px-2.5 py-1.5 text-left transition-colors hover:bg-hover"
      >
        <FolderIcon className="h-3.5 w-3.5 shrink-0 text-ink-faint" />
        <span className="min-w-0 flex-1 truncate text-body text-ink-soft">
          {shown ? shorten(shown) : t("common.loading")}
        </span>
      </button>
      {error && <div className="mt-1 text-meta text-red-600">{error}</div>}
    </div>
  );
}
