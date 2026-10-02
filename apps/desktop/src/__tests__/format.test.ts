import { describe, expect, it } from "vitest";
import { fmtMs, fmtPct, fmtRate, fmtRatePlain, healthColor } from "../lib/format";

describe("fmtRate", () => {
  it("formats gigabit rates", () => {
    expect(fmtRate(1_500_000_000)).toEqual({ value: "1.50", unit: "Gbps" });
  });

  it("drops decimals for big megabit rates", () => {
    expect(fmtRate(642_000_000)).toEqual({ value: "642", unit: "Mbps" });
  });

  it("keeps one decimal in the tens", () => {
    expect(fmtRate(21_400_000)).toEqual({ value: "21.4", unit: "Mbps" });
  });

  it("falls back to kilobits", () => {
    expect(fmtRate(480_000)).toEqual({ value: "480", unit: "Kbps" });
  });

  it("handles zero and garbage without panicking", () => {
    expect(fmtRate(0).value).toBe("0");
    expect(fmtRate(Number.NaN).value).toBe("0");
  });

  it("plain variant joins value and unit", () => {
    expect(fmtRatePlain(642_000_000)).toBe("642 Mbps");
  });
});

describe("probe formatting", () => {
  it("renders missing probe data as an honest dash", () => {
    expect(fmtMs(null)).toBe("—");
    expect(fmtPct(undefined)).toBe("—");
  });

  it("formats sub-100ms with one decimal", () => {
    expect(fmtMs(21.42)).toBe("21.4 ms");
  });

  it("formats loss percentages", () => {
    expect(fmtPct(0.2)).toBe("0.2%");
  });
});

describe("healthColor", () => {
  it("maps thresholds to semantic colors", () => {
    expect(healthColor(94)).toContain("ok");
    expect(healthColor(60)).toContain("warn");
    expect(healthColor(20)).toContain("danger");
    expect(healthColor(null)).toContain("faint");
  });
});
