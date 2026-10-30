import { useI18n, useT } from "../../i18n";
import { getItemExpiry } from "./expiry";
import { rotationLabel } from "./rotation";
import { useEffect, useMemo, useRef, useState } from "react";
import { useVirtualizer, defaultRangeExtractor } from "@tanstack/react-virtual";
import { ArrowDownUp, Check, ChevronDown, LayoutGrid, Star } from "lucide-react";
import { api } from "../../api/client";
import type { Filter } from "../../api/types";
import { useApp } from "../../lib/store";
import { useSettings } from "../settings/useSettings";
import { cn } from "../../lib/utils";
import {
  Badge,
  Button,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuTrigger,
  Skeleton,
  toast,
} from "../../ui";
import { CATEGORIES, ItemIcon } from "./categories";
import { CategoryIcon } from "../../components/CategoryIcon";
import { groupRows, type Row } from "./grouping";
import { useItemsUI } from "./state";
import { errText } from "./actions";
import {
  applySel,
  arrowSelect,
  clickSelect,
  prune,
  rangeFocus,
  readSel,
  selectAll,
} from "./selection";
import { requestDelete } from "./BulkPane";
import { useDragStore, getDragPreviewText } from "./dragState";
import { ItemListEmptyState } from "./ItemListEmptyState";
import { SearchHighlight } from "./SearchHighlight";
import { getCategoryLabel } from "./categoryLabel";
import { TrashBar } from "./TrashBar";
import { ItemContextMenu, type ItemMenuTarget } from "./ItemContextMenu";
import { trashHint } from "./trash";

/** Every row has a fixed, explicit height (the DOM enforces it), so virtual offsets are exact: no overlap, no stray gaps. */
export const HEADER_H = 32,
  ITEM_H = 54,
  ITEM_H_COMPACT = 32;
export type ListDensity = "comfortable" | "compact";
export const rowHeight = (r: Row, density: ListDensity = "comfortable") =>
  r.type === "header" ? HEADER_H : density === "compact" ? ITEM_H_COMPACT : ITEM_H;
const SORTS: {
  id: NonNullable<Filter["sort"]>;
  labelKey: "items.sortUpdated" | "items.sortCreated" | "items.sortTitle" | "items.sortRecent";
}[] = [
  { id: "updated", labelKey: "items.sortUpdated" },
  { id: "created", labelKey: "items.sortCreated" },
  { id: "title", labelKey: "items.sortTitle" },
  { id: "recent", labelKey: "items.sortRecent" },
];

