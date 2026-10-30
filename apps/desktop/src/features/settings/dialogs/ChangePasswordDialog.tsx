import { useState } from 'react';
import { api } from '../../../api/client';
import { Button, Modal, Input, toast } from '../../../ui';
import { useT } from '../../../i18n';
import { errMsg } from '../shared';
import { CapsLockHint, useCapsLockHint } from '../../../lib/capsLock';

export function ChangePasswordDialog({ open, onOpenChange }: { open: boolean; onOpenChange: (o: boolean) => void }) {
  const t = useT();
  const [cur, setCur] = useState('');
  const [next, setNext] = useState('');
  const [conf, setConf] = useState('');
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const capsCur = useCapsLockHint();
  const capsNext = useCapsLockHint();
  const capsConf = useCapsLockHint();
  const problem =
    next && next.length < 10
      ? t('settings.passwordShort')
      : conf && next !== conf
      ? t('settings.passwordMismatch')
      : null;
  const close = () => {
    setCur('');
    setNext('');
    setConf('');
    setErr(null);
    onOpenChange(false);
  };
  return (
    <Modal
      open={open}
      onOpenChange={(o) => !o && close()}
      title={t('settings.changePassword')}
      footer={
        <>
          <Button variant="outline" onClick={close}>
            {t('common.cancel')}
          </Button>
          <Button
            variant="default"
            disabled={!cur || !next || next !== conf || !!problem || busy}
            onClick={async () => {
              setBusy(true);
              setErr(null);
              try {
                await api.changePassword({ current: cur, next });
                toast(t('settings.passwordChanged'));
                close();
              } catch (e) {
                setErr(errMsg(e));
              } finally {
                setBusy(false);
              }
            }}
          >
            {t('common.save')}
          </Button>
        </>
      }
    >
      <div className="space-y-3">
        <Input
          type="password"
          autoComplete="current-password"
          aria-label={t('settings.currentPassword')}
          placeholder={t('settings.currentPassword')}
          value={cur}
          onChange={(e) => setCur(e.target.value)}
          {...capsCur.fieldProps}
        />
        <CapsLockHint id={capsCur.hintId} show={capsCur.show} />
        <Input
          type="password"
          autoComplete="new-password"
          aria-label={t('settings.newPassword')}
          placeholder={t('settings.newPassword')}
          value={next}
          onChange={(e) => setNext(e.target.value)}
          {...capsNext.fieldProps}
        />
        <CapsLockHint id={capsNext.hintId} show={capsNext.show} />
        <Input
          type="password"
          autoComplete="new-password"
          aria-label={t('settings.confirmPassword')}
          placeholder={t('settings.confirmPassword')}
          value={conf}
          onChange={(e) => setConf(e.target.value)}
          {...capsConf.fieldProps}
        />
        <CapsLockHint id={capsConf.hintId} show={capsConf.show} />
        {(problem || err) && (
          <p role="alert" className="text-sm text-danger">
            {problem ?? t('common.error', { msg: err! })}
          </p>
        )}
      </div>
    </Modal>
  );
}

