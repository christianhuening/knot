import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { useNavigate, useParams } from "react-router-dom";

import { useUi } from "../../stores/ui";
import { grantsApi } from "../docs/grants.api";
import { workspaceApi } from "../workspace/workspace.api";

export default function PermissionsDialog() {
  const { id } = useParams<{ id: string }>();
  const nav = useNavigate();
  const qc = useQueryClient();
  const notify = useUi((s) => s.notify);

  const grants = useQuery({
    queryKey: ["grants", id],
    queryFn: () => grantsApi.list(id!),
    enabled: Boolean(id),
  });
  const members = useQuery({
    queryKey: ["members"],
    queryFn: () => workspaceApi.listMembers(),
  });

  const [addUser, setAddUser] = useState("");
  const [addRole, setAddRole] = useState<"owner" | "editor" | "viewer">("viewer");
  const [addInherit, setAddInherit] = useState(true);

  const add = useMutation({
    mutationFn: async () =>
      grantsApi.put(id!, `user:${addUser}`, addRole, addInherit),
    onSuccess: async (r) => {
      if ("error" in r) notify("error", "Couldn't add grant");
      else {
        setAddUser("");
        await qc.invalidateQueries({ queryKey: ["grants", id] });
      }
    },
  });
  const remove = useMutation({
    mutationFn: async (principal: string) => grantsApi.remove(id!, principal),
    onSuccess: async (r) => {
      if ("error" in r) notify("error", "Couldn't remove grant");
      else await qc.invalidateQueries({ queryKey: ["grants", id] });
    },
  });

  if (!id) return null;

  return (
    <div
      role="dialog"
      data-testid="permissions-dialog"
      style={{
        position: "fixed",
        inset: 0,
        background: "rgba(0,0,0,0.4)",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        zIndex: 30,
      }}
      onClick={() => { void nav(-1); }}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        style={{
          background: "white",
          padding: 24,
          minWidth: 420,
          borderRadius: 6,
          maxHeight: "80vh",
          overflow: "auto",
        }}
      >
        <h2>Permissions</h2>
        <p style={{ color: "#666", marginBottom: 16 }}>Explicit grants on this document.</p>
        <table style={{ width: "100%", borderCollapse: "collapse", marginBottom: 16 }}>
          <thead>
            <tr>
              <th style={{ textAlign: "left", padding: 6 }}>Principal</th>
              <th style={{ textAlign: "left", padding: 6 }}>Role</th>
              <th style={{ textAlign: "left", padding: 6 }}>Inherits</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            {grants.data && "ok" in grants.data && grants.data.ok.map((g) => (
              <tr key={g.principal} data-testid={`grant-${g.principal}`}>
                <td style={{ padding: 6 }}>{g.principal}</td>
                <td style={{ padding: 6 }}>{g.role}</td>
                <td style={{ padding: 6 }}>{g.inherit ? "yes" : "no"}</td>
                <td style={{ padding: 6 }}>
                  <button onClick={() => remove.mutate(g.principal)}>Remove</button>
                </td>
              </tr>
            ))}
            {grants.data && "ok" in grants.data && grants.data.ok.length === 0 && (
              <tr>
                <td colSpan={4} style={{ padding: 6, color: "#888" }}>
                  No explicit grants. Effective role comes from workspace + ancestor inherits.
                </td>
              </tr>
            )}
          </tbody>
        </table>

        <h3>Add</h3>
        <div style={{ display: "flex", gap: 8, marginBottom: 12 }}>
          <select
            data-testid="grant-user"
            value={addUser}
            onChange={(e) => setAddUser(e.target.value)}
          >
            <option value="">Choose…</option>
            {members.data && "ok" in members.data && members.data.ok.map((m) => (
              <option key={m.user_id} value={m.user_id}>
                {m.display_name} ({m.email})
              </option>
            ))}
          </select>
          <select
            data-testid="grant-role"
            value={addRole}
            onChange={(e) => setAddRole(e.target.value as typeof addRole)}
          >
            <option value="viewer">Viewer</option>
            <option value="editor">Editor</option>
            <option value="owner">Owner</option>
          </select>
          <label style={{ display: "flex", alignItems: "center", gap: 4 }}>
            <input
              type="checkbox"
              checked={addInherit}
              onChange={(e) => setAddInherit(e.target.checked)}
            />
            Inherit
          </label>
          <button
            data-testid="grant-add"
            disabled={!addUser}
            onClick={() => add.mutate()}
          >
            Add
          </button>
        </div>

        <div style={{ display: "flex", justifyContent: "flex-end" }}>
          <button onClick={() => { void nav(-1); }}>Close</button>
        </div>
      </div>
    </div>
  );
}
