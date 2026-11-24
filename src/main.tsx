import "./styles/tailwind.css";

import { QueryClientProvider } from "@tanstack/react-query";
import { createRouter, RouterProvider } from "@tanstack/react-router";
import React, { type ReactNode } from "react";
import ReactDOM from "react-dom/client";

import { ErrorBoundary } from "./components";
import { ConfirmHost } from "./components/ConfirmDialog";
import { ToastProvider } from "./components/ToastProvider";
import { queryClient } from "./lib/query";
import { api } from "./lib/tauri";
import { useTheme } from "./modules/settings";
import { routeTree } from "./routeTree.gen";

const router = createRouter({ routeTree });

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
// Reveal the window once the app has mounted. It starts hidden (visible:false), and
// the window/webview backgroundColor (#0c0d18) plus the dark shell in index.html
// mean it reveals as the dark loading screen, never a white flash. The backend
// keeps it hidden when the user starts in the tray.
//
// Deliberately a direct call, not requestAnimationFrame: while the window is
// hidden the WebView document is `hidden`, which pauses rAF, so a deferred reveal
// could deadlock and never show the window.
function showAppWindow() {
  void api.showMainWindow();
}

// Theme provider component that applies theme to document
function ThemeProvider({ children }: { children: ReactNode }) {
  useTheme();
  return <>{children}</>;
}

// A failure the router and the boundary did not catch still reaches the log.
function reportUiError(message: string, stack?: string) {
  void api.logUiError({ message, stack, route: window.location.pathname }).catch(() => {});
}

window.addEventListener("error", (event) => {
  if (!event.message) return;
  reportUiError(event.message, event.error instanceof Error ? event.error.stack : undefined);
});

window.addEventListener("unhandledrejection", (event) => {
  const reason: unknown = event.reason;
  reportUiError(
    reason instanceof Error ? reason.message : String(reason),
    reason instanceof Error ? reason.stack : undefined,
  );
});

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <QueryClientProvider client={queryClient}>
      <ThemeProvider>
        <ToastProvider>
          <ErrorBoundary>
            <RouterProvider router={router} />
          </ErrorBoundary>
          <ConfirmHost />
        </ToastProvider>
      </ThemeProvider>
    </QueryClientProvider>
  </React.StrictMode>,
);

// Executes function to show window
showAppWindow();
