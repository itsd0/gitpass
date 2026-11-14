import { useEffect, useRef, useState } from 'react';
import QRCode from 'qrcode';
import { api } from '../../../api/client';
import { Button, Input, Modal } from '../../../ui';
import { useT, type Key } from '../../../i18n';
import { errMsg } from '../shared';

/**
 * Desktop side of pairing v2. This Mac shows the QR. The phone scans it.
 * There is no camera on this screen. A typed sync token stays in memory
 * and is cleared once the QR is showing.
 */
export function AddDeviceDialog({ open, onOpenChange }: { open: boolean; onOpenChange: (o: boolean) => void }) {
  const t = useT();
  const gen = useRef(0);
  const [phase, setPhase] = useState<'wait' | 'needToken' | 'qr' | 'code' | 'done' | 'error'>('wait');
  const [qr, setQr] = useState<string | null>(null);
  const [img, setImg] = useState<string | null>(null);
  const [expiresAt, setExpiresAt] = useState<number | null>(null);
  const [left, setLeft] = useState(0);
  const [code, setCode] = useState('');
  const [token, setToken] = useState('');
  const [errorKey, setErrorKey] = useState<Key | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!open) return;
    const id = ++gen.current;
    setPhase('wait');
    setQr(null);
    setImg(null);
    setExpiresAt(null);
    setCode('');
    setToken('');
    setErrorKey(null);
    void run(id, null);
    return () => {
      gen.current += 1;
      void api.pairingHostCancel().catch(() => {});
    };
    // run is recreated each render; the session starts only when the dialog opens.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  useEffect(() => {
    if (!qr) return;
    let live = true;
    QRCode.toDataURL(qr, { margin: 1, width: 240, errorCorrectionLevel: 'M', color: { dark: '#111111', light: '#ffffff' } })
      .then((url) => {
        if (live) setImg(url);
      })
      .catch(() => {
        if (live) setImg(null);
      });
    return () => {
      live = false;
    };
  }, [qr]);

  useEffect(() => {
    if ((phase !== 'qr' && phase !== 'code') || expiresAt == null) return;
    const tick = () => {
      const secs = Math.max(0, expiresAt - Math.floor(Date.now() / 1000));
      setLeft(secs);
      if (secs === 0) {
        setPhase('error');
        setErrorKey('devices.add.expired');
        void api.pairingHostCancel().catch(() => {});
      }
    };
    tick();
    const id = window.setInterval(tick, 1000);
    return () => window.clearInterval(id);
  }, [phase, expiresAt]);

  async function run(id: number, typed: string | null) {
    try {
      const prepared = await api.pairingHostListen({ token: typed });
      if (gen.current !== id) return;
      if (prepared.expiresAt * 1000 <= Date.now()) {
        setPhase('error');
        setErrorKey('devices.add.expired');
        await api.pairingHostCancel().catch(() => {});
        return;
      }
      setToken('');
      setQr(prepared.qr);
      setExpiresAt(prepared.expiresAt);
      setPhase('qr');
      await api.pairingHostAccept();
      if (gen.current !== id) return;
      setPhase('code');
    } catch (e) {
      if (gen.current !== id) return;
      const msg = safeError(e);
      if (msg === 'sync token is not configured') {
        setPhase('needToken');
        return;
      }
      setPhase('error');
      setErrorKey(messageKey(msg));
    }
  }

  async function close() {
    gen.current += 1;
    await api.pairingHostCancel().catch(() => {});
    onOpenChange(false);
  }

  async function submitCode() {
    const id = gen.current;
    setBusy(true);
    setErrorKey(null);
    try {
      const status = await api.pairingHostConfirm({ code });
      if (gen.current !== id) return;
      if (status.error) {
        const key = messageKey(safeError(status.error));
        setErrorKey(key);
        if (key !== 'devices.add.wrong') setPhase('error');
        return;
      }
      if (status.phase === 'done') setPhase('done');
    } catch (e) {
      if (gen.current !== id) return;
      setPhase('error');
      setErrorKey(messageKey(safeError(e)));
    } finally {
      if (gen.current === id) setBusy(false);
    }
  }

  const clock = `${String(Math.floor(left / 60)).padStart(2, '0')}:${String(left % 60).padStart(2, '0')}`;

  return (
    <Modal
      open={open}
      onOpenChange={(o) => {
        if (!o) void close();
      }}
      title={t('devices.add.title')}
      description={t('devices.add.intro')}
      footer={
        phase === 'done' ? (
          <Button onClick={() => onOpenChange(false)}>{t('settings.done')}</Button>
        ) : (
          <Button variant="outline" onClick={() => void close()}>
            {t('common.cancel')}
          </Button>
        )
      }
    >
      <p className="text-sm text-muted">{t('devices.add.firewall')}</p>
      <p className="mt-2 text-sm text-muted">{t('devices.add.isolation')}</p>
      {phase === 'wait' && <p className="mt-4 text-sm">{t('devices.add.waiting')}</p>}
      {phase === 'needToken' && (
        <form
          className="mt-4 space-y-2"
          onSubmit={(ev) => {
            ev.preventDefault();
            const id = ++gen.current;
            void run(id, token);
          }}
        >
          <label className="block text-sm" htmlFor="pairing-token">
            {t('devices.add.tokenLabel')}
          </label>
          <Input id="pairing-token" type="password" autoComplete="off" value={token} onChange={(ev) => setToken(ev.target.value)} />
          <p className="text-xs text-muted">{t('devices.add.tokenHelp')}</p>
          <Button type="submit">{t('devices.add.start')}</Button>
        </form>
      )}
      {(phase === 'qr' || phase === 'code') && (
        <div className="mt-4 flex flex-col items-center gap-2">
          {img && <img src={img} width={240} height={240} alt={t('devices.add.qrAlt')} />}
          {phase === 'qr' && <p className="text-sm">{t('devices.add.waiting')}</p>}
          <p className="font-mono text-sm text-accent">{t('devices.add.left', { time: clock })}</p>
        </div>
      )}
      {phase === 'code' && (
        <form
          className="mt-4 space-y-2"
          onSubmit={(ev) => {
            ev.preventDefault();
            void submitCode();
          }}
        >
          <label className="block text-sm" htmlFor="pairing-code">
            {t('devices.add.codeLabel')}
          </label>
          <Input
            id="pairing-code"
            inputMode="numeric"
            autoComplete="off"
            maxLength={6}
            value={code}
            aria-invalid={errorKey === 'devices.add.wrong'}
            onChange={(ev) => setCode(ev.target.value.replace(/\D/g, '').slice(0, 6))}
          />
          <p className="text-xs text-muted">{t('devices.add.codeHelp')}</p>
          {errorKey === 'devices.add.wrong' && <p className="text-sm text-danger">{t('devices.add.wrong')}</p>}
          <Button type="submit" disabled={busy || code.length !== 6}>
            {t('devices.add.confirm')}
          </Button>
        </form>
      )}
      {phase === 'done' && <p className="mt-4 text-sm">{t('devices.add.done')}</p>}
      {phase === 'error' && errorKey && <p className="mt-4 text-sm text-danger">{t(errorKey)}</p>}
    </Modal>
  );
}

function safeError(e: unknown): string {
  const msg = errMsg(e);
  return KNOWN.has(msg) ? msg : 'pairing failed';
}

function messageKey(msg: string): Key {
  switch (msg) {
    case 'pairing session expired':
      return 'devices.add.expired';
    case 'pairing verification code does not match':
      return 'devices.add.wrong';
    case 'too many wrong pairing codes':
      return 'devices.add.exhausted';
    case 'sync is not configured':
      return 'devices.add.noSync';
    case 'pairing payload field rejected':
      return 'devices.add.tokenRejected';
    case 'pairing timed out':
    case 'pairing connection closed':
    case 'pairing could not connect':
    case 'pairing needs a local network address':
    case 'pairing listener could not start':
      return 'devices.add.network';
    default:
      return 'devices.add.failed';
  }
}

const KNOWN = new Set([
  'pairing needs a local network address',
  'pairing listener could not start',
  'pairing could not connect',
  'pairing timed out',
  'pairing connection closed',
  'pairing failed',
  'pairing session expired',
  'pairing verification code does not match',
  'too many wrong pairing codes',
  'sync is not configured',
  'sync token is not configured',
  'pairing payload field rejected',
  'no pairing in progress',
  'vault is locked',
]);
