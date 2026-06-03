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
      className="fixed z-50 min-w-[160px] py-1 rounded-md bg-surface border border-border shadow-lg"
      style={{ top: y, left: x }}
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
          className={`block w-full text-left text-sm px-3 py-1.5 transition-colors disabled:opacity-40 disabled:cursor-not-allowed ${
            it.destructive
              ? "text-destructive hover:bg-destructive/10"
              : "text-fg hover:bg-muted"
          }`}
        >
          {it.label}
        </button>
      ))}
    </div>,
    document.body,
  );
}
