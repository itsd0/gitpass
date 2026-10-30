import { useEffect, useRef, useState } from "react";
import { Search } from "lucide-react";
import { api } from "../../api/client";
import type { Filter, ItemSummary } from "../../api/types";
import { ItemIcon } from "./categories";
import { useT } from "../../i18n";
import { cn } from "../../lib/utils";
import { Dialog, DialogContent, Input } from "../../ui";

export interface RelatedItemPickerModalProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  currentItemId?: string;
  excludeItemIds?: string[];
  onSelect: (item: ItemSummary) => void;
}

export function RelatedItemPickerModal({
  open,
  onOpenChange,
  currentItemId,
  excludeItemIds = [],
  onSelect,
}: RelatedItemPickerModalProps) {
  const t = useT();
  const [query, setQuery] = useState("");
  const [items, setItems] = useState<ItemSummary[]>([]);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) {
      setQuery("");
      setSelectedIndex(0);
      setItems([]);
      return;
    }
    let active = true;
    const filter: Filter = query.trim()
      ? { kind: "all", query: query.trim() }
      : { kind: "all", sort: "title" };

    const handle = setTimeout(() => {
      api
        .listItems({ filter })
        .then((res) => {
          if (!active) return;
          const filtered = res.filter(
            (item) => item.id !== currentItemId && !excludeItemIds.includes(item.id)
          );
          setItems(filtered);
          setSelectedIndex(0);
        })
        .catch(() => {
          if (active) setItems([]);
        });
    }, 50);

    return () => {
      active = false;
      clearTimeout(handle);
    };
  }, [open, query, currentItemId, excludeItemIds]);

  const select = (item: ItemSummary) => {
    onSelect(item);
    onOpenChange(false);
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setSelectedIndex((prev) => (items.length ? Math.min(prev + 1, items.length - 1) : 0));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setSelectedIndex((prev) => Math.max(prev - 1, 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (items[selectedIndex]) {
        select(items[selectedIndex]);
      }
    } else if (e.key === "Escape") {
      e.preventDefault();
      onOpenChange(false);
    }
  };

  useEffect(() => {
    listRef.current
      ?.querySelector(`[data-picker-idx="${selectedIndex}"]`)
      ?.scrollIntoView({ block: "nearest" });
  }, [selectedIndex]);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        title={t("related.pickerTitle")}
        className="max-w-md p-0 overflow-hidden"
        onOpenAutoFocus={(e) => {
          e.preventDefault();
          inputRef.current?.focus();
        }}
      >
        <div className="p-4 border-b border-border" onKeyDown={handleKeyDown}>
          <div className="relative flex items-center">
            <Search size={16} className="absolute left-3 text-muted pointer-events-none" />
            <Input
              ref={inputRef}
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder={t("related.searchPlaceholder")}
              aria-label={t("related.searchPlaceholder")}
              className="pl-9 h-10 w-full bg-card"
              data-testid="related-picker-search"
            />
          </div>
        </div>

        <div
          ref={listRef}
          role="listbox"
          aria-label={t("related.pickerTitle")}
          tabIndex={-1}
          className="max-h-72 overflow-y-auto p-2 space-y-1 outline-none"
          onKeyDown={handleKeyDown}
          data-testid="related-picker-list"
        >
          {items.length === 0 ? (
            <div className="py-8 text-center text-sm text-muted" data-testid="related-picker-empty">
              {t("related.noMatches")}
            </div>
          ) : (
            items.map((item, idx) => {
              const selected = idx === selectedIndex;
              return (
                <div
                  key={item.id}
                  role="option"
                  aria-selected={selected}
                  data-picker-idx={idx}
                  data-testid={`related-picker-item-${item.id}`}
                  onClick={() => select(item)}
                  onMouseEnter={() => setSelectedIndex(idx)}
                  className={cn(
                    "flex items-center gap-3 px-3 py-2 rounded-lg cursor-pointer text-sm outline-none transition-colors",
                    selected
                      ? "bg-accent/10 text-fg ring-1 ring-accent"
                      : "hover:bg-hover/60 text-fg"
                  )}
                >
                  <ItemIcon item={item} size={22} />
                  <div className="min-w-0 flex-1">
                    <div className="font-medium truncate">{item.title}</div>
                    {item.subtitle && (
                      <div className="text-xs text-muted truncate">{item.subtitle}</div>
                    )}
                  </div>
                </div>
              );
            })
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}
