import { useEffect, useState } from "react";
import { KnotEditor } from "./features/editor/KnotEditor";

export default function App() {
  const params = new URLSearchParams(location.search);
  const docId = params.get("docId") ?? "spike";
  const [name, setName] = useState<string>(
    () => localStorage.getItem("knot-spike-name") ?? `User-${Math.floor(Math.random() * 1000)}`,
  );

  useEffect(() => {
    localStorage.setItem("knot-spike-name", name);
  }, [name]);

  return (
    <main style={{ padding: 24, fontFamily: "system-ui, sans-serif", maxWidth: 720 }}>
      <h1>knot spike</h1>
      <p>
        Doc id: <code>{docId}</code> · You:{" "}
        <input
          data-testid="username"
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
      </p>
      <KnotEditor docId={docId} />
      <p style={{ marginTop: 16, color: "#666" }}>
        Open in two browsers with the same <code>?docId=...</code>.
      </p>
    </main>
  );
}
