/**
 * Bridge to the LinkFYR engine. In the Tauri shell we use IPC; in a plain
 * browser (vite dev without Tauri) the engine is unavailable and the UI
 * renders its honest offline state instead of faking data.
 */
import type { Config, Preferences, Request, Response, Snapshot } from "@linkfyr/types";

type SnapshotListener = (snap: Snapshot) => void;

const listeners = new Set<SnapshotListener>();

function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export const bridge = {
  available: isTauri(),

  async request(req: Request): Promise<Response> {
    if (!isTauri()) {
      return {
        type: "error",
        code: "engine_unavailable",
        message: "Engine not attached (browser preview).",
      };
    }
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<Response>("engine_request", { request: req });
  },

  async getSnapshot(): Promise<Snapshot | null> {
    const res = await this.request({ type: "get_snapshot" });
    if (res.type !== "snapshot") return null;
    const { type: _type, ...snapshot } = res;
    return snapshot;
  },

  async getConfig(): Promise<Config | null> {
    const res = await this.request({ type: "get_config" });
    if (res.type !== "config") return null;
    const { type: _type, ...config } = res;
    return config;
  },

  async updatePreferences(prefs: Preferences): Promise<Preferences | null> {
    const res = await this.request({ type: "update_preferences", preferences: prefs });
    if (res.type !== "preferences_updated") return null;
    const { type: _type, ...updated } = res;
    return updated;
  },

  /** Subscribe to live snapshots pushed by the engine. */
  async onSnapshot(fn: SnapshotListener): Promise<() => void> {
    listeners.add(fn);
    if (isTauri()) {
      const { listen } = await import("@tauri-apps/api/event");
      const unlisten = await listen<Snapshot>("snapshot", (event) => {
        fn(event.payload);
      });
      return () => {
        listeners.delete(fn);
        unlisten();
      };
    }
    return () => listeners.delete(fn);
  },
};
