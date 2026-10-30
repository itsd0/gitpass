import React, { FormEvent, useEffect, useState } from 'react';
import { Fingerprint, Loader2, LogIn } from 'lucide-react';
import { api } from '../../api/client';
import { useApp } from '../../lib/store';
import { Button, Input, Logo } from '../../ui';
import { useT } from '../../i18n';

export function QuickAccessUnlock({ onDone }: { onDone?: () => void }) {
  const t = useT();
  const { unlock, setAccount } = useApp();
  const [pw, setPw] = useState('');
  const [err, setErr] = useState('');
  const [busy, setBusy] = useState(false);
  const [vaultPath, setVaultPath] = useState('');
  const [bioAvailable, setBioAvailable] = useState(false);
  const [throttleLeft, setThrottleLeft] = useState<number>(0);

  useEffect(() => {
    let active = true;
    Promise.all([api.status(), api.getSettings()]).then(([st, sett]) => {
      if (!active) return;
      setVaultPath(st.vaultPath);
      const canBio = !!st.biometricAvailable && !!sett.biometricUnlock;
      setBioAvailable(canBio);
      if (st.throttledSecs && st.throttledSecs > 0) {
        setThrottleLeft(st.throttledSecs);
      }
      if (canBio && !st.unlocked) {
        // Auto-prompt Touch ID on show if enabled
        void touchId(st.vaultPath);
      }
    }).catch(() => {});
    return () => {
      active = false;
    };
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    if (throttleLeft <= 0) return;
    const interval = setInterval(() => {
      setThrottleLeft((n) => Math.max(0, n - 1));
    }, 1000);
    return () => clearInterval(interval);
  }, [throttleLeft]);

  async function touchId(vp?: string) {
    setBusy(true);
    setErr('');
    try {
      const s = await api.unlockBiometric({ vaultPath: vp ?? vaultPath });
      setAccount(s.account);
      unlock();
      onDone?.();
    } catch (x) {
      const msg = x instanceof Error ? x.message : String(x);
      const throttleMatch = msg.match(/throttled.*?(\d+)\s*s?/i);
      if (throttleMatch) {
        setThrottleLeft(parseInt(throttleMatch[1], 10));
      } else {
        setErr(msg);
      }
    } finally {
      setBusy(false);
    }
  }

  async function submit(e: FormEvent) {
    e.preventDefault();
    if (!pw || busy) return;
    setBusy(true);
    setErr('');
    try {
      const s = await api.unlock({ vaultPath, password: pw, secretKey: null });
      setAccount(s.account);
      unlock();
      onDone?.();
    } catch (x) {
      const msg = x instanceof Error ? x.message : String(x);
      setErr(msg);
    } finally {
      setPw('');
      setBusy(false);
    }
  }

  const onKey = (e: React.KeyboardEvent) => {
    if (e.key === 'Escape') {
      e.preventDefault();
      if ((window as any).__TAURI_INTERNALS__) {
        void import('@tauri-apps/api/window').then((m) => m.getCurrentWindow().hide());
      }
      onDone?.();
    }
  };

  return (
    <div
      role="dialog"
      aria-label={t('unlock.title')}
      onKeyDown={onKey}
      className="flex h-full w-full flex-col items-center justify-center overflow-hidden rounded-xl border border-border bg-panel p-6 text-fg select-none"
      data-testid="quick-access-unlock"
    >
      <div className="mb-4 flex flex-col items-center gap-2">
        <Logo size={40} />
        <h1 className="text-base font-semibold">{t('unlock.title')}</h1>
        <p className="text-xs text-muted truncate max-w-xs">{vaultPath || '…'}</p>
      </div>

      <form onSubmit={submit} className="w-full max-w-xs space-y-3">
        <Input
          type="password"
          autoFocus
          autoComplete="off"
          value={pw}
          onChange={(e) => setPw(e.target.value)}
          placeholder={t('unlock.masterPasswordPlaceholder')}
          aria-label={t('unlock.masterPassword')}
          className="h-10 focus-visible:ring-accent"
        />

        {(throttleLeft > 0 ? t('unlock.throttled', { n: throttleLeft }) : err) && (
          <p role="alert" className="text-xs text-danger leading-tight">
            {throttleLeft > 0 ? t('unlock.throttled', { n: throttleLeft }) : err}
          </p>
        )}

        <div className="flex gap-2">
          <Button
            type="submit"
            className="flex-1"
            disabled={busy || !pw || throttleLeft > 0}
          >
            {busy ? (
              <Loader2 className="h-4 w-4 animate-spin" />
            ) : (
              <span className="flex items-center gap-1.5">
                <LogIn className="h-4 w-4" />
                {t('unlock.submit')}
              </span>
            )}
          </Button>

          {bioAvailable && (
            <Button
              type="button"
              variant="outline"
              disabled={busy}
              onClick={() => void touchId()}
              aria-label={t('unlock.touchId')}
              title={t('unlock.touchId')}
            >
              <Fingerprint className="h-4 w-4 text-accent" />
            </Button>
          )}
        </div>
      </form>
    </div>
  );
}
