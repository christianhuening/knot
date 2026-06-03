import { useEffect } from "react";

import { useUi } from "../stores/ui";

const kindBorder: Record<string, string> = {
  error: "border-l-destructive",
  warn: "border-l-amber-500",
  info: "border-l-accent",
};

export function Toast() {
  const toasts = useUi((s) => s.toasts);
  const dismiss = useUi((s) => s.dismiss);

  useEffect(() => {
    const timers = toasts.map((t) => setTimeout(() => dismiss(t.id), 4000));
    return () => { timers.forEach(clearTimeout); };
  }, [toasts, dismiss]);

  if (toasts.length === 0) return null;
  return (
    <div
      data-testid="toast-stack"
      className="fixed bottom-4 right-4 z-50 grid gap-2"
    >
      {toasts.map((t) => (
        <div
          key={t.id}
          data-testid={`toast-${t.kind}`}
          role="status"
          aria-live="polite"
          className={`max-w-sm rounded-md border border-border border-l-4 ${kindBorder[t.kind] ?? "border-l-accent"} bg-surface shadow-lg px-4 py-3 text-sm text-fg`}
          style={{ animation: "slideIn 200ms ease-out" }}
        >
          {t.text}
        </div>
      ))}
    </div>
  );
}
