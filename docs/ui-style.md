# UI style guide

truelite's look follows the style of [Sure](https://github.com/we-promise/sure) (the community fork
of Maybe Finance), studied from its design system (`app/assets/tailwind/sure-design-system/`,
`app/components/DS/`, `design/tokens/sure.tokens.json`) and dashboard screenshots.

**Inspiration only.** Sure is AGPL-3.0 and truelite is MIT: re-implement the style in our own
plain CSS and Svelte components. Never copy Sure's code, templates, logo or images.

## 1. The style in one paragraph

Quiet, neutral, "pro tool" minimalism. Almost everything is grayscale: a light-gray page, white
cards that float on it with a hairline ring plus a very soft shadow (no visible borders), near-black
text, and a **black primary button** (white in dark mode). Colour is reserved for meaning: green
for good, red for bad, amber for warning, blue for links. Numbers are big and medium-weight, labels
are small and gray. Generous whitespace, 10–12 px rounded corners, one font (Geist, a geometric
sans) with a mono companion for figures. Dark mode is a true near-black inversion, not a blue-gray
theme. Nothing is decorative: no gradients, no illustrations, no heavy shadows.

## 2. Colour

Neutral gray ramp (light mode values; dark mode inverts the roles, see §2.3):

| Token | Hex | Used for |
|---|---|---|
| gray-25 | `#FAFAFA` | |
| gray-50 | `#F7F7F7` | page background (`surface`), inset areas, secondary button |
| gray-100 | `#F0F0F0` | hover on surface, insets |
| gray-200 | `#E7E7E7` | secondary button, dividers, tab hover |
| gray-300 | `#CFCFCF` | toggle track, strong secondary |
| gray-400 | `#9E9E9E` | subdued text, placeholders |
| gray-500 | `#737373` | secondary text, icons |
| gray-700 | `#363636` | dark-mode raised controls |
| gray-800 | `#242424` | dark-mode inset / hover |
| gray-900 | `#171717` | primary text, primary button; dark-mode card |
| black | `#0B0B0B` | dark-mode page background |
| white | `#FFFFFF` | light-mode card |

Translucent overlays are built from black/white at 3, 5, 8, 10, 15, 20, 30, 40, 50, 70, 85 %
(`alpha-black-*`, `alpha-white-*`). Borders and rings are always alpha, never solid gray, so they
work on any surface.

### 2.1 Semantic tokens (what components use)

| Role | Light | Dark |
|---|---|---|
| `surface` (page bg) | gray-50 | black `#0B0B0B` |
| `surface-hover` / `surface-inset` | gray-100 | gray-800 |
| `container` (card bg) | white | gray-900 |
| `container-hover` / `container-inset` | gray-50 | gray-800 |
| `container-inset-hover` | gray-100 | gray-700 |
| `text-primary` | gray-900 | white |
| `text-secondary` | gray-500 | gray-300 |
| `text-subdued` | gray-400 | gray-400 |
| `border-primary` | black 15 % | white 30 % |
| `border-secondary` | black 10 % | white 20 % |
| `border-subdued` / dividers | black 5 % | white 10 % |
| `focus-ring` | black 50 % | white 50 % |
| `overlay` (modal backdrop) | gray-100 at 50 % | black 85 % |
| `nav-indicator` | black | white |

### 2.2 Status colours (meaning only)

| Role | Light | Dark |
|---|---|---|
| success | green-700 `#078C52` | green-400 `#32D583` |
| warning | yellow-600 `#DC6803` | yellow-400 `#FDB022` |
| destructive | red-500 `#F13636` | red-400 `#ED4E4E` |
| info / link | blue-600 `#1570EF` | blue-500 `#2E90FA` |

Tinted backgrounds use the 500 stop at 5 % or 10 % alpha (`green-tint-10`, …), e.g. an
"Online" badge = green-50 bg + green-700 text (mixed 30 % with black for contrast) + green-200
border; in dark mode the bg becomes the 10 % tint and the text the 200 stop.

Charts: one accent per series, green for the main/positive line, gray for the comparison line,
category colours from the full ramps (red, orange, yellow, green, cyan, blue, indigo, violet,
fuchsia, pink).

### 2.3 Dark mode

