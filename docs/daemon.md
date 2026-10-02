# linkfyrd installation

`linkfyrd` keeps the engine, optimization jobs, and bridges running
while the desktop app is closed (exam-safe operation: close the GUI,
the network configuration you set up stays up).

## Windows (service via SCM)

The binary speaks the Windows service protocol when started with
`--service`:

```powershell
# elevated
sc.exe create linkfyrd binPath= "C:\Program Files\LinkFYR\linkfyrd.exe --service" start= auto
sc.exe description linkfyrd "LinkFYR background network service"
sc.exe start linkfyrd
```

Remove with `sc.exe delete linkfyrd`.

## Linux (systemd)

```bash
sudo cp deploy/systemd/linkfyr.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now linkfyr
```

## macOS (launchd)

```bash
sudo cp deploy/launchd/com.linkfyr.daemon.plist /Library/LaunchDaemons/
sudo launchctl load /Library/LaunchDaemons/com.linkfyr.daemon.plist
```

## Transport and auth (v1)

Loopback TCP only (default `127.0.0.1:58008`), per-user token file with
`0600` permissions created on first run (`<config dir>/daemon.token`).
Clients authenticate with one JSON handshake line before any request.
Requests/responses are the exact `linkfyr-ipc` envelope contract, so
GUI, CLI (`linkfyr daemon status`), and the daemon cannot drift.

Hardening roadmap (threat model T1/T15): Windows named pipe with DACL,
unix domain socket + SO_PEERCRED peer credentials, replacing the token.
Elevated operations (bridge create, DNS apply) are still explicit user
actions; the daemon never auto-elevates.
