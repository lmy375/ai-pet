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

        // 054-part1：显式 kick-start Idle motion 让 pet figure 不再静态。pixi-
        // live2d-display 的 motion manager 在 model 入 stage 后理应自动巡回
        // Idle group，但实测启动后保持完全静止；显式调一次 Idle (priority 1
        // = IDLE) 启动巡回，随后 motion manager 会自然循环 Idle group 内的
        // 多条 motion。priority 1 让 Tap / Flick / Flick3（priority 2）能正
        // 常打断 idle。空 Idle group 或加载异常时 try/catch 静默吞。
        try {
          (model as any).motion("Idle", undefined, 1);
        } catch (e) {
          console.debug("Live2D idle kick-start failed:", e);
        }

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
    // 高度由父级控制（App.tsx 给 Live2D 区显式 height），让父级布局能根据
    // 窗口高度调控 Live2D / ChatMini / ChatPanel 三段比例。Live2DCharacter
    // 自己只负责把 canvas 撑满父级宽高。
    <div style={{ position: "relative", width: "100%", height: "100%" }}>
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
            color: isError ? "var(--pet-tint-red-fg)" : "var(--pet-color-muted)",
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
