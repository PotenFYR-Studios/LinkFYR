import { create } from "zustand";
import type {
  Config,
  InterfaceTelemetry,
  Preferences,
  Snapshot,
} from "@linkfyr/types";
import { bridge } from "../lib/bridge";

export type ViewId = "overview" | "interfaces" | "internet" | "optimize" | "settings";

interface LinkFyrState {
  view: ViewId;
  setView: (v: ViewId) => void;

  snapshot: Snapshot | null;
  /** Interfaces as of the latest snapshot, sorted for display. */
  interfaces: InterfaceTelemetry[];
  connected: boolean;
  error: string | null;

  config: Config | null;
  setPreferences: (prefs: Preferences) => Promise<void>;
  refreshConfig: () => Promise<void>;

  init: () => Promise<void>;
}

export const useStore = create<LinkFyrState>((set, get) => ({
  view: "overview",
  setView: (v) => set({ view: v }),

  snapshot: null,
  interfaces: [],
  connected: false,
  error: null,

  config: null,
  setPreferences: async (prefs) => {
    const updated = await bridge.updatePreferences(prefs);
    const config = get().config;
    if (updated && config) {
      set({ config: { ...config, preferences: updated } });
      applyDocumentPrefs(updated);
    }
  },
  refreshConfig: async () => {
    const config = await bridge.getConfig();
    if (config) {
      set({ config });
      applyDocumentPrefs(config.preferences);
    }
  },

  init: async () => {
    try {
      await get().refreshConfig();
      await bridge.onSnapshot((snap) => {
        set({
          snapshot: snap,
          interfaces: sortInterfaces(snap.interfaces),
          connected: true,
          error: null,
        });
      });
      // Push a first paint when the engine answers even if no event yet.
      const snap = await bridge.getSnapshot();
      if (snap) {
        set({ snapshot: snap, interfaces: sortInterfaces(snap.interfaces), connected: true });
      }
    } catch (e) {
      set({ error: e instanceof Error ? e.message : String(e) });
    }
  },
}));

/** Active interfaces first, then by traffic, then by kind importance. */
function sortInterfaces(list: InterfaceTelemetry[]): InterfaceTelemetry[] {
  const kindRank: Record<string, number> = {
    ethernet: 0,
    wifi: 1,
    cellular: 2,
    tunnel: 3,
    virtual: 4,
    other: 5,
    loopback: 6,
  };
  return [...list].sort((a, b) => {
    const upA = a.interface.status === "up" ? 0 : 1;
    const upB = b.interface.status === "up" ? 0 : 1;
    if (upA !== upB) return upA - upB;
    const traffic = b.rxBps + b.txBps - (a.rxBps + a.txBps);
    if (Math.abs(traffic) > 1) return traffic;
    return (kindRank[a.interface.kind] ?? 9) - (kindRank[b.interface.kind] ?? 9);
  });
}

/** Reflect prefs on the document root (theme + motion gating). */
export function applyDocumentPrefs(prefs: Preferences): void {
  const root = document.documentElement;
  const theme =
    prefs.theme === "system"
      ? window.matchMedia("(prefers-color-scheme: light)").matches
        ? "light"
        : "dark"
      : prefs.theme;
  root.dataset.theme = theme;
  root.dataset.motion = prefs.animation;
}
