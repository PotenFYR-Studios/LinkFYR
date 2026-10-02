import { useEffect } from "react";
import { Shell } from "./components/Shell";
import { Dashboard } from "./components/Dashboard";
import { InterfacesView } from "./components/InterfacesView";
import { InternetView } from "./components/InternetView";
import { OptimizeView } from "./components/OptimizeView";
import { SettingsView } from "./components/SettingsView";
import { CommandPalette } from "./components/CommandPalette";
import { useStore } from "./state/store";

export default function App() {
  const { view, init } = useStore();

  useEffect(() => {
    // Surface unexpected errors instead of failing silently.
    const onErr = (e: ErrorEvent) => {
      const { error } = useStore.getState();
      if (!error) useStore.setState({ error: e.message });
    };
    const onRej = (e: PromiseRejectionEvent) => {
      const msg = e.reason instanceof Error ? e.reason.message : String(e.reason);
      const { error } = useStore.getState();
      if (!error) useStore.setState({ error: msg });
    };
    window.addEventListener("error", onErr);
    window.addEventListener("unhandledrejection", onRej);
    void init();
    // Follow system theme changes when preference is "system".
    const mq = window.matchMedia("(prefers-color-scheme: light)");
    const relight = () => {
      const prefs = useStore.getState().config?.preferences;
      if (prefs?.theme === "system") {
        // Re-apply by round-tripping the same prefs.
        void import("./state/store").then(({ applyDocumentPrefs }) =>
          applyDocumentPrefs(prefs),
        );
      }
    };
    mq.addEventListener("change", relight);
    return () => {
      window.removeEventListener("error", onErr);
      window.removeEventListener("unhandledrejection", onRej);
      mq.removeEventListener("change", relight);
    };
  }, [init]);

  return (
    <Shell>
      <div key={view} className="lf-enter h-full">
        {view === "overview" && <Dashboard />}
        {view === "interfaces" && <InterfacesView />}
        {view === "internet" && <InternetView />}
        {view === "optimize" && <OptimizeView />}
        {view === "settings" && <SettingsView />}
      </div>
      <CommandPalette />
    </Shell>
  );
}
