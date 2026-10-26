import { create } from 'zustand';
import type { Filter, ItemCategory, ItemSummary } from '../../api/types';
interface ItemsUI {
  query: string; setQuery: (q: string) => void;
  category: ItemCategory | null; setCategory: (c: ItemCategory | null) => void;
  sort: NonNullable<Filter['sort']>; setSort: (s: NonNullable<Filter['sort']>) => void;
  items: ItemSummary[]; setItems: (i: ItemSummary[]) => void;
  loading: boolean; setLoading: (b: boolean) => void;
}
export const useItemsUI = create<ItemsUI>((set) => ({
  query: '', setQuery: (query) => set({ query }),
  category: null, setCategory: (category) => set({ category }),
  sort: 'updated', setSort: (sort) => set({ sort }),
  items: [], setItems: (items) => set({ items }),
  loading: false, setLoading: (loading) => set({ loading }),
}));
