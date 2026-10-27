import { useEffect, useMemo, useRef, useState } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { ArrowDownUp, Check, ChevronDown, LayoutGrid, Star } from 'lucide-react';
import { api } from '../../api/client';
import type { Filter } from '../../api/types';
import { useApp } from '../../lib/store';
import { cn } from '../../lib/utils';
import { Button, DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuLabel, DropdownMenuTrigger, Skeleton, toast } from '../../ui';
import { CATEGORIES, ItemIcon, catMeta } from './categories';
import { CategoryIcon } from '../../components/CategoryIcon';
import { groupRows, type Row } from './grouping';
import { useItemsUI } from './state';
import { copyPrimarySecret, errText } from './actions';
import { applySel, arrowSelect, clickSelect, prune, rangeFocus, readSel, selectAll } from './selection';
import { requestDelete } from './BulkPane';

/** Every row has a fixed, explicit height (the DOM enforces it), so virtual offsets are exact: no overlap, no stray gaps. */
export const HEADER_H = 32, ITEM_H = 54;
export const rowHeight = (r: Row) => (r.type === 'header' ? HEADER_H : ITEM_H);
const SORTS: { id: NonNullable<Filter['sort']>; label: string }[] = [
  { id: 'updated', label: 'Date Edited' }, { id: 'created', label: 'Date Created' }, { id: 'title', label: 'Title' },
];

