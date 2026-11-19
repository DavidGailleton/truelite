# Contributing

The plan is in [ROADMAP.md](ROADMAP.md) and on the
[project board](https://github.com/users/DavidGailleton/projects/3). Pick an open issue, reference
it in your commit (`Closes #12`).

## Commits and versions

Every push to `main` is a release. After CI passes, the `release` job reads the
[conventional commits](https://www.conventionalcommits.org/) since the last tag, bumps the version,
updates `server/Cargo.toml`, `web/package.json` and `CHANGELOG.md`, tags `vX.Y.Z`, publishes a
GitHub Release and pushes the image `ghcr.io/davidgailleton/truelite:X.Y.Z` (+ `X.Y`, `latest`).

### TrueNAS API version

The TrueNAS API is pinned by `API_VERSION` in `server/src/truenas/mod.rs` (one literal; CI reads
it). From it, CI titles the release `vX.Y.Z — TrueNAS 25.10`, adds the image tags
`truenas25.10` (moving) and `X.Y.Z-truenas25.10`, and the label `io.truelite.truenas-api`. The
TrueNAS version is never part of the semver itself.

`main` always targets the newest supported TrueNAS; branches are not named after API versions.
When `main` moves to a new TrueNAS release and the previous one still needs fixes, cut a
maintenance branch `truenas/25.10` from its last release and release patches from there (the
CI triggers and the release push to `main` must then accept `truenas/*`). After
1.0, moving `main` to a new TrueNAS API is a breaking change (`feat!:`, major bump).

| Commit type | Bump while 0.x |
|---|---|
| `feat:` or a breaking change (`feat!:`, `BREAKING CHANGE:`) | minor (0.2.0 → 0.3.0) |
| anything else (`fix:`, `docs:`, `ci:`, `refactor:`, `chore:` …) | patch (0.2.0 → 0.2.1) |

- Never edit the version numbers by hand; CI owns them.
- CI pushes a `chore(release): vX.Y.Z` commit after yours: run `git pull --rebase` before your
  next push.
- v1.0.0 (and any forced version) is released from the Actions tab: run the CI workflow on `main`
  with the `version` input.

## Checks

Run what CI runs before pushing:

```sh
cd web && npm ci && npm run lint && npm run check && npm test && npm run build
cd server && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```
