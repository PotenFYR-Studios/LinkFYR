import { describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen } from "@testing-library/react";
import {
  AlertsCard,
  BridgesPanel,
  OptimizeView,
  ResultPanel,
  ToolRunPanel,
} from "../components/OptimizeView";
import type { Response, ToolRunReport } from "@linkfyr/types";

describe("OptimizeView", () => {
  it("shows the honest offline state when the engine is not attached", async () => {
    render(<OptimizeView />);
    // The capabilities probe resolves after mount; let it settle.
    await act(async () => {});
    expect(screen.getByText(/engine not attached/i)).toBeInTheDocument();
    expect(screen.getByText(/launch the desktop app/i)).toBeInTheDocument();
  });
});

describe("ToolRunPanel (registry renderer)", () => {
  const report = (data: unknown, summary = "2 open of 3 scanned"): ToolRunReport => ({
    tool: "port_scan",
    ok: true,
    summary,
    tookMs: 42,
    data,
  });

  it("renders typed panels for tools that have one", () => {
    const data = {
      target: "example.com",
      pathMtu: 1420,
      probes: 10,
      verdict: "Below-standard MTU (1420): tunnel overhead; clamp MSS to 1380",
    };
    render(<ToolRunPanel tool="mtu" report={report(data)} />);
    expect(screen.getByText("1420")).toBeInTheDocument();
    expect(screen.getByText(/clamp MSS to 1380/i)).toBeInTheDocument();
  });

  it("renders honest summary plus structured JSON for generic tools", () => {
    const data = { ports: [{ port: 443, open: true, latencyMs: 12.3 }] };
    render(<ToolRunPanel tool="port_scan" report={report(data)} />);
    expect(screen.getByText(/2 open of 3 scanned/i)).toBeInTheDocument();
    expect(screen.getByText(/measured in 42 ms/i)).toBeInTheDocument();
    expect(screen.getByText(/"port": 443/)).toBeInTheDocument();
  });

  it("shows failures without inventing data", () => {
    const failed: ToolRunReport = {
      tool: "icmp_ping",
      ok: false,
      summary: "invalid target: ; rm -rf",
      tookMs: 0,
      data: null,
    };
    render(<ToolRunPanel tool="icmp_ping" report={failed} />);
    expect(screen.getByText(/invalid target/i)).toBeInTheDocument();
  });
});

describe("BridgesPanel", () => {
  const noop = () => undefined;

  it("lists bridges with honest mode labels", () => {
    render(
      <BridgesPanel
        bridges={[
          { name: "br0", mode: "l2_switch", members: ["eth0", "eth1"], state: "up" },
          { name: "SharedLink", mode: "nat_share", members: [], state: "active" },
        ]}
        busy={false}
        onCreate={noop}
        onRemove={noop}
      />,
    );
    expect(screen.getByText("br0")).toBeInTheDocument();
    expect(screen.getByTitle("True layer-2 switch")).toBeInTheDocument();
    expect(screen.getByTitle(/NAT sharing: different subnets/)).toBeInTheDocument();
  });

  it("keeps the create button disabled until the form is valid", () => {
    render(<BridgesPanel bridges={[]} busy={false} onCreate={noop} onRemove={noop} />);
    const btn = screen.getByRole("button", { name: /create bridge/i });
    expect(btn).toBeDisabled();
    expect(
      screen.getByText(/No bridges on this machine/i),
    ).toBeInTheDocument();
  });

  it("enables create with valid input and emits the spec", async () => {
    const onCreate = vi.fn();
    render(<BridgesPanel bridges={[]} busy={false} onCreate={onCreate} onRemove={noop} />);
    await act(async () => {
      fireEvent.change(screen.getByLabelText("Bridge name"), { target: { value: "br0" } });
      fireEvent.change(screen.getByLabelText("Member interfaces"), {
        target: { value: "Ethernet, Ethernet 2" },
      });
    });
    const btn = screen.getByRole("button", { name: /create bridge/i });
    expect(btn).toBeEnabled();
    await act(async () => {
      fireEvent.click(btn);
    });
    expect(onCreate).toHaveBeenCalledWith({
      name: "br0",
      members: ["Ethernet", "Ethernet 2"],
      mode: "l2_switch",
    });
  });
});

describe("AlertsCard", () => {
  it("shows an honest empty state", () => {
    render(<AlertsCard alerts={[]} />);
    expect(screen.getByText(/Nothing yet/i)).toBeInTheDocument();
  });

  it("renders recent alerts with severity", () => {
    render(
      <AlertsCard
        alerts={[
          {
            id: "a-1",
            severity: "warning",
            title: "Wi-Fi 6 went down",
            body: "status changed from up to down",
            timestampMs: 1,
          },
          {
            id: "a-2",
            severity: "info",
            title: "Wi-Fi 6 came up",
            body: "interface is back",
            timestampMs: 2,
          },
        ]}
      />,
    );
    expect(screen.getByText(/Wi-Fi 6 went down/i)).toBeInTheDocument();
    expect(screen.getByText(/Wi-Fi 6 came up/i)).toBeInTheDocument();
  });
});

