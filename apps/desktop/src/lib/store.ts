import { create } from 'zustand';
import type { Filter } from '../api/types';

export type Route =
  | { name: 'home' }
  | { name: 'items'; filter: Filter }
  | { name: 'watchtower' }
  | { name: 'people' }
  | { name: 'settings'; tab?: string }
  | { name: 'quick-access' };

export interface ModalState { id: string; props?: Record<string, unknown> }

export interface AppState {
  route: Route;
  setRoute: (r: Route) => void;
  modal: ModalState | null;
  openModal: (id: string, props?: Record<string, unknown>) => void;
  closeModal: () => void;
  selectedItemId: string | null;
  selectItem: (id: string | null) => void;
  /** multi-select: ids of every selected item when more than one row is selected ([] = single mode) */
  multiIds: string[];
  setMultiIds: (ids: string[]) => void;
  /** pending destructive confirm triggered from the list keyboard (Delete) or the bulk pane */
  bulkConfirm: 'trash' | 'purge' | null;
  setBulkConfirm: (c: 'trash' | 'purge' | null) => void;
  locked: boolean;
  /** page the user was on when the vault locked; restored after unlock */
  resumeRoute: Route | null;
  lock: () => void;
  unlock: () => void;
  account: string | null;
  setAccount: (a: string | null) => void;
  theme: 'dark' | 'light';
  setTheme: (t: 'dark' | 'light') => void;
  sidebarCollapsed: boolean;
  toggleSidebar: () => void;
  /** bump to force item list refetch after mutations */
  itemsVersion: number;
  refreshItems: () => void;
  editing: boolean;
  setEditing: (e: boolean) => void;
}

export const defaultRoute: Route = { name: 'items', filter: { kind: 'all', sort: 'updated' } };

export const useApp = create<AppState>((set, get) => ({
  route: defaultRoute,
  setRoute: (route) => set({ route, selectedItemId: null, multiIds: [], bulkConfirm: null, editing: false }),
  modal: null,
  openModal: (id, props) => set({ modal: { id, props } }),
  closeModal: () => set({ modal: null }),
  selectedItemId: null,
  selectItem: (selectedItemId) => set({ selectedItemId, multiIds: [], editing: false }),
  multiIds: [],
  setMultiIds: (multiIds) => set({ multiIds, editing: false }),
  bulkConfirm: null,
  setBulkConfirm: (bulkConfirm) => set({ bulkConfirm }),
  locked: true,
  // Clearing UI state on lock; secrets never live here anyway.
  resumeRoute: null,
  // Clearing UI state on lock (AppShell unmounts, dropping cached vault/item lists); remembers the page.
  lock: () => { const s = get(); set({ locked: true, selectedItemId: null, multiIds: [], bulkConfirm: null, modal: null, editing: false, route: defaultRoute, resumeRoute: s.locked ? s.resumeRoute : s.route }); },
  unlock: () => set((s) => ({ locked: false, route: s.resumeRoute ?? s.route, resumeRoute: null, itemsVersion: s.itemsVersion + 1 })),
  account: null,
  setAccount: (account) => set({ account }),
  theme: 'dark',
  setTheme: (theme) => {
    if (typeof document !== 'undefined') document.documentElement.classList.toggle('dark', theme === 'dark');
    set({ theme });
  },
  sidebarCollapsed: false,
  toggleSidebar: () => set((s) => ({ sidebarCollapsed: !s.sidebarCollapsed })),
  itemsVersion: 0,
  refreshItems: () => set((s) => ({ itemsVersion: s.itemsVersion + 1 })),
  editing: false,
  setEditing: (editing) => set({ editing }),
}));
