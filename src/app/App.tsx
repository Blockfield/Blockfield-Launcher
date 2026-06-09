import { useState } from "react";
import { WindowChrome } from "./components/WindowChrome";
import { LoginScreen } from "./components/LoginScreen";
import { Shell } from "./components/Shell";
import { MainScreen } from "./components/MainScreen";
import { UpdateScreen } from "./components/UpdateScreen";
import { SettingsScreen } from "./components/SettingsScreen";

type Screen = "login" | "main" | "update" | "settings";

export default function App() {
  const [screen, setScreen] = useState<Screen>("login");

  return (
    <WindowChrome>
      {screen === "login" ? (
        <LoginScreen onSignIn={() => setScreen("main")} />
      ) : (
        <Shell
          active={screen as "main" | "update" | "settings"}
          onNavigate={(s) => setScreen(s)}
          onLogout={() => setScreen("login")}
        >
          {screen === "main" && (
            <MainScreen onPlay={() => setScreen("update")} />
          )}
          {screen === "update" && <UpdateScreen />}
          {screen === "settings" && (
            <SettingsScreen onLogout={() => setScreen("login")} />
          )}
        </Shell>
      )}
    </WindowChrome>
  );
}
