interface IconProps {
  className?: string;
}

export function GearIcon({ className }: IconProps) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
      aria-hidden="true"
    >
      <circle cx="12" cy="12" r="3" />
      <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06A1.65 1.65 0 0 0 15 19.4a1.65 1.65 0 0 0-1 .6 1.65 1.65 0 0 0-.38 1.06V21a2 2 0 1 1-4 0v-.09A1.65 1.65 0 0 0 8 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.6 15a1.65 1.65 0 0 0-.6-1 1.65 1.65 0 0 0-1.06-.38H3a2 2 0 1 1 0-4h.09A1.65 1.65 0 0 0 4.6 8a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.6a1.65 1.65 0 0 0 1-.6 1.65 1.65 0 0 0 .38-1.06V3a2 2 0 1 1 4 0v.09A1.65 1.65 0 0 0 16 4.6a1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9c.14.32.38.6.6 1a1.65 1.65 0 0 0 1.06.38H21a2 2 0 1 1 0 4h-.09A1.65 1.65 0 0 0 19.4 15Z" />
    </svg>
  );
}

export function SendIcon({ className }: IconProps) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
      aria-hidden="true"
    >
      <path d="m22 2-7 20-4-9-9-4Z" />
      <path d="M22 2 11 13" />
    </svg>
  );
}

/* Generic stroked icon wrapper to keep all icons visually consistent. */
function Stroke({ className, children }: IconProps & { children: React.ReactNode }) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
      aria-hidden="true"
    >
      {children}
    </svg>
  );
}

export function ChevronRight({ className }: IconProps) {
  return <Stroke className={className}><path d="m9 18 6-6-6-6" /></Stroke>;
}

export function ChevronDown({ className }: IconProps) {
  return <Stroke className={className}><path d="m6 9 6 6 6-6" /></Stroke>;
}

export function PlusIcon({ className }: IconProps) {
  return <Stroke className={className}><path d="M12 5v14M5 12h14" /></Stroke>;
}

export function RefreshIcon({ className }: IconProps) {
  return (
    <Stroke className={className}>
      <path d="M21 12a9 9 0 1 1-3-6.7L21 8" />
      <path d="M21 3v5h-5" />
    </Stroke>
  );
}

export function ExternalLinkIcon({ className }: IconProps) {
  return (
    <Stroke className={className}>
      <path d="M15 3h6v6" />
      <path d="M10 14 21 3" />
      <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" />
    </Stroke>
  );
}

export function TrashIcon({ className }: IconProps) {
  return (
    <Stroke className={className}>
      <path d="M3 6h18" />
      <path d="M8 6V4a1 1 0 0 1 1-1h6a1 1 0 0 1 1 1v2" />
      <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6" />
    </Stroke>
  );
}

export function WrenchIcon({ className }: IconProps) {
  return (
    <Stroke className={className}>
      <path d="M14.7 6.3a4 4 0 0 0-5.2 5.2L3 18l3 3 6.5-6.5a4 4 0 0 0 5.2-5.2l-2.6 2.6-2.4-.6-.6-2.4 2.6-2.6Z" />
    </Stroke>
  );
}

export function CheckIcon({ className }: IconProps) {
  return <Stroke className={className}><path d="M20 6 9 17l-5-5" /></Stroke>;
}

export function SpinnerIcon({ className }: IconProps) {
  return (
    <svg viewBox="0 0 24 24" fill="none" className={className} aria-hidden="true">
      <circle cx="12" cy="12" r="9" stroke="currentColor" strokeWidth="2" strokeOpacity="0.25" />
      <path d="M21 12a9 9 0 0 0-9-9" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
    </svg>
  );
}

export function TerminalIcon({ className }: IconProps) {
  return (
    <Stroke className={className}>
      <path d="m4 17 6-6-6-6" />
      <path d="M12 19h8" />
    </Stroke>
  );
}

export function FileTextIcon({ className }: IconProps) {
  return (
    <Stroke className={className}>
      <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8Z" />
      <path d="M14 2v6h6" />
      <path d="M8 13h8M8 17h8M8 9h2" />
    </Stroke>
  );
}

export function FilePlusIcon({ className }: IconProps) {
  return (
    <Stroke className={className}>
      <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8Z" />
      <path d="M14 2v6h6" />
      <path d="M12 12v6M9 15h6" />
    </Stroke>
  );
}

export function PencilIcon({ className }: IconProps) {
  return (
    <Stroke className={className}>
      <path d="M12 20h9" />
      <path d="M16.5 3.5a2.12 2.12 0 0 1 3 3L7 19l-4 1 1-4Z" />
    </Stroke>
  );
}

export function AgentIcon({ className }: IconProps) {
  return (
    <Stroke className={className}>
      <rect x="4" y="8" width="16" height="11" rx="2" />
      <path d="M12 3v3M9 13h.01M15 13h.01" />
      <path d="M8 19v1M16 19v1" />
    </Stroke>
  );
}

export function ClockIcon({ className }: IconProps) {
  return (
    <Stroke className={className}>
      <circle cx="12" cy="12" r="9" />
      <path d="M12 7v5l3 2" />
    </Stroke>
  );
}

export function ArrowUpIcon({ className }: IconProps) {
  return <Stroke className={className}><path d="M12 19V5M5 12l7-7 7 7" /></Stroke>;
}

export function ArrowDownIcon({ className }: IconProps) {
  return <Stroke className={className}><path d="M12 5v14M19 12l-7 7-7-7" /></Stroke>;
}

export function PinIcon({ className }: IconProps) {
  return <Stroke className={className}><path d="M12 17v5M9 10.76V5a2 2 0 0 1 2-2h2a2 2 0 0 1 2 2v5.76l1.5 2.24H7.5L9 10.76z" /></Stroke>;
}

export function ImageIcon({ className }: IconProps) {
  return <Stroke className={className}><rect x="3" y="3" width="18" height="18" rx="2" /><circle cx="9" cy="9" r="1.5" /><path d="m21 15-5-5L5 21" /></Stroke>;
}

export function BugIcon({ className }: IconProps) {
  return (
    <Stroke className={className}>
      <path d="M8 2 9.88 3.88M14.12 3.88 16 2" />
      <path d="M9 7.13v-1a3 3 0 1 1 6 0v1" />
      <path d="M12 20a6 6 0 0 0 6-6v-3a4 4 0 0 0-4-4h-4a4 4 0 0 0-4 4v3a6 6 0 0 0 6 6Z" />
      <path d="M12 20v-9M6.5 9C4.6 8.8 3 7.1 3 5M6 13H2M3 21c0-2.1 1.7-3.9 3.8-4" />
      <path d="M20.97 5c0 2.1-1.6 3.8-3.5 4M22 13h-4M17.2 17c2.1.1 3.8 1.9 3.8 4" />
    </Stroke>
  );
}
