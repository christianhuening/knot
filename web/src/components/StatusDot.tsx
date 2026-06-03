export type ConnStatus = "connecting" | "connected" | "offline" | "unauthorised" | "conflict";

const classOf: Record<ConnStatus, string> = {
  connecting: "bg-amber-500",
  connected: "bg-emerald-500",
  offline: "bg-fg-muted",
  unauthorised: "bg-destructive",
  conflict: "bg-destructive",
};

export function StatusDot({ status }: { status: ConnStatus }) {
  return (
    <span
      data-testid="status-dot"
      data-status={status}
      aria-label={`Connection ${status}`}
      title={status}
      className={`inline-block h-2 w-2 rounded-full ${classOf[status]} mr-2`}
    />
  );
}
