import { useEffect } from "react";
import { useStore, type ViewId } from "../state/store";
import {
  IconInterfaces,
  IconInternet,
  IconOptimize,
  IconOverview,
  IconSettings,
  LogoMark,
} from "./icons";

const NAV: { id: ViewId; label: string; icon: typeof IconOverview }[] = [
  { id: "overview", label: "Overview", icon: IconOverview },
  { id: "interfaces", label: "Interfaces", icon: IconInterfaces },
  { id: "internet", label: "Internet", icon: IconInternet },
  { id: "optimize", label: "Optimize", icon: IconOptimize },
  { id: "settings", label: "Settings", icon: IconSettings },
];

/**
 * App shell: left rail + content. Keyboard-first (Alt+1..4 switches
 * views), every nav target exists (no dead links).
 */
export function Shell({ children }: { children: React.ReactNode }) {
  const { view, setView, snapshot } = useStore();

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.altKey) {
        const idx = Number(e.key) - 1;
        if (idx >= 0 && idx < NAV.length) {
          setView(NAV[idx]?.id ?? "overview");
          e.preventDefault();
        }
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [setView]);

  return (
    <div className="flex h-full">
      <nav
        aria-label="Primary"
        className="flex w-[188px] shrink-0 flex-col border-r border-stroke-soft bg-surface"
      >
        <div className="flex items-center gap-2.5 px-5 pt-5 pb-4">
          <LogoMark size={22} />
          <div>
            <div className="text-sm font-semibold tracking-tight">LinkFYR</div>
            <div className="text-2xs text-ink-faint">
              {snapshot ? `engine ${snapshot.engineVersion}` : "engine offline"}
            </div>
          </div>
        </div>

        <div className="flex flex-1 flex-col gap-0.5 px-0 pt-2">
          {NAV.map(({ id, label, icon: Icon }) => (
            <button
              key={id}
              className="lf-rail-item text-left text-[13px]"
              data-active={view === id}
              onClick={() => setView(id)}
              aria-current={view === id ? "page" : undefined}
            >
              <Icon size={15} />
              {label}
            </button>
          ))}
        </div>

        <div className="border-t border-stroke-soft px-5 py-3 text-2xs text-ink-faint">
          {snapshot ? (
            <span className="flex items-center gap-1.5">
              <span
                className="lf-live-dot inline-block h-1.5 w-1.5 rounded-full"
                style={{ background: "var(--color-ok)" }}
              />
              live · {snapshot.timestampMs > 0 ? "sampling 1 Hz" : "idle"}
            </span>
          ) : (
            <span>engine not attached</span>
          )}
        </div>
      </nav>

      <main className="flex-1 overflow-y-auto" aria-live="polite">
        {children}
      </main>
    </div>
  );
}
