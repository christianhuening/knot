import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// When VITE_COLLAB_VIA_PROXY=1 (set by Playwright config in e2e), route
// the /collab WS through toxiproxy on :3001 so chaos tests can deterministically
// flap the connection. Without it, /collab goes straight to knot-server.
const collabTarget =
  process.env.VITE_COLLAB_VIA_PROXY === "1"
    ? "ws://localhost:3001"
    : "ws://localhost:3000";

export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    proxy: {
      "/api": "http://localhost:3000",
      "/auth": "http://localhost:3000",
      "/collab": { target: collabTarget, ws: true },
    },
  },
});
