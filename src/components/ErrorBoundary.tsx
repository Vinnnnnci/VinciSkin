import { WarningOctagonIcon } from "@phosphor-icons/react";
import { Component, type ErrorInfo, type ReactNode } from "react";

import { m } from "@/i18n";
import { api } from "@/lib/tauri";

import { Button } from "./Button";

interface ErrorBoundaryProps {
  children: ReactNode;
  /**
   * What to draw in place of the children once one threw, given the way to draw them again.
   *
   * A boundary around one pane draws a notice in that pane. The one above the router draws
   * the whole-window fallback and offers a reload instead.
   */
  fallback?: (retry: () => void) => ReactNode;
}

interface ErrorBoundaryState {
  crashed: boolean;
}

/**
 * The last thing between a render that threw and a window with nothing in it.
 *
 * A class because React offers no hook that catches a render, and it sits above
 * the router so a crash on any screen still draws something a reader can act on.
 */
export class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  state: ErrorBoundaryState = { crashed: false };

  static getDerivedStateFromError(): ErrorBoundaryState {
    return { crashed: true };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    void api
      .logUiError({
        message: `${error.name}: ${error.message}`,
        stack: info.componentStack ?? error.stack,
        route: window.location.pathname,
      })
      .catch(() => {});
  }

  retry = () => {
    this.setState({ crashed: false });
  };

  render() {
    if (!this.state.crashed) return this.props.children;
    if (this.props.fallback) return this.props.fallback(this.retry);

    return (
      <div
        className="flex h-screen w-screen items-center justify-center bg-surface-950 p-8"
        data-ui="ErrorBoundary"
      >
        <div className="flex max-w-md flex-col items-center gap-4 rounded-lg bg-surface-900 p-8 text-center">
          <WarningOctagonIcon className="h-10 w-10 text-danger-text" weight="duotone" />
          <div className="flex flex-col gap-2">
            <h1 className="text-base font-medium text-surface-100">{m.common_crash_title()}</h1>
            <p className="text-sm text-surface-400">{m.common_crash_description()}</p>
          </div>
          <div className="flex gap-2">
            <Button onClick={() => window.location.reload()}>
              {m.common_crash_reload_action()}
            </Button>
          </div>
        </div>
      </div>
    );
  }
}
