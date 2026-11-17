# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

truelite is a light, fast, mobile-friendly web UI for TrueNAS Community Edition (target **25.10
Goldeye**). A Rust (Axum) server embeds a SvelteKit SPA and bridges the browser to the TrueNAS
JSON-RPC 2.0 WebSocket API. MIT, English only, public repo `DavidGailleton/truelite`.

Project history, decisions and phase status live in the vault page
`~/obsidian-vault/Wiki/Projects/Truelite.md`; the full plan is
`~/.claude/plans/make-a-full-plan-ticklish-wilkes.md`. Update the vault page when a phase or a
decision changes.

## Commands

Docker dev env (only Docker needed; Vite hot reload on :5173, Rust server on :8080 rebuilt by
watchexec on change):

```sh
UID=$(id -u) GID=$(id -g) docker compose -f compose.dev.yaml up --build
docker compose -f compose.dev.yaml down -v   # reset node_modules / cargo caches
```

Without Docker (Rust stable, Node 26):

```sh
cd web && npm install && npm run dev          # :5173, proxies /api and /ws to :8080 (TRUELITE_BACKEND overrides)
cd web && npm run build                        # must run before a release build of the server
cd server && cargo run                         # config from env, see .env.example
```

Checks (exactly what CI runs, `.github/workflows/ci.yml`):

```sh
cd web && npm ci && npm run check && npm run build
cd server && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
cargo test health_ok                           # single test by name (from server/)
```

There is no JS test runner yet; the plan adds vitest. Release image: `docker build .`
(multi-stage: node build → musl static binary → `scratch`, ~5 MB). Every push to `main` releases a version and publishes
`ghcr.io/davidgailleton/truelite` (`X.Y.Z`, `X.Y`, `latest`, `sha-*`), see below.

## Architecture

```
Browser (Svelte SPA) ──WS /ws, session cookie──> truelite server (Axum) ──wss /api/v25.10.x JSON-RPC 2.0──> TrueNAS
```

- `server/` (crate `truelite`, edition 2024): `main.rs` builds the router (`router()` is `pub`
  so tests drive it with `tower::ServiceExt::oneshot`), `config.rs` reads `TRUELITE_*` env vars,
  `static_files.rs` serves `web/build` embedded with rust-embed: unknown paths fall back to
  `index.html` (SPA routing), unknown `/api/*` paths return 404, `_app/immutable/*` gets a
  1-year immutable cache and everything else `no-cache`.
- `web/` (SvelteKit 3, `adapter-static`, `ssr = false`): a pure SPA. **SvelteKit 3 has no
  `svelte.config.js`**: adapter and compiler options are in `vite.config.ts`. Runes mode is forced
  for project files. Import lib code via the `#lib` / `#lib/*` subpath imports (`package.json`).
- The server embeds `web/build` **at compile time**: build the web app first for a real binary.
  `#[allow_missing = true]` lets `cargo build` succeed without it in dev.

Planned (phases 2+), keep new code consistent with it:
- `server/src/truenas/` is the only place that knows the TrueNAS API: connection, `auth.login_ex`,
  then `auth.generate_token` for reconnects (the password is never stored), id matching, event
  subscriptions (`core.subscribe`), reconnect with backoff, API version pin. TrueNAS 27 switches
  to SCRAM auth; that change must stay inside this module.
- Browser ↔ server is our own small protocol, not raw JSON-RPC:
  `{id, op:"call", method, params}` → `{id, result|error}`; `{op:"sub", topic}` → `{topic, data}`.
  The server forwards **only allowlisted methods**; the allowlist grows per phase (reads first,
  then actions). Calls run as the logged-in TrueNAS user, so TrueNAS roles apply.
- Sessions: random id in an HttpOnly, Secure, SameSite=Strict cookie; one upstream WS per session;
  Origin check on WS upgrade, login rate limit, CSP headers. NAS TLS: pin the cert fingerprint,
  never "accept invalid certs". `midclt` over SSH was rejected; don't propose it.
