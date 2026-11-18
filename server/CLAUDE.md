# server/CLAUDE.md

Rust (Axum) server, crate `truelite`, edition 2024. Read the root `CLAUDE.md` first.

## Load the skills first

- **Before writing, reviewing or refactoring any Rust here, load the `rust-skills` skill**
  (`/rust-skills`, source in `.agents/skills/rust-skills`).
- For routes, extractors, shared state, middleware, WebSockets or router tests, also load the
  `axum` skill (`/axum`, Axum 0.8).

If a skill can't be loaded, use what it provides directly:

- `rust-skills`: 265 rules named by prefix, applied CRITICAL > HIGH > MEDIUM > LOW. Each rule has a
  file in `.agents/skills/rust-skills/rules/`. Pick the categories by task:

  | Task | Prefixes |
  |------|----------|
  | New function | `own-` `err-` `name-` `pat-` |
  | New struct / API | `api-` `type-` `conv-` `doc-` |
  | Async code | `async-` `own-` |
  | Concurrency | `conc-` `async-` `own-` |
  | Error handling | `err-` `api-` `pat-` |
  | Serde | `serde-` `type-` `api-` |
  | Logging | `obs-` `err-` |
  | Memory / performance | `mem-` `opt-` `perf-` |
  | Unsafe | `unsafe-` `type-` `test-` |
  | Code review | `anti-` `lint-` |

- `axum`: `.claude/skills/axum/SKILL.md`.

Project rules (root `CLAUDE.md`) win over generic skill advice; the CI checks are listed there.
