import { useCallback, useMemo, useState } from "react";
import { WindowChrome } from "./components/WindowChrome";
import { LoginScreen } from "./components/LoginScreen";
import { Shell } from "./components/Shell";
import { MainScreen } from "./components/MainScreen";
import { UpdateScreen } from "./components/UpdateScreen";
import { SettingsScreen } from "./components/SettingsScreen";
import { I18nContext, makeT, type Lang } from "./i18n";

type Screen = "login" | "main" | "update" | "settings";

const LANG_KEY = "blockfield.lang";

const loadLang = (): Lang => {
  if (typeof localStorage === "undefined") return "en";
  const saved = localStorage.getItem(LANG_KEY);
  return saved === "ru" || saved === "uk" || saved === "en" ? saved : "en";
};

export default function App() {
  const [screen, setScreen] = useState<Screen>("login");
  const [lang, setLangState] = useState<Lang>(loadLang);

  const setLang = useCallback((next: Lang) => {
    setLangState(next);
    if (typeof localStorage !== "undefined") localStorage.setItem(LANG_KEY, next);
    if (typeof document !== "undefined") document.documentElement.lang = next;
  }, []);

  const i18n = useMemo(() => ({ lang, setLang, t: makeT(lang) }), [lang, setLang]);

  return (
    <I18nContext.Provider value={i18n}>
      <WindowChrome key={lang}>
        {screen === "login" ? (
          <LoginScreen onSignIn={() => setScreen("main")} />
        ) : (
          <Shell
            active={screen as Exclude<Screen, "login">}
            onNavigate={setScreen}
            onLogout={() => setScreen("login")}
          >
            {screen === "main" && <MainScreen onPlay={() => setScreen("update")} />}
            {screen === "update" && <UpdateScreen />}
            {screen === "settings" && (
              <SettingsScreen onLogout={() => setScreen("login")} />
            )}
          </Shell>
        )}
      </WindowChrome>
    </I18nContext.Provider>
  );
}