Page `#0B0B0B`, cards gray-900 `#171717`, raised controls gray-700. Shadows become a faint white
glow (8 % white) and rings switch to white alpha. The primary button inverts to **white with black
text**. Toggle with `[data-theme]` on `<html>`, default from `prefers-color-scheme`.

## 3. Typography

- Font: **Geist** (variable, 100–900), mono: **Geist Mono**; fallbacks `system-ui` and
  `ui-monospace`. Weights used: 400, **500 (medium, the "bold" of this style)**, 600 rarely.
  Headings and big numbers are medium, never bold.
- Scale (Tailwind steps): 10–11 px (nav labels, uppercase markers), 12 px `text-xs` (form
  labels, table headers, captions), 14 px `text-sm` (**default UI text**: buttons, rows, cards),
  16 px `text-base`, 20 px `text-xl` (page title on mobile), 30 px `text-3xl` (page title on
  desktop, hero numbers like "$855,419.82").
- Table/section headers: 12 px, uppercase, wide letter-spacing, secondary colour
  ("CATEGORIES · 10   VALUE   WEIGHT").
- Numbers: tabular figures, right-aligned in tables, no wrapping. Deltas in mono and status colour
  ("$74,483.90 (↑9.5%) vs. last quarter": the delta green, the context gray).

## 4. Shape, depth, spacing

- Radius: `md` 8 px (inputs, small buttons, chips), `lg` 10 px (buttons, tabs, tooltips),
  `xl` 12 px (cards), `full` (status badges, avatar, mobile icon buttons).
- Depth = **hairline ring + soft shadow**, no CSS `border` on cards:
  `box-shadow: 0 1px 2px rgb(0 0 0 / 6%), 0 0 0 1px rgb(11 11 11 / 5%)` (`shadow-border-xs`).
  Larger levels (sm/md/lg/xl) only for popovers, menus and dialogs. All shadows are 6 % black.
- Spacing on a 4 px grid. Card padding 20 px (`p-5`), page gutter 12 px mobile / 40 px desktop,
  24 px between page header and content, 16–24 px between cards.

## 5. Layout

Desktop (≥ 1024 px):
- **Icon rail** on the far left (~84 px): logo at the top, nav items stacked (20 px icon in a
  32 px rounded square + 11 px label under it), help and user avatar pinned to the bottom. Active
  item: the icon square becomes a white card with `shadow-xs`, label turns primary, and a small
  4 × 16 px black **indicator bar** sits on the rail's left edge.
- Optional collapsible secondary sidebar (contextual list, ~320 px), separated by a hairline.
- **Sticky top bar** inside the content: sidebar toggle + breadcrumbs ("Home › Dashboard") on the
  left, small icon buttons on the right; bottom hairline, same bg as the page.
- **Page header**: title (30 px medium) + one-line gray subtitle on the left, actions on the
  right (outline icon button + black primary "+ New").
- Content: responsive grid of cards (2 columns on desktop, 1 on mobile). A period selector
  ("90D ▾") sits above the grid, right-aligned.

Mobile (< 1024 px):
- Top bar: menu button left, logo centred, icon buttons + avatar right; respects
  `env(safe-area-inset-top)`.
- **Bottom tab bar** with the same nav items (icon over label), fixed, hairline on top, padded by
  `env(safe-area-inset-bottom)`; main content gets bottom padding so nothing hides behind it.
- Text buttons in the header collapse to round icon buttons.

## 6. Components

- **Card**: `container` bg, 12 px radius, ring + xs shadow, 20 px padding, column flow. Header row =
  chevron (collapses the card, `<details>`) + 14 px medium title, optional control on the right
  (select, info icon). Big number first, then context line, then chart/list.
- **Buttons** (14 px medium, heights 28 / 36 / 48 px for sm / md / lg; icon-only buttons are
  square at the same heights and grow to **44 × 44 on touch** via `@media (pointer: coarse)`):
  - primary: inverse bg (gray-900 / white), inverse text;
  - secondary: gray-200 / gray-700 fill;
  - outline: transparent + `border-secondary`, hover fills `surface-hover`;
  - ghost / icon: transparent, hover fills `container-inset-hover`, icon in secondary colour;
  - destructive: red-600 fill, white text; outline-destructive: red text, neutral border.
  - Default `type="button"`. Icon-only buttons always get an `aria-label`.
- **Segmented control / tabs**: gray-50 track (black 50 % in dark), 4 px padding, segments 14 px
  medium secondary text; active segment = white pill with `shadow-sm` and primary text.
