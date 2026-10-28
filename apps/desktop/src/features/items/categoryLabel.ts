import type { ItemCategory } from "../../api/types";
import type { Key } from "../../i18n";
import { catMeta } from "./categories";

export function getCategoryLabel(
  category: ItemCategory,
  t: (key: Key, vars?: Record<string, string | number>) => string
): string {
  const key = `category.${category}` as Key;
  const translated = t(key);
  return translated !== key ? translated : catMeta(category).label;
}
