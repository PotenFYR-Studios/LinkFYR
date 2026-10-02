/**
 * Hand-drawn icon set, specific to networking concepts.
 * Deliberately not a generic icon library: these glyphs encode meaning
 * (an Ethernet jack is a jack, not a "plug" abstraction).
 */
import type { SVGProps } from "react";

type IconProps = SVGProps<SVGSVGElement> & { size?: number };

function base({ size = 16, ...props }: IconProps): SVGProps<SVGSVGElement> {
  return {
    width: size,
    height: size,
    viewBox: "0 0 16 16",
    fill: "none",
    stroke: "currentColor",
    strokeWidth: 1.4,
    strokeLinecap: "round",
    strokeLinejoin: "round",
    "aria-hidden": true,
    ...props,
  };
}

export const IconOverview = (p: IconProps) => (
  <svg {...base(p)}>
    <path d="M1.5 8h3l1.5-4 2.5 8L10 8h4.5" />
  </svg>
);

export const IconInterfaces = (p: IconProps) => (
  <svg {...base(p)}>
    <rect x="1.5" y="5.5" width="4" height="5" rx="1" />
    <rect x="10.5" y="5.5" width="4" height="5" rx="1" />
    <path d="M5.5 8h5" />
  </svg>
);

export const IconInternet = (p: IconProps) => (
  <svg {...base(p)}>
    <circle cx="8" cy="8" r="6" />
    <path d="M2 8h12M8 2c1.8 1.6 2.7 3.7 2.7 6S9.8 12.4 8 14c-1.8-1.6-2.7-3.7-2.7-6S6.2 3.6 8 2Z" />
  </svg>
);

export const IconRules = (p: IconProps) => (
  <svg {...base(p)}>
    <path d="M2.5 4h8M2.5 8h5M2.5 12h6" />
    <path d="m11 10.5 2 2 3-3.5" transform="translate(-1.5 -0.5)" />
  </svg>
);

export const IconSettings = (p: IconProps) => (
  <svg {...base(p)}>
    <circle cx="8" cy="8" r="2.2" />
    <path d="M8 1.8v1.7M8 12.5v1.7M1.8 8h1.7M12.5 8h1.7M3.6 3.6l1.2 1.2M11.2 11.2l1.2 1.2M12.4 3.6l-1.2 1.2M4.8 11.2l-1.2 1.2" />
  </svg>
);

/** Gauge with needle: the optimize view (measured, not decorative). */
export const IconOptimize = (p: IconProps) => (
  <svg {...base(p)}>
    <path d="M2.5 11a5.5 5.5 0 0 1 11 0" />
    <path d="M8 11 10.5 7" />
    <circle cx="8" cy="11" r="0.9" />
  </svg>
);

export const IconSun = (p: IconProps) => (
  <svg {...base(p)}>
    <circle cx="8" cy="8" r="3" />
    <path d="M8 1v1.5M8 13.5V15M1 8h1.5M13.5 8H15M3 3l1 1M12 12l1 1M13 3l-1 1M4 12l-1 1" />
  </svg>
);

export const IconMoon = (p: IconProps) => (
  <svg {...base(p)}>
    <path d="M13.5 9.5A5.5 5.5 0 0 1 6.5 2.5a5.5 5.5 0 1 0 7 7Z" />
  </svg>
);

export const IconSearch = (p: IconProps) => (
  <svg {...base(p)}>
    <circle cx="7" cy="7" r="4.5" />
    <path d="m10.5 10.5 3.5 3.5" />
  </svg>
);

export const IconDown = (p: IconProps) => (
  <svg {...base(p)}>
    <path d="M8 2v9M4.5 7.5 8 11l3.5-3.5M3 13.5h10" />
  </svg>
);

export const IconUp = (p: IconProps) => (
  <svg {...base(p)}>
    <path d="M8 11V2M4.5 5.5 8 2l3.5 3.5M3 13.5h10" />
  </svg>
);

export const IconLink = (p: IconProps) => (
  <svg {...base(p)}>
    <path d="M6.5 9.5a2.8 2.8 0 0 0 4 0l2.5-2.5a2.83 2.83 0 0 0-4-4L7.8 4.2" />
    <path d="M9.5 6.5a2.8 2.8 0 0 0-4 0L3 9a2.83 2.83 0 0 0 4 4l1.2-1.2" />
  </svg>
);

/** Brand mark: two interlocking links. Used in the rail footer. */
export const LogoMark = ({ size = 20 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" aria-hidden>
    <rect x="1.5" y="8.5" width="12.5" height="7" rx="3.5" stroke="var(--color-accent)" strokeWidth="2" />
    <rect x="10" y="8.5" width="12.5" height="7" rx="3.5" stroke="var(--color-accent-soft)" strokeWidth="2" />
  </svg>
);