export function ItemList() {
  const route = useApp((s) => s.route);
  const version = useApp((s) => s.itemsVersion);
  const selectedItemId = useApp((s) => s.selectedItemId);
  const selectItem = useApp((s) => s.selectItem);
  const multiIds = useApp((s) => s.multiIds);
  const multiSet = useMemo(() => new Set(multiIds), [multiIds]);
  const dragged = useDragStore((s) => s.dragged);
  const density: ListDensity = useSettings((s) => s.settings?.listDensity ?? "comfortable");
  const compact = density === "compact";
  const multiOn = multiIds.length > 1;
  const t = useT();
  const {
    query,
    category,
    setCategory,
    sort,
    setSort,
    items,
    setItems,
    loading,
    setLoading,
  } = useItemsUI();
  const base: Filter = route.name === "items" ? route.filter : { kind: "all" };
  // Trashed or archived items must not be dragged onto a vault/tag (moving would bypass restore).
  const canDrag = base.kind !== "deleted" && base.kind !== "archive";

  // debounce query (200ms) then call api.listItems
  const debounced = useDebounced(query, 200);
  const searchQuery = debounced.trim();
  useEffect(() => {
    let on = true;
    setLoading(true);
    const filter: Filter = {
      ...base,
      query: debounced || undefined,
      sort,
      ...(category ? { category } : {}),
    };
    api
      .listItems({ filter })
      .then((xs) => {
        if (on) setItems(xs);
      })
      .catch((e) => toast(errText(e), "danger"))
      .finally(() => on && setLoading(false));
    return () => {
      on = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [JSON.stringify(base), debounced, sort, category, version]);

  const lang = useI18n((s) => s.lang);
  const showRecentSection = base.kind === "all" && !searchQuery && sort !== "recent";
  const rows = useMemo(
    () =>
      groupRows(items, sort, {
        flat: !!searchQuery,
        showRecentSection,
        recentLabel: t("items.recentlyUsed"),
        neverUsedLabel: t("items.neverUsed"),
        todayLabel: t("items.section.today"),
        yesterdayLabel: t("items.section.yesterday"),
        previous7DaysLabel: t("items.section.previous7Days"),
        previous30DaysLabel: t("items.section.previous30Days"),
        locale: lang,
      }),
    [items, sort, showRecentSection, searchQuery, t, lang]
  );
  const stickyIndexes = useMemo(
    () =>
      rows
        .map((r, i) => (r.type === "header" ? i : -1))
        .filter((i) => i !== -1),
    [rows]
  );
  const parentRef = useRef<HTMLDivElement>(null);
  const activeStickyIndexRef = useRef<number>(-1);
  const virt = useVirtualizer({
    count: rows.length,
    getScrollElement: () => parentRef.current,
    estimateSize: (i) => rowHeight(rows[i], density),
    getItemKey: (i) => rows[i].key,
    overscan: 8,
    initialRect: { width: 320, height: 700 },
    rangeExtractor: (range) => {
      const activeIndex =
        [...stickyIndexes].reverse().find((index) => range.startIndex >= index);
      activeStickyIndexRef.current = activeIndex ?? -1;
      const next = new Set([
        ...(activeIndex !== undefined && activeIndex >= 0 ? [activeIndex] : []),
        ...(defaultRangeExtractor ? defaultRangeExtractor(range) : []),
      ]);
      return [...next].sort((a, b) => a - b);
    },
  });

  // auto-select first item when nothing selected
  useEffect(() => {
    if (!selectedItemId && rows.length > 0) {
      const first = rows.find((r) => r.type === "item");
      if (first && first.type === "item") selectItem(first.item.id);
    }
  }, [rows, selectedItemId, selectItem]);
  // drop selected ids that left the list (refresh, filter, search)
  const order = useMemo(
    () => rows.filter((r) => r.type === "item").map((r) => r.item.id),
    [rows]
  );
  useEffect(() => {
    const s = readSel();
    if (!s.multi.length) return;
    const p = prune(order, s);
    if (
      p !== s &&
      (p.multi.length !== s.multi.length || p.selected !== s.selected)
    )
      applySel(p);
  }, [order]);
  function onRowClick(e: React.MouseEvent, id: string) {
    const toggle = e.metaKey || e.ctrlKey,
      range = e.shiftKey;
    if (toggle || range) {
      e.preventDefault();
      applySel(clickSelect(order, readSel(), id, { toggle, range }));
    } else selectItem(id);
    parentRef.current?.focus();
  }

  const [menu, setMenu] = useState<ItemMenuTarget | null>(null);
  /** Open the item menu at (x, y) for the row `id`: a right-click inside a multi-selection acts on all of it, otherwise the row is selected first. */
  function openMenu(id: string, x: number, y: number) {
    const inMulti = multiOn && multiSet.has(id);
    if (!inMulti && (id !== selectedItemId || multiIds.length)) selectItem(id);
    const ids = inMulti ? multiIds : [id];
    const targets = items.filter((it) => ids.includes(it.id));
    if (targets.length) setMenu({ x, y, items: targets });
  }
  function closeMenu() {
    setMenu(null);
    if (!useApp.getState().editing) parentRef.current?.focus();
  }
  /** Keyboard path (Shift+F10 / the context-menu key): anchor the menu to the selected row. */
  function openMenuForSelection() {
    const id = selectedItemId ?? order[0];
    if (!id) return false;
    const rowEl = parentRef.current?.querySelector(`[data-item-id="${CSS.escape(id)}"]`) as HTMLElement | null;
    const r = rowEl?.getBoundingClientRect();
    openMenu(id, r ? r.left + Math.min(r.width / 2, 120) : 0, r ? r.bottom - 8 : 0);
    return true;
  }
  function onContextMenu(e: React.MouseEvent) {
    const rowEl = (e.target as HTMLElement).closest("[data-item-id]") as HTMLElement | null;
    e.preventDefault();
    if (rowEl) openMenu(rowEl.dataset.itemId!, e.clientX, e.clientY);
    else openMenuForSelection();
  }

  function onRowDragStart(e: React.DragEvent, item: (typeof items)[number]) {
    const isMulti = multiOn && multiSet.has(item.id);
    const draggedItems = isMulti ? items.filter((x) => multiSet.has(x.id)) : [item];
    useDragStore.getState().startDrag(draggedItems);
    try {
      e.dataTransfer.setData("application/json", JSON.stringify({ itemIds: draggedItems.map((x) => x.id) }));
      e.dataTransfer.setData("text/plain", draggedItems.map((x) => x.id).join(","));
      e.dataTransfer.effectAllowed = "copyMove"; // vault drop = move, tag drop = copy
    } catch {
      // ignore
    }
    const previewEl = document.getElementById("drag-preview");
    if (previewEl && e.dataTransfer.setDragImage) {
      previewEl.textContent = getDragPreviewText(draggedItems, t);
      e.dataTransfer.setDragImage(previewEl, 10, 10);
    }
  }

  function onRowDragEnd() {
    useDragStore.getState().endDrag();
  }

  function onKey(e: React.KeyboardEvent) {
    if (e.key === "ContextMenu" || (e.key === "F10" && e.shiftKey)) {
      e.preventDefault();
      openMenuForSelection();
      return;
    }
    const idx = items.findIndex((x) => x.id === selectedItemId);
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "a") {
      e.preventDefault();
      applySel(selectAll(order, readSel()));
      return;
    }
    if (e.key === " " && selectedItemId) {
      e.preventDefault();
      applySel(clickSelect(order, readSel(), selectedItemId, { toggle: true }));
      return;
    }
    if (e.key === "Escape" && multiOn) {
      e.preventDefault();
      e.stopPropagation();
      useApp.setState({ multiIds: [] });
      return;
    }
    if ((e.key === "Delete" || e.key === "Backspace") && !e.altKey) {
      e.preventDefault();
      requestDelete();
      return;
    }
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      // Shift+Arrow extends/shrinks the range from the anchor; plain Arrow
      // returns to a single selection. Both move from the focus row, and the
      // focus row (not the anchor) is scrolled into view.
      const next = arrowSelect(
        order,
        readSel(),
        e.key === "ArrowDown" ? 1 : -1,
        e.shiftKey
      );
      const focusId = rangeFocus(order, next) ?? next.selected;
      applySel(next);
      if (focusId) {
        const ri = rows.findIndex((r) => r.key === focusId);
        if (ri >= 0) virt.scrollToIndex(ri, { align: "auto" });
      }
    } else if (e.key === "Enter" && idx >= 0) {
      e.preventDefault();
      (
        document.querySelector("[data-detail-pane]") as HTMLElement | null
      )?.focus();
    }
  }

  return (
    <div className="flex h-full flex-col bg-panel" data-testid="item-list" data-list-density={density}>
      <div className="flex h-11 shrink-0 items-center justify-between border-b border-border px-2">
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="ghost" size="sm" className="gap-1.5 text-sm">
              {category ? (
                <CategoryIcon category={category} size={18} />
              ) : (
                <LayoutGrid size={14} className="text-accent" />
              )}
              {category ? getCategoryLabel(category, t) : t("items.allCategories")}
              <ChevronDown size={14} className="text-muted" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start">
            <DropdownMenuItem onSelect={() => setCategory(null)}>
              <LayoutGrid size={14} />
              {t("items.allCategories")}
            </DropdownMenuItem>
            {CATEGORIES.map((c) => (
              <DropdownMenuItem key={c.id} onSelect={() => setCategory(c.id)}>
                <CategoryIcon category={c.id} size={18} />
                {getCategoryLabel(c.id, t)}
              </DropdownMenuItem>
            ))}
          </DropdownMenuContent>
        </DropdownMenu>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="ghost" size="icon" aria-label={t("items.sort")}>
              <ArrowDownUp size={15} />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuLabel>{t("items.sortBy")}</DropdownMenuLabel>
            {SORTS.map((s) => (
              <DropdownMenuItem key={s.id} onSelect={() => setSort(s.id)}>
                <span className="w-3">{sort === s.id ? "✓" : ""}</span>
                {t(s.labelKey)}
              </DropdownMenuItem>
            ))}
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
      {base.kind === "deleted" && <TrashBar />}
      {searchQuery && !loading && items.length > 0 && (
        <div
          role="status"
          aria-live="polite"
          data-testid="search-result-count"
          className="flex h-7 shrink-0 items-center border-b border-border px-3 text-[11px] text-muted"
        >
          {items.length === 1
            ? t("items.search.resultOne")
            : t("items.search.resultCount", { count: items.length })}
        </div>
      )}
      <div
        ref={parentRef}
        tabIndex={items.length === 0 ? -1 : 0}
        onKeyDown={onKey}
        onContextMenu={onContextMenu}
        className="flex-1 overflow-y-auto outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-accent"
        role="listbox"
        aria-multiselectable="true"
        aria-label={t("items.ariaLabel")}
        data-item-list
      >
        {loading && items.length === 0 ? (
          <div className="space-y-3 p-3">
            {Array.from({ length: 8 }, (_, i) => (
              <div key={i} className="flex gap-3">
                <Skeleton className="h-8 w-8" />
                <div className="flex-1 space-y-1.5">
                  <Skeleton className="h-3 w-3/4" />
                  <Skeleton className="h-2.5 w-1/2" />
                </div>
              </div>
            ))}
          </div>
        ) : items.length === 0 ? (
          <ItemListEmptyState />
        ) : (
          <div style={{ height: virt.getTotalSize(), position: "relative" }}>
            {virt.getVirtualItems().map((v) => {
              const r = rows[v.index];
              const isHeader = r.type === "header";
              const isActiveSticky = isHeader && activeStickyIndexRef.current === v.index;
              return (
                <div
                  key={r.key}
                  data-index={v.index}
                  style={{
                    position: isActiveSticky ? "sticky" : "absolute",
                    top: 0,
                    left: 0,
                    width: "100%",
                    height: v.size,
                    transform: isActiveSticky ? undefined : `translateY(${v.start}px)`,
                    zIndex: isActiveSticky ? 10 : isHeader ? 1 : 0,
                  }}
                >
                  {isHeader ? (
                    <div
                      id={`header-${r.key}`}
                      role="group"
                      aria-label={r.label}
                      style={{ height: HEADER_H }}
                      className="flex items-end px-4 pb-1.5 text-[11px] font-semibold tracking-wider text-muted bg-panel/95 backdrop-blur-sm select-none"
                      data-testid="group-header"
                    >
                      {r.label}
                    </div>
                  ) : (() => {
                    const isChecked = multiOn && multiSet.has(r.item.id);
                    const isSelected = multiOn
                      ? multiSet.has(r.item.id)
                      : r.item.id === selectedItemId;
                    return (
                      <div
                        role="option"
                        data-testid="item-row"
                        data-item-id={r.item.id}
                        draggable={canDrag}
                        onDragStart={canDrag ? (e) => onRowDragStart(e, r.item) : undefined}
                        onDragEnd={canDrag ? onRowDragEnd : undefined}
                        aria-selected={isSelected}
                        aria-describedby={r.headerKey ? `header-${r.headerKey}` : undefined}
                        onClick={(e) => onRowClick(e, r.item.id)}
                        onMouseDown={(e) => {
                          if (e.shiftKey) e.preventDefault();
                        }}
                        style={{ height: (compact ? ITEM_H_COMPACT : ITEM_H) - 2 }}
                        className={cn(
                          "group mx-1.5 my-px box-border flex cursor-default items-center gap-2.5 overflow-hidden rounded-md px-2.5 transition-colors",
                          isSelected
                            ? "bg-accent/15 ring-1 ring-inset ring-accent hover:bg-accent/20"
                            : "hover:bg-hover"
                        )}
                      >
                        <button
                          type="button"
                          role="checkbox"
                          tabIndex={-1}
                          aria-checked={isChecked}
                          aria-label={t(
                            isChecked ? "items.deselectItem" : "items.selectItem",
                            { title: r.item.title }
                          )}
                          data-testid="row-check"
                          onClick={(e) => {
                            e.stopPropagation();
                            applySel(
                              clickSelect(order, readSel(), r.item.id, {
                                toggle: true,
                              })
                            );
                            parentRef.current?.focus();
                          }}
                          className="flex h-full w-6 shrink-0 cursor-pointer items-center justify-center -ml-1 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent"
                        >
                          <span
                            className={cn(
                              "flex h-4 w-4 items-center justify-center rounded border transition-opacity",
                              isChecked
                                ? "border-accent bg-accent text-white opacity-100"
                                : "border-border opacity-0 group-hover:opacity-100",
                              multiOn && "opacity-100"
                            )}
                          >
                            {isChecked && (
                              <Check size={11} strokeWidth={3} />
                            )}
                          </span>
                        </button>
                      <ItemIcon item={r.item} size={compact ? 20 : 24} />
                      {compact ? (
                        <div className="min-w-0 flex-1 truncate text-[14px] font-medium leading-5" data-testid="item-row-compact-line">
                          <SearchHighlight text={r.item.title} query={searchQuery} />
                          {base.kind === "deleted" ? (() => {
                            const hint = trashHint(r.item.deletedAt, Date.now(), t);
                            return hint ? <span data-testid="trash-days" className={cn("font-normal", hint.danger ? "text-danger" : "text-muted")}> · {hint.text}</span> : null;
                          })() : r.item.subtitle ? (
                            <span className="font-normal text-muted"> · <SearchHighlight text={r.item.subtitle} query={searchQuery} /></span>
                          ) : null}
                        </div>
                      ) : (
                      <div className="min-w-0 flex-1">
                        <div className="truncate text-[14px] font-medium leading-5">
                          <SearchHighlight text={r.item.title} query={searchQuery} />
                        </div>
                        {base.kind === "deleted" ? (() => {
                          const hint = trashHint(r.item.deletedAt, Date.now(), t);
                          return hint ? <div data-testid="trash-days" className={cn("truncate text-xs leading-4", hint.danger ? "text-danger" : "text-muted")}>{hint.text}</div> : null;
                        })() : r.item.subtitle ? (
                          <div className="truncate text-xs leading-4 text-muted">
                            <SearchHighlight text={r.item.subtitle} query={searchQuery} />
                          </div>
                        ) : null}
                      </div>
                      )}
                      {(() => {
                        const expiry = getItemExpiry(r.item, undefined, t);
                        if (!expiry) return null;
                        const label = t(
                          expiry.status === "expired"
                            ? "expiry.expired"
                            : "expiry.expiresSoon"
                        );
                        // Compact rows shrink pills to a dot that keeps the
                        // accessible name and a tooltip with the full label.
                        if (compact) return (
                          <span
                            data-testid="expiry-dot"
                            aria-label={label}
                            title={label}
                            className={cn(
                              "h-2 w-2 shrink-0 rounded-full",
                              expiry.status === "expired" ? "bg-danger" : "bg-warn"
                            )}
                          />
                        );
                        return (
                          <Badge
                            tone={
                              expiry.status === "expired" ? "danger" : "warn"
                            }
                            className="shrink-0 text-[11px] px-1.5 py-0"
                            data-testid="expiry-pill"
                            aria-label={label}
                          >
                            {label}
                          </Badge>
                        );
                      })()}
                      {(() => {
                        const rotation = r.item.rotation;
                        const label = rotationLabel(rotation, t);
                        if (!rotation || !label) return null;
                        if (compact) return (
                          <span
                            data-testid="rotation-dot"
                            aria-label={label}
                            title={label}
                            className="h-2 w-2 shrink-0 rounded-full bg-accent"
                          />
                        );
                        return (
                          <Badge
                            tone="accent"
                            className="shrink-0 text-[11px] px-1.5 py-0"
                            data-testid="rotation-pill"
                            aria-label={label}
                          >
                            {label}
                          </Badge>
                        );
                      })()}
                      {r.item.favorite && (
                        <Star
                          size={12}
                          className="shrink-0 fill-warn text-warn"
                        />
                      )}
                    </div>
                    );
                  })()}
                </div>
              );
            })}
          </div>
        )}
      </div>
      <ItemContextMenu target={menu} onClose={closeMenu} />
      <div
        id="drag-preview"
        data-testid="drag-preview"
        aria-hidden="true"
        className="pointer-events-none fixed -left-[9999px] top-0 z-50 rounded-md border border-accent bg-panel px-2.5 py-1 text-xs font-medium text-fg shadow-md"
      >
        {dragged ? getDragPreviewText(dragged.items, t) : ""}
      </div>
    </div>
  );
}

export function useDebounced<T>(value: T, ms: number): T {
  const [v, setV] = useState(value);
  useEffect(() => {
    const t = setTimeout(() => setV(value), ms);
    return () => clearTimeout(t);
  }, [value, ms]);
  return v;
}
