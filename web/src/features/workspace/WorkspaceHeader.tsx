import { Moon, Search, Settings, Sun, Users } from "lucide-react";
import { Link } from "react-router-dom";

import { useSession } from "../../auth/SessionContext";
import { IconButton } from "../../components/ui/IconButton";
import { useUi } from "../../stores/ui";

export function WorkspaceHeader() {
  const session = useSession();
  const user = session.data && "ok" in session.data ? session.data.ok : null;
  const openPalette = useUi((s) => s.openPalette);
  const theme = useUi((s) => s.theme);
  const toggleTheme = useUi((s) => s.toggleTheme);

  const initial = (user?.display_name ?? "?").slice(0, 1).toUpperCase();

  return (
    <div className="px-3 pt-3 pb-2 border-b border-border">
      <div className="flex items-center gap-2 mb-3">
        <div
          aria-hidden
          className="h-7 w-7 rounded bg-accent text-accent-fg flex items-center justify-center text-[13px] font-semibold shrink-0"
        >
          {initial}
        </div>
        <div className="min-w-0 flex-1">
          <div className="text-[13px] font-semibold text-fg truncate">{user?.display_name ?? "Workspace"}</div>
          <div className="text-[11px] text-fg-muted truncate">{user?.email ?? ""}</div>
        </div>
      </div>
      <button
        type="button"
        data-testid="sidebar-search"
        onClick={openPalette}
        className="w-full flex items-center gap-2 h-8 px-2 rounded bg-muted text-fg-muted hover:text-fg transition-colors ease-swift duration-150"
      >
        <Search size={14} aria-hidden />
        <span className="text-[13px]">Search…</span>
        <span className="ml-auto text-[11px] text-fg-muted/80">⌘K</span>
      </button>
      <nav className="mt-2 flex items-center gap-1">
        <Link
          to="/members"
          className="flex-1 inline-flex items-center gap-1.5 h-7 px-2 rounded text-[13px] text-fg-muted hover:text-fg hover:bg-muted transition-colors ease-swift duration-150"
        >
          <Users size={14} aria-hidden /> Members
        </Link>
        <Link
          to="/settings"
          className="flex-1 inline-flex items-center gap-1.5 h-7 px-2 rounded text-[13px] text-fg-muted hover:text-fg hover:bg-muted transition-colors ease-swift duration-150"
        >
          <Settings size={14} aria-hidden /> Settings
        </Link>
        <IconButton
          data-testid="theme-toggle"
          label={theme === "dark" ? "Switch to light mode" : "Switch to dark mode"}
          size="sm"
          onClick={toggleTheme}
        >
          {theme === "dark" ? <Sun size={14} aria-hidden /> : <Moon size={14} aria-hidden />}
        </IconButton>
      </nav>
    </div>
  );
}
