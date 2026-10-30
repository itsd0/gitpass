import { useEffect, useState } from 'react';
import * as T from '@radix-ui/react-toast';
import { create } from 'zustand';
import { translate, useI18n, useT, type Key } from '../i18n';

export interface ToastAction {
  label: string;
  onClick: () => void | Promise<void>;
  altText?: string;
}

/** Countdown for a concealed clipboard copy. `seconds === 0` means clearing is off. */
export interface ClipboardToast {
  base: string;
  seconds: number;
  onClearNow: () => Promise<boolean>;
  onOpenSetting: () => void;
}

export interface ToastOptions {
  tone?: 'default' | 'danger';
  duration?: number;
  action?: ToastAction;
  clipboard?: ClipboardToast;
}

export interface ToastMsg {
  id: number;
  title: string;
  tone?: 'default' | 'danger';
  duration?: number;
  action?: ToastAction;
  clipboard?: ClipboardToast;
}

export const useToasts = create<{
  list: ToastMsg[];
  push: (t: Omit<ToastMsg, 'id'>) => void;
  remove: (id: number) => void;
}>((set) => ({
  list: [],
  // A new toast replaces a clipboard countdown: the clipboard no longer holds that value.
  push: (t) => set((s) => ({
    list: [...s.list.filter((x) => !x.clipboard), { ...t, id: Date.now() + Math.random() }],
  })),
  remove: (id) => set((s) => ({ list: s.list.filter((x) => x.id !== id) })),
}));

// A locked vault is shown as the unlock screen, never as an error toast.
export const toast = (
  title: string,
  toneOrOpts?: 'default' | 'danger' | ToastOptions
) => {
  if (/vault (is )?locked/i.test(String(title))) return;
  const opts: ToastOptions =
    typeof toneOrOpts === 'string' ? { tone: toneOrOpts } : (toneOrOpts ?? {});
  useToasts.getState().push({ title, ...opts });
};

function tr(key: Key, vars?: Record<string, string | number>) {
  return translate(useI18n.getState().lang, key, vars);
}

function prefersReducedMotion() {
  return typeof window !== 'undefined' && !!window.matchMedia?.('(prefers-reduced-motion: reduce)').matches;
}

function clearsPhrase(base: string, n: number) {
  return `${base} · ${tr('clipboard.clearsIn', { n })}`;
}

