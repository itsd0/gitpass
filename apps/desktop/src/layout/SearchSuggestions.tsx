import { useMemo } from 'react';
import { useT } from '../i18n';
import type { VaultInfo } from '../api/types';
import { cn } from '../lib/utils';

export interface SuggestionItem {
  value: string;
  hint: string;
}

export interface ActiveOperator {
  op: 'tag' | 'type' | 'is' | 'has' | 'vault';
  prefix: string;
  start: number;
  end: number;
  negated: boolean;
}

export function detectActiveOperator(
  query: string,
  cursorPos: number | null
): ActiveOperator | null {
  const pos = cursorPos ?? query.length;
  const beforeCursor = query.slice(0, pos);
  const match = beforeCursor.match(/(?:^|\s)(-)?(tag|type|is|has|vault):([^\s]*)$/i);
  if (!match) return null;

  const negated = Boolean(match[1]);
  const op = match[2].toLowerCase() as ActiveOperator['op'];
  const rawVal = match[3];
  const prefix = rawVal.replace(/^["']/, '');
  const start = beforeCursor.length - (rawVal.length + op.length + 1 + (negated ? 1 : 0));
  const end = pos;

  return { op, prefix, start, end, negated };
}

export function applySuggestion(
  query: string,
  active: ActiveOperator,
  value: string
): string {
  const quoted = value.includes(' ') ? `"${value}"` : value;
  const prefix = active.negated ? '-' : '';
  const token = `${prefix}${active.op}:${quoted} `;
  return query.slice(0, active.start) + token + query.slice(active.end);
}

export interface SearchSuggestionsProps {
  active: ActiveOperator;
  tags: string[];
  vaults: VaultInfo[];
  selectedIndex: number;
  onSelect: (item: SuggestionItem) => void;
}

export function useSearchSuggestionsList(
  active: ActiveOperator | null,
  tags: string[],
  vaults: VaultInfo[]
): SuggestionItem[] {
  const t = useT();

  return useMemo(() => {
    if (!active) return [];
    const prefix = active.prefix.toLowerCase();

    switch (active.op) {
      case 'tag': {
        const hint = t('search.hint.tag');
        return tags
          .filter((tag) => tag.toLowerCase().includes(prefix))
          .map((tag) => ({ value: tag, hint }));
      }
      case 'type': {
        const types: Array<{ value: string; hint: string }> = [
          { value: 'login', hint: t('category.login') },
          { value: 'card', hint: t('category.credit_card') },
          { value: 'note', hint: t('category.secure_note') },
          { value: 'identity', hint: t('category.identity') },
          { value: 'api', hint: t('category.api_credential') },
          { value: 'ssh', hint: t('category.ssh_key') },
          { value: 'passkey', hint: t('category.passkey') },
          { value: 'bank', hint: t('category.bank_account') },
          { value: 'wifi', hint: t('category.wifi') },
          { value: 'server', hint: t('category.server') },
          { value: 'document', hint: t('category.document') },
        ];
        return types.filter(
          (item) =>
            item.value.toLowerCase().startsWith(prefix) ||
            item.hint.toLowerCase().includes(prefix)
        );
      }
      case 'is': {
        const flags: SuggestionItem[] = [
          { value: 'favorite', hint: t('search.val.favorite') },
          { value: 'archived', hint: t('search.val.archived') },
        ];
        return flags.filter(
          (item) =>
            item.value.toLowerCase().startsWith(prefix) ||
            item.hint.toLowerCase().includes(prefix)
        );
      }
      case 'has': {
        const features: SuggestionItem[] = [
          { value: 'otp', hint: t('search.val.otp') },
          { value: 'passkey', hint: t('search.val.passkey') },
          { value: 'attachment', hint: t('search.val.attachment') },
          { value: 'url', hint: t('search.val.url') },
        ];
        return features.filter(
          (item) =>
            item.value.toLowerCase().startsWith(prefix) ||
            item.hint.toLowerCase().includes(prefix)
        );
      }
      case 'vault': {
        const hint = t('search.hint.vault');
        return vaults
          .map((v) => ({ value: v.name, hint }))
          .filter(
            (item) =>
              item.value.toLowerCase().startsWith(prefix) ||
              item.value.toLowerCase().includes(prefix)
          );
      }
      default:
        return [];
    }
  }, [active, tags, vaults, t]);
}

export function SearchSuggestions({
  active,
  tags,
  vaults,
  selectedIndex,
  onSelect,
}: SearchSuggestionsProps) {
  const items = useSearchSuggestionsList(active, tags, vaults);

  if (items.length === 0) return null;

  return (
    <div
      role="listbox"
      aria-label="Search suggestions"
      data-testid="search-suggestions"
      className="absolute left-0 top-full z-50 mt-1 max-h-64 w-80 overflow-y-auto rounded-lg border border-border bg-panel p-1 shadow-lg backdrop-blur"
    >
      {items.map((item, index) => {
        const isSelected = index === selectedIndex;
        return (
          <div
            key={item.value}
            role="option"
            aria-selected={isSelected}
            data-testid={`search-suggestion-${item.value}`}
            data-highlighted={isSelected ? "true" : undefined}
            onMouseDown={(e) => {
              e.preventDefault();
              onSelect(item);
            }}
            className={cn(
              "flex cursor-pointer items-center justify-between rounded-md px-2.5 py-1.5 text-sm transition-colors",
              isSelected
                ? "bg-accent font-medium text-white"
                : "text-fg hover:bg-hover"
            )}
          >
            <span className="font-mono text-xs">{item.value}</span>
            <span className={cn("text-xs", isSelected ? "text-white/80" : "text-muted")}>
              {item.hint}
            </span>
          </div>
        );
      })}
    </div>
  );
}