export function ItemList() {
  const route = useApp((s) => s.route);
  const version = useApp((s) => s.itemsVersion);
  const selectedItemId = useApp((s) => s.selectedItemId);
  const selectItem = useApp((s) => s.selectItem);
  const multiIds = useApp((s) => s.multiIds);
  const multiSet = useMemo(() => new Set(multiIds), [multiIds]);
  const multiOn = multiIds.length > 1;
  const { query, category, setCategory, sort, setSort, items, setItems, loading, setLoading } = useItemsUI();
  const base: Filter = route.name === 'items' ? route.filter : { kind: 'all' };

  // debounce query (200ms) then call api.listItems
  const debounced = useDebounced(query, 200);
  useEffect(() => {
    let on = true; setLoading(true);
    const filter: Filter = { ...base, query: debounced || undefined, sort, ...(category ? { category } : {}) };
    api.listItems({ filter }).then((xs) => { if (on) setItems(xs); }).catch((e) => toast(errText(e), 'danger')).finally(() => on && setLoading(false));
    return () => { on = false; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [JSON.stringify(base), debounced, sort, category, version]);

  const rows = useMemo(() => groupRows(items, sort), [items, sort]);
  const parentRef = useRef<HTMLDivElement>(null);
  const virt = useVirtualizer({ count: rows.length, getScrollElement: () => parentRef.current, estimateSize: (i) => rowHeight(rows[i]), getItemKey: (i) => rows[i].key, overscan: 8, initialRect: { width: 320, height: 700 } });

  // auto-select first item when nothing selected
  useEffect(() => { if (!selectedItemId && items[0]) selectItem(items[0].id); }, [items, selectedItemId, selectItem]);
  // drop selected ids that left the list (refresh, filter, search)
  const order = useMemo(() => items.map((x) => x.id), [items]);
  useEffect(() => { const s = readSel(); if (!s.multi.length) return; const p = prune(order, s); if (p !== s && (p.multi.length !== s.multi.length || p.selected !== s.selected)) applySel(p); }, [order]);
  function onRowClick(e: React.MouseEvent, id: string) {
    const toggle = e.metaKey || e.ctrlKey, range = e.shiftKey;
    if (toggle || range) { e.preventDefault(); applySel(clickSelect(order, readSel(), id, { toggle, range })); }
    else selectItem(id);
    parentRef.current?.focus();
  }

  function onKey(e: React.KeyboardEvent) {
    const idx = items.findIndex((x) => x.id === selectedItemId);
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'a') {
      e.preventDefault(); applySel(selectAll(order, readSel())); return;
    }
    if (e.key === 'Escape' && multiOn) { e.preventDefault(); e.stopPropagation(); useApp.setState({ multiIds: [] }); return; }
    if ((e.key === 'Delete' || e.key === 'Backspace') && !e.altKey) { e.preventDefault(); requestDelete(); return; }
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      // Shift+Arrow extends/shrinks the range from the anchor; plain Arrow
      // returns to a single selection. Both move from the focus row, and the
      // focus row (not the anchor) is scrolled into view.
      const next = arrowSelect(order, readSel(), e.key === 'ArrowDown' ? 1 : -1, e.shiftKey);
      const focusId = rangeFocus(order, next) ?? next.selected;
      applySel(next);
      if (focusId) { const ri = rows.findIndex((r) => r.key === focusId); if (ri >= 0) virt.scrollToIndex(ri, { align: 'auto' }); }
    } else if (e.key === 'Enter' && idx >= 0) {
      e.preventDefault(); (document.querySelector('[data-detail-pane]') as HTMLElement | null)?.focus();
    } else if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'c' && idx >= 0 && !window.getSelection()?.toString()) {
      e.preventDefault(); void copyPrimarySecret(items[idx].vaultId, items[idx].id);
    }
  }

  return (
    <div className="flex h-full flex-col bg-panel" data-testid="item-list">
      <div className="flex h-11 shrink-0 items-center justify-between border-b border-border px-2">
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="ghost" size="sm" className="gap-1.5 text-sm">{category ? <CategoryIcon category={category} size={18} /> : <LayoutGrid size={14} className="text-accent" />}{category ? catMeta(category).label : 'All Categories'}<ChevronDown size={14} className="text-muted" /></Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start">
            <DropdownMenuItem onSelect={() => setCategory(null)}><LayoutGrid size={14} />All Categories</DropdownMenuItem>
            {CATEGORIES.map((c) => <DropdownMenuItem key={c.id} onSelect={() => setCategory(c.id)}><CategoryIcon category={c.id} size={18} />{c.label}</DropdownMenuItem>)}
          </DropdownMenuContent>
        </DropdownMenu>
        <DropdownMenu>
          <DropdownMenuTrigger asChild><Button variant="ghost" size="icon" aria-label="Sort"><ArrowDownUp size={15} /></Button></DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuLabel>Sort by</DropdownMenuLabel>
            {SORTS.map((s) => <DropdownMenuItem key={s.id} onSelect={() => setSort(s.id)}><span className="w-3">{sort === s.id ? '✓' : ''}</span>{s.label}</DropdownMenuItem>)}
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
      <div ref={parentRef} tabIndex={0} onKeyDown={onKey} className="flex-1 overflow-y-auto outline-none" role="listbox" aria-label="Items" data-item-list>
        {loading && items.length === 0 ? (
          <div className="space-y-3 p-3">{Array.from({ length: 8 }, (_, i) => <div key={i} className="flex gap-3"><Skeleton className="h-8 w-8" /><div className="flex-1 space-y-1.5"><Skeleton className="h-3 w-3/4" /><Skeleton className="h-2.5 w-1/2" /></div></div>)}</div>
        ) : items.length === 0 ? (
          <div className="p-8 text-center text-muted">{query ? `No items match “${query}”` : 'No items here yet'}</div>
        ) : (
          <div style={{ height: virt.getTotalSize(), position: 'relative' }}>
            {virt.getVirtualItems().map((v) => {
              const r = rows[v.index];
              return (
                <div key={r.key} data-index={v.index} style={{ position: 'absolute', top: 0, left: 0, width: '100%', height: v.size, transform: `translateY(${v.start}px)` }}>
                  {r.type === 'header' ? (
                    <div style={{ height: HEADER_H }} className="flex items-end px-4 pb-1.5 text-[11px] font-semibold tracking-wider text-muted" data-testid="group-header">{r.label}</div>
                  ) : (
                    <div role="option" aria-selected={multiOn ? multiSet.has(r.item.id) : r.item.id === selectedItemId} onClick={(e) => onRowClick(e, r.item.id)} onMouseDown={(e) => { if (e.shiftKey) e.preventDefault(); }}
                      style={{ height: ITEM_H - 2 }} className={cn('group mx-1.5 my-px box-border flex cursor-default items-center gap-3 overflow-hidden rounded-md px-2.5', (multiOn ? multiSet.has(r.item.id) : r.item.id === selectedItemId) ? 'bg-active' : 'hover:bg-hover', multiOn && multiSet.has(r.item.id) && 'ring-1 ring-inset ring-accent/50')}>
                      <span role="checkbox" aria-checked={multiOn && multiSet.has(r.item.id)} aria-label={`Select ${r.item.title}`} data-testid="row-check"
                        onClick={(e) => { e.stopPropagation(); applySel(clickSelect(order, readSel(), r.item.id, { toggle: true })); parentRef.current?.focus(); }}
                        className={cn('-ml-1 flex h-4 w-4 shrink-0 items-center justify-center rounded border transition-opacity',
                          multiOn && multiSet.has(r.item.id) ? 'border-accent bg-accent text-white opacity-100' : 'border-border opacity-0 group-hover:opacity-100', multiOn && 'opacity-100')}>
                        {multiOn && multiSet.has(r.item.id) && <Check size={11} strokeWidth={3} />}
                      </span>
                      <ItemIcon item={r.item} />
                      <div className="min-w-0 flex-1">
                        <div className="truncate text-[14px] font-medium leading-5">{r.item.title}</div>
                        {r.item.subtitle ? <div className="truncate text-xs leading-4 text-muted">{r.item.subtitle}</div> : null}
                      </div>
                      {r.item.favorite && <Star size={12} className="shrink-0 fill-warn text-warn" />}
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}

export function useDebounced<T>(value: T, ms: number): T {
  const [v, setV] = useState(value);
  useEffect(() => { const t = setTimeout(() => setV(value), ms); return () => clearTimeout(t); }, [value, ms]);
  return v;
}
