import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";
import { useState } from "react";

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

  const [pwCurrent, setPwCurrent] = useState("");
  const [pwNew, setPwNew] = useState("");
  const [pwError, setPwError] = useState<string | null>(null);
  const [pwOk, setPwOk] = useState(false);

  const changePw = useMutation({
    mutationFn: async () => authApi.changePassword(pwCurrent, pwNew),
    onMutate: () => { setPwError(null); setPwOk(false); },
    onSuccess: (r) => {
      if ("error" in r) {
        setPwError(
          r.error.code === "auth.invalid_credentials" ? "Current password is wrong."
            : r.error.code === "auth.weak_password" ? "New password must be at least 8 characters."
            : r.error.code === "auth.password_reuse" ? "New password must differ from current."
            : "Couldn't change password.",
        );
        return;
      }
      setPwCurrent(""); setPwNew(""); setPwOk(true);
    },
  });

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
      <section data-testid="change-password" style={{ marginBottom: 24 }}>
        <h2>Change password</h2>
        <form
          onSubmit={(e) => { e.preventDefault(); changePw.mutate(); }}
          style={{ display: "grid", gap: 8, maxWidth: 320 }}
        >
          <input
            data-testid="pw-current"
            type="password"
            placeholder="Current password"
            value={pwCurrent}
            onChange={(e) => setPwCurrent(e.target.value)}
            required
            style={{ padding: 6 }}
          />
          <input
            data-testid="pw-new"
            type="password"
            placeholder="New password (≥ 8 chars)"
            value={pwNew}
            onChange={(e) => setPwNew(e.target.value)}
            required
            minLength={8}
            style={{ padding: 6 }}
          />
          {pwError && <p data-testid="pw-error" style={{ color: "#b00020" }}>{pwError}</p>}
          {pwOk && <p data-testid="pw-ok" style={{ color: "#1f7a1f" }}>Password updated.</p>}
          <button data-testid="pw-submit" type="submit" disabled={changePw.isPending} style={{ padding: 8 }}>
            {changePw.isPending ? "Updating…" : "Update password"}
          </button>
        </form>
      </section>
      <button data-testid="logout" onClick={() => { void logout(); }}>
        Sign out
      </button>
    </main>
  );
}
