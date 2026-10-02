import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import App from "../../desktop/src/App";

/**
 * Mobile setup gate: the phone is a remote client, so first it needs
 * the PC's daemon address and token (shown by `linkfyrd` on start).
 * Once a ping succeeds, the full desktop UI renders on top of the
 * same engine, the same tools, and the same contract.
 */

const STORAGE_KEY = "linkfyr.daemon";

interface SavedDaemon {
  host: string;
  token: string;
}

function loadSaved(): SavedDaemon | null {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as SavedDaemon;
    if (parsed.host && parsed.token) return parsed;
  } catch {
    // Corrupt entry: fall through to the setup form.
  }
  return null;
}

type Phase =
  | { kind: "checking" }
  | { kind: "setup"; error?: string }
  | { kind: "ready"; version: string };

export function MobileGate() {
  const [phase, setPhase] = useState<Phase>({ kind: "checking" });
  const [host, setHost] = useState("");
  const [token, setToken] = useState("");
  const [busy, setBusy] = useState(false);

  const connect = async (saved: SavedDaemon) => {
    setBusy(true);
    try {
      await invoke("configure_daemon", { host: saved.host, token: saved.token });
      const version = await invoke<string>("daemon_ping");
      localStorage.setItem(STORAGE_KEY, JSON.stringify(saved));
      setPhase({ kind: "ready", version });
    } catch (e) {
      localStorage.removeItem(STORAGE_KEY);
      setPhase({
        kind: "setup",
        error: e instanceof Error ? e.message : String(e),
      });
      setHost(saved.host);
      setToken(saved.token);
    } finally {
      setBusy(false);
    }
  };

  useEffect(() => {
    const saved = loadSaved();
    if (saved) {
      void connect(saved);
    } else {
      setPhase({ kind: "setup" });
    }
    // Runs once: connection check on cold start.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  if (phase.kind === "ready") {
    return <App />;
  }

  return (
    <div className="flex h-full items-center justify-center p-6">
      <div className="lf-card w-full max-w-sm p-6">
        <h1 className="text-base font-semibold tracking-tight">LinkFYR</h1>
        <p className="mt-1 text-xs leading-relaxed text-ink-muted">
          The phone is a remote client. Start linkfyrd on your PC
          (<span className="num">LINKFYR_WEB=0.0.0.0:58009 linkfyrd</span>),
          then enter the address and token it prints.
        </p>

        {phase.kind === "checking" ? (
          <div className="mt-4 flex items-center gap-2.5 text-xs text-ink-muted">
            <span className="lf-live-dot inline-block h-2 w-2 rounded-full bg-accent" />
            Reaching your PC…
          </div>
        ) : (
          <form
            className="mt-5 flex flex-col gap-3"
            onSubmit={(e) => {
              e.preventDefault();
              void connect({ host: host.trim(), token: token.trim() });
            }}
          >
            <label className="text-2xs text-ink-faint">
              PC address (host:port)
              <input
                value={host}
                onChange={(e) => setHost(e.target.value)}
                placeholder="192.168.1.20:58009"
                autoCapitalize="off"
                autoCorrect="off"
                className="num mt-1 w-full rounded-[8px] border border-stroke bg-transparent px-3 py-2 text-sm outline-none focus:border-accent"
                aria-label="PC address"
              />
            </label>
            <label className="text-2xs text-ink-faint">
              Daemon token
              <input
                value={token}
                onChange={(e) => setToken(e.target.value)}
                placeholder="64-character token"
                autoCapitalize="off"
                autoCorrect="off"
                type="password"
                className="num mt-1 w-full rounded-[8px] border border-stroke bg-transparent px-3 py-2 text-sm outline-none focus:border-accent"
                aria-label="Daemon token"
              />
            </label>
            <button
              className="lf-btn justify-center"
              data-variant="primary"
              disabled={busy || host.trim().length === 0 || token.trim().length === 0}
            >
              {busy ? "Connecting" : "Connect"}
            </button>
            {phase.error ? (
              <p className="text-2xs leading-relaxed" style={{ color: "var(--color-danger)" }}>
                {phase.error}
              </p>
            ) : (
              <p className="text-2xs leading-relaxed text-ink-faint">
                Everything stays on your own network. The token is stored only on
                this phone.
              </p>
            )}
          </form>
        )}
      </div>
    </div>
  );
}
