import { useEffect, useRef, useState } from "react";

interface Props {
  modelPath: string;
}

export function Live2DCharacter({ modelPath }: Props) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [status, setStatus] = useState("initializing...");

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    let disposed = false; // component unmounted or modelPath changed
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
          width: 300,
          height: 350,
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

        const scale = Math.min(
          (app.screen.width * 0.65) / model.width,
          (app.screen.height * 0.75) / model.height,
        );
        model.scale.set(scale);
        model.anchor.set(0.5, 0.5);
        model.x = app.screen.width / 2;
        model.y = app.screen.height * 0.45;

        app.stage.addChild(model as any);
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
    <div className="relative h-[350px] w-full">
      <canvas ref={canvasRef} className="pointer-events-auto h-full w-full bg-transparent" />
      {status && (
        <div
          className={`absolute left-1/2 top-1/2 max-w-[90%] -translate-x-1/2 -translate-y-1/2 break-all rounded-lg bg-white/85 p-3 text-center text-[12px] ${
            status.startsWith("Error") ? "text-red-500" : "text-slate-500"
          }`}
        >
          {status}
        </div>
      )}
    </div>
  );
}
