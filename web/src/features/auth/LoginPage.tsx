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
    <main className="min-h-dvh flex items-center justify-center px-4 bg-bg">
      <div className="w-full max-w-sm bg-surface border border-border rounded-lg shadow-sm p-6">
        <h1 className="text-xl font-semibold text-fg mb-1">Sign in to knot</h1>
        <p className="text-sm text-fg-muted mb-6">Welcome back</p>
        <form data-testid="login-form" onSubmit={(e) => { void onSubmit(e); }} className="space-y-4">
          <label className="block">
            <span className="block text-[13px] font-medium text-fg mb-1">Email</span>
            <input
              data-testid="login-email"
              type="email"
              autoComplete="email"
              value={email}
              onChange={(e) => setEmail(e.target.value)}
              required
              className="h-9 w-full px-3 rounded border border-border bg-bg text-fg placeholder:text-fg-muted focus:outline-none focus:ring-2 focus:ring-accent text-sm"
            />
          </label>
          <label className="block">
            <span className="block text-[13px] font-medium text-fg mb-1">Password</span>
            <input
              data-testid="login-password"
              type="password"
              autoComplete="current-password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              required
              className="h-9 w-full px-3 rounded border border-border bg-bg text-fg placeholder:text-fg-muted focus:outline-none focus:ring-2 focus:ring-accent text-sm"
            />
          </label>
          {error && (
            <p data-testid="login-error" role="alert" className="text-destructive text-[13px]">
              {error}
            </p>
          )}
          <button
            data-testid="login-submit"
            type="submit"
            disabled={busy}
            className="w-full h-9 rounded bg-accent text-accent-fg text-sm font-medium hover:opacity-90 transition-opacity disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {busy ? "Signing in…" : "Sign in"}
          </button>
        </form>
        <div className="mt-6 pt-4 border-t border-border space-y-2 text-center">
          <a href="/auth/oidc/login" className="block text-[13px] text-accent hover:opacity-80">Sign in with SSO</a>
          <a href="/setup" className="block text-[13px] text-fg-muted hover:text-fg transition-colors">First-run setup</a>
        </div>
      </div>
    </main>
  );
}
