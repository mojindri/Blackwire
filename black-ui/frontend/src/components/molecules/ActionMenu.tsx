import { MoreVertical } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { IconButton } from "../atoms/IconButton";

export function ActionMenu({
  onEdit,
  onToggle,
  onDelete,
  enabled
}: {
  onEdit: () => void;
  onToggle: () => void;
  onDelete: () => void;
  enabled: boolean;
}) {
  const [open, setOpen] = useState(false);
  const [position, setPosition] = useState({ top: 0, left: 0 });
  const buttonRef = useRef<HTMLButtonElement | null>(null);
  const menuRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!open) return;

    const syncPosition = () => {
      if (!buttonRef.current) return;
      const rect = buttonRef.current.getBoundingClientRect();
      setPosition({
        top: Math.max(12, rect.bottom - 4),
        left: Math.max(12, rect.right - 148)
      });
    };

    const handlePointerDown = (event: MouseEvent) => {
      const target = event.target as Node | null;
      if (buttonRef.current?.contains(target) || menuRef.current?.contains(target)) return;
      setOpen(false);
    };

    syncPosition();
    window.addEventListener("resize", syncPosition);
    window.addEventListener("scroll", syncPosition, true);
    window.addEventListener("mousedown", handlePointerDown);
    return () => {
      window.removeEventListener("resize", syncPosition);
      window.removeEventListener("scroll", syncPosition, true);
      window.removeEventListener("mousedown", handlePointerDown);
    };
  }, [open]);

  const menuStyle = useMemo(
    () => ({
      top: `${position.top}px`,
      left: `${position.left}px`
    }),
    [position.left, position.top]
  );

  const run = (action: () => void) => {
    setOpen(false);
    action();
  };

  return (
    <div className="actions">
      <IconButton
        label="Open actions"
        buttonRef={buttonRef}
        aria-expanded={open}
        aria-haspopup="menu"
        onClick={() => setOpen((current) => !current)}
      >
        <MoreVertical size={16} />
      </IconButton>
      {open ? (
        <div ref={menuRef} className="actions-menu actions-menu-open" style={menuStyle} role="menu">
          <button type="button" onClick={() => run(onEdit)}>
            Edit
          </button>
          <button type="button" onClick={() => run(onToggle)}>
            {enabled ? "Disable" : "Enable"}
          </button>
          <button type="button" className="danger-text" onClick={() => run(onDelete)}>
            Delete
          </button>
        </div>
      ) : null}
    </div>
  );
}