/** Visual countdown is aria-hidden so the polite live region announces once, then only "Cleared". */
function ClipboardToastBody({ msg, toastId }: { msg: ClipboardToast; toastId: number }) {
  const t = useT();
  const total = msg.seconds;
  const [left, setLeft] = useState(total);
  const [done, setDone] = useState(false);
  const [busy, setBusy] = useState(false);
  const [reduce, setReduce] = useState(prefersReducedMotion);
  const initial = total > 0 ? clearsPhrase(msg.base, total) : `${msg.base} · ${tr('clipboard.wontClear')}`;
  const [announced, setAnnounced] = useState(initial);

  useEffect(() => {
    const mq = window.matchMedia?.('(prefers-reduced-motion: reduce)');
    if (!mq) return;
    const onChange = () => setReduce(mq.matches);
    mq.addEventListener?.('change', onChange);
    return () => mq.removeEventListener?.('change', onChange);
  }, []);

  useEffect(() => {
    if (total <= 0 || done) return;
    const id = window.setInterval(() => {
      setLeft((n) => (n <= 1 ? 0 : n - 1));
    }, 1000);
    return () => window.clearInterval(id);
  }, [total, done]);

  useEffect(() => {
    if (total > 0 && left === 0 && !done) {
      setDone(true);
      setAnnounced(tr('clipboard.cleared'));
    }
  }, [left, total, done]);

  useEffect(() => {
    if (!done) return;
    const id = window.setTimeout(() => useToasts.getState().remove(toastId), 1600);
    return () => window.clearTimeout(id);
  }, [done, toastId]);

  const visual = done
    ? t('clipboard.cleared')
    : total > 0
      ? `${msg.base} · ${t('clipboard.clearsIn', { n: left })}`
      : `${msg.base} · ${t('clipboard.wontClear')}`;

  async function clearNow() {
    if (busy || done) return;
    setBusy(true);
    try {
      const ok = await msg.onClearNow();
      if (ok) {
        setAnnounced(tr('clipboard.cleared'));
        setDone(true);
        setLeft(0);
      } else {
        useToasts.getState().remove(toastId);
      }
    } catch {
      setBusy(false);
    }
  }

  return (
    <>
      {/* Stable text for the one-shot toast announcer. Ticks stay on the aria-hidden visual. */}
      <T.Title data-testid="toast-copy-live" className="sr-only">{announced}</T.Title>
      {/* Separate polite region so "Cleared" is announced once; it stays empty until then. */}
      <span data-testid="toast-copy-cleared" className="sr-only" role="status" aria-live="polite">{done ? t('clipboard.cleared') : ''}</span>
      <span data-testid="toast-copy-visual" aria-hidden="true" className="min-w-0 text-sm">{visual}</span>
      {total > 0 && !done && (
        <button
          type="button"
          data-testid="toast-clear-now"
          disabled={busy}
          onClick={() => { void clearNow(); }}
          className="shrink-0 rounded px-2 py-0.5 text-xs font-semibold text-accent hover:underline focus:outline-none focus-visible:ring-2 focus-visible:ring-accent"
        >
          {t('clipboard.clearNow')}
        </button>
      )}
      {total <= 0 && !done && (
        <button
          type="button"
          data-testid="toast-clipboard-setting"
          onClick={() => {
            msg.onOpenSetting();
            useToasts.getState().remove(toastId);
          }}
          className="shrink-0 rounded px-2 py-0.5 text-xs font-semibold text-accent hover:underline focus:outline-none focus-visible:ring-2 focus-visible:ring-accent"
        >
          {t('clipboard.openSetting')}
        </button>
      )}
      {total > 0 && !done && (
        <span data-testid="toast-copy-bar" aria-hidden="true" className="pointer-events-none absolute inset-x-0 bottom-0 h-0.5">
          <span
            className="block h-full bg-accent motion-reduce:transition-none"
            style={{
              width: `${Math.max(0, Math.min(100, (left / total) * 100))}%`,
              transition: reduce ? 'none' : 'width 1s linear',
            }}
          />
        </span>
      )}
    </>
  );
}

export function Toaster() {
  const { list, remove } = useToasts();
  return (
    <T.Provider swipeDirection="right" duration={2500}>
      {list.map((t) => (
        <T.Root
          key={t.id}
          type={t.clipboard ? 'background' : 'foreground'}
          duration={t.clipboard
            ? (t.duration ?? (t.clipboard.seconds > 0 ? t.clipboard.seconds * 1000 + 4000 : 8000))
            : t.duration}
          onOpenChange={(o) => !o && remove(t.id)}
          className={`relative flex items-center justify-between gap-3 overflow-hidden rounded-lg border border-border px-4 py-2.5 shadow-xl ${
            t.clipboard && t.clipboard.seconds > 0 ? 'pb-3' : ''
          } ${t.tone === 'danger' ? 'bg-danger text-white' : 'bg-panel text-fg'}`}
        >
          {t.clipboard ? (
            <ClipboardToastBody msg={t.clipboard} toastId={t.id} />
          ) : (
            <>
              <T.Title className="text-sm">{t.title}</T.Title>
              {t.action && (
                <T.Action
                  altText={t.action.altText ?? t.action.label}
                  asChild
                  onClick={() => {
                    remove(t.id);
                    void t.action!.onClick();
                  }}
                >
                  <button
                    type="button"
                    className="shrink-0 rounded px-2 py-0.5 text-xs font-semibold uppercase tracking-wider text-accent hover:underline focus:outline-none"
                  >
                    {t.action.label}
                  </button>
                </T.Action>
              )}
            </>
          )}
        </T.Root>
      ))}
      <T.Viewport className="fixed bottom-4 right-4 z-[100] flex w-80 flex-col gap-2 outline-none" />
    </T.Provider>
  );
}
