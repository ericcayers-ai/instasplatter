import { useEffect } from "react";
import DropZone from "./components/DropZone";
import Processing from "./components/Processing";
import TitleBar from "./components/shell/TitleBar";
import SceneTree from "./components/shell/SceneTree";
import PropertiesPanel from "./components/shell/PropertiesPanel";
import LogConsole from "./components/shell/LogConsole";
import StatusBar from "./components/shell/StatusBar";
import { useStore } from "./state/store";

export default function App() {
  const screen = useStore((s) => s.screen);
  const init = useStore((s) => s.init);

  useEffect(() => {
    void init();
  }, [init]);

  return (
    <div className="flex h-full w-full flex-col">
      <TitleBar />
      <div className="flex min-h-0 flex-1">
        <SceneTree />
        <main className="min-w-0 flex-1">{screen === "home" ? <DropZone /> : <Processing />}</main>
        <PropertiesPanel />
      </div>
      <LogConsole />
      <StatusBar />
    </div>
  );
}
