import { Copy, X } from 'lucide-react';
import { api } from '../../../api/client';
import type { ItemDetail as Detail } from '../../../api/types';
import type { RefTarget } from '../../../lib/secretRef';
import { useApp } from '../../../lib/store';
import { useSettings } from '../../settings/useSettings';
import { useT } from '../../../i18n';
import { cn } from '../../../lib/utils';
import { Badge, Button, Tooltip, toast } from '../../../ui';
import { ItemIcon } from '../categories';
import { FieldRow } from '../FieldView';
import { SSH_DETAIL_FIELD_IDS } from '../SshKeyDetail';
import { isPasskeyDetailField } from '../PasskeyDetail';
import { SafeMarkdown } from '../SafeMarkdown';
import { errText } from '../actions';

interface DetailSectionsProps {
  detail: Detail;
  refTarget: RefTarget | null;
  deleted: boolean;
}

export function DetailSections({ detail, refTarget, deleted }: DetailSectionsProps) {
  const t = useT();
  const { refreshItems } = useApp();
  const ctx = { vaultId: detail.vaultId, itemId: detail.id };
  const renderNotesMarkdown = useSettings((s) => s.settings?.renderNotesMarkdown ?? true);

  return (
    <>
      {detail.sections.map((s) => detail.category === 'ssh_key'
        ? { ...s, fields: s.fields.filter((f) => !(SSH_DETAIL_FIELD_IDS as readonly string[]).includes(f.id)) }
        : detail.category === 'passkey'
        ? { ...s, fields: s.fields.filter((f) => !isPasskeyDetailField(f)) }
        : s).filter((s) => s.fields.length).map((s) => (
        <div key={s.id} className="mb-4">
          {s.title && <div className="mb-1.5 px-1 text-xs font-semibold uppercase tracking-wider text-muted">{s.title}</div>}
          <div className="overflow-hidden rounded-lg border border-border bg-card">
            {s.fields.map((f) => (
              <FieldRow
                key={f.id}
                field={f}
                ctx={ctx}
                category={detail.category}
                strength={detail.passwordStrength}
                passwordChangedAt={/password/i.test(f.label) ? detail.passwordChangedAt : undefined}
                refTarget={refTarget}
                itemTitle={detail.title}
                username={detail.subtitle || detail.sections.flatMap((sec) => sec.fields).find((x) => x.id === 'username')?.value || undefined}
                passwordHistory={/password/i.test(f.label) && detail.category === 'login' ? detail.passwordHistory : undefined}
                onHistoryCleared={refreshItems}
              />
            ))}
          </div>
        </div>
      ))}

      {detail.notes && (
        <div className="mb-4 rounded-lg border border-border bg-card px-4 py-2.5" data-testid="detail-notes">
          <div className="flex items-center justify-between pb-1">
            <div className="text-xs text-label">{t('detail.notes')}</div>
            <Tooltip content={t('common.copy')}>
              <Button
                variant="ghost"
                size="iconSm"
                aria-label={`Copy ${t('detail.notes')}`}
                data-testid="copy-notes-btn"
                onClick={() =>
                  navigator.clipboard
                    ?.writeText(detail.notes ?? '')
                    .then(() => toast(t('common.copied')))
                }
              >
                <Copy size={13} />
              </Button>
            </Tooltip>
          </div>
          {renderNotesMarkdown ? (
            <SafeMarkdown content={detail.notes} />
          ) : (
            <div className="whitespace-pre-wrap text-[14px]">{detail.notes}</div>
          )}
        </div>
      )}

      {((detail.relatedItems && detail.relatedItems.length > 0) || (detail.linkedFrom && detail.linkedFrom.length > 0)) && (
        <div
          className="mb-4 overflow-hidden rounded-lg border border-border bg-card"
          data-testid="related-items-card"
        >
          {detail.relatedItems && detail.relatedItems.length > 0 && (
            <>
              <div className="px-4 pt-2.5 pb-1">
                <div className="text-xs text-label">{t('related.title')}</div>
              </div>
              <div className="divide-y divide-border/50">
                {detail.relatedItems.map((item) => (
                  <div
                    key={item.id}
                    tabIndex={item.available ? 0 : -1}
                    role={item.available ? "button" : "row"}
                    aria-label={item.available ? item.title : t('related.notAvailable')}
                    data-testid={`related-item-${item.id}`}
                    onClick={() => {
                      if (item.available) {
                        const a = useApp.getState();
                        if (
                          a.route.name !== 'items' ||
                          (a.route.filter.kind === 'vault' && a.route.filter.vaultId !== item.vaultId) ||
                          (a.route.filter.kind === 'category' && a.route.filter.category !== item.category)
                        ) {
                          a.setRoute({ name: 'items', filter: { kind: 'all' } });
                        }
                        a.selectItem(item.id);
                      }
                    }}
                    onKeyDown={(e) => {
                      if ((e.key === 'Enter' || e.key === ' ') && item.available) {
                        e.preventDefault();
                        const a = useApp.getState();
                        if (
                          a.route.name !== 'items' ||
                          (a.route.filter.kind === 'vault' && a.route.filter.vaultId !== item.vaultId) ||
                          (a.route.filter.kind === 'category' && a.route.filter.category !== item.category)
                        ) {
                          a.setRoute({ name: 'items', filter: { kind: 'all' } });
                        }
                        a.selectItem(item.id);
                      }
                    }}
                    className={cn(
                      "group flex items-center gap-3 px-4 py-2.5 outline-none transition-colors",
                      item.available
                        ? "cursor-pointer hover:bg-hover/50 focus-visible:bg-hover focus-visible:ring-1 focus-visible:ring-accent"
                        : "opacity-80"
                    )}
                  >
                    <ItemIcon item={{ category: item.category ?? 'login', title: item.title, subtitle: item.subtitle ?? '' }} size={24} />
                    <div className="min-w-0 flex-1">
                      <div
                        className={cn(
                          "truncate text-sm font-medium",
                          item.available ? "text-fg group-hover:text-accent" : "text-muted italic"
                        )}
                      >
                        {item.available ? item.title : t('related.notAvailable')}
                      </div>
                      {item.available && item.subtitle && (
                        <div className="truncate text-xs text-muted">{item.subtitle}</div>
                      )}
                    </div>
                    {!item.available && !deleted && (
                      <Button
                        variant="ghost"
                        size="iconSm"
                        className="text-muted hover:text-danger"
                        aria-label={t('related.removeLink')}
                        data-testid={`remove-unavailable-related-${item.id}`}
                        onClick={async (e) => {
                          e.stopPropagation();
                          try {
                            await api.removeRelatedItem({
                              vaultId: detail.vaultId,
                              itemId: detail.id,
                              relatedId: item.id,
                            });
                            toast(t('related.toast.unlinked'));
                            refreshItems();
                          } catch (err) {
                            toast(errText(err), 'danger');
                          }
                        }}
                      >
                        <X size={13} />
                      </Button>
                    )}
                  </div>
                ))}
              </div>
            </>
          )}

          {detail.linkedFrom && detail.linkedFrom.length > 0 && (
            <>
              <div
                className={cn(
                  "px-4 pt-2.5 pb-1",
                  detail.relatedItems && detail.relatedItems.length > 0 && "border-t border-border/50"
                )}
              >
                <div className="text-xs text-label">{t('related.linkedFrom')}</div>
              </div>
              <div className="divide-y divide-border/50" data-testid="linked-from-group">
                {detail.linkedFrom.map((item) => (
                  <div
                    key={item.id}
                    tabIndex={item.available ? 0 : -1}
                    role={item.available ? "button" : "row"}
                    aria-label={item.available ? item.title : t('related.notAvailable')}
                    data-testid={`linked-from-item-${item.id}`}
                    onClick={() => {
                      if (item.available) {
                        const a = useApp.getState();
                        if (
                          a.route.name !== 'items' ||
                          (a.route.filter.kind === 'vault' && a.route.filter.vaultId !== item.vaultId) ||
                          (a.route.filter.kind === 'category' && a.route.filter.category !== item.category)
                        ) {
                          a.setRoute({ name: 'items', filter: { kind: 'all' } });
                        }
                        a.selectItem(item.id);
                      }
                    }}
                    onKeyDown={(e) => {
                      if ((e.key === 'Enter' || e.key === ' ') && item.available) {
                        e.preventDefault();
                        const a = useApp.getState();
                        if (
                          a.route.name !== 'items' ||
                          (a.route.filter.kind === 'vault' && a.route.filter.vaultId !== item.vaultId) ||
                          (a.route.filter.kind === 'category' && a.route.filter.category !== item.category)
                        ) {
                          a.setRoute({ name: 'items', filter: { kind: 'all' } });
                        }
                        a.selectItem(item.id);
                      }
                    }}
                    className={cn(
                      "group flex items-center gap-3 px-4 py-2.5 outline-none transition-colors",
                      item.available
                        ? "cursor-pointer hover:bg-hover/50 focus-visible:bg-hover focus-visible:ring-1 focus-visible:ring-accent"
                        : "opacity-80"
                    )}
                  >
                    <ItemIcon item={{ category: item.category ?? 'login', title: item.title, subtitle: item.subtitle ?? '' }} size={24} />
                    <div className="min-w-0 flex-1">
                      <div
                        className={cn(
                          "truncate text-sm font-medium",
                          item.available ? "text-fg group-hover:text-accent" : "text-muted italic"
                        )}
                      >
                        {item.available ? item.title : t('related.notAvailable')}
                      </div>
                      {item.available && item.subtitle && (
                        <div className="truncate text-xs text-muted">{item.subtitle}</div>
                      )}
                    </div>
                    <Tooltip content={t('related.openToEdit')}>
                      <span
                        className="shrink-0 text-xs text-muted/60 group-hover:text-muted transition-colors select-none"
                        data-testid={`backlink-hint-${item.id}`}
                      >
                        {t('related.openToEdit')}
                      </span>
                    </Tooltip>
                  </div>
                ))}
              </div>
            </>
          )}
        </div>
      )}

      {detail.tags.length > 0 && (
        <div className="mb-4 flex flex-wrap gap-1.5 px-1">
          {detail.tags.map((t) => (
            <Badge key={t} tone="accent">
              {t}
            </Badge>
          ))}
        </div>
      )}
    </>
  );
}
