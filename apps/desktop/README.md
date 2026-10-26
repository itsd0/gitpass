# gitpass desktop (v0.4.0, Tauri 2)

```
apps/desktop/
  gui-core/     gitpass-gui-core: Tauri-free command layer (GuiState: Session, auto-lock, DTOs) + unit tests
                (member of the root Cargo workspace)
  src-tauri/    Tauri 2 app (thin #[tauri::command] wrappers, clipboard w/ 30 s clear, idle watcher)
                (standalone workspace; EXCLUDED from root so `cargo test --workspace` needs no GTK/WebKit)
  src/          React + TypeScript frontend (Vite)
```

Build: `npm install && npm run build` (frontend); `npx tauri dev` / `npx tauri build --bundles deb`.

Requirements: Rust >= 1.85 (src-tauri/.cargo/config.toml sets `incompatible-rust-versions = "fallback"` so the
resolver picks deps compatible with 1.85; resolved tauri 2.11.x). Keep `@tauri-apps/api` on the same minor (`~2.11`).
Linux: libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev librsvg2-dev pkg-config.

Security: the Session, master password and Secret Key live only in Rust state (`GuiState`) and are zeroized on
lock/auto-lock (password = `Zeroizing<String>`, SecretKey/Key32 are ZeroizeOnDrop). The frontend never persists
anything; list/detail DTOs carry no secrets; copy happens in Rust (value never sent to JS); "Show password" is the
only path that sends a password to the webview (auto-hidden after 30 s). Sync keeps password+Secret Key in memory
while unlocked because `sync --merge` must re-unlock.
