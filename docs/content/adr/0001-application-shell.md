# ADR-0001: Application Shell â€” Tauri 2 over Alternatives

Status: Accepted (2026-09-30)
Deciders: LinkFYR architecture team

## Context

LinkFYR needs one application shell that reaches every target platform:
Windows x64/ARM64, macOS Apple Silicon/Intel, Linux x64/ARM64, Android
ARM64/x86_64, iOS/iPadOS ARM64 â€” plus headless targets (servers, SBCs,
routers, containers) where **no GUI shell exists at all**. The networking
engine is Rust. The UI must support premium real-time charts, animation,
dark/light theming, accessibility, and a signed auto-updater.

## Options evaluated (checked 2026-09-30)

| Option | Version | All-OS reach | UI quality ceiling | Accessibility | Updater | Verdict |
|---|---|---|---|---|---|---|
| **Tauri 2** | 2.12.0 | Desktop stable; iOS/Android stable channel | Full web platform (charts, animation, a11y) | Web a11y tree | First-class plugin (`tauri-plugin-updater`, signed manifests) | **Chosen** |
| Electron | 44.5.1 | Desktop + limited mobile story | Full web platform | Web a11y | Mature | ~10Ã— memory baseline; bundles Chromium; contradicts low-footprint goal |
| Dioxus | 0.7.10 | Desktop + mobile (native renderer) | Improving but 0.x; chart/animation ecosystem immature | Weak | Custom | Re-evaluate yearly; shares Rust model |
| iced | 0.14.0 | Desktop; mobile/experimental | Immediate-mode; heavy custom charting work | Weak | Custom | Not viable for premium a11y UI |
| egui | 0.36.2 | Desktop; mobile weak | Power-tool aesthetic only | Weak | Custom | Wrong product fit |
| Slint | 1.18.1 | Desktop + mobile + embedded | Good | Moderate | Custom | **GPL/commercial dual license** â€” GPL terms are incompatible with our distribution model, and the commercial license is a fee we will not pay (project is fully free) |
| Flutter | 3.x stable | Mobile strong, desktop weaker | High | Good | Mature (mobile), weaker desktop | Second language (Dart) alongside Rust core; desktop windowing/IPC maturity below web |
| Qt/QML (CXX-Qt) | 6.x | Desktop + mobile | High | Good | Mature | Licensing cost + C++ toolchain complexity |
| .NET MAUI / Avalonia | 11.x | Desktop + mobile | Good | Good | Moderate | Forces C# runtime; networking engine would still be Rust â†’ two runtimes |

## Decision

**Tauri 2 is the application shell.** One TypeScript/React frontend, one
Rust core, system WebView per platform (WebView2 / WKWebView / WebKitGTK).

## Anti-lock-in mitigation (binding architectural rule)

The shell is a *replaceable client*. All product capability lives in Rust
crates behind the stable `linkfyr-ipc` API surface:

```
UI (Tauri 2) â”€â”
CLI â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”¼â”€â†’ linkfyr-ipc (stable, versioned, typed) â†’ linkfyr-core â†’ engine
linkfyrd â”€â”€â”€â”€â”€â”˜         â”‚
remote/mobile clients â”€â”€â”˜ (authenticated transport, later phase)
```

Rules that follow:

1. **No business logic in the frontend.** The frontend renders state and
   issues commands; everything else is Rust.
2. **No Tauri types outside `apps/desktop`.** Crates never depend on
   `tauri`; only the desktop app wires Tauri commands to `linkfyr-ipc`.
3. **Headless parity.** Every GUI action must be expressible via
   `linkfyrd` + `linkfyr` CLI. CI enforces feature parity for core APIs.
4. If Tauri mobile ever proves insufficient, a native mobile shell can be
   built against the same IPC API without touching the engine.

## Consequences

- + Smallest memory footprint of the viable options; signed updates built in.
- + Single UI codebase for 6 GUI platforms; headless targets skip the shell entirely.
- âˆ’ WebView variance (WebKitGTK on Linux) â€” mitigated by a browser-support
  matrix in CI and avoiding bleeding-edge CSS in core flows.
- âˆ’ Tauri mobile is younger than desktop â€” acceptable: mobile phases are
  Phase 8; the IPC boundary keeps the option to switch shells open.
