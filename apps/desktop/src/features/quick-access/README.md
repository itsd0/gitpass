# Quick Access window (admin stream → core stream wiring)

Frontend exports `QuickAccessPage` (feature id `quick-access`, route `quick-access`). The shell should render it
**instead of the normal layout** when `location.hash === '#/quick-access'` or `?window=quick-access` is present:

```tsx
if (isQuickAccessWindow()) root.render(<QuickAccessPage />);
```

Rust side (core stream, later):

1. `tauri.conf.json` → add a hidden window or create it lazily:
   ```rust
   WebviewWindowBuilder::new(app, "quick-access", WebviewUrl::App("index.html#/quick-access".into()))
     .title("gitpass Quick Access").inner_size(680.0, 420.0).decorations(false).always_on_top(true)
     .resizable(false).center().visible(false).skip_taskbar(true).build()?;
   ```
2. Register `Settings.quickAccessHotkey` (default `CmdOrCtrl+Shift+Space`) with `tauri-plugin-global-shortcut`;
   on press: if vault locked show main window unlock, else `show()` + `set_focus()` the `quick-access` window.
3. Hide the window on blur (`WindowEvent::Focused(false)`) and after a copy (the component calls
   `getCurrentWindow().hide()` — requires `core:window:allow-hide` capability for the `quick-access` label).
4. Capabilities for label `quick-access`: same commands as main (`list_items`, `get_item`, `copy_field`, `totp_now`).
   Secrets still never reach JS: Enter → `copy_field` (password), Cmd/Ctrl+Enter → `copy_field` (username).
