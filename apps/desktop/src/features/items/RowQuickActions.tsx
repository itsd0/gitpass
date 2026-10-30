import { CreditCard, ExternalLink, KeyRound, ShieldCheck, User } from 'lucide-react';
import type { ItemSummary } from '../../api/types';
import { useT, type Key } from '../../i18n';
import { cn } from '../../lib/utils';
import { copyPrimarySecret, copyTotpCode, copyUsername, openItemWebsite } from './actions';
import { useRowFields } from './rowFields';

interface Action {
  id: string;
  testId: string;
  icon: typeof User;
  labelKey: Key;
  act: (vaultId: string, itemId: string) => Promise<void>;
}

/**
 * Hover/focus quick actions at the right edge of an item row: copy username / password / one-time code and open
 * website for logins (TOTP and website only when the item really has them), copy number for cards. Absolutely
 * positioned over the row so rendering it never shifts the row layout; fades in on row hover, row focus-within, or
 * when the row is the selected one while the list has keyboard focus. Every copy goes through the shared
 * clipboard helpers (Rust-side copy with auto-clear + toast), never a new clipboard path.
 */
export function RowQuickActions({ item, selected, listFocused }: { item: ItemSummary; selected: boolean; listFocused: boolean }) {
  const t = useT();
  const fields = useRowFields(item.vaultId, item.id);
  const login = item.category === 'login';
  const card = item.category === 'credit_card';
  if (!login && !card) return null;

  const actions: Action[] = [];
  if (login) {
    // Username/password render before the field read resolves: every login row offers them and the copy helpers
    // already toast when a field is absent. TOTP and website wait for the detail so they only appear when present.
    actions.push(
      { id: 'username', testId: 'row-copy-username', icon: User, labelKey: 'itemKeys.copyUsername', act: copyUsername },
      { id: 'password', testId: 'row-copy-password', icon: KeyRound, labelKey: 'itemKeys.copyPassword', act: copyPrimarySecret }
    );
    if (fields?.totp)
      actions.push({ id: 'totp', testId: 'row-copy-totp', icon: ShieldCheck, labelKey: 'itemKeys.copyTotp', act: copyTotpCode });
    if (fields?.website)
      actions.push({ id: 'website', testId: 'row-open-website', icon: ExternalLink, labelKey: 'itemKeys.openWebsite', act: openItemWebsite });
  } else {
    actions.push({ id: 'number', testId: 'row-copy-number', icon: CreditCard, labelKey: 'itemKeys.copyNumber', act: copyPrimarySecret });
  }

  const onKeyDown = (e: React.KeyboardEvent<HTMLDivElement>) => {
    if (e.key === 'Escape') {
      e.stopPropagation();
      e.preventDefault();
      (e.currentTarget.closest('[data-item-id]') as HTMLElement | null)?.focus();
      return;
    }
    if (e.key === 'ArrowRight' || e.key === 'ArrowDown' || e.key === 'ArrowLeft' || e.key === 'ArrowUp') {
      // Arrow keys move between the cluster's buttons; without this they would bubble to the list and change the selection.
      e.stopPropagation();
      e.preventDefault();
      const btns = Array.from(e.currentTarget.querySelectorAll<HTMLButtonElement>('button'));
      const i = btns.indexOf(document.activeElement as HTMLButtonElement);
      const d = e.key === 'ArrowRight' || e.key === 'ArrowDown' ? 1 : -1;
      btns[(i + d + btns.length) % btns.length]?.focus();
    }
  };

  return (
    <div
      data-testid="row-actions"
      role="toolbar"
      aria-label={t('items.rowActions', { title: item.title })}
      onKeyDown={onKeyDown}
      className={cn(
        'absolute right-1 top-1/2 flex -translate-y-1/2 items-center gap-0.5 rounded-md bg-panel/90 pl-0.5 backdrop-blur-sm',
        'opacity-0 transition-opacity duration-[120ms] motion-reduce:transition-none group-hover:opacity-100 group-focus-within:opacity-100',
        selected && listFocused && 'opacity-100'
      )}
    >
      {actions.map((a) => (
        <button
          key={a.id}
          type="button"
          data-testid={a.testId}
          // Only the selected row's cluster is in the tab order: Tab from the list enters it, other rows' buttons stay click/hover-only.
          tabIndex={selected ? 0 : -1}
          aria-label={t(a.labelKey)}
          title={t(a.labelKey)}
          onClick={(e) => {
            e.stopPropagation();
            void a.act(item.vaultId, item.id);
          }}
          className="flex h-7 w-7 items-center justify-center rounded text-muted outline-none transition-colors hover:bg-hover hover:text-accent focus-visible:bg-hover focus-visible:text-accent focus-visible:ring-1 focus-visible:ring-accent"
        >
          <a.icon size={16} aria-hidden="true" />
        </button>
      ))}
    </div>
  );
}