- **Pill / badge**: two modes. *Status badge*: rounded-full, normal case, 12 px, soft tone
  (tinted bg + darker text + tone border), optional leading dot for live states ("Running",
  "Syncing"). *Marker*: rounded-md, uppercase 10–11 px, wide tracking ("BETA", "NEW").
  Semantic tones: success→green, warning→amber, error→red, info→indigo, neutral→gray.
- **Form field**: a bordered box (8 px radius, `border-secondary`, xs shadow) holding a 12 px gray
  label above a borderless 14 px input; on focus the shadow goes and a 4 px soft halo
  (black 10 % / white 15 %) appears. Placeholder at 50 % opacity.
- **Select / dropdown trigger**: outline button with a chevron-down ("September 2026 ▾", "90D ▾").
- **Table**: no outer border inside cards; header row on `container-inset` with uppercase 12 px
  secondary labels; rows separated by 5 % hairlines; numeric columns right-aligned with tabular
  digits; leading icon in a small tinted circle. Wide tables scroll sideways with edge shadows
  that only appear when there is more to scroll.
- **Dialog**: native `<dialog>`, backdrop `overlay`, card-like panel; actions row bottom-right:
  Cancel (secondary/ghost) then the primary (or destructive) action, natural width. Single-action
  forms end in a full-width primary button. On mobile dialogs become full-height sheets.
- **Alert**: 16 px padding, 10 px radius, tinted bg + tone border + icon + text; `role="status"`
  or `role="alert"` when live.
- **Empty state**: centred icon, short title, one gray sentence, one primary action.
- **Loading**: skeleton blocks in `surface-inset` with a pulse; numbers being refreshed pulse
  instead of being replaced by spinners.
- **Tooltip**: container bg, 10 px radius, 10 × 12 px padding, ring + `0 8px 24px` 12 % shadow.
- **Icons**: [Lucide](https://lucide.dev) outline icons, 1.5–2 px stroke; sizes 12 / 16 / **20
  (default)** / 24 px; colour secondary by default, `currentColor` when active.

## 7. Interaction and accessibility

- Focus: one recipe everywhere, a 2 px `outline` (not box-shadow) in `focus-ring`, offset 0,
  shown on `:focus-visible` only; transparent outline at rest so transitions don't flash.
- Transitions: colours 150–200 ms, layout (sidebar) 300 ms; respect `prefers-reduced-motion`.
- Skip-to-content link, `aria-current="page"` on the active nav item, landmarks (`nav`, `main`).
- Touch targets 44 px on coarse pointers; safe-area insets on top and bottom bars.
- Charts and money values never rely on colour alone (sign, arrow or label too).

## 8. Applying it to truelite

- **Tokens:** `web/src/lib/styles/tokens.css` is the only place colours/sizes live. Name them by
  role as in §2.1 (`--surface`, `--container`, `--text-primary`, `--border-subdued`, …), keep the
  ramps as private `--gray-*` etc., and define dark values under both
  `@media (prefers-color-scheme: dark) :root:not([data-theme='light'])` and
  `:root[data-theme='dark']`.
- **Navigation:** rail on desktop, bottom tab bar on mobile, 4–5 items: Dashboard
  (`layout-dashboard`), Apps (`box`), Storage (`hard-drive`), System (`server` / updates),
  plus the user menu (logout, theme).
- **Dashboard:** page header with hostname + "TrueNAS 25.10.x · up 12 days"; cards for CPU/RAM/net
  (big number + sparkline), pools (name, status badge, usage bar), apps (counts by state + list),
  alerts (alert rows with tone icons). Problems surface first: a degraded pool or critical alert
  goes to the top as an Alert.
- **Status mapping:** ONLINE/RUNNING/HEALTHY → success, DEGRADED/DEPLOYING/updates available →
  warning, FAULTED/STOPPED with error/CRASHED → destructive, STOPPED → neutral.
- **Actions:** destructive actions (stop app, delete snapshot, reboot) use a dialog with a
  destructive primary; delete and reboot ask the user to type the name.
- **Weight:** system font stack by default (zero bytes); Geist only if self-hosted as one
  subset variable woff2 with `font-display: swap`. Icons are inlined SVG components for the
  icons actually used, not a whole icon package at runtime.