- Check every TrueNAS method/event name against api.truenas.com (25.10) before using it.
- Phase order: API client + login → dashboard (read-only) → apps → storage → updates & reboot →
  PWA + polish → releases. Read-only first; every action needs a confirmation dialog.

## Performance rules

"Light and fast" is the product. Treat these as hard constraints:
- Initial JS < 100 KB gzip, first paint < 1 s on a phone over Tailscale. Check
  `web/build` sizes after adding anything.
- No CSS framework, no UI kit, no runtime icon package, no date/chart library without asking.
  Plain CSS with tokens, scoped component styles, small hand-written SVG charts (sparklines,
  usage bars). Inline only the Lucide icons used.
- Push live data over the WebSocket (TrueNAS events, `reporting.realtime`) instead of polling.
  Ask TrueNAS only for needed fields (`select` in `*.query` options); keep large payloads off the
  wire.
- Server: one static musl binary from `scratch`, async everywhere, no blocking calls on the
  runtime, no per-request allocation of big buffers. Prefer few, well-known dependencies with
  minimal features.

## UI and UX

The visual style follows Sure (we-promise/sure): read **`docs/ui-style.md`** before any UI work.
In short: neutral grayscale, white cards with hairline ring + soft shadow on a gray-50 page,
black primary button (white in dark mode), colour only for status, medium-weight type, Lucide
icons, desktop icon rail + mobile bottom tab bar. Sure is AGPL: reproduce the style, never copy
its code or assets.

- Every colour, size, radius and shadow comes from `web/src/lib/styles/tokens.css`, named by role
  (`--surface`, `--container`, `--text-secondary`, …). No hex values in components. Dark mode via
  `prefers-color-scheme` plus a `[data-theme]` override on `<html>`.
- Usability first: what needs attention (degraded pool, critical alert, update available)
  appears at the top of the dashboard; one primary action per screen; plain words, no TrueNAS
  jargon where a simpler word exists; status shown with text + colour, never colour alone.
- Mobile first at 360 px wide; touch targets 44 px on coarse pointers; safe-area insets.
- Every async view has loading (skeleton), empty and error states, and a "reconnecting to NAS"
  state when the socket drops.
- Destructive actions: confirmation dialog with a destructive button; delete and reboot require
  typing the name. Show job progress for long actions.
- Accessibility: semantic HTML (`<dialog>`, `<details>`, `<nav>`, `<main>`), `:focus-visible`
  outline, `aria-label` on icon-only buttons, `aria-current` on the active nav item.

## Roadmap, commits and versions

- The roadmap is GitHub: one milestone per phase, one issue per task, project board
  https://github.com/users/DavidGailleton/projects/3, mirrored in `ROADMAP.md` (tick the box when
  an issue closes; add new tasks as issues first). Reference the issue in commits (`Closes #12`).
- **Every push to `main` releases a version.** CI's `release` job (after `check`) bumps from
  conventional commits with git-cliff (`cliff.toml`): `feat`/breaking → minor, anything else →
  patch while 0.x. It rewrites `server/Cargo.toml`, `Cargo.lock`, `web/package.json`,
  `package-lock.json`, `CHANGELOG.md`, commits `chore(release): vX.Y.Z`, tags, creates the GitHub
  Release, then the `image` job pushes `X.Y.Z`, `X.Y`, `latest`, `sha-*`.
- So: always write conventional commit messages, never bump versions by hand, and
  `git pull --rebase` before the next push (CI added a commit). v1.0.0 = manual
  `workflow_dispatch` with the `version` input. Details in `CONTRIBUTING.md`.

## Gotchas

- GHCR packages start private: the NAS can only pull the image once the package is public.
- CI actions are on current majors (checkout/setup-node v7, docker/* v4–v7, Nov 2026).
- In compose dev, files under `web/` are created as `UID:GID`; without those vars they belong
  to 1000:1000.