describe("OptimizeView result panels", () => {
  it("ranks DNS resolvers and marks the recommended one", () => {
    const res: Response = {
      type: "dns_benchmark",
      results: [
        {
          server: "1.1.1.1",
          label: "Cloudflare",
          success: true,
          cachedMs: 8.2,
          uncachedMs: 30.1,
          score: 16.9,
          error: null,
        },
        {
          server: "8.8.8.8",
          label: "Google",
          success: true,
          cachedMs: 19.4,
          uncachedMs: 44.0,
          score: 29.2,
          error: null,
        },
      ],
      recommended: "1.1.1.1",
      systemResolvers: ["192.168.1.1"],
    };
    render(<ResultPanel response={res} />);
    expect(screen.getByText("1.1.1.1")).toBeInTheDocument();
    expect(screen.getByText(/fastest/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /set 1\.1\.1\.1 as system dns/i })).toBeInTheDocument();
    expect(screen.getByText(/192\.168\.1\.1/)).toBeInTheDocument();
  });

  it("grades bufferbloat per direction with measured added latency", () => {
    const res: Response = {
      type: "bloat_test",
      baselineMs: 11.5,
      downAddedMs: 180,
      upAddedMs: 640,
      downGrade: "d",
      upGrade: "f",
      downMbps: 212.4,
      upMbps: 21.3,
      durationS: 8,
      probeTarget: "1.1.1.1:443",
    };
    render(<ResultPanel response={res} />);
    expect(screen.getByText("D")).toBeInTheDocument();
    expect(screen.getByText("F")).toBeInTheDocument();
    expect(screen.getByText(/\+180 ms when busy/i)).toBeInTheDocument();
    expect(screen.getByText(/baseline latency 11\.5 ms/i)).toBeInTheDocument();
  });

  it("keeps an empty wifi scan honest instead of inventing networks", () => {
    const res: Response = {
      type: "wifi_scan",
      networks: [],
      channelLoad2g: [],
      channelLoad5g: [],
      recommendation2g: null,
      recommendation5g: null,
      explanation: "netsh wlan scan failed (Wi-Fi adapter off or no permission)",
    };
    render(<ResultPanel response={res} />);
    expect(screen.getByText(/wi-fi adapter off/i)).toBeInTheDocument();
    expect(screen.queryByText(/networks visible/i)).not.toBeInTheDocument();
  });

  it("highlights recommended wifi channels in the load list", () => {
    const res: Response = {
      type: "wifi_scan",
      networks: [
        {
          ssid: "A",
          bssid: "00:00:00:00:00:01",
          channel: 6,
          frequencyMhz: 2437,
          band: "2.4",
          signalPct: 70,
          security: "WPA2",
        },
      ],
      channelLoad2g: [{ channel: 6, networks: 1 }],
      channelLoad5g: [],
      recommendation2g: 11,
      recommendation5g: null,
      explanation: "2.4 GHz: channel 11 sees 0 co-channel and 0 overlapping networks. Best channel: 11.",
    };
    render(<ResultPanel response={res} />);
    expect(screen.getByText(/best channel: 11/i)).toBeInTheDocument();
    expect(screen.getByText(/1 networks visible/i)).toBeInTheDocument();
  });

  it("lists TCP suboptimal settings with their exact fix command", () => {
    const res: Response = {
      type: "tcp_audit",
      platform: "linux",
      elevated: false,
      checks: [
        {
          key: "tcp_slow_start_after_idle",
          current: "1",
          recommended: "0",
          status: "suboptimal",
          fix: "sysctl -w net.ipv4.tcp_slow_start_after_idle=0",
        },
        { key: "tcp_ecn", current: "1", recommended: "1", status: "ok", fix: null },
      ],
      summary: "1 of 2 settings below par; fixes need elevation and explicit confirmation",
    };
    render(<ResultPanel response={res} />);
    expect(screen.getByText("tcp_slow_start_after_idle")).toBeInTheDocument();
    expect(screen.getByText(/sysctl -w net\.ipv4\.tcp_slow_start_after_idle=0/)).toBeInTheDocument();
    expect(screen.getByText(/1 of 2 settings below par/i)).toBeInTheDocument();
  });

  it("reports route anomalies from the real routing table", () => {
    const res: Response = {
      type: "route_audit",
      entries: [
        { destination: "0.0.0.0/0", gateway: "192.168.1.1", interface: "eth0", metric: 100 },
      ],
      defaultCount: 1,
      anomalies: ["Classic VPN full-tunnel pattern (0.0.0.0/1 + 128.0.0.0/1)"],
    };
    render(<ResultPanel response={res} />);
    expect(screen.getByText(/full-tunnel/i)).toBeInTheDocument();
    expect(screen.getByText(/default route/i)).toBeInTheDocument();
    expect(screen.getByText("1")).toBeInTheDocument();
  });

  it("shows the measured path MTU and its verdict", () => {
    const res: Response = {
      type: "mtu",
      target: "example.com",
      pathMtu: 1420,
      probes: 10,
      verdict: "Below-standard MTU (1420): tunnel overhead; clamp MSS to 1380",
    };
    render(<ResultPanel response={res} />);
    expect(screen.getByText("1420")).toBeInTheDocument();
    expect(screen.getByText(/clamp MSS to 1380/i)).toBeInTheDocument();
  });

  it("renders speedtest numbers with an honest endpoint-failure note", () => {
    const res: Response = {
      type: "speed_test",
      endpoint: "https://speed.cloudflare.com",
      latencyMs: null,
      downloadMbps: 0,
      uploadMbps: 0,
      durationS: 8,
    };
    render(<ResultPanel response={res} />);
    expect(screen.getByText(/unreachable/i)).toBeInTheDocument();
    expect(screen.getByText(/the endpoint did not answer/i)).toBeInTheDocument();
  });

  it("reports a flushed DNS cache outcome", () => {
    const res: Response = {
      type: "repaired",
      action: "flush_dns",
      outcome: "applied",
      detail: "resolvectl flush-caches succeeded",
    };
    render(<ResultPanel response={res} />);
    expect(screen.getByText(/dns cache flushed/i)).toBeInTheDocument();
    expect(screen.getByText(/resolvectl flush-caches succeeded/i)).toBeInTheDocument();
  });
});
