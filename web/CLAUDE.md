# web/CLAUDE.md

SvelteKit 3 SPA (Svelte 5, runes). Read the root `CLAUDE.md` and `docs/ui-style.md` first.

## Load the Svelte skills first

**Before creating, editing or reviewing a `.svelte`, `.svelte.ts` or `.svelte.js` file, load the
`svelte:svelte-core-bestpractices` and `svelte:svelte-code-writer` skills**, or hand the edit to
the `svelte:svelte-file-editor` subagent.

What the Svelte plugin (`svelte@svelte`, enabled in `.claude/settings.json`) provides:

- **MCP server `svelte`**:
  - `list-sections`: list the Svelte 5 / SvelteKit doc sections.
  - `get-documentation`: fetch one or more sections (`$state`, `$derived`, `routing`, …).
  - `svelte-autofixer`: analyse a component and suggest fixes.
- **CLI fallback** when the MCP tools aren't available:

  ```sh
  npx @sveltejs/mcp list-sections
  npx @sveltejs/mcp get-documentation "\$state,\$derived,\$effect"
  npx @sveltejs/mcp svelte-autofixer ./src/lib/Component.svelte --svelte-version <version from package.json>
  ```

  Escape `$` as `\$` when passing runes inline in the shell.
- **LSP** `svelteserver` for `.svelte` diagnostics.

Workflow: unsure of the syntax → `list-sections` then `get-documentation`; edit; **always run
`svelte-autofixer`** on the changed file and fix until it is clean.

Project rules (root `CLAUDE.md`) win over generic skill advice; the CI checks are listed there.
