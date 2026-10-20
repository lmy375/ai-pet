import { useRef, useState } from "react";

interface Props {
  side: "left" | "right";
  width: number;
  minWidth: number;
  maxWidth: number;
  onChange: (width: number) => void;
  label: string;
}

export function ResizeHandle({ side, width, minWidth, maxWidth, onChange, label }: Props) {
  const drag = useRef<{ startX: number; startWidth: number } | null>(null);
  const [dragging, setDragging] = useState(false);

  const start = (event: React.PointerEvent<HTMLDivElement>) => {
    event.preventDefault();
    event.currentTarget.setPointerCapture(event.pointerId);
    drag.current = { startX: event.clientX, startWidth: width };
    setDragging(true);
  };

  const move = (event: React.PointerEvent<HTMLDivElement>) => {
    if (!drag.current) return;
    const delta = event.clientX - drag.current.startX;
    const next = drag.current.startWidth + (side === "left" ? delta : -delta);
    onChange(Math.min(maxWidth, Math.max(minWidth, next)));
  };

  const finish = (event: React.PointerEvent<HTMLDivElement>) => {
    if (!drag.current) return;
    drag.current = null;
    setDragging(false);
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
  };

  return (
    <div
      role="separator"
      aria-orientation="vertical"
      aria-label={label}
      title={label}
      onPointerDown={start}
      onPointerMove={move}
      onPointerUp={finish}
      onPointerCancel={finish}
      onLostPointerCapture={finish}
      className={`group relative z-10 flex w-2 shrink-0 cursor-col-resize touch-none items-stretch justify-center ${
        dragging ? "bg-accent/15" : "hover:bg-accent/10"
      }`}
    >
      <span className={`w-px transition-colors ${dragging ? "bg-accent" : "bg-line group-hover:bg-accent/70"}`} />
    </div>
  );
}
