import { useEffect, useState } from 'react';
import type { VaultInfo } from '../../api/types';
import { api } from '../../api/client';
import { useT } from '../../i18n';
import { Button, Modal, toast } from '../../ui';
import { VAULT_COLORS } from './vaultIcons';
import { VaultFields } from './VaultFields';

export function NewVaultDialog({
  open,
  onOpenChange,
  onCreated,
}: {
  open: boolean;
  onOpenChange: (o: boolean) => void;
  onCreated: (v: VaultInfo) => void;
}) {
  const t = useT();
  const [name, setName] = useState('');
  const [description, setDescription] = useState('');
  const [icon, setIcon] = useState('vault');
  const [color, setColor] = useState(VAULT_COLORS[1]);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  useEffect(() => {
    if (open) {
      setName('');
      setDescription('');
      setIcon('vault');
      setColor(VAULT_COLORS[1]);
      setErr(null);
    }
  }, [open]);

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!name.trim()) return;
    setBusy(true);
    setErr(null);
    try {
      const v = await api.createVault({
        name: name.trim(),
        description: description.trim() || null,
        icon,
        color,
      });
      toast(t('vault.created', { name: v.name }));
      onCreated(v);
      onOpenChange(false);
      setName('');
      setDescription('');
    } catch (e) {
      const msg = (e as Error)?.message ?? String(e);
      setErr(t('vault.createError', { msg }));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal
      open={open}
      onOpenChange={onOpenChange}
      title={t('vault.newTitle')}
      description={t('vault.newDesc')}
      className="max-w-md"
    >
      <form onSubmit={submit} className="space-y-4" data-testid="new-vault">
        <VaultFields
          name={name}
          setName={setName}
          description={description}
          setDescription={setDescription}
          icon={icon}
          setIcon={setIcon}
          color={color}
          setColor={setColor}
        />
        {err && (
          <p role="alert" className="text-sm text-danger">
            {err}
          </p>
        )}
        <div className="flex justify-end gap-2">
          <Button type="button" variant="ghost" onClick={() => onOpenChange(false)}>
            {t('common.cancel')}
          </Button>
          <Button type="submit" variant="default" disabled={!name.trim() || busy}>
            {t('vault.create')}
          </Button>
        </div>
      </form>
    </Modal>
  );
}
