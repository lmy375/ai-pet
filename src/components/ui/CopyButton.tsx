import { useEffect, useRef, useState, type MouseEvent } from "react";
import { IconActionButton } from "./IconButton";
import { CopyIcon, CheckIcon } from "../Icons";
import { useI18n } from "../../i18n";

/** Put `text` on the clipboard. The async API is the normal path; WKWebView can
 *  refuse it (clipboard permission), so fall back to the selection-based copy. */
async function writeClipboard(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    const ta = document.createElement("textarea");
    ta.value = text;
    ta.style.position = "fixed";
    ta.style.opacity = "0";
    document.body.appendChild(ta);
    ta.select();
    const ok = document.execCommand("copy");
    ta.remove();
    return ok;
  }
}

/** Copy-to-clipboard icon button: the glyph turns into a check for a moment
 *  after a successful copy (the only feedback — no toast). */
export function CopyButton({ text, className = "" }: { text: string; className?: string }) {
  const { t } = useI18n();
  const [copied, setCopied] = useState(false);
  const timer = useRef<number | undefined>(undefined);

  useEffect(() => () => window.clearTimeout(timer.current), []);

  const handleClick = async (e: MouseEvent<HTMLButtonElement>) => {
    e.stopPropagation();
    if (!(await writeClipboard(text))) return;
    setCopied(true);
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => setCopied(false), 1500);
  };

  return (
    <IconActionButton
      type="button"
      size="sm"
      title={t(copied ? "common.copied" : "common.copy")}
      aria-label={t("common.copy")}
      onClick={handleClick}
      className={className}
    >
      {copied ? <CheckIcon className="h-3.5 w-3.5 text-accent" /> : <CopyIcon className="h-3.5 w-3.5" />}
    </IconActionButton>
  );
}
