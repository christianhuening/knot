import { lazy, Suspense } from "react";
import type { ReactNode } from "react";
import { createBrowserRouter, Navigate, Outlet } from "react-router-dom";

import { RequireAuth } from "./auth/RequireAuth";
import { AppShell } from "./components/AppShell";
import { DocTree } from "./features/docs/DocTree";

const LoginPage = lazy(() => import("./features/auth/LoginPage"));
const SetupPage = lazy(() => import("./features/auth/SetupPage"));
const DocPage = lazy(() => import("./features/docs/DocPage"));
const MembersPage = lazy(() => import("./features/workspace/MembersPage"));
const SettingsPage = lazy(() => import("./features/workspace/SettingsPage"));

function Lazy({ children }: { children: ReactNode }) {
  return <Suspense fallback={<div style={{ padding: 24 }}>Loading…</div>}>{children}</Suspense>;
}

function DocTreeAndLanding() {
  return (
    <>
      <DocTree />
      <div style={{ padding: 24 }}>Select a document from the sidebar.</div>
    </>
  );
}

function DocTreeAndDoc() {
  return (
    <>
      <DocTree />
      <Outlet />
      <Lazy><DocPage /></Lazy>
    </>
  );
}

export const router = createBrowserRouter([
  { path: "/login", element: <Lazy><LoginPage /></Lazy> },
  { path: "/setup", element: <Lazy><SetupPage /></Lazy> },
  {
    element: <RequireAuth />,
    children: [
      {
        element: <AppShell />,
        children: [
          { index: true, element: <DocTreeAndLanding /> },
          { path: "doc/:id", element: <DocTreeAndDoc /> },
          { path: "members", element: <Lazy><MembersPage /></Lazy> },
          { path: "settings", element: <Lazy><SettingsPage /></Lazy> },
        ],
      },
    ],
  },
  { path: "*", element: <Navigate to="/" replace /> },
]);
