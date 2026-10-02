import { useStore } from "../state/store";
import type { AnimationLevel, Theme } from "@linkfyr/types";

/** Settings that work today: appearance + engine mode. Wired to the engine config store. */
export function SettingsView() {
  const { config, setPreferences } = useStore();
  const prefs = config?.preferences;

  if (!prefs) {
    return (
      <div className="mx-auto max-w-[760px] p-6 text-xs text-ink-muted">
        Loading preferences…
      </div>
    );
  }

  return (
    <div className="mx-auto flex max-w-[760px] flex-col gap-4 p-6">
      <header>
        <h1 className="text-base font-semibold tracking-tight">Settings</h1>
        <p className="mt-0.5 text-xs text-ink-muted">
          Stored locally. LinkFYR is local-only by default: nothing leaves this device.
        </p>
      </header>

      <section className="lf-card p-5" aria-label="Appearance">
        <h2 className="text-xs font-semibold tracking-wide text-ink-muted">Appearance</h2>

        <Row label="Theme" hint="Follows the system when set to automatic.">
          <Segmented<Theme>
            value={prefs.theme}
            options={[
              { value: "system", label: "System" },
              { value: "dark", label: "Dark" },
              { value: "light", label: "Light" },
            ]}
            onChange={(theme) => void setPreferences({ ...prefs, theme })}
          />
        </Row>

        <Row label="Animation" hint="Reduced disables decorative motion; off stops all transitions.">
          <Segmented<AnimationLevel>
            value={prefs.animation}
            options={[
              { value: "full", label: "Full" },
              { value: "reduced", label: "Reduced" },
              { value: "off", label: "Off" },
            ]}
            onChange={(animation) => void setPreferences({ ...prefs, animation })}
          />
        </Row>
      </section>

      <section className="lf-card p-5" aria-label="Interface">
        <h2 className="text-xs font-semibold tracking-wide text-ink-muted">Experience</h2>
        <Row
          label="Expert mode"
          hint="Surfaces scheduler internals, factor breakdowns, and advanced controls as they ship."
        >
          <Toggle
            checked={prefs.expertMode}
            onChange={(expertMode) => void setPreferences({ ...prefs, expertMode })}
          />
        </Row>
        <Row label="Local only" hint="Always on. LinkFYR never uploads telemetry or browsing data.">
          <Toggle checked onChange={() => undefined} disabled />
        </Row>
      </section>

      <section className="lf-card p-5" aria-label="Background">
        <h2 className="text-xs font-semibold tracking-wide text-ink-muted">Background</h2>
        <Row
          label="Keep running in background"
          hint="Closing the window hides LinkFYR to the tray icon; monitoring, alerts, and notifications continue. Turn off to quit when the window closes. Quit is always available from the tray menu."
        >
          <Toggle
            checked={prefs.closeToTray}
            onChange={(closeToTray) => void setPreferences({ ...prefs, closeToTray })}
          />
        </Row>
        <Row
          label="System service"
          hint="For always-on operation without the app (for example during exams), install the linkfyrd service; see docs/content/daemon.md."
        >
          <span className="text-2xs text-ink-faint">docs/content/daemon.md</span>
        </Row>
      </section>
    </div>
  );
}

function Row({
  label,
  hint,
  children,
}: {
  label: string;
  hint: string;
  children: React.ReactNode;
}) {
  return (
    <div className="mt-4 flex items-center justify-between gap-6 first:mt-0">
      <div className="min-w-0">
        <div className="text-sm font-medium">{label}</div>
        <div className="mt-0.5 text-2xs leading-relaxed text-ink-muted">{hint}</div>
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  );
}

function Segmented<T extends string>({
  value,
  options,
  onChange,
}: {
  value: T;
  options: { value: T; label: string }[];
  onChange: (v: T) => void;
}) {
  return (
    <div
      role="radiogroup"
      className="flex overflow-hidden rounded-lg border border-stroke"
      style={{ background: "var(--color-raised)" }}
    >
      {options.map((o) => (
        <button
          key={o.value}
          role="radio"
          aria-checked={value === o.value}
          onClick={() => onChange(o.value)}
          className="px-3 py-1.5 text-xs font-medium"
          style={{
            background:
              value === o.value
                ? "color-mix(in srgb, var(--color-accent) 18%, transparent)"
                : "transparent",
            color: value === o.value ? "var(--color-accent-soft)" : "var(--color-ink-muted)",
          }}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

function Toggle({
  checked,
  onChange,
  disabled = false,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  disabled?: boolean;
}) {
  return (
    <button
      role="switch"
      aria-checked={checked}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className="relative h-6 w-10 rounded-full border transition-colors"
      style={{
        background: checked ? "var(--color-accent)" : "var(--color-stroke)",
        borderColor: checked ? "var(--color-accent)" : "var(--color-stroke)",
        opacity: disabled ? 0.55 : 1,
      }}
    >
      <span
        className="absolute top-0.5 h-4.5 w-4.5 rounded-full bg-white transition-all"
        style={{ left: checked ? "18px" : "2px", width: 18, height: 18 }}
      />
    </button>
  );
}
