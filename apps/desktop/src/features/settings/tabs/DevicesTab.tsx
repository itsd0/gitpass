import { useEffect, useState } from 'react';
import { Smartphone } from 'lucide-react';
import { api } from '../../../api/client';
import { DeviceInfo } from '../../../api/types';
import { cn } from '../../../lib/utils';
import { Badge, Button, Modal, Skeleton, toast } from '../../../ui';
import { useT } from '../../../i18n';
import { SettingsSection, SettingsRow } from '../SettingsSection';
import { errMsg } from '../shared';

export function DevicesTab({ highlightedId }: { highlightedId?: string | null }) {
  const t = useT();
  const [list, setList] = useState<DeviceInfo[] | null>(null);
  const [target, setTarget] = useState<DeviceInfo | null>(null);
  const [rotate, setRotate] = useState(true);
  const [busy, setBusy] = useState(false);
  const load = () =>
    api
      .listDevices()
      .then(setList)
      .catch((e) => {
        setList([]);
        toast(t('common.error', { msg: errMsg(e) }), 'danger');
      });
  useEffect(() => {
    load();
  }, []); // eslint-disable-line react-hooks/exhaustive-deps
  if (!list) return <Skeleton className="h-24" />;
  if (!list.length) return <p className="py-6 text-sm text-muted">{t('devices.empty')}</p>;
  return (
    <>
      <SettingsSection id="setting-devices" title={t('settings.sectionDevices')} className={cn(highlightedId === 'setting-devices' && 'rounded-lg bg-accent/15 ring-2 ring-accent')}>
        {list.map((d) => (
          <SettingsRow
            key={d.id}
            label={<div className="flex items-center gap-2"><Smartphone className="h-4 w-4 text-muted" aria-hidden /><span>{d.name}</span></div>}
            description={
              <div className="flex flex-col gap-0.5">
                <div className="flex items-center gap-1.5">
                  {d.current && <Badge tone="ok">{t('devices.current')}</Badge>}
                  {d.revoked && <Badge tone="danger">{t('devices.revoked')}</Badge>}
                </div>
                <div>{t('devices.lastSeen', { when: new Date(d.lastSeen).toLocaleString() })}</div>
              </div>
            }
          >
            {!d.current && !d.revoked && (
              <Button
                size="sm"
                variant="ghost"
                onClick={() => {
                  setRotate(true);
                  setTarget(d);
                }}
              >
                {t('devices.revoke')}
              </Button>
            )}
          </SettingsRow>
        ))}
      </SettingsSection>
      <Modal
        open={!!target}
        onOpenChange={(o) => !o && setTarget(null)}
        title={t('devices.revokeTitle', { name: target?.name ?? '' })}
        description={t('devices.revokeBody')}
        footer={
          <>
            <Button variant="outline" onClick={() => setTarget(null)}>
              {t('common.cancel')}
            </Button>
            <Button
              variant="danger"
              disabled={busy}
              onClick={async () => {
                if (!target) return;
                setBusy(true);
                try {
                  await api.revokeDevice({ id: target.id, rotateCredentials: rotate });
                  toast(t('devices.done'));
                  setTarget(null);
                  load();
                } catch (e) {
                  toast(t('common.error', { msg: errMsg(e) }), 'danger');
                } finally {
                  setBusy(false);
                }
              }}
            >
              {t('devices.revoke')}
            </Button>
          </>
        }
      >
        <label className="flex items-start gap-2 text-sm">
          <input
            type="checkbox"
            className="mt-0.5 h-4 w-4"
            checked={rotate}
            onChange={(e) => setRotate(e.target.checked)}
          />
          {t('devices.rotate')}
        </label>
        <p className="mt-2 text-xs text-muted">{t('devices.rotateHelp')}</p>
      </Modal>
    </>
  );
}

