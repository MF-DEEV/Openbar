# Island context menu

Right-click the compact or expanded island, or a floating quick-access button.
Page-specific menus, such as those in Files and the mixer, take precedence.

The menu follows `NotchView.islandMenu` on macOS:

- **Sections** opens the existing gallery of enabled, visible sections. Choose a tile to navigate.
  While the gallery is open the action becomes **Back**. It is absent when no sections are available.
- **Pin / Unpin** changes the current island pin state.
- **Settings** collapses the island and opens its settings window.
- **Collapse** uses the island's existing collapse action, including unpinning it.

The macOS menu only offers **Update** when an update is available. Windows currently
has no application updater, update-check command, or available-update state. That
conditional action is therefore absent; updates still require a newer installer.

Escape or an outside click dismisses the menu before acting on the island. Arrow
keys and Enter/Space operate its items. Opening the menu preserves the island's
layout; the menu is drawn above it and constrained to the native window's viewport.
The existing capture and focus commands let a compact island receive menu input
without opening it. They are released when the menu closes, loses focus, resizes,
is disabled or unmounts.

Verification:

```sh
npm run typecheck
npm run test -- src/island/contextMenu.test.ts src/island/store.test.ts
npm run test:context-menu
```

The last command uses the existing Edge browser harness and saves a screenshot to
`build/island-context-menu.png`. Native Windows focus and click-through should
also be checked in the built application.

## Appearance

The standard Mac island uses opaque black (`#000000`) when compact and expanded.
Opening its context menu does not recolour it. The Mac's optional translucent and
Liquid Glass backgrounds are separate opt-in modes, not click highlights.

The island menu has a separate, dark surface, 13px regular text, compact 22px rows,
one symbol column, a separator, and blue selection. Its Back action keeps the grid
symbol, as `NotchView.islandMenu` does. This styling is scoped to the island menu.

On macOS the menu itself is drawn by AppKit; its material, font rasterization and
accent depend on the system. The Windows treatment is an approximation of the
standard dark menu. It does not reproduce AppKit's desktop vibrancy or bundle SF
Pro. Exact visual matching requires a screenshot and the macOS version/settings
being used as the reference.
