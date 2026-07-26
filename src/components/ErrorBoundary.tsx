import { Component, type ReactNode } from "react";
import { useStore } from "../state/store";

interface ErrorBoundaryProps {
  children: ReactNode;
  fallback?: ReactNode;
}

interface ErrorBoundaryState {
  error: Error | null;
}

class ErrorBoundaryInner extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  constructor(props: ErrorBoundaryProps) {
    super(props);
    this.state = { error: null };
  }

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return { error };
  }

  render() {
    if (this.state.error) {
      return (
        this.props.fallback ?? (
          <ErrorFallback
            error={this.state.error}
            onReset={() => this.setState({ error: null })}
          />
        )
      );
    }
    return this.props.children;
  }
}

function ErrorFallback({ error, onReset }: { error: Error; onReset: () => void }) {
  const exportDiagnosticsAction = useStore((s) => s.exportDiagnosticsAction);

  return (
    <div className="flex min-h-screen items-center justify-center bg-bg p-8">
      <div
        role="alert"
        className="max-w-md rounded-lg border border-critical bg-panel p-6 text-center"
      >
        <h2 className="mb-2 text-base font-semibold text-ink">Something went wrong</h2>
        <p className="mb-4 text-xs text-ink-dim">
          An unexpected error occurred. You can try resetting or export diagnostics for support.
        </p>
        <pre className="mb-4 max-h-32 overflow-auto rounded bg-bg p-2 text-left font-mono text-[10px] text-critical">
          {error.message}
        </pre>
        <div className="flex gap-2 justify-center">
          <button type="button" className="btn btn-ghost-teal px-3 py-1.5 text-xs" onClick={onReset}>
            Try again
          </button>
          <button
            type="button"
            className="btn btn-ghost px-3 py-1.5 text-xs"
            onClick={() => void exportDiagnosticsAction()}
          >
            Diagnostics
          </button>
        </div>
      </div>
    </div>
  );
}

export default function ErrorBoundary({
  children,
  fallback,
}: {
  children: ReactNode;
  fallback?: ReactNode;
}) {
  return <ErrorBoundaryInner fallback={fallback}>{children}</ErrorBoundaryInner>;
}
