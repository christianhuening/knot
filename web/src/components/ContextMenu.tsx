import { createPortal } from "react-dom";
import { useEffect, useRef } from "react";

export type ContextMenuItem = {
  label: string;
  testId: string;
  destructive?: boolean;
  disabled?: boolean;
  onSelect: () => void;
};

export function ContextMenu({
  x,
  y,
  items,
  onClose,
}: {
  x: number;
  y: number;
  items: ContextMenuItem[];
  onClose: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const onDoc = (e: MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) onClose();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("mousedown", onDoc);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDoc);
      document.removeEventListener("keydown", onKey);
    };
  }, [onClose]);

  return createPortal(
    <div
      ref={ref}
      role="menu"
      data-testid="context-menu"
      style={{
        position: "fixed",
        top: y,
        left: x,
        background: "white",
        border: "1px solid #e5e5e5",
        borderRadius: 4,
        boxShadow: "0 4px 12px rgba(0,0,0,0.1)",
        minWidth: 160,
        padding: 4,
        zIndex: 50,
      }}
    >
      {items.map((it) => (
        <button
          key={it.testId}
          type="button"
          role="menuitem"
          data-testid={it.testId}
          disabled={it.disabled}
          onClick={() => {
            onClose();
            it.onSelect();
          }}
          style={{
            display: "block",
            width: "100%",
            textAlign: "left",
            padding: "6px 10px",
            border: "none",
            background: "transparent",
            color: it.destructive ? "#b00020" : "inherit",
            cursor: it.disabled ? "not-allowed" : "pointer",
          }}
        >
          {it.label}
        </button>
      ))}
    </div>,
    document.body,
  );
}
