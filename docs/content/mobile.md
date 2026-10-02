# LinkFYR Mobile

The mobile companion is a Tauri 2 app in `apps/mobile`, in this repo.
It shares the entire desktop UI source (`apps/mobile/src/main.tsx`
imports the desktop `App`) and is a **remote client** for `linkfyrd`:
the phone talks to the PC over your own network, sends the exact same
IPC envelopes the desktop app sends, and renders the same dashboard,
tools, bridges, and alerts.

No network engine runs on the phone by design. Mobile OSes forbid the
raw sockets and OS commands the toolkit needs; the daemon is the
engine, the phone is a first-class client. One contract, zero drift.

## 1. Start the daemon on your PC

```bash
# Linux/macOS
LINKFYR_WEB=0.0.0.0:58009 linkfyrd
# Windows (PowerShell)
$env:LINKFYR_WEB="0.0.0.0:58009"; linkfyrd --service   # or run foreground
```

The daemon prints the token file path. Its content is the token the
phone will use (`<config dir>/daemon.token`, 0600).

To get the full UI in the phone, copy the built frontend into the
daemon's web directory once:

```bash
pnpm build && cp -r apps/desktop/dist "<config dir>/web-dist"
```

Then `http://<pc-ip>:58009` already shows the dashboard in a phone
browser. The Tauri mobile app wraps that same UI with a native shell
and stores the token in the OS, so you do not need the copy step for
the app itself; it talks to `/api/` directly.

## 2. Build the mobile app

Prerequisites: Rust stable, Node 22+, pnpm, and the platform toolchain
for your target (Android Studio + NDK for Android, Xcode for iOS).

```bash
pnpm install
cd apps/mobile

# One-time platform init (generates src-tauri/gen/...)
pnpm tauri android init
pnpm tauri ios init

# Dev on a connected device/emulator
pnpm tauri android dev
pnpm tauri ios dev

# Release builds
pnpm tauri android build   # Play Store artifacts (.aab injected per docs)
pnpm tauri ios build       # Archive for TestFlight/App Store
```

Verification in Docker (the same gate CI runs):

```bash
powershell -File scripts/test/test-docker.ps1   # includes mobile typecheck + build
```

## 3. First run on the phone

1. Open the app; the setup screen asks for the PC address
   (`192.168.x.x:58009`) and the daemon token.
2. It pings the daemon once and then renders the full UI.
3. The token is stored only on this phone.

## Security posture

- The daemon binds loopback by default. Remote access is opt-in and
  LAN-scoped (`LINKFYR_WEB`); token auth (constant-time compare) is
  required on every request, and the phone sends it as a Bearer
  header.
- No cloud relay, no account, no telemetry. If you want access from
  outside your LAN, put the daemon behind your own VPN or reverse
  proxy; hosted relays are intentionally not part of v0.
- Threat model coverage: T1/T15 in `docs/content/threat-model.md`.

## Honest limits

- The phone cannot run tools that mutate the PC (DNS apply, bridges,
  enforcement) without the daemon being configured elevated there;
  capability states from the registry are shown as they really are.
- iOS background execution is restricted by the OS; the app works
  while foregrounded, which is the documented platform limit in
  `docs/content/platform-support.md`.
