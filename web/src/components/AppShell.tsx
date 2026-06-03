import { Outlet } from "react-router-dom";

import { DocTree } from "../features/docs/DocTree";
import { useViewport } from "../hooks/useViewport";
import { useUi } from "../stores/ui";

import { CommandPalette } from "./CommandPalette";
import { Toast } from "./Toast";

export function AppShell() {
  const sidebarOpen = useUi((s) => s.sidebarOpen);
  const toggleSidebar = useUi((s) => s.toggleSidebar);
  const vp = useViewport();
  const mobile = vp === "mobile";

  return (
    <div
      style={{
        display: mobile ? "block" : "grid",
        gridTemplateColumns: mobile ? undefined : sidebarOpen ? "260px 1fr" : "0 1fr",
        height: "100vh",
        fontFamily: "system-ui, sans-serif",
      }}
    >
      {mobile && !sidebarOpen && (
        <button
          type="button"
          data-testid="menu-toggle"
          onClick={toggleSidebar}
          aria-label="Open menu"
          style={{
            position: "fixed",
            top: 12,
            left: 12,
            zIndex: 25,
            width: 36,
            height: 36,
            border: "1px solid #e5e5e5",
            borderRadius: 6,
            background: "white",
            fontSize: 20,
            lineHeight: 1,
            cursor: "pointer",
          }}
        >
          ☰
        </button>
      )}
      {mobile && sidebarOpen && (
        <div
          data-testid="sidebar-backdrop"
          onClick={toggleSidebar}
          style={{
            position: "fixed",
            inset: 0,
            background: "rgba(0,0,0,0.4)",
            zIndex: 20,
          }}
        />
      )}
      <aside
        data-testid="sidebar"
        style={{
          borderRight: "1px solid #e5e5e5",
          overflow: "auto",
          background: "#fafafa",
          position: mobile ? "fixed" : "static",
          left: mobile ? (sidebarOpen ? 0 : "-280px") : undefined,
          top: mobile ? 0 : undefined,
          width: mobile ? 260 : undefined,
          height: mobile ? "100vh" : undefined,
          zIndex: mobile ? 30 : undefined,
          transition: mobile ? "left 200ms ease-out" : undefined,
        }}
      >
        <DocTree />
      </aside>
      <main style={{ overflow: "auto", height: mobile ? "100vh" : undefined }}>
        <Outlet />
      </main>
      <Toast />
      <CommandPalette />
    </div>
  );
}
