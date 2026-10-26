import { LayoutGrid } from 'lucide-react';
import { ItemsPage } from './ItemsPage';
export const feature = { id: 'items', route: 'items', sidebar: { label: 'All Items', icon: LayoutGrid, section: 'top' as const }, Page: ItemsPage };
export { ItemsPage } from './ItemsPage';
export { useItemsUI } from './state';
