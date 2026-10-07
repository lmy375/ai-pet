/**
 * Built-in default pet: raster character art shipped in `public/pet/*.png`
 * (transparent background, one image per emotion). This replaced an earlier
 * hand-drawn SVG — the generated raster art looks far better and needs no
 * vector tracing.
 *
 * Custom art: point `pet_image_dir` (config.yaml) at a directory with images
 * named by emotion stem — `idle.png`, `thinking.jpg`, … (any image extension,
 * case-insensitive stem). The settings UI validates at pick time that all three
 * emotions exist, and rejects incomplete directories — no fallback. Likewise
 * at render time: with a directory configured, a missing emotion renders
 * nothing rather than silently swapping in built-in art. Only an *empty*
 * `pet_image_dir` means "use the bundled art".
 *
 * Motion layering:
 *  - breathing: the shared `.animate-breath` on the React wrapper (whole bob);
 *  - the image swaps src per emotion for the facial expression;
 *  - CSS adds a subtle head tilt (thinking) and a little hop (happy).
 *
 * Adding an emotion later = drop another `public/pet/<name>.png` and extend the
 * Emotion union; the wrapper and motion CSS stay as they are.
 */

import { useEffect, useState } from "react";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";

export type Emotion = "idle" | "thinking" | "happy";

const EMOTIONS: readonly Emotion[] = ["idle", "thinking", "happy"];

/** Directory scan result: emotion → absolute file path (only matched stems). */
type EmotionFiles = Partial<Record<Emotion, string>>;

/** List the image files in `dir` and map them to emotions by file stem.
 *  `ready` is false until the scan settles, so callers can tell "still
 *  scanning" from "scanned, nothing matched". An unreadable dir or scan error
 *  is treated as an empty match set — with a configured directory that renders
 *  nothing (by design: no silent fallback), empty dir stays on built-in art. */
function useEmotionFiles(dir: string): { files: EmotionFiles; ready: boolean } {
  const [files, setFiles] = useState<EmotionFiles>({});
  const [ready, setReady] = useState(false);
  useEffect(() => {
    setFiles({});
    setReady(false);
    if (!dir.trim()) return;
    let cancelled = false;
    invoke<{ path: string; kind: string }[]>("list_gallery_media", { dir })
      .then((items) => {
        if (cancelled) return;
        const map: EmotionFiles = {};
        for (const item of items) {
          if (item.kind !== "image") continue;
          const stem = item.path
            .split(/[\\/]/)
            .pop()!
            .replace(/\.[^.]+$/, "")
            .toLowerCase();
          if ((EMOTIONS as readonly string[]).includes(stem)) {
            map[stem as Emotion] = item.path;
          }
        }
        setFiles(map);
        setReady(true);
      })
      .catch(() => {
        if (!cancelled) setReady(true);
      });
    return () => {
      cancelled = true;
    };
  }, [dir]);
  return { files, ready };
}

export function AnimatedPet({ emotion, dir = "" }: { emotion: Emotion; dir?: string }) {
  const { files, ready } = useEmotionFiles(dir);
  const configured = !!dir.trim();

  // Custom dir: only its own files, nothing borrowed from the built-in art —
  // an emotion whose file vanished renders blank (the settings UI validated
  // the directory at pick time, so this only happens on external deletion).
  // Empty dir: the bundled web root (Vite publicDir → served at /, packaged
  // into the Tauri app), same mechanism as the bundled /models and /lib assets.
  if (configured && !ready) return null;
  const src = configured
    ? files[emotion]
      ? convertFileSrc(files[emotion]!)
      : null
    : `/pet/${emotion}.png`;
  if (!src) return null;

  return (
    <img
      src={src}
      alt="pet"
      draggable={false}
      data-emotion={emotion}
      className="builtin-pet block h-auto w-full select-none"
    />
  );
}
