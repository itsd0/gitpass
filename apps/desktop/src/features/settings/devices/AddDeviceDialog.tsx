import { useCallback, useEffect, useRef, useState } from 'react';
import QRCode from 'qrcode';
import { api } from '../../../api/client';
import type { PairingHostPrepared } from '../../../api/types';
import { Button, Input, Modal } from '../../../ui';
import { useT, type Key } from '../../../i18n';
import { errMsg } from '../shared';

/**
 * Add device (#458): QR device pairing, desktop host role (#457).
 *
 * 1. The phone shows its `offer` QR; this dialog reads it with the camera (or an image).
 * 2. The desktop shows the `announce` QR; the phone scans it and shows a 6-digit code.
 *    The person types that code here. The desktop never displays a code of its own.
 * 3. On a match the desktop shows the `grant` QR (ciphertext). The phone scans it and the person
 *    types the Master Password on the phone. The Master Password is never part of pairing.
 *
 * Camera frames go to the Rust core for decoding and are not stored. The optional sync token is
 * kept in component state only until it is handed to the core, then cleared.
 */
type Step = 'scan' | 'confirm' | 'grant' | 'failed';

/** Map fixed backend error strings to UI text. Unknown messages fall back to a generic error. */
export function pairingErrorKey(message: string): Key | null {
  const m = message.toLowerCase();
  if (m.includes('verification code does not match')) return 'devices.add.badCode';
  if (m.includes('too many wrong pairing codes')) return 'devices.add.tooMany';
  if (m.includes('expired')) return 'devices.add.expired';
  if (m.includes('already used')) return 'devices.add.used';
  if (m.includes('sync token is not configured')) return 'devices.add.noToken';
  if (m.includes('sync is not configured')) return 'devices.add.notConfigured';
  if (m.includes('payload field rejected')) return 'devices.add.badToken';
  if (m.includes("isn't a gitpass pairing code")) return 'devices.add.notPairing';
  return null;
}

const SCAN_INTERVAL_MS = 400;
const MAX_FRAME_EDGE = 960;

