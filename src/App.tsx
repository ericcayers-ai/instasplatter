import { useEffect, Suspense } from "react";
import DropZone from "./components/DropZone";
import Processing from "./components/Processing";
import GeoViewport from "./geospatial/GeoViewport";
import AppShell from "./components/shell/AppShell";
import ErrorBoundary from "./components/ErrorBoundary";
import { useStore } from "./state/store";
import { api } from "./lib/ipc";

// Local WebView smoke checks can inspect state and defer IPC without mock wiring.
// The production build removes these development-only test hooks.
if (import.meta.env.DEV) Object.assign(window, { __INSTA_STORE__: useStore, __INSTA_API__: api });

function LoadingShell() {
  return (
    <div className="flex min-h-screen items-center justify-center bg-bg">
      <div className="text-center">
        <div className="loading-ring mb-4" />
        <span className="text-xs text-ink-dim">Loading InstaSplatter</span>
      </div>
    </div>
  );
}

function AppRoutes() {
  const screen = useStore((s) => s.screen);
  const suite = useStore((s) => s.suite);
  const init = useStore((s) => s.init);
  const initError = useStore((s) => s.initError);

  useEffect(() => {
    void init();
  }, [init]);

  if (initError) {
    return (
      <div className="flex min-h-screen items-center justify-center bg-bg p-8">
        <div role="alert" className="max-w-md rounded-lg border border-critical bg-panel p-6 text-center">
          <h2 className="mb-2 text-base font-semibold text-ink">Initialization failed</h2>
          <p className="text-xs text-ink-dim">{initError}</p>
        </div>
      </div>
    );
  }

  const main =
    suite === "geospatial" ? (
      <GeoViewport />
    ) : screen === "home" ? (
      <DropZone />
    ) : (
      <Processing />
    );

  return <AppShell main={main} />;
}

export default function App() {
  return (
    <ErrorBoundary>
      <Suspense fallback={<LoadingShell />}>
        <AppRoutes />
      </Suspense>
    </ErrorBoundary>
  );
}
