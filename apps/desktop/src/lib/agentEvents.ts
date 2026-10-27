import { useEffect } from 'react';
import { api } from '../api/client';
import { defaultRoute, useApp } from './store';

const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

export interface OpenItemPayload { vaultId?: string | null; itemId?: string | null }

/** Navigate to an item requested by the browser extension (`open_in_app`). */
export function openItemFromAgent(p: OpenItemPayload | null | undefined) {
  if (!p || typeof p.itemId !== 'string' || !p.itemId) return;
  const s = useApp.getState();
  if (s.locked) { pending = p; return; } // shown after unlock
  s.setRoute(defaultRoute);
  s.selectItem(p.itemId);
}
let pending: OpenItemPayload | null = null;
let refreshPending = false;

/** Live sync: the vault was written by someone (this window, the browser extension, CLI, git sync).
 *  Refetch lists/detail now, or right after the user finishes editing (never clobber an open editor). */
export function onVaultChanged() {
  const s = useApp.getState();
  if (s.locked) return;
  if (s.editing) { refreshPending = true; return; }
  s.refreshItems();
}

/** Extension-triggered unlock (Touch ID via the agent) and open-item events from Rust (src-tauri/src/agent_ui.rs). */
export function useAgentEvents() {
  const locked = useApp((s) => s.locked);
  const editing = useApp((s) => s.editing);
  useEffect(() => {
    if (!editing && refreshPending) { refreshPending = false; useApp.getState().refreshItems(); }
  }, [editing]);
  useEffect(() => {
    if (!locked && pending) { const p = pending; pending = null; openItemFromAgent(p); }
  }, [locked]);
  useEffect(() => {
    const onDom = (e: Event) => openItemFromAgent((e as CustomEvent<OpenItemPayload>).detail);
    window.addEventListener('gitpass:open-item', onDom);
    const unlisten: Array<() => void> = [];
    if (isTauri) {
      import('@tauri-apps/api/event').then(({ listen }) => {
        // agent_ui.rs emits `open-item` and also dispatches the DOM event; selecting twice is idempotent.
        listen<OpenItemPayload>('open-item', (ev) => openItemFromAgent(ev.payload)).then((u) => unlisten.push(u));
        // Backend lock on any path (idle, sleep, manual, extension/agent): back to the unlock screen.
        listen<string>('locked', () => { const s = useApp.getState(); if (!s.locked) s.lock(); }).then((u) => unlisten.push(u));
        listen('vault-changed', () => onVaultChanged()).then((u) => unlisten.push(u));
        // Unlocked from the extension (Touch ID): refresh state in place instead of reloading the window.
        listen('unlocked', () => {
          api.status().then((st) => {
            if (!st.unlocked) return;
            const s = useApp.getState();
            s.setAccount(st.account);
            if (s.locked) s.unlock(); else s.refreshItems();
          }, () => {});
        }).then((u) => unlisten.push(u));
      }).catch(() => {});
    }
    return () => { window.removeEventListener('gitpass:open-item', onDom); unlisten.forEach((u) => u()); };
  }, []);
}