export function AddDeviceDialog({ open, onClose }: { open: boolean; onClose: (paired: boolean) => void }) {
  const t = useT();
  const [step, setStep] = useState<Step>('scan');
  const [token, setToken] = useState('');
  const [hint, setHint] = useState<string | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const [cameraError, setCameraError] = useState(false);
  const [prepared, setPrepared] = useState<PairingHostPrepared | null>(null);
  const [qr, setQr] = useState<string | null>(null);
  const [code, setCode] = useState('');
  const [busy, setBusy] = useState(false);
  const [remaining, setRemaining] = useState<number | null>(null);
  const videoRef = useRef<HTMLVideoElement | null>(null);
  const tokenRef = useRef('');
  tokenRef.current = token;
  const accepting = useRef(false);
  const grantRef = useRef<string | null>(null);

  const message = (e: unknown) => {
    const key = pairingErrorKey(errMsg(e));
    return key ? t(key) : t('common.error', { msg: errMsg(e) });
  };

  const reset = useCallback(() => {
    setStep('scan');
    setHint(null);
    setFailure(null);
    setCameraError(false);
    setPrepared(null);
    setQr(null);
    setCode('');
    setBusy(false);
    setRemaining(null);
    accepting.current = false;
  }, []);

  useEffect(() => {
    if (open) reset();
    else setToken('');
  }, [open, reset]);

  const accept = useCallback(
    async (offer: string) => {
      if (accepting.current) return;
      accepting.current = true;
      setBusy(true);
      try {
        const typed = tokenRef.current.trim();
        const p = await api.pairingHostPrepare({ offer, token: typed ? typed : null });
        setToken('');
        setPrepared(p);
        setStep('confirm');
      } catch (e) {
        setFailure(message(e));
        setStep('failed');
      } finally {
        setBusy(false);
      }
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [t],
  );

  const scanData = useCallback(
    async (dataBase64: string, fromImage: boolean) => {
      try {
        const offer = await api.pairingScanOffer({ dataBase64 });
        await accept(offer);
      } catch (e) {
        const key = pairingErrorKey(errMsg(e));
        if (key === 'devices.add.notPairing') setHint(t(key));
        else if (fromImage) setHint(t('devices.add.noQr'));
      }
    },
    [accept, t],
  );

  // Camera loop while scanning. Frames are decoded in Rust and dropped.
  useEffect(() => {
    if (!open || step !== 'scan') return;
    let live = true;
    let stream: MediaStream | null = null;
    let timer: ReturnType<typeof setInterval> | undefined;
    let inFlight = false;
    const canvas = document.createElement('canvas');
    const tick = () => {
      const video = videoRef.current;
      if (!live || inFlight || accepting.current || !video || !video.videoWidth) return;
      const scale = Math.min(1, MAX_FRAME_EDGE / Math.max(video.videoWidth, video.videoHeight));
      canvas.width = Math.round(video.videoWidth * scale);
      canvas.height = Math.round(video.videoHeight * scale);
      const ctx = canvas.getContext('2d');
      if (!ctx) return;
      ctx.drawImage(video, 0, 0, canvas.width, canvas.height);
      inFlight = true;
      scanData(canvas.toDataURL('image/jpeg', 0.85), false).finally(() => {
        inFlight = false;
      });
    };
    const media = typeof navigator !== 'undefined' ? navigator.mediaDevices : undefined;
    if (!media?.getUserMedia) {
      setCameraError(true);
      return;
    }
    media
      .getUserMedia({ video: { width: { ideal: 1280 }, height: { ideal: 720 } }, audio: false })
      .then((s) => {
        if (!live) {
          s.getTracks().forEach((tr) => tr.stop());
          return;
        }
        stream = s;
        const video = videoRef.current;
        if (video) {
          video.srcObject = s;
          void video.play().catch(() => undefined);
        }
        timer = setInterval(tick, SCAN_INTERVAL_MS);
      })
      .catch(() => live && setCameraError(true));
    return () => {
      live = false;
      if (timer) clearInterval(timer);
      stream?.getTracks().forEach((tr) => tr.stop());
      if (videoRef.current) videoRef.current.srcObject = null;
    };
  }, [open, step, scanData]);

  // QR for the current step: announce, then grant.
  useEffect(() => {
    let live = true;
    const text = step === 'confirm' ? prepared?.announce : step === 'grant' ? grantRef.current : null;
    if (!text) {
      setQr(null);
      return;
    }
    QRCode.toDataURL(text, { margin: 2, width: step === 'grant' ? 340 : 260, errorCorrectionLevel: step === 'grant' ? 'L' : 'M', color: { dark: '#111111', light: '#ffffff' } })
      .then((url) => live && setQr(url))
      .catch(() => live && setQr(null));
    return () => {
      live = false;
    };
  }, [step, prepared]);

  // Countdown to the offer's expiry. The grant is only useful to the phone until then too.
  useEffect(() => {
    if (!prepared || (step !== 'confirm' && step !== 'grant')) return;
    const update = () => {
      const left = Math.max(0, Math.ceil(prepared.expiresAt - Date.now() / 1000));
      setRemaining(left);
      if (left <= 0) {
        grantRef.current = null;
        setFailure(t('devices.add.expired'));
        setStep('failed');
        void api.pairingHostCancel().catch(() => undefined);
      }
    };
    update();
    const timer = setInterval(update, 1000);
    return () => clearInterval(timer);
  }, [prepared, step, t]);

  const confirm = async () => {
    setBusy(true);
    setHint(null);
    try {
      grantRef.current = await api.pairingHostRelease({ code: code.trim() });
      setCode('');
      setStep('grant');
    } catch (e) {
      const key = pairingErrorKey(errMsg(e));
      if (key === 'devices.add.badCode') {
        setHint(t(key));
        setCode('');
      } else {
        setFailure(message(e));
        setStep('failed');
      }
    } finally {
      setBusy(false);
    }
  };

  const close = () => {
    const paired = step === 'grant';
    grantRef.current = null;
    setToken('');
    void api.pairingHostCancel().catch(() => undefined);
    onClose(paired);
  };

  const onImage = (file: File | undefined) => {
    if (!file) return;
    const reader = new FileReader();
    reader.onload = () => typeof reader.result === 'string' && void scanData(reader.result, true);
    reader.readAsDataURL(file);
  };

  const footer =
    step === 'grant' ? (
      <Button onClick={close} data-testid="pair-done">{t('devices.add.done')}</Button>
    ) : step === 'failed' ? (
      <>
        <Button variant="outline" onClick={close}>{t('common.cancel')}</Button>
        <Button onClick={reset} data-testid="pair-restart">{t('devices.add.restart')}</Button>
      </>
    ) : (
      <Button variant="outline" onClick={close} data-testid="pair-cancel">{t('common.cancel')}</Button>
    );

  return (
    <Modal open={open} onOpenChange={(o) => !o && close()} title={t('devices.add.title')} footer={footer}>
      {step === 'scan' && (
        <div className="flex flex-col gap-3 text-sm">
          <p>{t('devices.add.intro')}</p>
          <label className="flex flex-col gap-1">
            <span className="font-medium">{t('devices.add.token')}</span>
            <Input
              type="password"
              autoComplete="off"
              spellCheck={false}
              value={token}
              onChange={(e) => setToken(e.target.value)}
              placeholder="github_pat_…"
              data-testid="pair-token"
            />
            <span className="text-xs text-muted">{t('devices.add.tokenHelp')}</span>
          </label>
          {cameraError ? (
            <p className="text-xs text-danger" data-testid="pair-camera-error">{t('devices.add.cameraError')}</p>
          ) : (
            <div className="overflow-hidden rounded-md border border-border bg-black">
              <video ref={videoRef} className="aspect-video w-full" muted playsInline aria-label={t('devices.add.cameraLabel')} />
            </div>
          )}
          <p className="text-xs text-muted">{busy ? t('devices.add.connecting') : t('devices.add.looking')}</p>
          {hint && <p className="text-xs text-danger" role="alert">{hint}</p>}
          <label className="self-start">
            <span className="cursor-pointer text-xs text-accent underline">{t('devices.add.chooseImage')}</span>
            <input type="file" accept="image/*" className="sr-only" data-testid="pair-image" onChange={(e) => onImage(e.target.files?.[0])} />
          </label>
        </div>
      )}
      {step === 'confirm' && (
        <div className="flex flex-col items-center gap-3 text-sm">
          <p className="self-start">{t('devices.add.scanAnnounce')}</p>
          {qr && <img src={qr} alt={t('devices.add.qrAlt')} className="h-[260px] w-[260px] rounded bg-white" data-testid="pair-announce-qr" />}
          {remaining !== null && <p className="text-xs text-muted" data-testid="pair-remaining">{t('devices.add.expiresIn', { s: remaining })}</p>}
          <label className="flex w-full flex-col gap-1">
            <span className="font-medium">{t('devices.add.codeLabel')}</span>
            <Input
              inputMode="numeric"
              autoComplete="one-time-code"
              maxLength={6}
              value={code}
              onChange={(e) => setCode(e.target.value.replace(/\D/g, '').slice(0, 6))}
              onKeyDown={(e) => e.key === 'Enter' && code.length === 6 && !busy && void confirm()}
              data-testid="pair-code"
            />
            <span className="text-xs text-muted">{t('devices.add.codeHelp')}</span>
          </label>
          {hint && <p className="self-start text-xs text-danger" role="alert">{hint}</p>}
          <Button className="self-end" disabled={code.length !== 6 || busy} onClick={() => void confirm()} data-testid="pair-confirm">
            {t('devices.add.confirm')}
          </Button>
        </div>
      )}
      {step === 'grant' && (
        <div className="flex flex-col items-center gap-3 text-sm">
          <p className="self-start">{t('devices.add.scanGrant')}</p>
          {qr && <img src={qr} alt={t('devices.add.qrAlt')} className="h-[340px] w-[340px] rounded bg-white" data-testid="pair-grant-qr" />}
          {remaining !== null && <p className="text-xs text-muted">{t('devices.add.expiresIn', { s: remaining })}</p>}
          <p className="self-start text-xs text-muted">{t('devices.add.passwordNote')}</p>
        </div>
      )}
      {step === 'failed' && (
        <p className="text-sm text-danger" role="alert" data-testid="pair-failure">{failure}</p>
      )}
    </Modal>
  );
}
