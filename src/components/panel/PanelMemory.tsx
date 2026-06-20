import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Card } from "../ui/Card";
import { TextArea } from "../ui/fields";
import { Button } from "../ui/Button";
import { StatusText } from "../ui/StatusText";
import { ExternalLinkIcon } from "../Icons";

/**
 * Memory tab: view and edit the pet's always-present markdown files
 * (SOUL.md / USER.md / MEMORY.md / HEARTBEAT.md) and open the memory folder.
 *
 * The pet maintains USER.md / MEMORY.md / HEARTBEAT.md itself (during chat or on
 * a scheduled heartbeat), so a file can change while this tab is open. To avoid
 * clobbering those writes with stale content, edits save on blur ONLY when the
 * textarea actually differs from the last loaded/saved content, and focusing a
 * field re-reads the file first.
 */

type FieldKey = "soul" | "user" | "memory" | "heartbeat";

interface FieldDef {
  key: FieldKey;
  title: string;
  getCmd: string;
  saveCmd: string;
  rows: number;
  placeholder: string;
}

const FIELDS: FieldDef[] = [
  { key: "soul", title: "SOUL.md（本质）", getCmd: "get_soul", saveCmd: "save_soul", rows: 6, placeholder: "宠物的本质 / 人格设定..." },
  { key: "user", title: "USER.md（关于主人）", getCmd: "get_user", saveCmd: "save_user", rows: 10, placeholder: "关于主人的事实与偏好（宠物会在对话中自行补充）..." },
  { key: "memory", title: "MEMORY.md（日记）", getCmd: "get_memory", saveCmd: "save_memory", rows: 10, placeholder: "宠物自己的理解与想法（宠物会在对话中自行记录）..." },
  { key: "heartbeat", title: "HEARTBEAT.md（定时任务）", getCmd: "get_heartbeat", saveCmd: "save_heartbeat", rows: 8, placeholder: "宠物的定时任务清单（每次心跳会读它来判断该做什么；宠物也会自行维护）..." },
];

const EMPTY: Record<FieldKey, string> = { soul: "", user: "", memory: "", heartbeat: "" };

export function PanelMemory() {
  const [values, setValues] = useState<Record<FieldKey, string>>(EMPTY);
  const [loaded, setLoaded] = useState(false);
  const [message, setMessage] = useState("");
  // Last on-disk content we loaded or saved, per field. The dirty check and
  // focus-refresh both compare against this baseline.
  const baseline = useRef<Record<FieldKey, string>>({ ...EMPTY });

  useEffect(() => {
    Promise.all(FIELDS.map((f) => invoke<string>(f.getCmd)))
      .then((contents) => {
        const next = { ...EMPTY };
        FIELDS.forEach((f, i) => (next[f.key] = contents[i]));
        setValues(next);
        baseline.current = { ...next };
        setLoaded(true);
      })
      .catch((e) => {
        setMessage(`加载失败: ${e}`);
        setLoaded(true);
      });
  }, []);

  // Save only when the field changed since load/last-save, so opening the tab
  // and clicking through fields never overwrites memory the pet just wrote.
  const saveField = async (f: FieldDef, value: string) => {
    if (value === baseline.current[f.key]) return;
    try {
      await invoke(f.saveCmd, { content: value });
      baseline.current[f.key] = value;
      setMessage("已保存");
    } catch (e: any) {
      setMessage(`保存失败: ${e}`);
    }
  };

  // On focus, pull the latest on-disk content. If the file changed underneath
  // (e.g. the pet wrote to it) and the user has no unsaved local edits, adopt
  // the fresh content so they don't edit and then overwrite a stale version.
  const refreshField = async (f: FieldDef) => {
    try {
      const fresh = await invoke<string>(f.getCmd);
      setValues((prev) => {
        const unchangedOnDisk = fresh === baseline.current[f.key];
        const noLocalEdits = prev[f.key] === baseline.current[f.key];
        if (!unchangedOnDisk && noLocalEdits) {
          baseline.current[f.key] = fresh;
          return { ...prev, [f.key]: fresh };
        }
        return prev;
      });
    } catch {
      // Ignore refresh failures; keep showing the current content.
    }
  };

  const openMemoryDir = async () => {
    try {
      await invoke("open_memory_dir");
    } catch (e: any) {
      setMessage(`打开记忆文件夹失败: ${e}`);
    }
  };

  if (!loaded) {
    return <div className="flex h-full items-center justify-center text-[14px] text-slate-400">加载中...</div>;
  }

  return (
    <div className="h-full overflow-y-auto px-5 py-5">
      <div className="mb-4 flex items-center justify-between">
        <p className="text-[12px] text-slate-500">宠物的长期记忆，跨对话保存</p>
        <Button variant="ghost" size="sm" onClick={openMemoryDir} title="在系统文件管理器中打开记忆文件夹">
          <ExternalLinkIcon className="h-4 w-4" />
          打开记忆文件夹
        </Button>
      </div>

      {FIELDS.map((f) => (
        <Card key={f.key} title={f.title}>
          <TextArea
            value={values[f.key]}
            onChange={(e) => setValues((prev) => ({ ...prev, [f.key]: e.target.value }))}
            onFocus={() => refreshField(f)}
            onBlur={() => saveField(f, values[f.key])}
            rows={f.rows}
            placeholder={f.placeholder}
          />
        </Card>
      ))}

      {message && (
        <StatusText ok={!message.includes("失败")} className="mt-1 text-[13px]">{message}</StatusText>
      )}
    </div>
  );
}
