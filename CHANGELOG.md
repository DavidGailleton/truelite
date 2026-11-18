# Changelog

Generated from conventional commits by git-cliff on every push to main.

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

