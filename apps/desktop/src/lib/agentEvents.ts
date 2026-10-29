import { useEffect } from 'react';
import { api } from '../api/client';
import { defaultRoute, useApp } from './store';

export const isTauri = () => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

export interface OpenItemPayload { vaultId?: string | null; itemId?: string | null }
export interface OpenViewPayload { view: 'generator_history' | 'watchtower' | 'settings' | string; section?: string | null }

let pendingItem: OpenItemPayload | null = null;
let pendingView: OpenViewPayload | null = null;
let refreshPending = false;

/** Navigate to an item requested by the browser extension (`open_in_app`). */
export function openItemFromAgent(p: OpenItemPayload | null | undefined) {
  if (!p || typeof p.itemId !== 'string' || !p.itemId) return;
  const s = useApp.getState();
  if (s.locked) { pendingItem = p; pendingView = null; return; } // shown after unlock
  s.closeModal();
  s.setRoute(defaultRoute);
  s.selectItem(p.itemId);
}

/** Navigate to a specific view requested by the browser extension (`open_in_app`). */
export function openViewFromAgent(p: OpenViewPayload | null | undefined) {
  if (!p || typeof p.view !== 'string' || !p.view) return;
  if (p.view !== 'generator_history' && p.view !== 'watchtower' && p.view !== 'settings') return;
  const s = useApp.getState();
  if (s.locked) { pendingView = p; pendingItem = null; return; } // navigated after unlock
  s.closeModal();
  switch (p.view) {
    case 'generator_history':
      s.setRoute(defaultRoute);
      s.openModal('password-generator');
      break;
    case 'watchtower':
      s.setRoute({ name: 'watchtower' });
      break;
    case 'settings':
      s.setRoute({ name: 'settings', tab: (p.section && typeof p.section === 'string') ? p.section : undefined });
      break;
  }
}

/** Live sync: the vault was written by someone (this window, the browser extension, CLI, git sync).
 *  Refetch lists/detail now, or right after the user finishes editing (never clobber an open editor). */
export function onVaultChanged() {
  const s = useApp.getState();
  if (s.locked) return;
  if (s.editing) { refreshPending = true; return; }
  s.refreshItems();
}

/** Extension-triggered unlock (Touch ID via the agent) and open-item / open-view events from Rust (src-tauri/src/agent_ui.rs). */
export function useAgentEvents() {
  const locked = useApp((s) => s.locked);
  const editing = useApp((s) => s.editing);
  useEffect(() => {
    if (!editing && refreshPending) { refreshPending = false; useApp.getState().refreshItems(); }
  }, [editing]);
  useEffect(() => {
    if (!locked) {
      if (pendingItem) {
        const p = pendingItem;
        pendingItem = null;
        openItemFromAgent(p);
      } else if (pendingView) {
        const p = pendingView;
        pendingView = null;
        openViewFromAgent(p);
      }
    }
  }, [locked]);
  useEffect(() => {
    const onDomOpen = (e: Event) => openItemFromAgent((e as CustomEvent<OpenItemPayload>).detail);
    const onDomOpenView = (e: Event) => openViewFromAgent((e as CustomEvent<OpenViewPayload>).detail);
    const onDomVaultChanged = () => onVaultChanged();
    const onDomLocked = () => { const s = useApp.getState(); if (!s.locked) s.lock(); };
    window.addEventListener('gitpass:open-item', onDomOpen);
    window.addEventListener('gitpass:open-view', onDomOpenView);
    window.addEventListener('gitpass:vault-changed', onDomVaultChanged);
    window.addEventListener('gitpass:locked', onDomLocked);
    const unlisten: Array<() => void> = [];
    if (isTauri()) {
      import('@tauri-apps/api/event').then(({ listen }) => {
        // agent_ui.rs emits `open-item` / `open-view` and also dispatches the DOM events; selecting twice is idempotent.
        listen<OpenItemPayload>('open-item', (ev) => openItemFromAgent(ev.payload)).then((u) => unlisten.push(u));
        listen<OpenViewPayload>('open-view', (ev) => openViewFromAgent(ev.payload)).then((u) => unlisten.push(u));
        // Backend lock on any path (idle, sleep, manual, extension/agent): back to the unlock screen.
        listen<string>('locked', () => { const s = useApp.getState(); if (!s.locked) s.lock(); }).then((u) => unlisten.push(u));
        listen('vault-changed', () => onVaultChanged()).then((u) => unlisten.push(u));
        listen<string>('menu-action', (ev) => {
          if (ev.payload === 'open-generator') {
            useApp.getState().openModal('password-generator');
          } else if (ev.payload === 'open-settings') {
            useApp.getState().setRoute({ name: 'settings' });
          }
        }).then((u) => unlisten.push(u));
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
    return () => {
      window.removeEventListener('gitpass:open-item', onDomOpen);
      window.removeEventListener('gitpass:open-view', onDomOpenView);
      window.removeEventListener('gitpass:vault-changed', onDomVaultChanged);
      window.removeEventListener('gitpass:locked', onDomLocked);
      unlisten.forEach((u) => u());
    };
  }, []);
}
