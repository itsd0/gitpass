import { useCallback, useEffect, useRef, useState } from 'react';
import { Copy, Eye, EyeOff, Plus, RefreshCw, Trash2, Wand2 } from 'lucide-react';
import type { GeneratorHistoryEntry } from '../../api/types';
import { api } from '../../api/client';
import { Button, Input, Modal, Tooltip, toast } from '../../ui';
import { useT, type Key } from '../../i18n';
import { useApp } from '../../lib/store';
import { useSettings } from '../settings/useSettings';
import { cn } from '../../lib/utils';
import { SecretText } from '../../lib/secretText';

export type GeneratorMode = 'random' | 'memorable' | 'pin';

const MODE_LIMITS: Record<GeneratorMode, { min: number; max: number; def: number }> = {
  random: { min: 8, max: 64, def: 20 },
  memorable: { min: 3, max: 15, def: 5 },
  pin: { min: 4, max: 12, def: 6 },
};

/** Word-passphrase separators offered in Memorable mode; ' ' renders as ␣. */
const WORD_SEPARATORS = ['-', '.', '_', ' '] as const;

function clamp(val: number, min: number, max: number): number {
  if (isNaN(val)) return min;
  return Math.max(min, Math.min(max, val));
}

export function ColoredPassword({ password }: { password: string }) {
  if (!password) return <span className="font-mono text-xl sm:text-2xl text-muted">…</span>;
  return (
    <SecretText
      value={password}
      testId="colored-password"
      label={password}
      className="font-mono text-xl sm:text-2xl tracking-wider select-all break-all"
    />
  );
}

export function StrengthMeter({ password }: { password: string }) {
  const [score, setScore] = useState<number | null>(null);
  useEffect(() => {
    let live = true;
    if (!password) {
      setScore(null);
      return;
    }
    const t = setTimeout(() => {
      api.passwordStrength({ password })
        .then((s) => live && setScore(s))
        .catch(() => live && setScore(null));
    }, 150);
    return () => {
      live = false;
      clearTimeout(t);
    };
  }, [password]);

  if (score === null) return null;
  const label = score < 40 ? 'Weak' : score < 70 ? 'Fair' : score < 90 ? 'Good' : 'Excellent';
  const color = score < 40 ? 'bg-danger' : score < 70 ? 'bg-warn' : score < 90 ? 'bg-lime-500' : 'bg-emerald-500';

  return (
    <div className="mt-2 flex items-center gap-2" aria-label={`Password strength ${label}`} data-testid="strength-meter">
      <div className="h-1.5 flex-1 rounded bg-hover overflow-hidden">
        <div className={`h-1.5 rounded transition-all duration-300 ${color}`} style={{ width: `${score}%` }} />
      </div>
      <span className="w-16 text-right text-xs text-muted">{label}</span>
    </div>
  );
}

export interface PasswordGeneratorProps {
  onUse?: (pw: string) => void;
  standalone?: boolean;
  onClose?: () => void;
  onSaveAsLogin?: (pw: string, host?: string) => void;
  /** Controlled popover mode (used by the item editor): when defined, the
   * trigger button is not rendered and visibility follows `open`. The parent
   * anchors the panel under its field and restores focus on close. */
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
}

