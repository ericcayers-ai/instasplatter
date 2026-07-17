import { useEffect } from "react";
import DropZone from "./components/DropZone";
import Processing from "./components/Processing";
import GeoViewport from "./geospatial/GeoViewport";
import TitleBar from "./components/shell/TitleBar";
import ExperimentalBanner from "./components/shell/ExperimentalBanner";
import ExperimentalLicenseModal from "./components/shell/ExperimentalLicenseModal";
import AboutPanel from "./components/shell/AboutPanel";
import SceneTree from "./components/shell/SceneTree";
import PropertiesPanel from "./components/shell/PropertiesPanel";
import LogConsole from "./components/shell/LogConsole";
import StatusBar from "./components/shell/StatusBar";
import { useStore } from "./state/store";

export default function App() {
  const screen = useStore((s) => s.screen);
  const suite = useStore((s) => s.suite);
  const init = useStore((s) => s.init);
  const leftPanelOpen = useStore((s) => s.leftPanelOpen);
  const setLeftPanelOpen = useStore((s) => s.setLeftPanelOpen);
  const rightPanelOpen = useStore((s) => s.rightPanelOpen);
  const toggleRightPanel = useStore((s) => s.toggleRightPanel);
  const logConsoleOpen = useStore((s) => s.logConsoleOpen);
  const setLogConsoleOpen = useStore((s) => s.setLogConsoleOpen);
  const aboutOpen = useStore((s) => s.aboutOpen);
  const setAboutOpen = useStore((s) => s.setAboutOpen);

  useEffect(() => {
    void init();
  }, [init]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const meta = e.metaKey || e.ctrlKey;
      const target = e.target as HTMLElement | null;
      const typing =
        target &&
        (target.tagName === "INPUT" ||
          target.tagName === "TEXTAREA" ||
          target.tagName === "SELECT" ||
          target.isContentEditable);

      if (e.key === "Escape") {
        if (aboutOpen) {
          setAboutOpen(false);
          e.preventDefault();
          return;
        }
        if (rightPanelOpen) {
          toggleRightPanel();
          e.preventDefault();
          return;
        }
        if (logConsoleOpen) {
          setLogConsoleOpen(false);
          e.preventDefault();
        }
        return;
      }

      if (typing) return;

      if (meta && e.key === ",") {
        e.preventDefault();
        toggleRightPanel();
        return;
      }
      if (meta && e.key.toLowerCase() === "b") {
        e.preventDefault();
        setLeftPanelOpen(!leftPanelOpen);
        return;
      }
      if (meta && e.key.toLowerCase() === "l") {
        e.preventDefault();
        setLogConsoleOpen(!logConsoleOpen);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [
    aboutOpen,
    leftPanelOpen,
    logConsoleOpen,
    rightPanelOpen,
    setAboutOpen,
    setLeftPanelOpen,
    setLogConsoleOpen,
    toggleRightPanel,
  ]);

  const main =
    suite === "geospatial" ? (
      <GeoViewport />
    ) : screen === "home" ? (
      <DropZone />
    ) : (
      <Processing />
    );

  return (
    <div className="flex h-full w-full flex-col">
      <TitleBar />
      <ExperimentalBanner />
      <div className="flex min-h-0 flex-1">
        <SceneTree />
        <main className="min-w-0 flex-1" id="main-stage">
          {main}
        </main>
        <PropertiesPanel />
      </div>
      <LogConsole />
      <StatusBar />
      <ExperimentalLicenseModal />
      <AboutPanel />
    </div>
  );
}
