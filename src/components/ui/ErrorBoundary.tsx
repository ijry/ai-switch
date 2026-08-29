import { Component } from "react";
import type { ErrorInfo, ReactNode } from "react";

type ErrorBoundaryProps = {
  children: ReactNode;
  label?: string;
};

type ErrorBoundaryState = {
  error: Error | null;
};

/**
 * Without a boundary a single render throw unmounts the whole tree and the window
 * goes blank, which is impossible to diagnose from a screenshot. This keeps the
 * failure on screen with the message and stack.
 */
export class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  state: ErrorBoundaryState = { error: null };

  static getDerivedStateFromError(error: unknown): ErrorBoundaryState {
    return { error: error instanceof Error ? error : new Error(String(error)) };
  }

  componentDidCatch(error: unknown, info: ErrorInfo) {
    console.error("Render failed", error, info.componentStack);
  }

  private reset = () => {
    this.setState({ error: null });
  };

  render() {
    const { error } = this.state;
    if (!error) {
      return this.props.children;
    }

    return (
      <div
        className="flex h-full min-h-0 w-full flex-col gap-3 overflow-auto bg-stone-950 p-6 text-left text-stone-100"
        data-testid="error-boundary-fallback"
        role="alert"
      >
        <p className="text-sm font-semibold">
          {this.props.label ? `${this.props.label}: ${error.message}` : error.message}
        </p>
        {error.stack && (
          <pre className="max-h-64 overflow-auto whitespace-pre-wrap rounded-lg bg-black/40 p-3 text-[11px] leading-relaxed">
            {error.stack}
          </pre>
        )}
        <button
          className="self-start rounded-lg border border-stone-600 px-3 py-1.5 text-[12px] font-semibold transition hover:border-stone-400"
          onClick={this.reset}
          type="button"
        >
          Retry
        </button>
      </div>
    );
  }
}
