# Quick Access window (implemented)

Quick Access is a small always-on-top window for copying a password or username
without opening the main window. It is fully wired up — there are no remaining
"later" steps.

## Frontend

`QuickAccessPage` (`QuickAccess.tsx`, exported from `index.ts`) renders the
search-and-copy UI. `App.tsx` renders it instead of the normal layout when
`isQuickAccessWindow()` is true, i.e. when the URL is `#/quick-access` or
`?window=quick-access` (`index.ts`). When the vault is locked, the same window
shows `QuickAccessUnlock` (`QuickAccessUnlock.tsx`) instead.

The feature definition in `index.ts` (feature id `quick-access`, route
`quick-access`) also adds an "Open Quick Access" command that calls
`api.showQuickAccess()` inside Tauri and falls back to the in-app route in the
browser mock.

## Rust side

- Window: `apps/desktop/src-tauri/tauri.conf.json` defines the `quick-access`
  window (560×380, hidden at startup, undecorated, non-resizable,
  always-on-top, centered, skipped from the taskbar) with URL
  `index.html?window=quick-access`.
- Hotkey: `apps/desktop/src-tauri/src/main.rs` registers
  `tauri-plugin-global-shortcut` with a handler that calls
  `commands::toggle_quick_access`. At startup, `main.rs` calls
  `hotkey::register_startup_hotkey` (`src-tauri/src/hotkey.rs`), which reads
  `Settings.quickAccessHotkey` (default `CmdOrCtrl+Shift+Space`) from the app
  state, parses and validates it, and registers it.
- Toggle: `commands::toggle_quick_access` (`src-tauri/src/commands.rs`) hides
  the window when it is visible and focused, and otherwise shows it and
  focuses it. Pressing the hotkey while the vault is locked shows the unlock
  screen in the quick-access window. Touch ID starts only after that show:
  the hidden webview stays mounted across auto-lock, and prompting from the
  background activates the app.
- Hide after copy: `QuickAccessPage` hides its window via
  `getCurrentWindow().hide()` once a copy completes, and
  `QuickAccessUnlock` hides it after a successful unlock.

## Capabilities

`apps/desktop/src-tauri/capabilities/default.json` covers both the `main` and
`quick-access` windows and grants `core:window:allow-hide` (needed for the
hide-after-copy behavior) alongside the `dialog` permissions.

## Secrets

Secrets still never reach JS: Enter copies the password and Cmd/Ctrl+Enter
copies the username through the `copy_field` command, the same as the main
window.
