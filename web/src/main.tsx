import "./styles/global.css";

import { QueryClientProvider } from "@tanstack/react-query";
import React from "react";
import ReactDOM from "react-dom/client";
import { RouterProvider } from "react-router-dom";

import { SessionProvider } from "./auth/SessionContext";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { queryClient } from "./lib/queryClient";
import { router } from "./routes";

const initialTheme = (localStorage.getItem("knot.theme") as "light" | "dark" | null) ?? "light";
document.documentElement.setAttribute("data-theme", initialTheme);

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <ErrorBoundary>
      <QueryClientProvider client={queryClient}>
        <SessionProvider>
          <RouterProvider router={router} />
        </SessionProvider>
      </QueryClientProvider>
    </ErrorBoundary>
  </React.StrictMode>,
);
