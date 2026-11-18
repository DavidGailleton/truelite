# Changelog

Generated from conventional commits by git-cliff on every push to main.

## v0.15.0 — 2026-11-18

### Features

- Start, stop and restart apps with job progress
- Upgrade apps with job progress
- **web:** Start, stop, restart and update apps from the apps list

### Fixes

- **web:** Keep action progress until the app settles
- **web:** Show action progress as text, without a bar
- **web:** Hide app links unless running, show restart for at least 2 s
- **web:** Align app list pills and hover the whole row
- **server:** Keep log tails and events from being lost
- **server:** Find an action's job when its first event was missed
- **web:** Never send an action long after it was confirmed
- **web:** App actions match what the user saw and end at the right time

## v0.14.0 — 2026-11-18

### Features

- **web:** Apps list with state, version and filter
- **web:** App detail with containers, ports and folders
- Follow app container logs

## v0.13.0 — 2026-11-18

### Features

- **web:** Shared card states with a stale-data notice
- **web:** Light/dark theme toggle in the account menu
- **web:** Sun/moon theme button in the top right
- **web:** Keep the theme button fixed in the top-right corner
- **web:** Theme button scrolls with the page on phones

### Fixes

- **server:** Keep live stats when one core or interface is malformed
- **web:** Reset dashboard data on logout and recover from stale errors

### Refactoring

- **web:** Cards share store summaries, one ago() helper

### Other

- **web:** Theme button without its square background

## v0.12.0 — 2026-11-18

### Features

- **web:** System info card and dashboard header
- **server:** Allow pool queries and reshape pool events
- **web:** Pool health and usage card on the dashboard
- **server:** Allow app queries and reshape app events
- **web:** Apps summary card on the dashboard
- **server:** Reshape alert.list events and results
- **web:** Alerts card with live updates, attention first

## v0.11.0 — 2026-11-18

### Features

- **server:** Allow and reshape reporting.realtime
- **web:** Live CPU, RAM and network cards with SVG sparklines

## v0.10.1 — 2026-11-18

### Fixes

- **web:** Say "unexpected answer" instead of "can't reach" on an unknown login reply

### Refactoring

- **web:** Reuse Icon and endSession, simplify safeNext and the banner check

### Documentation

- Add server/ and web/ CLAUDE.md with skill loading

### Other

- Commit the agent skills and Claude settings
- **web:** Add a Prettier config and lint check
- **web:** Format with Prettier

## v0.10.0 — 2026-11-18

### Features

- **web:** Login page with one-time code step

### Fixes

- **server:** Say why a login has to start again
- **web:** End the code step when the server drops the login, accept codes with spaces
- **web:** Keep the focus outline on login fields
- **web:** Reload the session when it changes, keep the hash after logging in again

## v0.9.0 — 2026-11-18

### Features

- **web:** Design tokens and app shell with icon rail and bottom tabs

### Performance

- **server:** Tune the release profile (opt-level s, fat LTO, 1 codegen unit, strip)

### Other

- Tick release profile tuning in the roadmap

## v0.8.1 — 2026-11-18

### Tests

- **server:** Share browser test helpers between ws and login tests
- **server:** End-to-end journey from login through reconnect to logout

## v0.8.0 — 2026-11-18

### Features

- **web:** WebSocket client with reconnecting state

### Fixes

- **web:** Keep the browser's Host through the dev proxy

### Documentation

- CLAUDE.md commit rule, ignore local agent skills

### CI and build

- **web:** Add vitest and run it in CI

## v0.7.0 — 2026-11-18

### Features

- **server:** Origin check and security headers

## v0.6.0 — 2026-11-18

### Features

- **server:** WebSocket bridge with our protocol and method allowlist

## v0.5.0 — 2026-11-18

### Features

- **server:** Sessions with a secure cookie, idle timeout and logout
- **server:** Login endpoint with one-time codes and a rate limit

### Tests

- **server:** Keep the refused port reserved in connect_refused

## v0.3.0 — 2026-11-18

### Features

- **server:** Log in with auth.login_ex and reconnect with a token
- **server:** Event subscriptions via core.subscribe

## v0.2.0 — 2026-11-18

### Features

- **server:** TrueNAS JSON-RPC client with id matching and API pin
- **server:** Pin the NAS TLS certificate fingerprint

### Tests

- **server:** Opt-in smoke test of the pinned TLS connection to a real NAS

## v0.1.1 — 2026-11-17

### Documentation

- Add roadmap, contributing guide, CLAUDE.md and UI style guide
- Drop phase 0 (NAS upgrade done) from the roadmap

### CI and build

- Release a new version on every push to main

## v0.1.0 — 2026-11-17

### Other

- Initial skeleton: Axum server with embedded SvelteKit app, CI, container image
- Add Docker Compose dev environment with hot reload

