import { useEffect, useMemo, useRef, useState } from "react";
import { useStore, type ViewId } from "../state/store";

interface Command {
  id: string;
  label: string;
  hint: string;
  run: () => void;
}

/**
 * Command palette (Ctrl/Cmd+K). Keyboard-first control: every action here
 * performs something real — navigation or a state change — no dead entries.
 */
export function CommandPalette() {
  const { setView, config, setPreferences } = useStore();
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  const commands = useMemo<Command[]>(() => {
    const nav = (id: ViewId, label: string): Command => ({
      id: `go-${id}`,
      label,
      hint: "Go to",
      run: () => setView(id),
    });
    const theme = config?.preferences.theme ?? "dark";
    return [
      nav("overview", "Overview"),
      nav("interfaces", "Interfaces"),
      nav("internet", "Internet"),
      nav("optimize", "Optimize"),
      nav("settings", "Settings"),
      {
        id: "theme-toggle",
        label: theme === "dark" ? "Switch to light theme" : "Switch to dark theme",
        hint: "Appearance",
        run: () => {
          if (config) {
            void setPreferences({ ...config.preferences, theme: theme === "dark" ? "light" : "dark" });
          }
        },
      },
      {
        id: "expert-toggle",
        label: config?.preferences.expertMode ? "Disable expert mode" : "Enable expert mode",
        hint: "Appearance",
        run: () => {
          if (config) {
            void setPreferences({ ...config.preferences, expertMode: !config.preferences.expertMode });
          }
        },
      },
    ];
  }, [config, setPreferences, setView]);

  const results = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return commands;
    return commands.filter((c) => c.label.toLowerCase().includes(q));
  }, [commands, query]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setOpen((o) => !o);
        setQuery("");
        setSelected(0);
        return;
      }
      if (e.key === "Escape") setOpen(false);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  useEffect(() => {
    if (open) inputRef.current?.focus();
  }, [open]);

  if (!open) return null;

  const commit = (c: Command | undefined) => {
    if (!c) return;
    c.run();
    setOpen(false);
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center pt-[14vh]"
      style={{ background: "color-mix(in srgb, var(--color-base) 62%, transparent)" }}
      onMouseDown={() => setOpen(false)}
      role="dialog"
      aria-modal="true"
      aria-label="Command palette"
    >
      <div
        className="lf-card w-[560px] overflow-hidden p-0 shadow-2xl"
        style={{ background: "var(--color-overlay)" }}
        onMouseDown={(e) => e.stopPropagation()}
      >
        <input
          ref={inputRef}
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
            setSelected(0);
          }}
          onKeyDown={(e) => {
            if (e.key === "ArrowDown") {
              setSelected((s) => Math.min(s + 1, results.length - 1));
              e.preventDefault();
            } else if (e.key === "ArrowUp") {
              setSelected((s) => Math.max(s - 1, 0));
              e.preventDefault();
            } else if (e.key === "Enter") {
              commit(results[selected]);
            }
          }}
          placeholder="Type a command… (views, appearance)"
          className="w-full border-0 border-b border-stroke-soft bg-transparent px-4 py-3 text-sm outline-none"
          style={{ color: "var(--color-ink)" }}
          aria-label="Search commands"
        />
        <ul className="max-h-[320px] overflow-y-auto p-1.5" role="listbox">
          {results.length === 0 ? (
            <li className="px-3 py-6 text-center text-xs text-ink-muted">No matching commands.</li>
          ) : (
            results.map((c, i) => (
              <li key={c.id} role="option" aria-selected={i === selected}>
                <button
                  className="flex w-full items-center justify-between rounded-lg px-3 py-2 text-left text-[13px]"
                  style={{
                    background: i === selected ? "color-mix(in srgb, var(--color-accent) 14%, transparent)" : "transparent",
                    color: i === selected ? "var(--color-accent-soft)" : "var(--color-ink)",
                  }}
                  onMouseEnter={() => setSelected(i)}
                  onClick={() => commit(c)}
                >
                  <span>{c.label}</span>
                  <span className="text-2xs text-ink-faint">{c.hint}</span>
                </button>
              </li>
            ))
          )}
        </ul>
        <div className="flex items-center justify-between border-t border-stroke-soft px-4 py-2 text-2xs text-ink-faint">
          <span>↑↓ navigate · ↵ run · esc close</span>
          <span className="num">Ctrl+K</span>
        </div>
      </div>
    </div>
  );
}
