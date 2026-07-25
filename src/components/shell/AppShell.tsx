import { useCallback, useEffect, useState, type ReactNode } from "react";
import { useStore } from "../../state/store";
import TitleBar from "./TitleBar";
import ExperimentalBanner from "./ExperimentalBanner";
import ExperimentalLicenseModal from "./ExperimentalLicenseModal";
import AboutPanel from "./AboutPanel";
import StatusBar from "./StatusBar";
import IconRail from "./IconRail";
import LeftDock from "./LeftDock";
import RightDock from "./RightDock";
import type { IconRailAction, ShellFocus } from "./shellFocus";

export default function AppShell({ main }: { main: ReactNode }) {
  const suite = useStore((s) => s.suite);
  const leftPanelOpen = useStore((s) => s.leftPanelOpen);
  const setLeftPanelOpen = useStore((s) => s.setLeftPanelOpen);
  const rightPanelOpen = useStore((s) => s.rightPanelOpen);
  const setRightPanelOpen = useStore((s) => s.setRightPanelOpen);
  const logConsoleOpen = useStore((s) => s.logConsoleOpen);
  const setLogConsoleOpen = useStore((s) => s.setLogConsoleOpen);
  const aboutOpen = useStore((s) => s.aboutOpen);
  const setAboutOpen = useStore((s) => s.setAboutOpen);

  const [focus, setFocus] = useState<ShellFocus | null>("capture");
  const [railActive, setRailActive] = useState<IconRailAction | null>("capture");

  const onRailAction = useCallback(
    (action: IconRailAction) => {
      setRailActive(action);
      if (action === "capture") {
        setLeftPanelOpen(true);
        setFocus(suite === "geospatial" ? "layers" : "capture");
        return;
      }
      if (action === "scene") {
        setLeftPanelOpen(true);
        setFocus(suite === "geospatial" ? "layers" : "scene");
        return;
      }
      if (action === "settings") {
        if (suite === "geospatial") {
          setRightPanelOpen(true);
          setFocus("scenario");
        } else {
          setLeftPanelOpen(true);
          setFocus("reconstruction");
        }
        return;
      }
      if (action === "log") {
        const next = !logConsoleOpen;
        setLogConsoleOpen(next);
        if (next) {
          setRightPanelOpen(true);
          setFocus("log");
        }
      }
    },
    [suite, logConsoleOpen, setLeftPanelOpen, setRightPanelOpen, setLogConsoleOpen],
  );

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
          setRightPanelOpen(false);
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
        onRailAction("settings");
        return;
      }
      if (meta && e.key.toLowerCase() === "b") {
        e.preventDefault();
        const next = !leftPanelOpen;
        setLeftPanelOpen(next);
        if (next) {
          setFocus(suite === "geospatial" ? "layers" : "capture");
          setRailActive("capture");
        } else {
          setRailActive(null);
        }
        return;
      }
      if (meta && e.key.toLowerCase() === "l") {
        e.preventDefault();
        onRailAction("log");
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [
    aboutOpen,
    leftPanelOpen,
    logConsoleOpen,
    rightPanelOpen,
    suite,
    onRailAction,
    setAboutOpen,
    setLeftPanelOpen,
    setLogConsoleOpen,
    setRightPanelOpen,
  ]);

  return (
    <div className="app-shell">
      <TitleBar onOpenSettings={() => onRailAction("settings")} />
      <ExperimentalBanner />
      <div className="app-shell__body">
        <IconRail active={railActive} onAction={onRailAction} />
        <LeftDock focus={focus} />
        <main className="app-shell__main" id="main-stage">
          {main}
        </main>
        <RightDock focus={focus} />
      </div>
      <StatusBar />
      <ExperimentalLicenseModal />
      <AboutPanel />
    </div>
  );
}
