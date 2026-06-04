export type ConnStatus = "connecting" | "connected" | "offline" | "unauthorised" | "conflict";

/** Connection status PLUS a flag for "all local writes durable so far".
 *  When `status === "connected"` and `pendingBytes === 0`, the user can
 *  safely close the tab — every keystroke has reached the server. While
 *  `pendingBytes > 0`, some edits are still buffered locally; the WS will
 *  push them when the kernel socket drains. */
export type SyncState = { status: ConnStatus; pendingBytes: number };

const dotClass: Record<ConnStatus, string> = {
  connecting: "bg-amber-500",
  connected: "bg-emerald-500",
  offline: "bg-fg-muted",
  unauthorised: "bg-destructive",
  conflict: "bg-destructive",
};

function labelFor({ status, pendingBytes }: SyncState): string {
  if (status === "connected") {
    return pendingBytes > 0 ? "Saving…" : "Saved";
  }
  if (status === "connecting") return "Connecting…";
  if (status === "offline") return "Offline";
  if (status === "unauthorised") return "No access";
  return "Conflict";
}

export function StatusDot({ status }: { status: ConnStatus }) {
  return (
    <span
      data-testid="status-dot"
      data-status={status}
      aria-label={`Connection ${status}`}
      title={status}
      className={`inline-block h-2 w-2 rounded-full ${dotClass[status]} mr-2`}
    />
  );
}

/** Verbose status indicator combining the dot + a textual label.
 *  Used in the doc-page header and the board modal header so the user
 *  knows whether their local edits have made it to the server. */
export function SyncStatus({ sync }: { sync: SyncState }) {
  const cls = sync.status === "connected" && sync.pendingBytes > 0
    ? "bg-amber-500"
    : dotClass[sync.status];
  const label = labelFor(sync);
  return (
    <span
      data-testid="sync-status"
      data-status={sync.status}
      data-pending={sync.pendingBytes > 0 ? "1" : "0"}
      aria-label={`Editor: ${label}`}
      className="inline-flex items-center gap-1.5 text-[11px] text-fg-muted"
    >
      <span className={`inline-block h-2 w-2 rounded-full ${cls}`} aria-hidden />
      {label}
    </span>
  );
}
