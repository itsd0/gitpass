import {
  Archive,
  Download,
  Plus,
  Search,
  Star,
  Tag,
  Trash2,
  Vault,
  KeyRound,
  type LucideIcon,
} from "lucide-react";
import type { Filter, ItemCategory } from "../../api/types";
import { useT } from "../../i18n";
import { useApp } from "../../lib/store";
import { Button } from "../../ui";
import { catMeta } from "./categories";
import { getCategoryLabel } from "./categoryLabel";
import { useItemsUI } from "./state";

export interface ItemListEmptyStateProps {
  query?: string;
  category?: ItemCategory | null;
  filter?: Filter;
  onClearSearch?: () => void;
  onCreateItem?: (title?: string) => void;
  onNewCategory?: (category: ItemCategory) => void;
  onNewItem?: () => void;
  onImport?: () => void;
}

export function ItemListEmptyState(props: ItemListEmptyStateProps = {}) {
  const t = useT();
  const route = useApp((s) => s.route);
  const openModal = useApp((s) => s.openModal);
  const ui = useItemsUI();

  const query = (props.query !== undefined ? props.query : ui.query).trim();
  const category = props.category !== undefined ? props.category : ui.category;
  const filter: Filter =
    props.filter ?? (route.name === "items" ? route.filter : { kind: "all" });
  const activeCategory =
    category ??
    (filter.category ?? (filter.kind === "category" ? filter.category : null));

  let icon: LucideIcon = Vault;
  let title = "";
  let subtitle = "";
  let primaryAction: { label: string; onClick: () => void; icon?: LucideIcon } | null =
    null;
  let secondaryAction: {
    label: string;
    onClick: () => void;
    icon?: LucideIcon;
  } | null = null;

  if (query) {
    icon = Search;
    title = t("items.empty.searchTitle", { q: query });
    subtitle = t("items.empty.searchSubtitle");
    primaryAction = {
      label: t("items.empty.createItem", { q: query }),
      icon: Plus,
      onClick: () => {
        if (props.onCreateItem) {
          props.onCreateItem(query);
        } else {
          openModal("new-item", {
            category: "login",
            initial: { title: query },
          });
        }
      },
    };
    secondaryAction = {
      label: t("items.empty.clearSearch"),
      onClick: () => {
        if (props.onClearSearch) {
          props.onClearSearch();
        } else {
          ui.setQuery("");
        }
      },
    };
  } else if (activeCategory) {
    const meta = catMeta(activeCategory);
    icon = (meta.icon as LucideIcon) ?? KeyRound;
    const catLabel = getCategoryLabel(activeCategory, t);
    title = t("items.empty.categoryTitle", { category: catLabel });
    subtitle = t("items.empty.categorySubtitle", { category: catLabel });
    primaryAction = {
      label: t("items.empty.newCategory", { category: catLabel }),
      icon: Plus,
      onClick: () => {
        if (props.onNewCategory) {
          props.onNewCategory(activeCategory);
        } else {
          openModal("new-item", { category: activeCategory });
        }
      },
    };
  } else if (filter.kind === "favorites") {
    icon = Star;
    title = t("items.empty.favoritesTitle");
    subtitle = t("items.empty.favoritesSubtitle");
  } else if (filter.kind === "archive") {
    icon = Archive;
    title = t("items.empty.archiveTitle");
    subtitle = t("items.empty.archiveSubtitle");
  } else if (filter.kind === "deleted") {
    icon = Trash2;
    title = t("trash.emptyTitle");
    subtitle = t("trash.emptyBody");
  } else if (filter.kind === "tag") {
    icon = Tag;
    const tag = filter.tag ?? "";
    title = t("items.empty.tagTitle", { tag });
    subtitle = t("items.empty.tagSubtitle");
  } else {
    icon = Vault;
    title = t("items.empty.vaultTitle");
    subtitle = t("items.empty.vaultSubtitle");
    primaryAction = {
      label: t("items.empty.newItem"),
      icon: Plus,
      onClick: () => {
        if (props.onNewItem) {
          props.onNewItem();
        } else {
          openModal(
            "new-item",
            route.name === "items" && route.filter.vaultId
              ? { vaultId: route.filter.vaultId }
              : undefined
          );
        }
      },
    };
    secondaryAction = {
      label: t("items.empty.import"),
      icon: Download,
      onClick: () => {
        if (props.onImport) {
          props.onImport();
        } else {
          openModal("new-item", { view: "import" });
        }
      },
    };
  }

  const IconComponent = icon;

  return (
    <div
      role="status"
      aria-live="polite"
      className="flex min-h-full flex-col items-center justify-center p-6 text-center select-none"
      data-testid="item-list-empty"
    >
      {filter.kind === "deleted" && (
        <span data-testid="trash-empty" className="sr-only" aria-hidden="true" />
      )}
      <div
        className="mb-4 flex h-16 w-16 items-center justify-center rounded-2xl bg-accent/10 text-accent"
        data-testid="empty-icon"
      >
        <IconComponent size={40} className="text-accent" aria-hidden="true" />
      </div>
      <h3 className="text-base font-semibold text-fg" data-testid="empty-title">
        {title}
      </h3>
      <p
        className="mt-1 max-w-[260px] text-xs leading-relaxed text-muted"
        data-testid="empty-subtitle"
      >
        {subtitle}
      </p>
      {(primaryAction || secondaryAction) && (
        <div className="mt-5 flex flex-wrap items-center justify-center gap-2">
          {primaryAction && (
            <Button
              variant="default"
              size="sm"
              onClick={primaryAction.onClick}
              data-testid="empty-primary-action"
            >
              {primaryAction.icon && <primaryAction.icon size={14} />}
              {primaryAction.label}
            </Button>
          )}
          {secondaryAction && (
            <Button
              variant="outline"
              size="sm"
              onClick={secondaryAction.onClick}
              data-testid="empty-secondary-action"
            >
              {secondaryAction.icon && <secondaryAction.icon size={14} />}
              {secondaryAction.label}
            </Button>
          )}
        </div>
      )}
    </div>
  );
}
