import { useEffect } from "react";
import DropZone from "./components/DropZone";
import Processing from "./components/Processing";
import GeoHome from "./geospatial/GeoHome";
import TitleBar from "./components/shell/TitleBar";
import ExperimentalBanner from "./components/shell/ExperimentalBanner";
import ExperimentalLicenseModal from "./components/shell/ExperimentalLicenseModal";
import SceneTree from "./components/shell/SceneTree";
import PropertiesPanel from "./components/shell/PropertiesPanel";
import LogConsole from "./components/shell/LogConsole";
import StatusBar from "./components/shell/StatusBar";
import { useStore } from "./state/store";

export default function App() {
  const screen = useStore((s) => s.screen);
  const suite = useStore((s) => s.suite);
  const init = useStore((s) => s.init);

  useEffect(() => {
    void init();
  }, [init]);

  const main =
    suite === "geospatial" ? (
      <GeoHome />
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
        <main className="min-w-0 flex-1">{main}</main>
        <PropertiesPanel />
      </div>
      <LogConsole />
      <StatusBar />
      <ExperimentalLicenseModal />
    </div>
  );
}
