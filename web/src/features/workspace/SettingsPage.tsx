import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";

import { authApi } from "../../auth/session.api";
import { useSession } from "../../auth/SessionContext";
import { useUi } from "../../stores/ui";

import { workspaceApi } from "./workspace.api";

export default function SettingsPage() {
  const ws = useQuery({ queryKey: ["workspace"], queryFn: () => workspaceApi.get() });
  const session = useSession();
  const qc = useQueryClient();
  const nav = useNavigate();
  const notify = useUi((s) => s.notify);

  async function logout() {
    const r = await authApi.logout();
    if ("error" in r) {
      notify("error", "Logout failed");
      return;
    }
    qc.removeQueries();
    await nav("/login", { replace: true });
  }

  return (
    <main style={{ padding: 24, fontFamily: "system-ui, sans-serif" }}>
      <h1>Settings</h1>
      {ws.data && "ok" in ws.data && (
        <section data-testid="workspace-info" style={{ marginBottom: 24 }}>
          <p><strong>Workspace:</strong> {ws.data.ok.name} ({ws.data.ok.slug})</p>
        </section>
      )}
      {session.data && "ok" in session.data && (
        <section data-testid="user-info" style={{ marginBottom: 24 }}>
          <p><strong>Signed in as:</strong> {session.data.ok.display_name} ({session.data.ok.email})</p>
          <p><strong>Role:</strong> {session.data.ok.role}</p>
        </section>
      )}
      <button data-testid="logout" onClick={() => { void logout(); }}>
        Sign out
      </button>
    </main>
  );
}
