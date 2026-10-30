import { useEffect, useState } from 'react';
import { Download, Pencil, Trash2 } from 'lucide-react';
import { api } from '../../../api/client';
import { VaultInfo } from '../../../api/types';
import { cn } from '../../../lib/utils';
import { Button, Skeleton, toast } from '../../../ui';
import { useT } from '../../../i18n';
import { useApp } from '../../../lib/store';
import { VaultIcon } from '../../home/vaultIcons';
import { DeleteVaultDialog, EditVaultDialog } from '../../home/VaultDialogs';
import { ExportDialog } from '../../export';
import { SettingsSection, SettingsRow } from '../SettingsSection';
import { errMsg } from '../shared';

export function VaultsTab({ highlightedId }: { highlightedId?: string | null }) {
  const t = useT();
  const [vs, setVs] = useState<VaultInfo[] | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [editV, setEditV] = useState<VaultInfo | null>(null);
  const [delV, setDelV] = useState<VaultInfo | null>(null);
  const [exportV, setExportV] = useState<VaultInfo | null>(null);
  const reload = () => api.listVaults().then(setVs, (e) => setErr(errMsg(e)));
  useEffect(() => {
    void reload();
  }, []);
  if (err) return <p role="alert" className="py-4 text-sm text-danger">{err}</p>;
  if (!vs) return <Skeleton className="my-4 h-24" />;
  const done = () => {
    void reload();
    useApp.getState().refreshItems();
  };
  return (
    <div data-testid="vaults-settings">
      <SettingsSection
        id="setting-vaults"
        title={t('settings.sectionVaults')}
        description={t('vault.manageHelp')}
        className={cn(highlightedId === 'setting-vaults' && 'rounded-lg bg-accent/15 ring-2 ring-accent')}
      >
        {vs.map((v) => (
          <SettingsRow
            key={v.id}
            label={<div className="flex items-center gap-2"><VaultIcon icon={v.icon} color={v.color} size={20} /><span>{v.name}</span></div>}
            description={`${v.description ? `${v.description} · ` : ''}${t('settings.vaultItems')}: ${v.itemCount}`}
          >
            <div className="flex items-center gap-1">
              <Button
                variant="ghost"
                size="sm"
                aria-label={t('export.vaultLabel', { vault: v.name })}
                title={t('export.vaultLabel', { vault: v.name })}
                onClick={() => setExportV(v)}
              >
                <Download size={14} />
              </Button>
              <Button
                variant="ghost"
                size="sm"
                aria-label={`${t('vault.edit')} ${v.name}`}
                onClick={() => setEditV(v)}
              >
                <Pencil size={14} />
              </Button>
              <Button
                variant="ghost"
                size="sm"
                aria-label={`${t('vault.delete')} ${v.name}`}
                disabled={vs.length <= 1}
                title={vs.length <= 1 ? t('vault.lastVault') : undefined}
                onClick={() => setDelV(v)}
              >
                <Trash2 size={14} className="text-danger" />
              </Button>
            </div>
          </SettingsRow>
        ))}
      </SettingsSection>
      <EditVaultDialog
        vault={editV}
        onOpenChange={(o) => !o && setEditV(null)}
        onSaved={() => {
          toast(t('vault.saved'));
          done();
        }}
      />
      <DeleteVaultDialog
        vault={delV}
        isLast={vs.length <= 1}
        onOpenChange={(o) => !o && setDelV(null)}
        onDeleted={() => {
          toast(t('vault.deleted', { vault: delV?.name ?? '' }));
          useApp.getState().selectItem(null);
          done();
        }}
      />
      <ExportDialog
        open={!!exportV}
        onOpenChange={(o) => !o && setExportV(null)}
        vaultId={exportV?.id}
        vaultName={exportV?.name}
      />
    </div>
  );
}

/** `settings` route with tab `vault:<id>` (Home → vault card → settings). */
export function VaultPanel({ vaultId }: { vaultId: string }) {
  const t = useT();
  const [v, setV] = useState<VaultInfo | null | undefined>(undefined);
  const [count, setCount] = useState(0);
  const [edit, setEdit] = useState(false);
  const [del, setDel] = useState(false);
  const [exp, setExp] = useState(false);
  const reload = () =>
    api
      .listVaults()
      .then((vs) => {
        setCount(vs.length);
        setV(vs.find((x) => x.id === vaultId) ?? null);
      })
      .catch(() => setV(null));
  useEffect(() => {
    void reload();
  }, [vaultId]); // eslint-disable-line react-hooks/exhaustive-deps
  if (v === undefined) return <Skeleton className="h-24" />;
  if (v === null) return <p role="alert" className="py-4 text-sm text-danger">{t('settings.vaultMissing')}</p>;
  const app = useApp.getState();
  return (
    <SettingsSection title={t('settings.vault')}>
      <SettingsRow label={t('settings.vaultName')}>
        <span className="text-sm font-medium">{v.name}</span>
      </SettingsRow>
      {v.description && (
        <SettingsRow label={t('settings.vaultDescription')}>
          <span className="text-sm">{v.description}</span>
        </SettingsRow>
      )}
      <SettingsRow label={t('settings.vaultItems')}>
        <span className="text-sm">{v.itemCount}</span>
      </SettingsRow>
      <SettingsRow label={t('people.vaultMembers')}>
        <span className="text-sm">{v.members.length}</span>
      </SettingsRow>
      <div className="flex gap-2 py-4">
        <Button
          variant="default"
          onClick={() => app.setRoute({ name: 'items', filter: { kind: 'vault', vaultId: v.id } })}
        >
          {t('settings.vaultOpen')}
        </Button>
        <Button variant="outline" onClick={() => app.openModal('people', { vaultId: v.id })}>
          {t('people.invite')}
        </Button>
        <Button variant="outline" onClick={() => setExp(true)}>
          <Download size={14} />
          {t('export.buttonVault')}
        </Button>
        <Button variant="outline" onClick={() => setEdit(true)}>
          <Pencil size={14} />
          {t('vault.edit')}
        </Button>
        <Button
          variant="outline"
          className="text-danger"
          disabled={count <= 1}
          title={count <= 1 ? t('vault.lastVault') : undefined}
          onClick={() => setDel(true)}
        >
          <Trash2 size={14} />
          {t('vault.delete')}
        </Button>
      </div>
      <EditVaultDialog
        vault={edit ? v : null}
        onOpenChange={setEdit}
        onSaved={(nv) => {
          toast(t('vault.saved'));
          setV(nv);
          app.refreshItems();
        }}
      />
      <DeleteVaultDialog
        vault={del ? v : null}
        isLast={count <= 1}
        onOpenChange={setDel}
        onDeleted={() => {
          toast(t('vault.deleted', { vault: v.name }));
          app.selectItem(null);
          app.refreshItems();
          app.setRoute({ name: 'settings', tab: 'vaults' });
        }}
      />
      <ExportDialog open={exp} onOpenChange={setExp} vaultId={v.id} vaultName={v.name} />
    </SettingsSection>
  );
}

