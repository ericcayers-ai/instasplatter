import { useEffect } from "react";
import DropZone from "./components/DropZone";
import Processing from "./components/Processing";
import Preferences from "./components/Preferences";
import { useStore } from "./state/store";

export default function App() {
  const screen = useStore((s) => s.screen);
  const init = useStore((s) => s.init);

  useEffect(() => {
    void init();
  }, [init]);

  return (
    <div className="relative h-full w-full">
      {screen === "home" ? <DropZone /> : <Processing />}
      <Preferences />
    </div>
  );
}
