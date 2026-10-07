# Research

This document records what Localhost is based on: the products it learns from, the stack it uses and the system interfaces underneath. Decisions are listed with the reason behind them, so you can disagree with a specific reason rather than with the whole design.

## Contents

- [The problem](#the-problem)
- [What existing tools do well](#what-existing-tools-do-well)
- [Where they are more complicated than they need to be](#where-they-are-more-complicated-than-they-need-to-be)
- [Product decisions](#product-decisions)
- [Desktop stack](#desktop-stack)
- [System interfaces](#system-interfaces)
- [Crates](#crates)
- [Open questions](#open-questions)

## The problem

A developer's machine accumulates processes: dev servers in terminal tabs, databases installed by a package manager, containers from three different Compose files, a forgotten `python -m http.server`. The operating system describes them as PIDs, executables and sockets. The developer thinks in projects, services and URLs. Every tool that exists today either speaks the operating system's language (Activity Monitor, Task Manager, `lsof`, `netstat`) or speaks the developer's language for one ecosystem only (Docker Desktop for containers, an IDE for its own run configurations).

Localhost translates between the two. It reads the raw signals and presents them as projects, services and addresses, with the evidence it used.

## What existing tools do well

- **Activity Monitor and Task Manager.** Instant, complete and trustworthy. Sorting by CPU or memory answers "what is slow" in one click. The inspector window shows open files and ports for a single process.
- **Docker Desktop.** Groups containers by Compose project, which is the right unit of thought. Logs, environment and a terminal sit one click away from each container.
- **OrbStack.** Proves that a developer tool can be fast and quiet: low idle CPU, native-feeling UI, `*.orb.local` names instead of port numbers.
- **Raycast.** Search is the primary navigation. Typing narrows everything at once, and actions appear next to results.
- **Xcode.** The navigator, editor and inspector split. The selection on the left drives detail on the right, and the inspector stays out of the way until you need it.
- **Vercel CLI and `next dev`.** Dev servers print a clear local URL and readable, colored output. Localhost keeps that output intact instead of reformatting it.
- **Process managers such as PM2, Foreman and Overmind.** A project file describes the processes, one command starts them and logs are prefixed per process.

## Where they are more complicated than they need to be

- **PIDs as the main identifier.** A PID is meaningless to a person and unstable over time. Localhost shows the service name, the project and the URL first, and keeps the PID in the inspector.
- **Everything at once.** Activity Monitor shows hundreds of system processes by default. Localhost shows only processes that listen on a port, belong to a project or were started by Localhost, and hides system services behind one line.
- **Separate silos.** Docker Desktop knows nothing about the Next.js server that talks to the container, and Activity Monitor knows nothing about Compose. Localhost puts both under the same project.
- **Configuration before value.** Process managers need a config file before they show anything. Localhost is useful at first launch with zero configuration and never requires a file in your repository.
- **Dashboards.** Card grids, charts and badges look busy and answer fewer questions than a well-sorted list. Localhost uses one list, one inspector and text.
- **Embedded terminals.** Docker Desktop and many IDEs embed a terminal that is worse than the one you already use. See [Terminal](#terminal).

## Product decisions

### Projects are inferred, not declared

Localhost starts from running processes and works backwards: working directory, then the nearest manifest, then the repository root. A directory with `.git`, a workspace file (`pnpm-workspace.yaml`, `turbo.json`, `nx.json`, `go.work`, a Cargo workspace) or a Compose file is a strong root signal. A lone manifest is a medium signal. You can also add a folder by hand to see its scripts while nothing runs.

### Services are process trees, not processes

`pnpm dev` starts `node`, which starts `cmd /c next dev`, which starts the server that owns the socket. The developer started one thing. Localhost walks up from the process that owns the port while the parent belongs to the same package and is not an interactive shell, terminal, editor or system process. The result is one service per thing you started, with CPU and memory summed across the tree.

### Identification is evidence-based

A signature table maps executables, command tokens, `node_modules` package paths and default ports to tools. Each match adds a weighted score, and the strongest score above a threshold wins. Project dependencies refine the result: Vite plus `@sveltejs/kit` becomes SvelteKit, and a bare `node server.js` with `express` in `package.json` becomes Express. The inspector shows the evidence in plain sentences, so a wrong guess is visible and explainable.

### Health means a response, not a process

A process can exist while its server hangs, and a socket in the listening state is accepted by the kernel even when the application is stuck. Localhost sends one lightweight request when a port first appears, and again when you select the service:

- HTTP: `HEAD /` with a short timeout. Any status below 500 counts as responding.
- Redis: `PING`.
- MySQL: the server greeting.
- PostgreSQL and others: a TCP connect only. PostgreSQL does not log a connection that sends no data.

There is no periodic polling of HTTP endpoints, because many dev servers log every request and compile on demand. Probes go only to loopback addresses, never to system services and never to other hosts. You can turn them off.

### Logs are honest about what is possible

No operating system lets one process read another process's stdout after the fact. Localhost captures output for services it starts and streams logs for Docker containers through the Engine API. For anything else it says so and offers to restart the service in Localhost: it stops the process, then runs the same command in the same folder with the same environment.

### Lifecycle is reversible where possible

- **Stop** sends the equivalent of Control-C: `SIGINT` to the process group on macOS and Linux, and a console `CTRL_C_EVENT` on Windows. If a service Localhost started does not exit within six seconds, Localhost ends its process tree. If a service started elsewhere does not exit, Localhost asks before forcing it.
- **Force quit** always asks first and names how many processes end.
- Stopping a service that Localhost started shows an Undo action.
- Quitting Localhost with running services asks first. Services it started stop with it, the same way they stop when you close a terminal.

### The boundary with Compose and scripts

Localhost runs the scripts your project already has (`package.json`, `deno.json`, Cargo, Django, Compose). It does not define its own process file and does not replace Docker Compose, Turborepo or a `Procfile`. If a project needs orchestration, that belongs in the project, where teammates and CI can use it.

### Terminal

Localhost does not embed a terminal. An embedded terminal needs a PTY layer (ConPTY on Windows), a terminal emulator in the window and its own key handling, and it would still be worse than the terminal you chose. Localhost opens your terminal in the right folder instead, and captures output for the services it starts. This keeps the interface small and the product focused.

## Desktop stack

| Option | Strengths | Weaknesses for this product |
| --- | --- | --- |
| Electron with a Rust engine | Mature window chrome on every platform (title bar overlay on Windows, vibrancy on macOS), the same toolkit and design system as Kumo, a predictable Chromium renderer | Larger download, more memory for the window |
| Tauri 2 | Small binaries, strong IPC permission model | System web views differ per platform. The first version of Localhost used it and was replaced to share Kumo's shell and design |
| Tauri 3 | Same model | Alpha in 2026 |
| egui | Pure Rust, fast to build | Immediate-mode look, limited text layout, does not feel native on macOS |
| Iced | Pure Rust, Elm architecture | Incomplete accessibility, limited text selection and rich text, which a log viewer needs |
| Slint | Declarative, good tooling | Custom markup and licensing terms, fewer native integrations |
| GPUI | Very fast, used by Zed | Built for Zed first, little documentation for third parties |

**Decision: Electron and React for the window, a Rust engine for everything else.** This is the same split Kumo uses for its relay: the Rust binary ships inside the app and Electron talks to it. Here the Rust side is the whole product. `localhost-engine` runs as a child process and speaks JSON lines over stdin and stdout. It owns process inspection, sockets, project detection, health, logs, lifecycle and secrets. The Electron main process only creates windows, menus, dialogs and notifications and relays messages. The renderer is sandboxed and only presents.

## System interfaces

| Need | macOS | Linux | Windows |
| --- | --- | --- | --- |
| Processes, CPU, memory, command line, working directory | `sysinfo` (libproc and sysctl) | `sysinfo` (`/proc`) | `sysinfo` (NtQuery and PEB reads) |
| Listening sockets with owner | `libproc` file descriptor walk | `/proc/net/tcp{,6}` plus socket inode to PID map from `/proc/*/fd` | `GetExtendedTcpTable` with `TCP_TABLE_OWNER_PID_LISTENER` |
| Graceful stop | `SIGINT` to the process group | `SIGINT` to the process group | `AttachConsole` and `GenerateConsoleCtrlEvent` from a short-lived helper process |
| Stop a whole tree | Process group | Process group | Job object with kill-on-close |
| Containers | Docker Engine API over the Unix socket | Same | Same over the `docker_engine` named pipe |
| Environment of another process | `sysinfo` (same user only) | `/proc/<pid>/environ` (same user only) | PEB read (same user only) |

Notes from the research:

- **Event-driven process monitoring** exists (the Linux proc connector, `kqueue` `NOTE_EXIT`, ETW on Windows), but each needs elevated rights or a large dependency, and none reports listening sockets. Localhost polls instead and adapts: every 2 seconds while the window is visible and every 10 seconds while it is hidden. Expensive fields (command line, working directory) are read once per process. Exits of services Localhost started are event-driven through the child handle.
- **File watchers** are not needed. Manifests are re-read only when their modification time changes, checked at most every 5 seconds per package.
- **The Ctrl+C trick on Windows** must run in a separate process, because attaching to another console changes process-wide state. The engine re-launches itself with a hidden flag for this. Windows also passes an "ignore Ctrl+C" flag from parent to child, so the engine clears it at startup to keep the services it starts stoppable.
- **GUI apps on macOS** do not inherit the shell `PATH`, so `pnpm` from Homebrew or nvm is invisible to them. Localhost reads the login shell's `PATH` once at startup and uses it only for the commands it runs.

## Crates

| Crate | Why |
| --- | --- |
| `sysinfo` 0.39 | Maintained, cross-platform process table with per-field refresh control |
| `libproc` 0.14 (macOS only) | Typed access to `proc_pidfdinfo` socket structures |
| `windows-sys` 0.61 (Windows only) | Raw bindings for the TCP table, consoles and job objects |
| `tokio` | Async runtime for the engine: process spawning, timers, sockets, named pipes, stdio |
| `serde`, `serde_json`, `toml` | Manifest parsing and IPC |
| `which` | Resolves `npm.cmd`, `pnpm` and other tools on `PATH` without a shell |
| Electron, React 19, `motion`, Zustand | Window shell and interface, shared with Kumo |

Rejected: `netstat2` (needs `bindgen` and `libclang` on Linux, returns every connection rather than listeners only) and `listeners` (resolves process names itself, duplicating `sysinfo`).

## Open questions

- **Menu bar extra on macOS.** A small status item with the number of running services fits the product. It is left out of the first version to keep the surface small.
- **Local hostnames.** OrbStack's `*.orb.local` names are pleasant. Doing the same for arbitrary processes needs a resolver and a proxy, which is a larger security surface than this version should have.
- **Process events instead of polling** where they do not need privileges, for example `kqueue` exits on macOS.
