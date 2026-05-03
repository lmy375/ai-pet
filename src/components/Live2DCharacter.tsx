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

  // Iter R49: end user shouldn't see dev-y stages like "importing pixi.js…"
  // (they don't know what that means). Map all non-error init messages to
  // a friendly "正在唤醒…" while keeping `status` itself populated so a dev
  // can still inspect via React DevTools / console. Errors keep the raw
  // detail so debugging info isn't lost.
  const isError = status.startsWith("Error");
  const displayStatus = isError ? status : status ? "正在唤醒…" : "";

  return (
    <div style={{ position: "relative", width: "100%", height: "350px" }}>
      <style>{`
        @keyframes pet-live2d-status-fade-in {
          from { opacity: 0; transform: translate(-50%, calc(-50% + 4px)); }
          to   { opacity: 1; transform: translate(-50%, -50%); }
        }
      `}</style>
      <canvas
        ref={canvasRef}
        style={{
          width: "100%",
          height: "100%",
          background: "transparent",
          pointerEvents: "auto",
        }}
      />
      {displayStatus && (
        <div
          style={{
            position: "absolute",
            top: "50%",
            left: "50%",
            transform: "translate(-50%, -50%)",
            color: isError ? "#e53935" : "#888",
            fontSize: "12px",
            textAlign: "center",
            padding: "12px",
            background: "rgba(255,255,255,0.85)",
            borderRadius: "8px",
            maxWidth: "90%",
            wordBreak: "break-all",
            animation: "pet-live2d-status-fade-in 240ms ease-out",
          }}
        >
          {displayStatus}
        </div>
      )}
    </div>
  );
}
