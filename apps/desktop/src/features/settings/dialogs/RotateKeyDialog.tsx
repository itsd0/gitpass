import { EmergencyKitFlow } from '../../onboarding/EmergencyKit';
import { useState } from 'react';
import { api } from '../../../api/client';
import { Button, Modal, toast } from '../../../ui';
import { useT } from '../../../i18n';
import { errMsg } from '../shared';

export function RotateKeyDialog({ open, onOpenChange }: { open: boolean; onOpenChange: (o: boolean) => void }) {
  const t = useT();
  const [kit, setKit] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);
  const [busy, setBusy] = useState(false);
  const [showKit, setShowKit] = useState(false);
  const close = () => {
    if (kit && !saved) return;
    setKit(null);
    setSaved(false);
    onOpenChange(false);
  };
  return (
    <Modal
      open={open}
      onOpenChange={(o) => !o && close()}
      title={kit ? t('settings.emergencyKit') : t('settings.rotateKey')}
      description={kit ? t('settings.kitWarning') : t('settings.rotateKeyHelp')}
      wide={!!kit}
      footer={
        kit ? (
          <Button variant="default" disabled={!saved} onClick={close}>
            {t('settings.done')}
          </Button>
        ) : (
          <>
            <Button variant="outline" onClick={close}>
              {t('common.cancel')}
            </Button>
            <Button
              variant="danger"
              disabled={busy}
              onClick={async () => {
                setBusy(true);
                try {
                  setKit((await api.rotateSecretKey()).emergencyKit);
                  setShowKit(true);
                } catch (e) {
                  toast(t('common.error', { msg: errMsg(e) }), 'danger');
                } finally {
                  setBusy(false);
                }
              }}
            >
              {t('settings.rotateKeyConfirm')}
            </Button>
          </>
        )
      }
    >
      {kit && (
        <>
          <pre
            className="max-h-64 overflow-auto whitespace-pre-wrap rounded-md border border-border bg-bg p-3 font-mono text-xs"
            data-testid="emergency-kit"
          >
            {kit}
          </pre>
          {showKit && <EmergencyKitFlow onClose={() => setShowKit(false)} />}
          <div className="mt-3 flex gap-2">
            <Button variant="default" size="sm" onClick={() => setShowKit(true)}>
              {t('kit.openPrintable')}
            </Button>
            <Button
              variant="outline"
              size="sm"
              onClick={() =>
                navigator.clipboard.writeText(kit).then(() => toast(t('common.copied')))
              }
            >
              {t('common.copy')}
            </Button>
          </div>
          <label className="mt-4 flex items-center gap-2 text-sm">
            <input
              type="checkbox"
              className="h-4 w-4"
              checked={saved}
              onChange={(e) => setSaved(e.target.checked)}
            />
            {t('settings.kitSaved')}
          </label>
        </>
      )}
    </Modal>
  );
}

