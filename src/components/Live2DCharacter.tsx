import { useEffect, useRef, useState } from "react";

interface Props {
  modelPath: string;
  onModelReady?: (model: any) => void;
}

export function Live2DCharacter({ modelPath, onModelReady }: Props) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [status, setStatus] = useState("initializing...");

  useEffect(() => {
    if (!canvasRef.current) return;
    let destroyed = false;

    (async () => {
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

        if (destroyed) return;
        setStatus("creating pixi app...");

        const app = new PIXI.Application({
          view: canvasRef.current!,
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

        if (destroyed) {
          app.destroy(true);
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
        onModelReady?.(model);

        // Cleanup on destroy
        const cleanup = () => {
          destroyed = true;
          app.destroy(true);
        };
        (canvasRef.current as any).__cleanup = cleanup;
      } catch (err: any) {
        console.error("Live2D init error:", err);
        if (!destroyed) setStatus(`Error: ${err.message || err}`);
      }
    })();

    return () => {
      destroyed = true;
      (canvasRef.current as any)?.__cleanup?.();
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