export function PasswordGenerator({ onUse, standalone, onClose, onSaveAsLogin, open, onOpenChange }: PasswordGeneratorProps) {
  const t = useT();
  const [popoverOpen, setPopoverOpen] = useState(false);
  const controlled = open !== undefined;
  const isOpen = standalone || (controlled ? open : popoverOpen);
  const setOpen = (o: boolean) => {
    if (controlled) onOpenChange?.(o);
    else setPopoverOpen(o);
  };
  const panelRef = useRef<HTMLDivElement>(null);

  // When the popover opens, move focus into it (first focusable control).
  useEffect(() => {
    if (!isOpen || standalone) return;
    const panel = panelRef.current;
    if (!panel) return;
    const first = Array.from(panel.querySelectorAll<HTMLElement>('button, input, select, textarea, [tabindex]:not([tabindex="-1"])')).find((el) => !el.hasAttribute('disabled'));
    first?.focus();
  }, [isOpen, standalone]);

  // Popover keyboard contract: Esc closes, Enter applies (except on buttons,
  // where the native click already applies/closes), Tab cycles inside.
  const onPanelKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      setOpen(false);
      return;
    }
    if (e.key === 'Enter') {
      if ((e.target as HTMLElement).tagName === 'BUTTON') return;
      e.preventDefault();
      if (onUse && pw) onUse(pw);
      setOpen(false);
      return;
    }
    if (e.key === 'Tab') {
      const panel = panelRef.current;
      if (!panel) return;
      const focusables = Array.from(panel.querySelectorAll<HTMLElement>('button, input, select, textarea, [tabindex]:not([tabindex="-1"])')).filter((el) => !el.hasAttribute('disabled'));
      if (focusables.length === 0) return;
      const first = focusables[0];
      const last = focusables[focusables.length - 1];
      if (e.shiftKey && document.activeElement === first) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && document.activeElement === last) {
        e.preventDefault();
        first.focus();
      }
    }
  };

  // Settings from app store
  const savedSettings = useSettings((s) => s.settings);

  // Initialize from persisted settings
  const initialMode = (savedSettings?.generatorMode as GeneratorMode) || 'random';
  const [mode, setMode] = useState<GeneratorMode>(initialMode);
  const limits = MODE_LIMITS[mode];
  const initialLength = savedSettings?.generatorLength != null ? clamp(savedSettings.generatorLength, limits.min, limits.max) : limits.def;

  const [length, setLength] = useState<number>(initialLength);
  const [symbols, setSymbols] = useState<boolean>(savedSettings?.generatorSymbols ?? true);
  const [digits, setDigits] = useState<boolean>(savedSettings?.generatorDigits ?? true);
  // Words-mode options. `separator === null` means "never chosen": the
  // historical default applies ('-' with symbols, '.' without), so existing
  // users see no change until they pick a separator.
  const [separator, setSeparator] = useState<string | null>(savedSettings?.generatorSeparator ?? null);
  const [capitalize, setCapitalize] = useState<boolean>(savedSettings?.generatorCapitalize ?? false);
  const effectiveSeparator = separator ?? (symbols ? '-' : '.');

  const [pw, setPw] = useState<string>('');
  const [err, setErr] = useState<string | null>(null);
  const [history, setHistory] = useState<GeneratorHistoryEntry[]>([]);
  const [revealedIds, setRevealedIds] = useState<Set<string>>(new Set());
  const [confirmClearOpen, setConfirmClearOpen] = useState(false);

  const loadHistory = useCallback(async () => {
    if (!standalone) return;
    try {
      const list = await api.getGeneratorHistory();
      setHistory(list);
    } catch {
      // Ignored if locked or error
    }
  }, [standalone]);

  const toggleReveal = (id: string) => {
    setRevealedIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const copyEntry = async (value: string) => {
    try {
      await api.copyPassword({ password: value });
      toast(t('generator.copied'));
    } catch (e) {
      toast(String((e as Error)?.message ?? e), 'danger');
    }
  };

  const handleSaveEntryAsLogin = (entry: GeneratorHistoryEntry) => {
    if (onSaveAsLogin) {
      onSaveAsLogin(entry.value, entry.host ?? undefined);
    } else {
      if (onClose) onClose();
      useApp.getState().openModal('new-item', {
        category: 'login',
        initial: {
          password: entry.value,
          url: entry.host ? (entry.host.startsWith('http') ? entry.host : `https://${entry.host}`) : undefined,
          title: entry.host ?? undefined,
        },
      });
    }
  };

  const deleteEntry = async (id: string) => {
    try {
      await api.deleteGeneratorHistoryEntry({ id });
      setHistory((prev) => prev.filter((e) => e.id !== id));
      toast(t('generator.entryDeleted'));
    } catch (e) {
      toast(String((e as Error)?.message ?? e), 'danger');
    }
  };

  const confirmClearHistory = async () => {
    try {
      await api.clearGeneratorHistory();
      setHistory([]);
      setConfirmClearOpen(false);
      toast(t('generator.historyCleared'));
    } catch (e) {
      toast(String((e as Error)?.message ?? e), 'danger');
    }
  };

  // Keep a ref to save timer to debounce settings persistence
  const saveTimeout = useRef<ReturnType<typeof setTimeout>>();

  const persistSettings = useCallback((m: GeneratorMode, l: number, d: boolean, s: boolean, sep: string | null, cap: boolean) => {
    clearTimeout(saveTimeout.current);
    saveTimeout.current = setTimeout(() => {
      void useSettings.getState().save({
        generatorMode: m,
        generatorLength: l,
        generatorDigits: d,
        generatorSymbols: s,
        generatorSeparator: sep ?? undefined,
        generatorCapitalize: cap,
      }).catch(() => {});
    }, 250);
  }, []);

  const gen = useCallback(() => {
    const isWords = mode === 'memorable';
    const isPin = mode === 'pin';
    return api
      .generatePassword({
        length,
        symbols: isPin ? false : symbols,
        digits: isPin ? true : digits,
        words: isWords,
        pin: isPin,
        ...(isWords ? { separator: effectiveSeparator, capitalize } : {}),
      })
      .then((p) => {
        setPw(p);
        setErr(null);
        if (standalone) {
          void loadHistory();
        }
      })
      .catch((e) => setErr(String((e as Error)?.message ?? e)));
  }, [mode, length, symbols, digits, effectiveSeparator, capitalize]);

  useEffect(() => {
    if (isOpen) {
      void gen();
      if (standalone) {
        void loadHistory();
      }
    }
  }, [isOpen, standalone, gen, loadHistory]);

  // Keyboard shortcut inside dialog: ⌘R to regenerate
  useEffect(() => {
    if (!isOpen) return;
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'r') {
        e.preventDefault();
        void gen();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [isOpen, gen]);

  const setModeAndGen = (m: GeneratorMode) => {
    setMode(m);
    const lim = MODE_LIMITS[m];
    const newLen = clamp(length, lim.min, lim.max);
    setLength(newLen);
    persistSettings(m, newLen, digits, symbols, separator, capitalize);
  };

  const updateLength = (val: number) => {
    const clamped = clamp(val, limits.min, limits.max);
    setLength(clamped);
    persistSettings(mode, clamped, digits, symbols, separator, capitalize);
  };

  const updateDigits = (val: boolean) => {
    setDigits(val);
    persistSettings(mode, length, val, symbols, separator, capitalize);
  };

  const updateSymbols = (val: boolean) => {
    setSymbols(val);
    persistSettings(mode, length, digits, val, separator, capitalize);
  };

  const updateSeparator = (val: string) => {
    setSeparator(val);
    persistSettings(mode, length, digits, symbols, val, capitalize);
  };

  const updateCapitalize = (val: boolean) => {
    setCapitalize(val);
    persistSettings(mode, length, digits, symbols, separator, val);
  };

  // Arrow-key navigation inside the separator segmented control: moving
  // focus also selects, so the preview regenerates immediately.
  const onSeparatorKeyDown = (e: React.KeyboardEvent) => {
    if (e.key !== 'ArrowRight' && e.key !== 'ArrowLeft' && e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
    e.preventDefault();
    const dir = e.key === 'ArrowRight' || e.key === 'ArrowDown' ? 1 : -1;
    const idx = WORD_SEPARATORS.indexOf(effectiveSeparator as (typeof WORD_SEPARATORS)[number]);
    const next = WORD_SEPARATORS[(idx + dir + WORD_SEPARATORS.length) % WORD_SEPARATORS.length];
    updateSeparator(next);
    const group = e.currentTarget;
    (group.querySelector(`[data-sep="${next}"]`) as HTMLElement | null)?.focus();
  };

  const handleCopy = async () => {
    if (!pw) return;
    try {
      await api.copyPassword({ password: pw });
      toast(t('generator.copied'));
    } catch (e) {
      toast(String((e as Error)?.message ?? e), 'danger');
    }
  };

  const handleSaveAsLogin = () => {
    if (!pw) return;
    if (onSaveAsLogin) {
      onSaveAsLogin(pw);
    } else {
      if (onClose) onClose();
      useApp.getState().openModal('new-item', { category: 'login', initial: { password: pw } });
    }
  };

  const content = (
    <div className="space-y-4">
      <div
        className="flex items-center justify-between gap-3 rounded-lg border border-border bg-bg p-4 shadow-inner"
        data-testid="generated-password-box"
      >
        <ColoredPassword password={pw} />
        <Tooltip content={`${t('generator.regenerate')} (⌘R)`}>
          <Button
            type="button"
            size="icon"
            variant="ghost"
            aria-label={t('generator.regenerate')}
            onClick={() => void gen()}
            className="shrink-0"
          >
            <RefreshCw size={16} />
          </Button>
        </Tooltip>
      </div>

      <StrengthMeter password={pw} />
      {err && <p className="text-xs text-danger">{err}</p>}

      {/* Mode selection tabs */}
      <div
        role="tablist"
        aria-label={t('generator.mode')}
        className="flex rounded-lg border border-border bg-bg p-1"
      >
        {(['random', 'memorable', 'pin'] as const).map((m) => (
          <button
            key={m}
            role="tab"
            type="button"
            aria-selected={mode === m}
            onClick={() => setModeAndGen(m)}
            className={cn(
              'flex-1 rounded-md py-1.5 text-xs font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent',
              mode === m ? 'bg-panel text-fg shadow-sm font-semibold' : 'text-muted hover:text-fg'
            )}
          >
            {t(`generator.mode.${m}` as Key)}
          </button>
        ))}
      </div>

      {/* Length slider and Number input */}
      <div className="rounded-lg border border-border bg-panel p-3 space-y-2">
        <div className="flex items-center justify-between text-xs font-medium">
          <label htmlFor="gen-length-slider" className="text-muted">
            {mode === 'memorable' ? t('generator.words') : mode === 'pin' ? t('generator.digits') : t('generator.length')}
          </label>
          <Input
            id="gen-length-number"
            type="number"
            min={limits.min}
            max={limits.max}
            value={length}
            aria-label={`${mode === 'memorable' ? t('generator.words') : mode === 'pin' ? t('generator.digits') : t('generator.length')} input`}
            onChange={(e) => updateLength(Number(e.target.value))}
            className="h-7 w-16 text-right text-xs"
          />
        </div>
        <input
          id="gen-length-slider"
          type="range"
          min={limits.min}
          max={limits.max}
          value={length}
          aria-label={mode === 'memorable' ? t('generator.words') : mode === 'pin' ? t('generator.digits') : t('generator.length')}
          onChange={(e) => updateLength(Number(e.target.value))}
          className="w-full accent-[#f97316] cursor-pointer"
        />
      </div>

      {/* Toggles */}
      {mode !== 'pin' && (
        <div className="grid grid-cols-2 gap-3 text-xs">
          <label className="flex items-center gap-2 cursor-pointer text-fg">
            <input
              type="checkbox"
              checked={digits}
              onChange={(e) => updateDigits(e.target.checked)}
              className="h-4 w-4 rounded border-border accent-[#f97316]"
            />
            <span>{mode === 'memorable' ? t('generator.includeDigits') : t('generator.digits')}</span>
          </label>
          <label className="flex items-center gap-2 cursor-pointer text-fg">
            <input
              type="checkbox"
              checked={symbols}
              onChange={(e) => updateSymbols(e.target.checked)}
              className="h-4 w-4 rounded border-border accent-[#f97316]"
            />
            <span>{mode === 'memorable' ? t('generator.includeSymbols') : t('generator.symbols')}</span>
          </label>
        </div>
      )}

      {/* Words-mode options: separator + capitalize, hidden in Random/PIN modes */}
      {mode === 'memorable' && (
        <div className="rounded-lg border border-border bg-panel p-3 space-y-3">
          <div>
            <span id="gen-separator-label" className="text-xs font-medium text-muted">
              {t('generator.separator')}
            </span>
            <div
              role="radiogroup"
              aria-labelledby="gen-separator-label"
              onKeyDown={onSeparatorKeyDown}
              className="mt-1.5 flex rounded-lg border border-border bg-bg p-1"
            >
              {WORD_SEPARATORS.map((sep) => (
                <button
                  key={sep === ' ' ? 'space' : sep}
                  data-sep={sep}
                  type="button"
                  role="radio"
                  aria-checked={effectiveSeparator === sep}
                  aria-label={sep === ' ' ? t('generator.separatorSpace') : sep}
                  onClick={() => updateSeparator(sep)}
                  className={cn(
                    'flex-1 rounded-md py-1.5 text-xs font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent',
                    effectiveSeparator === sep ? 'bg-[#f97316] text-white shadow-sm font-semibold' : 'text-muted hover:text-fg'
                  )}
                >
                  {sep === ' ' ? '␣' : sep}
                </button>
              ))}
            </div>
          </div>
          <label className="flex items-center gap-2 cursor-pointer text-xs text-fg">
            <input
              type="checkbox"
              checked={capitalize}
              onChange={(e) => updateCapitalize(e.target.checked)}
              className="h-4 w-4 rounded border-border accent-[#f97316]"
            />
            <span>{t('generator.capitalize')}</span>
          </label>
        </div>
      )}

      {/* Action buttons */}
      {standalone ? (
        <>
          <div className="mt-6 flex flex-wrap items-center justify-between gap-2 border-t border-border pt-4">
            <Button
              type="button"
              variant="outline"
              onClick={handleSaveAsLogin}
              disabled={!pw}
              className="gap-1.5"
            >
              <Plus size={14} />
              {t('generator.saveAsLogin')}
            </Button>
            <div className="flex items-center gap-2">
              {onClose && (
                <Button type="button" variant="ghost" onClick={onClose}>
                  {t('common.cancel')}
                </Button>
              )}
              <Button
                type="button"
                onClick={handleCopy}
                disabled={!pw}
                className="bg-[#f97316] hover:bg-[#ea580c] text-white gap-1.5 shadow-sm focus-visible:ring-[#f97316]/50"
              >
                <Copy size={14} />
                {t('common.copy')}
              </Button>
            </div>
          </div>

          {/* History section */}
          <div className="mt-6 border-t border-border pt-4 space-y-3" data-testid="generator-history">
            <div className="flex items-center justify-between">
              <h3 className="text-xs font-semibold uppercase tracking-wider text-muted">
                {t('generator.historyTitle')}
              </h3>
              {history.length > 0 && (
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  data-testid="clear-history-btn"
                  onClick={() => setConfirmClearOpen(true)}
                  className="h-7 text-xs text-muted hover:text-danger gap-1"
                >
                  <Trash2 size={12} />
                  {t('generator.clearHistory')}
                </Button>
              )}
            </div>

            {history.length === 0 ? (
              <div
                data-testid="generator-history-empty"
                className="rounded-lg border border-dashed border-border p-6 text-center text-xs text-muted"
              >
                {t('generator.historyEmpty')}
              </div>
            ) : (
              <div className="space-y-2 max-h-60 overflow-y-auto pr-1" data-testid="generator-history-list">
                {history.map((entry) => {
                  const isRevealed = revealedIds.has(entry.id);
                  return (
                    <div
                      key={entry.id}
                      data-testid="generator-history-item"
                      className="group rounded-lg border border-border bg-panel p-2.5 transition-colors hover:border-accent/40 space-y-1.5"
                    >
                      <div className="flex items-center justify-between text-[11px] text-muted">
                        <div className="flex items-center gap-2">
                          <span className="rounded bg-hover px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-wider text-muted">
                            {entry.source}
                          </span>
                          {entry.host && (
                            <span data-testid="history-entry-host" className="font-medium text-accent">
                              {entry.host}
                            </span>
                          )}
                        </div>
                        <div className="flex items-center gap-1">
                          <Tooltip content={isRevealed ? t('common.hide') : t('common.show')}>
                            <Button
                              type="button"
                              size="iconSm"
                              variant="ghost"
                              data-testid="history-reveal-btn"
                              aria-label={isRevealed ? t('common.hide') : t('common.show')}
                              onClick={() => toggleReveal(entry.id)}
                              className="h-6 w-6 text-muted hover:text-fg"
                            >
                              {isRevealed ? <EyeOff size={12} /> : <Eye size={12} />}
                            </Button>
                          </Tooltip>
                          <Tooltip content={t('common.copy')}>
                            <Button
                              type="button"
                              size="iconSm"
                              variant="ghost"
                              data-testid="history-copy-btn"
                              aria-label={t('common.copy')}
                              onClick={() => void copyEntry(entry.value)}
                              className="h-6 w-6 text-muted hover:text-fg hover:text-accent"
                            >
                              <Copy size={12} />
                            </Button>
                          </Tooltip>
                          <Tooltip content={t('generator.saveAsLogin')}>
                            <Button
                              type="button"
                              size="iconSm"
                              variant="ghost"
                              data-testid="history-save-login-btn"
                              aria-label={t('generator.saveAsLogin')}
                              onClick={() => handleSaveEntryAsLogin(entry)}
                              className="h-6 w-6 text-muted hover:text-fg hover:text-accent"
                            >
                              <Plus size={12} />
                            </Button>
                          </Tooltip>
                          <Tooltip content={t('generator.deleteEntry')}>
                            <Button
                              type="button"
                              size="iconSm"
                              variant="ghost"
                              data-testid="history-delete-btn"
                              aria-label={t('generator.deleteEntry')}
                              onClick={() => void deleteEntry(entry.id)}
                              className="h-6 w-6 text-muted hover:text-danger"
                            >
                              <Trash2 size={12} />
                            </Button>
                          </Tooltip>
                        </div>
                      </div>
                      <div className="flex items-center">
                        <span
                          data-testid="history-entry-value"
                          className={cn(
                            "font-mono text-xs select-all break-all tracking-wider",
                            isRevealed ? "text-fg font-medium" : "text-muted"
                          )}
                        >
                          {isRevealed ? entry.value : "••••••••••••••••••••"}
                        </span>
                      </div>
                    </div>
                  );
                })}
              </div>
            )}

            {confirmClearOpen && (
              <Modal
                open
                onOpenChange={(o) => !o && setConfirmClearOpen(false)}
                title={t('generator.clearHistoryConfirmTitle')}
                className="w-96"
              >
                <div className="space-y-4">
                  <p className="text-xs text-muted">
                    {t('generator.clearHistoryConfirmBody')}
                  </p>
                  <div className="flex justify-end gap-2">
                    <Button
                      type="button"
                      size="sm"
                      variant="ghost"
                      onClick={() => setConfirmClearOpen(false)}
                    >
                      {t('common.cancel')}
                    </Button>
                    <Button
                      type="button"
                      size="sm"
                      variant="danger"
                      data-testid="confirm-clear-history-btn"
                      onClick={() => void confirmClearHistory()}
                    >
                      {t('generator.clearHistory')}
                    </Button>
                  </div>
                </div>
              </Modal>
            )}
          </div>
        </>
      ) : (
        <div className="mt-4 flex justify-end gap-2">
          <Button type="button" size="sm" variant="ghost" onClick={() => setOpen(false)}>
            {t('common.cancel')}
          </Button>
          <Button
            type="button"
            size="sm"
            variant="default"
            disabled={!pw}
            onClick={() => {
              if (onUse && pw) onUse(pw);
              setOpen(false);
            }}
          >
            {t('generator.usePassword')}
          </Button>
        </div>
      )}
    </div>
  );

  if (standalone) {
    return content;
  }

  // Controlled mode: the parent owns the trigger and anchors this panel.
  if (controlled) {
    if (!open) return null;
    return (
      <div
        ref={panelRef}
        role="dialog"
        aria-label={t('generator.title')}
        data-generator-popover
        onKeyDown={onPanelKeyDown}
        className="absolute left-0 z-20 mt-1 w-80 rounded-lg border border-border bg-panel p-4 shadow-xl"
      >
        {content}
      </div>
    );
  }

  return (
    <div className="relative">
      <Button
        type="button"
        size="sm"
        variant="ghost"
        aria-label={t('generator.generatePassword')}
        onClick={() => setOpen(!popoverOpen)}
      >
        <Wand2 size={14} /> Generate
      </Button>
      {popoverOpen && (
        <div
          ref={panelRef}
          role="dialog"
          aria-label={t('generator.title')}
          data-generator-popover
          onKeyDown={onPanelKeyDown}
          className="absolute right-0 z-20 mt-1 w-80 rounded-lg border border-border bg-panel p-4 shadow-xl"
        >
          {content}
        </div>
      )}
    </div>
  );
}

export function PasswordGeneratorModalHost() {
  const { modal, closeModal, openModal } = useApp();
  const t = useT();
  if (modal?.id !== 'password-generator') return null;

  return (
    <Modal
      open
      onOpenChange={(o) => !o && closeModal()}
      title={t('generator.title')}
      className="w-[500px] max-w-full max-h-[85vh] overflow-y-auto"
    >
      <PasswordGenerator
        standalone
        onClose={closeModal}
        onSaveAsLogin={(pw, host) => {
          openModal('new-item', {
            category: 'login',
            initial: {
              password: pw,
              url: host ? (host.startsWith('http') ? host : `https://${host}`) : undefined,
              title: host ?? undefined,
            },
          });
        }}
      />
    </Modal>
  );
}
