import { useEffect, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";

interface Props {
  /** Path to the .model3.json. A `/`-prefixed web path is used as-is (bundled);
   *  an absolute filesystem path is loaded via the asset protocol. */
  modelPath: string;
  /** Path to live2dcubismcore.min.js, resolved like modelPath. Callers must
   *  gate on both paths being set — App.tsx shows a "no model" notice instead
   *  of mounting this component when either is empty. */
  corePath: string;
}

// PIXI drawing area. It is deliberately taller than any model needs: the model
// is fitted inside it, so where the feet land depends on the model's aspect
// ratio. `drawnHeight` below reports where they actually landed.
const CANVAS_W = 300;
const CANVAS_H = 350;

// Live2DCubismCore is an IIFE that attaches to `window`. It is loaded here —
// never by index.html — from the configured core path (a bundled `/lib/…` web
// path or an absolute file path), so a user can swap SDK versions without
// rebuilding. Cache by URL so repeated builds (e.g. after a WebGL context
// restore) don't re-inject.
let loadedCoreUrl: string | null = null;
let coreLoadPromise: Promise<void> | null = null;

async function ensureCore(externalUrl: string): Promise<void> {
  if (loadedCoreUrl === externalUrl && (window as any).Live2DCubismCore) return;
  if (coreLoadPromise) return coreLoadPromise;
  const p = new Promise<void>((resolve, reject) => {
    const s = document.createElement("script");
    s.src = externalUrl;
    s.onload = () => {
      loadedCoreUrl = externalUrl;
      resolve();
    };
    s.onerror = () => reject(new Error(`Failed to load Live2D core from ${externalUrl}`));
    document.head.appendChild(s);
  });
  coreLoadPromise = p;
  try {
    await p;
  } finally {
    coreLoadPromise = null;
  }
}

// Resolve a Live2D asset path (core JS or model3.json) to a loadable URL.
// A `/models/` or `/lib/` prefix is a bundled web path served by vite from
// public/ — used as-is. An absolute filesystem path needs protocol routing:
// in dev mode the Tauri asset:// protocol is blocked by WKWebView CORS (page
// origin is http://localhost:1420), so we route through a vite middleware at
// /__live2d__/. In release mode the page itself is served via the asset
// protocol, so convertFileSrc works. encodeURI (not encodeURIComponent) keeps
// slashes intact so relative references in model3.json resolve correctly.
function resolveAssetUrl(path: string): string {
  if (/^(https?:|asset:|tauri:)/.test(path)) return path;
  if (/^\/(models|lib)\//.test(path)) return path;
  if (import.meta.env.DEV) {
    return `/__live2d__/${encodeURI(path)}`;
  }
  return convertFileSrc(path);
}

// Reset the module-level `CubismShader_WebGL` singleton so the next build
// compiles fresh shader programs. That singleton outlives React unmounts (ESM
// module cache) but its `_shaderSets` hold `WebGLProgram` objects belonging to
// the context they were compiled in. A model swap tears down the PIXI app and
// mounts a fresh <canvas> — hence a fresh WebGL context — while the singleton
// still hands out the previous context's programs, so `gl.useProgram()` throws
// INVALID_OPERATION "object does not belong to this context" on every frame and
// the model never appears.
//
// This is not self-healing: the plugin's own reset path
// (`InternalModel.updateWebGLContext`) returns early via
// `if (!this.renderer._clippingManager) return;` when a model has no clipping
// masks, so it never reaches its `_shaderSets = []` line.
//
// Called synchronously right after the old app is destroyed and before the new
// one is constructed. `deleteProgram` runs on the OLD context, which may already
// be lost, hence the try/catch. Also fine on a first build: the singleton does
// not exist yet and `deleteInstance()` is a no-op.
function releaseCubismShaderSingleton(mod: any) {
  try {
    mod?.CubismShader_WebGL?.deleteInstance?.();
  } catch (e) {
    console.warn("Live2D: failed to release Cubism shader singleton:", e);
  }
}

export function Live2DCharacter({ modelPath, corePath }: Props) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [status, setStatus] = useState("initializing...");
  // Height the model actually occupies, measured from the canvas top. The
  // wrapper shrinks to it so the empty canvas below the feet doesn't push the
  // chat box down (and doesn't become dead space when the window collapses).
  const [drawnHeight, setDrawnHeight] = useState<number | null>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;


    let disposed = false; // component unmounted or modelPath changed
    setDrawnHeight(null); // a different model lands its feet somewhere else
    let app: any = null; // current PIXI application
    let model: any = null; // current Live2DModel (an automator on Ticker.shared)
    let building = false; // guard against overlapping (re)builds
    // Set once the plugin module has loaded, so the unmount cleanup below can
    // release the shader singleton even if the build never got that far.
    let live2dMod: any = null;

    const teardown = (mod: any = null) => {
      if (app) {
        // PIXI's ContextSystem.destroy() calls loseContext(), which permanently
        // breaks the canvas's WebGL context. Under React StrictMode the same
        // canvas element is reused across the double-invoked effect, so the next
        // build gets an already-lost context and fails with
        // "useProgram: object does not belong to this context". Null out the
        // extension so destroy() skips that call; the context is released when
        // the canvas is garbage-collected (unmount / key change).
        try {
          const ext = app.renderer?.context?.extensions;
          if (ext) ext.loseContext = null;
        } catch {}
        // The GL context may already be gone here, so destroy can throw.
        try {
          app.destroy(true);
        } catch (e) {
          console.warn("Live2D teardown error (context likely lost):", e);
        }
        app = null;
      }
      // Live2DModel registers an Automator on Ticker.shared, and Application
      // .destroy() only tears down the stage without destroying its children — so
      // the model would keep updating a discarded instance every frame. Destroy
      // the automator to detach: it clears the ticker's update listener plus the
      // `globalpointermove` / `pointertap` listeners, and drops the ticker ref.
      //
      // Deliberately NOT `model.destroy()`: that chain reaches
      // `InternalModel.destroy()` → `renderer.release()`, which does
      // `this._clippingManager.release()` UNCONDITIONALLY — and _clippingManager
      // only exists when `isUsingMasking()` is true. Models without clipping masks
      // (wanko) throw "undefined is not an object" mid-chain, after the automator
      // was already detached but before `super.destroy()` runs, leaving the model
      // half-destroyed. `automator.destroy()` covers the part we actually need
      // and cannot hit that null deref.
      if (model) {
        try {
          model.automator.destroy();
        } catch (e) {
          console.warn("Live2D automator detach error:", e);
        }
        model = null;
      }
      // Release the plugin's cached GL programs AFTER the app is gone, so the
      // ticker can't draw against a half-released singleton. Done on every
      // teardown (not just unmount) so a rebuild onto a new canvas/context —
      // model swap, gallery toggle, context restore — starts from clean shaders.
      if (mod) releaseCubismShaderSingleton(mod);
    };

    // (Re)create the PIXI app and load the model onto the SAME canvas. Used for
    // both the initial build and rebuilding after a WebGL context restore.
    const build = async () => {
      if (disposed || building) return;
      building = true;
      try {
        setStatus("importing pixi.js...");
        const PIXI = await import("pixi.js");
        (window as any).PIXI = PIXI;

        setStatus("checking cubism core...");
        // No bundled fallback: the configured core path is the only source.
        // Callers guarantee it's non-empty; failures surface via the status UI.
        await ensureCore(resolveAssetUrl(corePath));

        setStatus("importing live2d...");
        // Use cubism4-specific entry to avoid cubism2 conflicts. Keep the whole
        // module: teardown() needs it to release the shader singleton.
        const live2d = await import("pixi-live2d-display-lipsyncpatch/cubism4");
        live2dMod = live2d;
        const { Live2DModel } = live2d;

        if (disposed) return;

        // Drop any previous app (e.g. a context-lost one, or the StrictMode
        // double-invoke) before creating a new one, and clear the plugin's
        // shader cache so the new context compiles its own programs.
        teardown(live2d);

        setStatus("creating pixi app...");
        app = new PIXI.Application({
          view: canvas,
          backgroundAlpha: 0,
          width: CANVAS_W,
          height: CANVAS_H,
          autoDensity: true,
          resolution: window.devicePixelRatio || 1,
        });

        const modelUrl = resolveAssetUrl(modelPath);

        setStatus(`loading model: ${modelUrl}...`);
        const loaded = await Live2DModel.from(modelUrl, {
          autoInteract: false,
        });
        model = loaded;

        if (disposed) {
          teardown(live2d);
          return;
        }

        // Read the unscaled size once: PIXI's width/height getters fold in
        // scale, so after scale.set() they no longer describe the model.
        const rawW = loaded.width;
        const rawH = loaded.height;
        const scale = Math.min((app.screen.width * 0.65) / rawW, (app.screen.height * 0.75) / rawH);
        loaded.scale.set(scale);
        loaded.anchor.set(0.5, 0.5);
        loaded.x = app.screen.width / 2;
        loaded.y = app.screen.height * 0.45;

        app.stage.addChild(loaded as any);
        // Anchored at its middle, so the feet sit half a scaled height below y.
        setDrawnHeight(Math.ceil(loaded.y + (rawH * scale) / 2));
        setStatus("");
      } catch (err: any) {
        console.error("Live2D init error:", err);
        if (!disposed) setStatus(`Error: ${err?.message || err}`);
      } finally {
        building = false;
      }
    };

    // The WebGL context can be dropped while the pet is auto-hidden (the window
    // slides offscreen / gets occluded). Without handling this the canvas comes
    // back blank after the pet collapses and re-expands. Listeners live on the
    // canvas (not the app) so they survive teardown/rebuild. See CLAUDE.md.
    const onContextLost = (e: Event) => {
      e.preventDefault(); // required so 'webglcontextrestored' will fire
      console.warn("Live2D WebGL context lost — will rebuild on restore");
      if (!disposed) setStatus("restoring...");
    };
    const onContextRestored = () => {
      console.warn("Live2D WebGL context restored — rebuilding");
      build();
    };
    canvas.addEventListener("webglcontextlost", onContextLost);
    canvas.addEventListener("webglcontextrestored", onContextRestored);

    build();

    return () => {
      disposed = true;
      canvas.removeEventListener("webglcontextlost", onContextLost);
      canvas.removeEventListener("webglcontextrestored", onContextRestored);
      teardown(live2dMod);
    };
  }, [modelPath, corePath]);

  return (
    // overflow-hidden, not a shorter canvas: the canvas keeps its full drawing
    // area (PIXI sizes it inline) and we just crop the transparent strip under
    // the feet, so nothing below it sits beneath an invisible click target.
    <div
      className="relative w-full overflow-hidden"
      style={{ height: drawnHeight ?? CANVAS_H }}
    >
      <canvas ref={canvasRef} className="pointer-events-auto absolute left-0 top-0 bg-transparent" />
      {status && (
        <div
          className={`absolute left-1/2 top-1/2 max-w-[90%] -translate-x-1/2 -translate-y-1/2 break-all rounded-lg bg-surface/85 p-3 text-center text-[12px] ${
            status.startsWith("Error") ? "text-red-500" : "text-ink-soft"
          }`}
        >
          {status}
        </div>
      )}
    </div>
  );
}
