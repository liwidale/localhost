# Architecture

Localhost has three layers. A Rust engine owns every decision about processes, ports, projects and security. An Electron main process owns windows, menus and dialogs. A React interface renders what the engine reports and sends commands back.

```
┌────────────── renderer (React, sandboxed) ──────────────┐
│ views, store, actions, i18n            window.localhost  │
└───────────────────────────────┬──────────────────────────┘
                                │ contextBridge + ipcMain
┌──────────────── main (Electron) ┴─────────────────────────┐
│ window.ts  menus.ts  ipc.ts  engine.ts  index.ts          │
│ windows, native menus, dialogs, notifications, shell      │
└───────────────────────────────┬──────────────────────────┘
                                │ JSON lines over stdin/stdout
┌──────────────── localhost-engine (Rust) ┴────────────────┐
│ engine   scan loop, service grouping, actions, health    │
│ system   process table, listening sockets, signals, jobs │
│ detect   signatures, confidence, project roots, scripts  │
│ runner   services started by Localhost                   │
│ docker   Engine API over a Unix socket or named pipe     │
│ health   HTTP, Redis and TCP probes on loopback          │
│ logs     ring buffer, ANSI parsing, port conflicts       │
│ redact   secrets in environments, arguments and URLs     │
│ store    settings, saved projects, trusted roots         │
│ desktop  open a terminal or editor in a folder           │
└──────────────────────────────────────────────────────────┘
```

## Contents

