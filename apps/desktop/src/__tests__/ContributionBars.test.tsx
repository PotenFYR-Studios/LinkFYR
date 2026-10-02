import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { ContributionBars } from "../components/ContributionBars";
import type { InterfaceTelemetry } from "@linkfyr/types";

function telemetry(
  id: string,
  kind: InterfaceTelemetry["interface"]["kind"],
  status: InterfaceTelemetry["interface"]["status"],
  rxBps: number,
): InterfaceTelemetry {
  return {
    interface: {
      id,
      name: id,
      friendlyName: id,
      kind,
      status,
      mac: null,
      ipv4: [],
      ipv6: [],
      gateway: null,
      mtu: 1500,
      speedBps: null,
      metered: false,
    },
    rxBps,
    txBps: 0,
    health: null,
    errors: {
      id,
      rxBytes: 0,
      txBytes: 0,
      rxPackets: 0,
      txPackets: 0,
      rxErrors: 0,
      txErrors: 0,
      timestampMs: 0,
    },
  };
}

describe("ContributionBars", () => {
  it("shows an honest empty state with no active interfaces", () => {
    render(<ContributionBars interfaces={[telemetry("wifi0", "wifi", "down", 5)]} />);
    expect(screen.getByText(/no active interfaces/i)).toBeInTheDocument();
  });

  it("renders each active interface with its share", () => {
    const list = [
      telemetry("eth0", "ethernet", "up", 7_500_000),
      telemetry("wifi0", "wifi", "up", 2_500_000),
    ];
    render(<ContributionBars interfaces={list} />);
    expect(screen.getByText(/eth0/)).toBeInTheDocument();
    expect(screen.getByText(/wifi0/)).toBeInTheDocument();
    // eth0 carries 75% of the combined load
    const meter = screen.getByRole("meter", { name: /eth0 carries 75 percent/i });
    expect(meter).toBeInTheDocument();
  });
});
