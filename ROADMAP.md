# Roadmap

Live view: [truelite roadmap project](https://github.com/users/DavidGailleton/projects/3) ·
[milestones](https://github.com/DavidGailleton/truelite/milestones).
Each phase is a GitHub milestone; each task is an issue. Read-only features come first, then
actions behind confirmation dialogs. Every push to `main` releases a new version (see
[CONTRIBUTING.md](CONTRIBUTING.md)); v1.0.0 closes phase 8.

## Phase 1 — Skeleton

Repo, Axum server serving the embedded SvelteKit SPA, CI, container image, dev env, versioning.
Done when CI is green and every push to main releases a version.

- [x] #4 Axum server serving the embedded SvelteKit SPA
- [x] #5 CI and container image on GHCR
- [x] #6 Docker Compose dev environment with hot reload
- [x] #7 CLAUDE.md and UI style guide
- [x] #8 Release a new version on every push to main

## Phase 2 — API client + login

Server talks to TrueNAS (JSON-RPC over WSS), users log in with TrueNAS accounts, browser talks to
the server over our own WS protocol with a method allowlist. Done when a user can log in, see a
shell page and survive an upstream reconnect.

- [x] #9 TrueNAS client: connect, JSON-RPC id matching, API version pin
- [x] #10 Pin the NAS TLS certificate fingerprint
- [x] #11 Login with auth.login_ex and reconnect with auth.generate_token
- [x] #12 Event subscriptions via core.subscribe
- [x] #13 Mock TrueNAS JSON-RPC server for tests
- [x] #14 Sessions: cookie, idle timeout, logout
- [x] #15 Login endpoint with rate limit
- [x] #16 WebSocket bridge with our protocol and method allowlist
- [x] #17 Origin check on WS upgrade and security headers (CSP)
- [x] #18 Release profile tuning (lto, codegen-units, strip)
- [x] #19 Design tokens and app shell (icon rail / bottom tabs)
- [x] #20 Login page
- [x] #21 Browser WS client with reconnecting state
- [x] #22 Add vitest to the web app and CI

## Phase 3 — Dashboard (read-only)

One screen with what needs attention first: alerts, pool health, live CPU/RAM/net, apps summary.
Done when the dashboard works on a phone at 360 px with live data and no polling.

- [x] #23 System info card
- [x] #24 Live CPU, RAM and network with SVG sparklines
- [x] #25 Pool health and usage bars
- [x] #26 Apps summary card
- [x] #27 Alerts list with live updates, attention first
- [x] #28 Loading skeletons, empty and error states
- [x] #29 Light/dark theme toggle

## Phase 4 — Apps

List, inspect, follow logs, then start/stop/restart/upgrade apps with confirmation and job progress.

- [x] #30 Apps list with state
- [x] #31 App detail: containers, ports, image
- [x] #32 Follow app logs
- [x] #33 Start, stop and restart apps with job progress
- [x] #34 Upgrade apps with job progress

## Phase 5 — Storage

Pools, datasets, snapshots, scrubs, disks and SMART. Destructive actions need typed confirmation.

- [ ] #35 Pools page
- [ ] #36 Dataset tree with usage
- [ ] #37 Snapshots: list, create, delete
- [ ] #38 Scrub status and start
- [ ] #39 Disks and SMART results

## Phase 6 — Updates & reboot

See and apply TrueNAS updates with progress; reboot/shutdown with typed confirmation; clear
reconnecting state.

- [ ] #40 Show available update and release notes
- [ ] #41 Apply update with progress
- [ ] #42 Reboot and shutdown with typed confirmation
- [ ] #43 "NAS reconnecting" screen

## Phase 7 — PWA + polish

Installable PWA, offline/unreachable states, performance budget enforced, accessibility pass.

- [ ] #44 Web manifest and icons
- [ ] #45 Service worker caching the app shell
- [ ] #46 Offline and "NAS unreachable" states
- [ ] #47 Enforce the performance budget in CI
- [ ] #48 Accessibility pass
- [ ] #49 Lighthouse on a phone over Tailscale

## Phase 8 — Deploy & v1.0

Runs as a TrueNAS custom app from the public GHCR image, documented; release v1.0.0.

- [ ] #50 Make the GHCR package public
- [ ] #51 Run truelite as a TrueNAS custom app
- [ ] #52 Optional subdomain through the VPS proxy over Tailscale
- [ ] #53 README deploy guide and screenshots
- [ ] #54 Release v1.0.0

