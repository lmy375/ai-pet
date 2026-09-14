import { useEffect, useRef, useState } from "react";

interface Props {
  modelPath: string;
}

// PIXI drawing area. It is deliberately taller than any model needs: the model
// is fitted inside it, so where the feet land depends on the model's aspect
// ratio. `drawnHeight` below reports where they actually landed.
const CANVAS_W = 300;
const CANVAS_H = 350;

export function Live2DCharacter({ modelPath }: Props) {
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
    let building = false; // guard against overlapping (re)builds

    const teardown = () => {
      if (app) {
        // The GL context may already be gone here, so destroy can throw.
        try {
          app.destroy(true);
        } catch (e) {
          console.warn("Live2D teardown error (context likely lost):", e);
        }
        app = null;
      }
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
        // Ensure Live2DCubismCore is loaded from the <script> tag
        if (!(window as any).Live2DCubismCore) {
          throw new Error("Live2DCubismCore not found on window. Check that live2dcubismcore.min.js is loaded in index.html.");
        }

        setStatus("importing live2d...");
        // Use cubism4-specific entry to avoid cubism2 conflicts
        const { Live2DModel } = await import(
          "pixi-live2d-display-lipsyncpatch/cubism4"
        );

        if (disposed) return;

        // Drop any previous (e.g. context-lost) app before creating a new one.
        teardown();

        setStatus("creating pixi app...");
        app = new PIXI.Application({
          view: canvas,
          backgroundAlpha: 0,
          width: CANVAS_W,
          height: CANVAS_H,
          autoDensity: true,
          resolution: window.devicePixelRatio || 1,
        });

        setStatus(`loading model: ${modelPath}...`);
        const model = await Live2DModel.from(modelPath, {
          autoInteract: false,
        });

        if (disposed) {
          teardown();
          return;
        }

        // Read the unscaled size once: PIXI's width/height getters fold in
        // scale, so after scale.set() they no longer describe the model.
        const rawW = model.width;
        const rawH = model.height;
        const scale = Math.min((app.screen.width * 0.65) / rawW, (app.screen.height * 0.75) / rawH);
        model.scale.set(scale);
        model.anchor.set(0.5, 0.5);
        model.x = app.screen.width / 2;
        model.y = app.screen.height * 0.45;

        app.stage.addChild(model as any);
        // Anchored at its middle, so the feet sit half a scaled height below y.
        setDrawnHeight(Math.ceil(model.y + (rawH * scale) / 2));
        setStatus("");
      } catch (err: any) {
        console.error("Live2D init error:", err);
        if (!disposed) setStatus(`Error: ${err.message || err}`);
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
      teardown();
    };
  }, [modelPath]);

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
