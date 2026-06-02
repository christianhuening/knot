import { useQueryClient } from "@tanstack/react-query";
import React, { useState } from "react";
import { useLocation, useNavigate } from "react-router-dom";

import { authApi } from "../../auth/session.api";

export default function LoginPage() {
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const nav = useNavigate();
  const loc = useLocation();
  const qc = useQueryClient();

  const from = ((loc.state as { from?: string } | null)?.from) ?? "/";

  async function onSubmit(e: React.FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError(null);
    const r = await authApi.login(email, password);
    setBusy(false);
    if ("error" in r) {
      setError(r.error.code === "auth.invalid_credentials"
        ? "Invalid email or password."
        : "Login failed.");
      return;
    }
    await qc.invalidateQueries({ queryKey: ["session"] });
    await nav(from, { replace: true });
  }

  return (
    <main
      style={{
        maxWidth: 380,
        margin: "10vh auto",
        padding: 24,
        fontFamily: "system-ui, sans-serif",
      }}
    >
      <h1 style={{ marginBottom: 24 }}>Sign in to knot</h1>
      <form data-testid="login-form" onSubmit={(e) => { void onSubmit(e); }}>
        <label style={{ display: "block", marginBottom: 12 }}>
          <span style={{ display: "block", marginBottom: 4 }}>Email</span>
          <input
            data-testid="login-email"
            type="email"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
            required
            style={{ width: "100%", padding: 8 }}
          />
        </label>
        <label style={{ display: "block", marginBottom: 16 }}>
          <span style={{ display: "block", marginBottom: 4 }}>Password</span>
          <input
            data-testid="login-password"
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            required
            style={{ width: "100%", padding: 8 }}
          />
        </label>
        {error && (
          <p data-testid="login-error" style={{ color: "#b00020", marginBottom: 12 }}>
            {error}
          </p>
        )}
        <button
          data-testid="login-submit"
          type="submit"
          disabled={busy}
          style={{ width: "100%", padding: 10 }}
        >
          {busy ? "Signing in…" : "Sign in"}
        </button>
      </form>
      <p style={{ marginTop: 24, textAlign: "center" }}>
        <a href="/auth/oidc/login">Sign in with SSO</a>
      </p>
      <p style={{ marginTop: 12, textAlign: "center" }}>
        <a href="/setup">First-run setup</a>
      </p>
    </main>
  );
}
