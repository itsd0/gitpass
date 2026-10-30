import { create } from 'zustand';
import type { ItemCategory, ItemSummary } from '../../api/types';

export interface SavedSearch {
  id: string;
  name: string;
  query?: string;
  category?: ItemCategory | null;
  vaultId?: string;
  tag?: string;
}

export const SAVED_SEARCHES_KEY = 'gitpass.savedSearches';

export function loadSavedSearches(): SavedSearch[] {
  try {
    const raw = localStorage.getItem(SAVED_SEARCHES_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(
      (x): x is SavedSearch =>
        typeof x === 'object' &&
        x !== null &&
        typeof (x as any).id === 'string' &&
        typeof (x as any).name === 'string'
    );
  } catch {
    return [];
  }
}

export function persistSavedSearches(searches: SavedSearch[]) {
  try {
    if (searches.length === 0) {
      localStorage.removeItem(SAVED_SEARCHES_KEY);
    } else {
      localStorage.setItem(SAVED_SEARCHES_KEY, JSON.stringify(searches));
    }
  } catch {
    // storage unavailable: keep in memory
  }
}

/** Matches an item summary against saved search criteria for computing sidebar badge counts */
export function matchSavedSearch(item: ItemSummary, search: SavedSearch): boolean {
  if (item.deletedAt || item.archived) return false;
  if (search.vaultId && item.vaultId !== search.vaultId) return false;
  if (search.tag && !item.tags.includes(search.tag)) return false;
  if (search.category && item.category !== search.category) return false;
  if (search.query && search.query.trim()) {
    const tokens = search.query.trim().split(/\s+/);
    for (const token of tokens) {
      if (!token) continue;
      const lower = token.toLowerCase();
      if (lower.startsWith('tag:')) {
        const val = lower.slice(4).replace(/^["']|["']$/g, '');
        if (!item.tags.some((t) => t.toLowerCase() === val)) return false;
      } else if (lower.startsWith('type:')) {
        const val = lower.slice(5).replace(/^["']|["']$/g, '');
        if (item.category.toLowerCase() !== val) return false;
      } else if (lower === 'is:favorite' || lower === 'is:fav') {
        if (!item.favorite) return false;
      } else {
        const cleaned = lower.replace(/^["']|["']$/g, '');
        const inTitle = item.title.toLowerCase().includes(cleaned);
        const inSubtitle = item.subtitle ? item.subtitle.toLowerCase().includes(cleaned) : false;
        const inUrl = item.url ? item.url.toLowerCase().includes(cleaned) : false;
        const inTag = item.tags.some((t) => t.toLowerCase().includes(cleaned));
        if (!inTitle && !inSubtitle && !inUrl && !inTag) return false;
      }
    }
  }
  return true;
}

interface SavedSearchesState {
  searches: SavedSearch[];
  activeSearchId: string | null;
  savePopoverOpen: boolean;
  setActiveSearchId: (id: string | null) => void;
  setSavePopoverOpen: (open: boolean) => void;
  saveSearch: (data: {
    name: string;
    query?: string;
    category?: ItemCategory | null;
    vaultId?: string;
    tag?: string;
  }) => SavedSearch;
  renameSearch: (id: string, name: string) => void;
  deleteSearch: (id: string) => SavedSearch | null;
  restoreSearch: (search: SavedSearch, index?: number) => void;
  reload: () => void;
}

export const useSavedSearches = create<SavedSearchesState>((set, get) => ({
  searches: loadSavedSearches(),
  activeSearchId: null,
  savePopoverOpen: false,
  setActiveSearchId: (activeSearchId) => set({ activeSearchId }),
  setSavePopoverOpen: (savePopoverOpen) => set({ savePopoverOpen }),
  saveSearch: (data) => {
    const id =
      typeof crypto !== 'undefined' && crypto.randomUUID
        ? crypto.randomUUID()
        : `search-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`;
    const newSearch: SavedSearch = {
      id,
      name: data.name.trim(),
      ...(data.query?.trim() ? { query: data.query.trim() } : {}),
      ...(data.category ? { category: data.category } : {}),
      ...(data.vaultId ? { vaultId: data.vaultId } : {}),
      ...(data.tag ? { tag: data.tag } : {}),
    };
    const next = [...get().searches, newSearch];
    persistSavedSearches(next);
    set({ searches: next, activeSearchId: newSearch.id, savePopoverOpen: false });
    return newSearch;
  },
  renameSearch: (id, name) => {
    const trimmed = name.trim();
    if (!trimmed) return;
    const next = get().searches.map((s) => (s.id === id ? { ...s, name: trimmed } : s));
    persistSavedSearches(next);
    set({ searches: next });
  },
  deleteSearch: (id) => {
    const existing = get().searches.find((s) => s.id === id);
    if (!existing) return null;
    const next = get().searches.filter((s) => s.id !== id);
    persistSavedSearches(next);
    set((state) => ({
      searches: next,
      activeSearchId: state.activeSearchId === id ? null : state.activeSearchId,
    }));
    return existing;
  },
  restoreSearch: (search, index) => {
    const cur = get().searches;
    if (cur.some((s) => s.id === search.id)) return;
    const next =
      typeof index === 'number' && index >= 0 && index <= cur.length
        ? [...cur.slice(0, index), search, ...cur.slice(index)]
        : [...cur, search];
    persistSavedSearches(next);
    set({ searches: next });
  },
  reload: () => {
    set({ searches: loadSavedSearches(), activeSearchId: null, savePopoverOpen: false });
  },
}));
