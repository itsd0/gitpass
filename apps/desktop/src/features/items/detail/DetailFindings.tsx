import { ExternalLink, EyeOff, ShieldAlert } from 'lucide-react';
import { api } from '../../../api/client';
import type { ItemDetail as Detail } from '../../../api/types';
import { useApp } from '../../../lib/store';
import { useT } from '../../../i18n';
import { Button, toast } from '../../../ui';
import { errText } from '../actions';

export function DetailFindings({ detail }: { detail: Detail }) {
  const t = useT();
  const { refreshItems, selectItem } = useApp();
  const findings = detail.findings ?? [];
  const canChangePassword = !!detail.changePasswordUrl && findings.some((f) => ['weak', 'reused', 'pwned', 'insecure_url'].includes(f.kind));
  const changePassword = () => api.openChangePassword({ vaultId: detail.vaultId, itemId: detail.id }).catch((e) => toast(errText(e), 'danger'));

  return (
    <>
      {findings.length > 0 && (
        <div
          className="mb-4 flex flex-wrap items-center justify-between gap-2 rounded-lg border border-warn/40 bg-warn/10 px-4 py-2 text-xs"
          data-testid="watchtower-findings"
        >
          <div className="flex min-w-0 items-start gap-1.5">
            <ShieldAlert size={14} className="mt-0.5 shrink-0 text-warn" />
            <div className="min-w-0">
              <div className="font-medium text-fg">{t('detail.watchtowerFindings')}</div>
              <div className="text-muted">{findings.map((f) => f.description).join(' · ')}</div>
            </div>
          </div>
          {canChangePassword && (
            <Button
              variant="outline"
              size="sm"
              onClick={changePassword}
              aria-label={t('watchtower.changePasswordFor', { title: detail.title })}
            >
              <ExternalLink size={13} className="text-accent" />{t('watchtower.changePassword')}
            </Button>
          )}
        </div>
      )}
      {detail.ignoredFindings && detail.ignoredFindings.length > 0 && (
        <div
          className="mb-4 flex items-center justify-between rounded-lg border border-border bg-card/60 px-4 py-2 text-xs text-muted"
          data-testid="watchtower-ignored-note"
        >
          <div className="flex items-center gap-1.5">
            <EyeOff size={14} className="text-muted" />
            <span>{t('detail.watchtowerIgnored')}</span>
          </div>
          <button
            type="button"
            onClick={async () => {
              try {
                for (const k of detail.ignoredFindings ?? []) {
                  await api.unignoreFinding({ vaultId: detail.vaultId, itemId: detail.id, kind: k });
                }
                toast(t('detail.warningRestored'));
                refreshItems();
                selectItem(detail.id);
              } catch (e) {
                toast(errText(e), 'danger');
              }
            }}
            className="font-medium text-accent hover:underline focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent"
          >
            {t('common.undo')}
          </button>
        </div>
      )}
    </>
  );
}