- [Data model](#data-model)
- [Engine protocol](#engine-protocol)
- [Scan loop](#scan-loop)
- [From processes to services](#from-processes-to-services)
- [Actions](#actions)
- [Security](#security)
- [Performance](#performance)
- [Platform support](#platform-support)
- [Interface](#interface)

## Data model

All types live in `crates/localhost-core/src/model.rs`, serialize to camelCase JSON and are mirrored in `src/shared/types.ts`.

| Type | Meaning |
| --- | --- |
| `Snapshot` | Everything the window shows: projects, services, Docker availability and the platform |
| `Project` | A root folder with its ecosystems, package manager, runnable scripts and whether you saved and trusted it |
| `Service` | One thing you started: a process tree, a container or a command Localhost runs. Holds identity, status, ports, resource use and redacted command |
| `Identity` | Tool, kind, runtime, confidence and the evidence behind it |
| `Port` | A listening port with its bind address, protocol, health and URL |
| `Health` | `listening`, `responding`, `error` or `unreachable`, with HTTP status and latency |
| `ServiceDetail` | Loaded on demand: process tree, ancestry, redacted environment and executable |
| `Settings` | System services, health checks, notifications, theme, motion and language |

Service IDs are stable and safe against PID reuse:

- `p<pid>-<start time>` for a process tree,
- `s<project>-<hash>` for a script Localhost runs, stable across restarts,
- `a<n>-<pid>` for a process restarted in Localhost,
- `c<container id>` for a container.

The engine keeps an internal reference for each ID. Actions look up that reference instead of trusting anything the window sends.

## Engine protocol

Electron starts `localhost-engine --data <folder> --host-pid <pid>` with pipes for stdin and stdout. Each line is one JSON message.

```json
{ "id": 7, "method": "stop", "params": { "id": "p4120-1791389128", "force": false } }
{ "id": 7, "ok": true, "value": null }
{ "id": 8, "ok": false, "error": { "code": "protected" } }
{ "event": "snapshot", "data": { "projects": [], "services": [] } }
{ "event": "engine", "data": { "type": "crashed", "id": "s1f…", "title": "dev", "code": 1 } }
{ "event": "logs", "data": { "sub": 3, "lines": [] } }
```

Methods: `snapshot`, `settings`, `setSettings`, `refresh`, `setVisible`, `inspect`, `reveal`, `stop`, `restart`, `stopProject`, `startScript`, `dismiss`, `clearLogs`, `subscribeLogs`, `unsubscribeLogs`, `url`, `folder`, `openTerminal`, `openEditor`, `editor`, `addProject`, `keepProject`, `removeProject` and `runningManaged`.

When stdin closes, the engine stops the services it started and exits. If the engine exits unexpectedly, Electron starts it again after one second.

## Scan loop

`Engine::start` runs one loop on a two-thread Tokio runtime:

1. Refresh Docker containers when due (every 3 seconds while visible, 15 while hidden).
2. On a blocking thread: refresh the process table, read listening sockets, group services, locate projects and read manifests.
3. Publish the snapshot. The engine writes it to stdout and Electron forwards it to the window.
4. Start health probes for ports seen for the first time.
5. Sleep 2 seconds while the window is visible and 10 seconds while it is hidden or minimized, or wake early when an action or a probe asks for a refresh.

## From processes to services

1. **Listeners.** Each listening socket names its owning PID. Sockets owned by the engine, by Electron (`--host-pid`) and by their helper processes are ignored, except for services Localhost started. Sockets owned by Docker or WSL port forwarders are attributed to the container that published the port.
2. **Roots.** From each owning process, the engine walks up the parent chain while the parent belongs to the same user, is not protected, is not a terminal, editor, init system or interactive shell, and sits in the same package folder. Script shells such as `cmd /c` and `sh -c` are part of the service. The walk stops at package boundaries, so a monorepo task runner that starts two apps produces two services.
3. **Tasks.** Owned processes without ports but with a strong task signature (`tsc --watch`, `cargo watch`, `docker compose up`) become services too.
4. **Membership.** Each process belongs to its nearest root, so nested services never double-count CPU or memory.
5. **Identity.** `detect::classify` scores every member against the signature table, refines with project dependencies and records evidence.
6. **Projects.** `detect::project` finds the root and package folder for each service and caches the answer. Manifests are re-read when their modification time changes.
7. **Health and conflicts.** Ports that appear in more than one service are marked as shared. Probe results attach to `(service, port)`.

## Actions

| Action | Process started elsewhere | Service started by Localhost | Container |
| --- | --- | --- | --- |
| Stop | Ctrl+C equivalent, then asks to force | Ctrl+C equivalent, forced after 6 seconds if the same process is still running | `POST /containers/{id}/stop` |
| Force quit | Kills the tree, leaves first | Kills the job or process group | `POST /containers/{id}/kill` |
| Restart | Stops it, then runs the same command, folder and environment under Localhost | Stops and starts the same spec | `POST /containers/{id}/restart` |
| Logs | Not available, explained in the window | Captured stdout and stderr | Streamed with `follow=1` |

Every process action re-reads the process table and checks the start time before it signals anything.

## Security

- **Least privilege.** Localhost never asks for administrator rights. Processes that belong to another user are visible but not controllable, and the window says why.
- **No network IPC.** The engine talks only to its parent through stdin and stdout. Nothing listens on a port.
- **Sandboxed window.** The renderer runs with `sandbox`, `contextIsolation` and no Node integration. Navigation and new windows are blocked, and a strict content security policy forbids remote content, inline scripts and network requests.
- **Narrow bridge.** The preload exposes one object, `window.localhost`, with fixed methods. The main process trims and type-checks every argument before it reaches the engine.
- **No shell.** Commands run as program plus argument vector. Scripts run through their own tool (`pnpm run dev`, `deno task`, `cargo run`). Rust's standard library escapes arguments for `.cmd` files on Windows and refuses unsafe ones.
- **Trust before running.** The first time you run a script from a project, Localhost shows the exact command and folder and asks. The answer is stored per project root.
- **Untrusted input.** Manifests are read only from regular files under 1 MB, symlinks are ignored and parsing errors are skipped. Script IDs from the window are looked up in the manifest on disk and never executed as strings.
- **Secrets.** Environment values with secret-like names or token-like values are replaced with `••••••` in Rust before they leave the engine. Passwords in URLs are masked in commands, environments and log lines. Revealing a value is an explicit action that reads the single variable from the running process.
- **Probes.** Health checks go only to loopback, only to services that are not system services, at most once per port appearance or selection.
- **Opening things.** URLs are built by the engine from known listeners and checked again in the main process before `shell.openExternal`. Folders come from the engine's own project and service references.
- **Docker.** Container IDs are validated as hexadecimal before they appear in an API path, and only four actions are allowed.

## Performance

Measured on Windows 11 with the packaged build, window visible:

- The engine uses about 10 MB of memory and 0.4% of one CPU core.
- One process table refresh per tick, with command line, working directory and user read once per process.
- Socket tables are read directly from the OS. On Linux the inode-to-PID map is rebuilt only when an unknown socket appears.
- Manifests and project roots are cached with short time-to-live checks instead of file watchers.
- The window receives one snapshot per tick through a single IPC message. The log viewer is virtualized and keeps at most 5,000 lines. Log lines are batched for 60 ms.

## Platform support

| Capability | macOS | Linux | Windows |
| --- | --- | --- | --- |
| Processes and listening ports | Yes | Yes | Yes |
| Ports of other users' processes | Owner visible only for your own processes | Owner visible only for your own processes | Yes, owner shown as a system service |
| Graceful stop | `SIGINT` | `SIGINT` | Ctrl+C for console programs. GUI programs need Force quit |
| Environment of other processes | Your own processes | Your own processes | Your own processes |
| Containers | Docker Desktop, OrbStack, Colima, Rancher Desktop sockets | Docker socket, rootless socket | Docker Desktop named pipe |
| Login shell `PATH` for scripts | Yes | Yes | Uses the system `PATH` |
| WSL services | Not applicable | Not applicable | Shown as a WSL port forward. The process inside WSL is not visible |
| Window chrome | Inset traffic lights, sidebar vibrancy, native menu bar | Standard frame | Title bar overlay with native window buttons |

UDP listeners are not shown. Development servers almost always use TCP, and UDP sockets add noise without telling you what owns a port you want to use.

## Interface

The interface follows the Kumo design: the same tokens (`src/renderer/styles/tokens.css`), sidebar navigation, large section titles, grouped rounded boxes with hairline rows, segmented controls with a sliding background, spring toggles, a pill toast and centered dialogs. Windows uses Segoe UI Variable, smaller radii, blue primary buttons and Fluent-style toggles. macOS uses SF Pro, vibrancy in the sidebar and dark-on-light primary buttons.

- `src/renderer/App.tsx` lays out the sidebar and content and handles keyboard shortcuts and engine events.
- `src/renderer/store.ts` holds navigation, selection, dialogs and toasts in a small Zustand store.
- `src/renderer/actions.ts` decides when to confirm and turns errors into sentences.
- `src/renderer/views/` contains the service lists, ports table, service detail, logs, environment, processes and settings.
- `src/shared/i18n.ts` and `src/shared/locales/` hold translations. English is the source language. Russian is included.

Motion uses the `motion` library for page transitions, dialogs, toasts, toggles and segmented controls, and stops when you choose reduced motion in Settings.
