import { useEffect, useRef, useState } from "react";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";

interface MediaItem {
  path: string;
  kind: "image" | "video";
}

/**
 * Fills the pet window with a rotating slideshow of the chosen gallery folder.
 * Images advance every `intervalSec` seconds; videos play through and advance
 * when they end (so a long clip isn't cut off). Replaces the Live2D character
 * when gallery mode is on.
 */
export function GallerySlideshow({ dir, intervalSec }: { dir: string; intervalSec: number }) {
  const [items, setItems] = useState<MediaItem[]>([]);
  const [index, setIndex] = useState(0);
  // Bumped on every advance so the media element remounts and the timer effect
  // re-runs even when the same index is randomly drawn twice in a row (otherwise
  // a repeat would freeze the slideshow).
  const [tick, setTick] = useState(0);
  const [error, setError] = useState<string | null>(null);

  // Load (and refresh) the media list whenever the directory changes.
  useEffect(() => {
    let cancelled = false;
    setError(null);
    invoke<MediaItem[]>("list_gallery_media", { dir })
      .then((list) => {
        if (cancelled) return;
        setItems(list);
        setIndex(list.length ? Math.floor(Math.random() * list.length) : 0);
        setTick((t) => t + 1);
      })
      .catch((e) => !cancelled && setError(String(e)));
    return () => {
      cancelled = true;
    };
  }, [dir]);

  // Pure random: pick any item each time (may repeat the same one back-to-back).
  const advance = () => {
    setIndex(items.length ? Math.floor(Math.random() * items.length) : 0);
    setTick((t) => t + 1);
  };
  const advanceRef = useRef(advance);
  advanceRef.current = advance;

  const current = items[index];

  // Image timer (videos advance via onEnded instead). Clamp to a sane minimum so
  // a misconfigured 0 doesn't spin. Keyed on `tick` so it restarts on every draw.
  useEffect(() => {
    if (!current || current.kind !== "image") return;
    const ms = Math.max(1, intervalSec || 10) * 1000;
    const t = setTimeout(() => advanceRef.current(), ms);
    return () => clearTimeout(t);
  }, [tick, current, intervalSec]);

  if (error) {
    return (
      <div className="flex h-full w-full items-center justify-center px-4 text-center text-[13px] text-slate-500">
        无法读取图库目录：{error}
      </div>
    );
  }

  if (!current) {
    return (
      <div className="flex h-full w-full items-center justify-center px-4 text-center text-[13px] text-slate-400">
        图库目录中没有图片或视频
      </div>
    );
  }

  const src = convertFileSrc(current.path);

  // Apple-style media: hug the image's real size (so the rounding follows the
  // photo, not a letterboxed box), big soft corners, a diffuse shadow, and a
  // hairline edge to lift it off the transparent background.
  const mediaClass =
    "animate-gallery-fade max-h-full max-w-full rounded-[18px] object-contain shadow-[0_10px_34px_rgba(0,0,0,0.22)] ring-1 ring-black/5";

  return (
    <div className="flex h-full w-full items-center justify-center overflow-hidden bg-transparent p-1.5">
      {current.kind === "video" ? (
        <video
          key={tick}
          src={src}
          autoPlay
          muted
          playsInline
          onEnded={() => advanceRef.current()}
          onError={() => advanceRef.current()}
          className={mediaClass}
        />
      ) : (
        <img
          key={tick}
          src={src}
          alt=""
          onError={() => advanceRef.current()}
          className={mediaClass}
        />
      )}
    </div>
  );
}
