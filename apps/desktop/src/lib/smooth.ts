import { useEffect, useRef, useState } from "react";

/**
 * rAF-smoothed numeric display: the rendered value eases toward the target
 * instead of jumping every tick. This is what makes live metrics feel
 * premium rather than flickery. Disabled when the user sets motion to
 * reduced/off (they get the raw value).
 */
export function useSmoothNumber(target: number, enabled: boolean): number {
  const [display, setDisplay] = useState(target);
  const raf = useRef(0);
  const current = useRef(target);

  useEffect(() => {
    if (!enabled || !Number.isFinite(target)) {
      current.current = target;
      setDisplay(target);
      return;
    }
    let last = performance.now();
    const step = (now: number) => {
      const dt = Math.min((now - last) / 1000, 0.1);
      last = now;
      // Critically-damped-ish approach: fast when far, gentle when close.
      const diff = target - current.current;
      const rate = 1 - Math.exp(-6 * dt);
      current.current += diff * rate;
      if (Math.abs(target - current.current) < Math.abs(target) * 0.0005 + 0.5) {
        current.current = target;
        setDisplay(target);
        return; // settled; resume when target changes
      }
      setDisplay(current.current);
      raf.current = requestAnimationFrame(step);
    };
    raf.current = requestAnimationFrame(step);
    return () => cancelAnimationFrame(raf.current);
  }, [target, enabled]);

  return display;
}

export interface SeriesPoint {
  timestampMs: number;
  rxBps: number;
  txBps: number;
}

/**
 * rAF-smoothed series for live charts: y-values ease toward the incoming
 * samples so the curve flows instead of stepping once per second.
 * Hover readouts in LiveChart still read the raw target samples.
 */
export function useSmoothedSeries(target: SeriesPoint[], enabled: boolean): SeriesPoint[] {
  const [display, setDisplay] = useState<SeriesPoint[]>(target);
  const displayRef = useRef<SeriesPoint[]>(target);
  const raf = useRef(0);

  useEffect(() => {
    if (!enabled) {
      displayRef.current = target;
      setDisplay(target);
      return;
    }
    let last = performance.now();
    const step = (now: number) => {
      const dt = Math.min((now - last) / 1000, 0.1);
      last = now;
      const cur = displayRef.current;
      const rate = 1 - Math.exp(-7 * dt);

      // Align display to the target window (same length, same timestamps).
      const next: SeriesPoint[] = target.map((tp, i) => {
        const cp = cur[i];
        if (!cp || cp.timestampMs !== tp.timestampMs) {
          // New or shifted sample: ease in from the previous value.
          const baseRx = cp?.rxBps ?? 0;
          const baseTx = cp?.txBps ?? 0;
          return {
            timestampMs: tp.timestampMs,
            rxBps: baseRx + (tp.rxBps - baseRx) * rate,
            txBps: baseTx + (tp.txBps - baseTx) * rate,
          };
        }
        return {
          timestampMs: tp.timestampMs,
          rxBps: cp.rxBps + (tp.rxBps - cp.rxBps) * rate,
          txBps: cp.txBps + (tp.txBps - cp.txBps) * rate,
        };
      });
      displayRef.current = next;

      const settled = next.every((p, i) => {
        const tp = target[i];
        return tp && Math.abs(tp.rxBps - p.rxBps) < 1 && Math.abs(tp.txBps - p.txBps) < 1;
      });
      setDisplay(next);
      if (settled) return; // settled; restart on next target change
      raf.current = requestAnimationFrame(step);
    };
    raf.current = requestAnimationFrame(step);
    return () => cancelAnimationFrame(raf.current);
  }, [target, enabled]);

  return display;
}

/**
 * Eased, quantized chart scale. The y-axis top snaps to 1/2/5×10^n steps
 * (stable, readable) and eases toward changes instead of rescaling
 * instantly — the instant rescale is what made the chart wobble when
 * throughput shifted. Exceeding the scale grows it immediately (peaks are
 * never clipped); the scale only shrinks slowly after the peak is gone.
 */
export function useEasedScale(targetMax: number, enabled: boolean): number {
  const [display, setDisplay] = useState(() => niceCeil(targetMax));
  const current = useRef(niceCeil(targetMax));
  const raf = useRef(0);

  useEffect(() => {
    const nice = niceCeil(targetMax);
    if (!enabled) {
      current.current = nice;
      setDisplay(nice);
      return;
    }
    let last = performance.now();
    const step = (now: number) => {
      const dt = Math.min((now - last) / 1000, 0.1);
      last = now;
      const diff = nice - current.current;
      // Fast growth (never clip peaks), slow shrink (no wobble).
      const rate = 1 - Math.exp((diff > 0 ? -8 : -1.2) * dt);
      current.current += diff * rate;
      if (Math.abs(nice - current.current) < nice * 0.002) {
        current.current = nice;
        setDisplay(nice);
        return;
      }
      setDisplay(current.current);
      raf.current = requestAnimationFrame(step);
    };
    raf.current = requestAnimationFrame(step);
    return () => cancelAnimationFrame(raf.current);
  }, [targetMax, enabled]);

  return display;
}

/** Smallest 1/2/5×10^n value ≥ v. */
export function niceCeil(v: number): number {
  if (!Number.isFinite(v) || v <= 0) return 1;
  const exp = Math.floor(Math.log10(v));
  const base = 10 ** exp;
  const mant = v / base;
  const nice = mant <= 1 ? 1 : mant <= 2 ? 2 : mant <= 5 ? 5 : 10;
  return nice * base;
}
