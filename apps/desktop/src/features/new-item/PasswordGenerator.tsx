import { useCallback, useEffect, useRef, useState } from 'react';
import { Copy, Plus, RefreshCw, Wand2 } from 'lucide-react';
import { api } from '../../api/client';
import { Button, Input, Modal, Tooltip, toast } from '../../ui';
import { useT, type Key } from '../../i18n';
import { useApp } from '../../lib/store';
import { useSettings } from '../settings/useSettings';
import { cn } from '../../lib/utils';

export type GeneratorMode = 'random' | 'memorable' | 'pin';

const MODE_LIMITS: Record<GeneratorMode, { min: number; max: number; def: number }> = {
  random: { min: 8, max: 64, def: 20 },
  memorable: { min: 3, max: 15, def: 5 },
  pin: { min: 4, max: 12, def: 6 },
};

function clamp(val: number, min: number, max: number): number {
  if (isNaN(val)) return min;
  return Math.max(min, Math.min(max, val));
}

export function ColoredPassword({ password }: { password: string }) {
  if (!password) return <span className="font-mono text-xl sm:text-2xl text-muted">…</span>;
  return (
    <span
      className="font-mono text-xl sm:text-2xl tracking-wider select-all break-all"
      data-testid="colored-password"
      aria-label="Generated password"
    >
      {Array.from(password).map((ch, idx) => {
        let cls = 'text-fg';
        if (/[0-9]/.test(ch)) {
          cls = 'text-accent font-semibold';
        } else if (/[^a-zA-Z0-9\s]/.test(ch)) {
          cls = 'text-warn font-semibold';
        }
        return (
          <span key={idx} className={cls}>
            {ch}
          </span>
        );
      })}
    </span>
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
  onSaveAsLogin?: (pw: string) => void;
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

  const [pw, setPw] = useState<string>('');
  const [err, setErr] = useState<string | null>(null);

  // Keep a ref to save timer to debounce settings persistence
  const saveTimeout = useRef<ReturnType<typeof setTimeout>>();

  const persistSettings = useCallback((m: GeneratorMode, l: number, d: boolean, s: boolean) => {
    clearTimeout(saveTimeout.current);
    saveTimeout.current = setTimeout(() => {
      void useSettings.getState().save({
        generatorMode: m,
        generatorLength: l,
        generatorDigits: d,
        generatorSymbols: s,
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
      })
      .then((p) => {
        setPw(p);
        setErr(null);
      })
      .catch((e) => setErr(String((e as Error)?.message ?? e)));
  }, [mode, length, symbols, digits]);

  useEffect(() => {
    if (isOpen) {
      void gen();
    }
  }, [isOpen, gen]);

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
    persistSettings(m, newLen, digits, symbols);
  };

  const updateLength = (val: number) => {
    const clamped = clamp(val, limits.min, limits.max);
    setLength(clamped);
    persistSettings(mode, clamped, digits, symbols);
  };

  const updateDigits = (val: boolean) => {
    setDigits(val);
    persistSettings(mode, length, val, symbols);
  };

  const updateSymbols = (val: boolean) => {
    setSymbols(val);
    persistSettings(mode, length, digits, val);
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

      {/* Action buttons */}
      {standalone ? (
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
      className="w-[480px]"
    >
      <PasswordGenerator
        standalone
        onClose={closeModal}
        onSaveAsLogin={(pw) => {
          openModal('new-item', { category: 'login', initial: { password: pw } });
        }}
      />
    </Modal>
  );
}
