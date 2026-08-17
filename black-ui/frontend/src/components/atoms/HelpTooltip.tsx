import { CircleHelp } from "lucide-react";
import { useEffect, useId, useRef, useState } from "react";

export interface HelpContent {
  description: string;
  recommended?: string;
  warning?: string;
}

export function HelpTooltip({ label, content }: { label: string; content: HelpContent }) {
  const [open, setOpen] = useState(false);
  const [position, setPosition] = useState({ left: 0, top: 0, placement: "above" as "above" | "below" });
  const id = useId();
  const root = useRef<HTMLSpanElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);

  const show = () => {
    const rect = trigger.current?.getBoundingClientRect();
    if (!rect) return;
    const width = Math.min(310, window.innerWidth - 38);
    const left = Math.min(Math.max(rect.left + rect.width / 2, 19 + width / 2), window.innerWidth - 19 - width / 2);
    const placement = rect.top > 210 ? "above" : "below";
    setPosition({ left, top: placement === "above" ? rect.top - 8 : rect.bottom + 8, placement });
    setOpen(true);
  };

  useEffect(() => {
    const closeOutside = (event: PointerEvent) => {
      if (!root.current?.contains(event.target as Node)) setOpen(false);
    };
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setOpen(false);
        root.current?.querySelector("button")?.blur();
      }
    };
    const closeOnMovement = () => setOpen(false);
    document.addEventListener("pointerdown", closeOutside);
    document.addEventListener("keydown", closeOnEscape);
    window.addEventListener("resize", closeOnMovement);
    window.addEventListener("scroll", closeOnMovement, true);
    return () => {
      document.removeEventListener("pointerdown", closeOutside);
      document.removeEventListener("keydown", closeOnEscape);
      window.removeEventListener("resize", closeOnMovement);
      window.removeEventListener("scroll", closeOnMovement, true);
    };
  }, []);

  return <span ref={root} className="help-tooltip" data-open={open} onMouseEnter={show} onMouseLeave={() => { if (document.activeElement !== trigger.current) setOpen(false); }} onBlur={(event) => { if (!root.current?.contains(event.relatedTarget as Node | null)) setOpen(false); }}>
    <button ref={trigger} type="button" className="help-tooltip-trigger" aria-label={`Help for ${label}`} aria-expanded={open} aria-describedby={open ? id : undefined} onFocus={show} onClick={show}><CircleHelp size={14} /></button>
    {open ? <span id={id} role="tooltip" className="help-tooltip-content" data-placement={position.placement} style={{ left: position.left, top: position.top }}><strong>{label}</strong><span>{content.description}</span>{content.recommended ? <span><b>Recommended:</b> {content.recommended}</span> : null}{content.warning ? <span className="help-tooltip-warning"><b>Note:</b> {content.warning}</span> : null}</span> : null}
  </span>;
}
