# truelite

A light, fast, mobile-friendly web interface for [TrueNAS](https://www.truenas.com/) Community Edition.

> Status: early development (phase 1, skeleton). Not usable yet.
> truelite is an independent project, not affiliated with iXsystems.

## Goals

- **Light and fast:** a small static web app and a ~5 MB container image.
- **Usability first:** the things you check every day (apps, storage, alerts, updates) on one screen.
- **Responsive:** works on a phone; installs as a PWA.
- **Safe:** read-only first; every action goes through TrueNAS permissions and asks for confirmation.

## How it works

```
Browser (Svelte app) ──WebSocket──> truelite server (Rust) ──WebSocket JSON-RPC 2.0──> TrueNAS
```

You log in with your TrueNAS account. The server talks to the TrueNAS API
(`/api/v25.10.x`) as you, so TrueNAS roles apply. The browser never talks to TrueNAS directly
and only allowed methods are forwarded.

Targets TrueNAS **25.10** (Goldeye).

## Development

Requirements: Rust (stable), Node.js 26, npm.

```sh
# web app (dev server on :5173, proxies /api and /ws to the Rust server)
cd web && npm install && npm run dev

# server (serves the built web app from web/build)
cd web && npm run build
cd ../server && cargo run
```

Configuration is read from environment variables, see [`.env.example`](.env.example).

Checks run in CI: `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`, `npm run check`.

## Deploy on TrueNAS

The image is published to `ghcr.io/davidgailleton/truelite`. Install it as a Custom App
(Apps → Discover → Install via YAML) with [`compose.example.yaml`](compose.example.yaml).

## Roadmap

1. Skeleton, CI, container image
2. TrueNAS API client and login
3. Dashboard (system, pools, apps, alerts; read-only)
4. Apps: list, logs, start/stop/restart/update
5. Storage: pools, datasets, snapshots, scrub, disks
6. Updates and reboot
7. PWA and polish
8. Releases

## Licence

[MIT](LICENSE)
