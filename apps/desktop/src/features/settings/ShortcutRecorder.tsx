import React, { useEffect, useRef, useState } from 'react';
import { RotateCcw, Ban, Keyboard } from 'lucide-react';
import { Button, Kbd } from '../../ui';
import { useT } from '../../i18n';
import { cn } from '../../lib/utils';
import { isMac } from '../palette/shortcuts';
import { isTauri } from '../../lib/agentEvents';

export const DEFAULT_HOTKEY = 'CommandOrControl+Shift+Space';

/** Parse and format a Tauri global shortcut string for display. */
export function formatShortcut(combo: string): string[] {
  if (!combo || combo === 'none' || combo === 'disabled') return [];
  const mac = isMac();
  return combo.split('+').map((p) => {
    const u = p.trim().toUpperCase();
    if (u === 'COMMANDORCONTROL' || u === 'CMDORCTRL' || u === 'COMMAND' || u === 'CMD' || u === 'SUPER') {
      return mac ? '⌘' : 'Ctrl';
    }
    if (u === 'SHIFT') return mac ? '⇧' : 'Shift';
    if (u === 'ALT' || u === 'OPTION') return mac ? '⌥' : 'Alt';
    if (u === 'CONTROL' || u === 'CTRL') return mac ? '⌃' : 'Ctrl';
    if (u === 'SPACE') return 'Space';
    if (u === 'ENTER' || u === 'RETURN') return '↵';
    if (u === 'ESCAPE' || u === 'ESC') return 'Esc';
    if (u === 'ARROWUP' || u === 'UP') return '↑';
    if (u === 'ARROWDOWN' || u === 'DOWN') return '↓';
    if (u === 'ARROWLEFT' || u === 'LEFT') return '←';
    if (u === 'ARROWRIGHT' || u === 'RIGHT') return '→';
    if (u.startsWith('KEY') && u.length === 4) return u.slice(3);
    if (u.startsWith('DIGIT') && u.length === 6) return u.slice(5);
    return p;
  });
}

/** Convert a keydown event into a Tauri global shortcut combo string. */
export function eventToTauriShortcut(e: KeyboardEvent | React.KeyboardEvent): string | null {
  const mods: string[] = [];

  // Modifiers: CommandOrControl (Cmd/Super or Ctrl), Control (if Cmd also pressed), Alt, Shift
  if (e.metaKey) {
    mods.push('CommandOrControl');
    if (e.ctrlKey) mods.push('Control');
  } else if (e.ctrlKey) {
    mods.push('CommandOrControl');
  }
  if (e.altKey) {
    mods.push('Alt');
  }
  if (e.shiftKey) {
    mods.push('Shift');
  }

  // Key token
  const code = e.code;
  let key: string | null = null;
  if (code === 'Space' || e.key === ' ') {
    key = 'Space';
  } else if (code.startsWith('Key') && code.length === 4) {
    key = code.slice(3).toUpperCase();
  } else if (code.startsWith('Digit') && code.length === 6) {
    key = code.slice(5);
  } else if (/^F\d{1,2}$/i.test(e.key)) {
    key = e.key.toUpperCase();
  } else if (['ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight'].includes(e.key)) {
    key = e.key.replace('Arrow', '');
  } else if (['Enter', 'Tab', 'Backspace', 'Delete', 'Escape', 'Home', 'End', 'PageUp', 'PageDown'].includes(e.key)) {
    key = e.key;
  } else if (e.key.length === 1 && !['Meta', 'Control', 'Alt', 'Shift'].includes(e.key)) {
    key = e.key.toUpperCase();
  }

  // Require at least one modifier and one non-modifier key
  if (mods.length === 0 || !key) {
    return null;
  }

  return [...mods, key].join('+');
}

export interface ShortcutRecorderProps {
  id?: string;
  value: string;
  defaultValue?: string;
  onChange: (hotkey: string) => Promise<void>;
  registerPlugin?: (combo: string) => Promise<void>;
  unregisterPlugin?: (combo: string) => Promise<void>;
}

export function ShortcutRecorder({
  id = 'setting-quick-access-hotkey',
  value,
  defaultValue = DEFAULT_HOTKEY,
  onChange,
  registerPlugin,
  unregisterPlugin,
}: ShortcutRecorderProps) {
  const t = useT();
  const [recording, setRecording] = useState(false);
  const [pendingCombo, setPendingCombo] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const buttonRef = useRef<HTMLButtonElement>(null);

  const isDisabled = !value || value === 'none' || value === 'disabled';
  const isDefault = value === defaultValue;

  // Cancel recording if focus leaves button
  useEffect(() => {
    if (!recording) return;
    const onDocClick = (e: MouseEvent) => {
      if (buttonRef.current && !buttonRef.current.contains(e.target as Node)) {
        setRecording(false);
        setPendingCombo(null);
      }
    };
    document.addEventListener('mousedown', onDocClick);
    return () => document.removeEventListener('mousedown', onDocClick);
  }, [recording]);

  async function applyShortcut(newCombo: string) {
    if (newCombo === value) {
      setRecording(false);
      setPendingCombo(null);
      setErr(null);
      return;
    }
    setBusy(true);
    setErr(null);
    const oldCombo = value;

    try {
      if (isTauri() || registerPlugin || unregisterPlugin) {
        // Unregister previous hotkey if active
        if (oldCombo && oldCombo !== 'none' && oldCombo !== 'disabled') {
          if (unregisterPlugin) {
            await unregisterPlugin(oldCombo);
          } else if (isTauri()) {
            const { unregister } = await import('@tauri-apps/plugin-global-shortcut');
            await unregister(oldCombo).catch(() => {});
          }
        }

        // Register new hotkey if not disabled
        if (newCombo && newCombo !== 'none' && newCombo !== 'disabled') {
          if (registerPlugin) {
            await registerPlugin(newCombo);
          } else if (isTauri()) {
            const { register } = await import('@tauri-apps/plugin-global-shortcut');
            await register(newCombo, () => {});
          }
        }
      }

      await onChange(newCombo);
      setRecording(false);
      setPendingCombo(null);
      setErr(null);
    } catch (e) {
      // Registration failed! Show inline error and restore old hotkey
      setErr(t('settings.shortcutConflict'));
      setRecording(false);
      setPendingCombo(null);

      // Attempt to re-register old hotkey
      if (oldCombo && oldCombo !== 'none' && oldCombo !== 'disabled') {
        try {
          if (registerPlugin) {
            await registerPlugin(oldCombo);
          } else if (isTauri()) {
            const { register } = await import('@tauri-apps/plugin-global-shortcut');
            await register(oldCombo, () => {});
          }
        } catch {
          // ignore rollback errors
        }
      }
    } finally {
      setBusy(false);
    }
  }

  function onKeyDown(e: React.KeyboardEvent<HTMLButtonElement>) {
    if (!recording) {
      if (e.key === 'Enter' || e.key === ' ') {
        e.preventDefault();
        setRecording(true);
        setPendingCombo(null);
        setErr(null);
      }
      return;
    }

    e.preventDefault();
    e.stopPropagation();

    if (e.key === 'Escape') {
      setRecording(false);
      setPendingCombo(null);
      setErr(null);
      return;
    }

    if (e.key === 'Enter') {
      if (pendingCombo) {
        void applyShortcut(pendingCombo);
      }
      return;
    }

    const detected = eventToTauriShortcut(e);
    if (detected) {
      setPendingCombo(detected);
    }
  }

  const displayCombo = recording
    ? pendingCombo
      ? formatShortcut(pendingCombo)
      : null
    : isDisabled
    ? null
    : formatShortcut(value);

  return (
    <div className="flex flex-col items-end gap-1.5" data-testid="shortcut-recorder-container">
      <div className="flex items-center gap-2">
        <button
          ref={buttonRef}
          id={id}
          type="button"
          role="button"
          disabled={busy}
          data-testid="shortcut-recorder-button"
          aria-label={
            recording
              ? t('settings.shortcutRecording')
              : `${t('settings.quickAccessHotkey')}: ${
                  isDisabled ? t('settings.shortcutDisabled') : formatShortcut(value).join('')
                }`
          }
          onClick={() => {
            if (!recording) {
              setRecording(true);
              setPendingCombo(null);
              setErr(null);
            }
          }}
          onKeyDown={onKeyDown}
          className={cn(
            'flex min-h-[32px] min-w-[140px] items-center justify-center gap-1 rounded-md border px-3 py-1 text-xs font-medium transition-all focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent',
            recording
              ? 'border-accent bg-accent/15 text-accent ring-2 ring-accent animate-pulse'
              : 'border-border bg-panel text-fg hover:border-accent/60 hover:bg-hover'
          )}
        >
          {recording ? (
            pendingCombo ? (
              <span className="flex items-center gap-1 font-medium">
                {displayCombo?.map((k, idx) => (
                  <Kbd key={idx}>{k}</Kbd>
                ))}
                <span className="ml-1 text-[11px] text-muted font-normal">
                  ({t('settings.shortcutPressEnter')})
                </span>
              </span>
            ) : (
              <span className="flex items-center gap-1.5 text-accent">
                <Keyboard className="h-3.5 w-3.5" aria-hidden />
                <span>{t('settings.shortcutRecording')}</span>
              </span>
            )
          ) : isDisabled ? (
            <span className="text-muted italic">{t('settings.shortcutDisabled')}</span>
          ) : (
            <span className="flex items-center gap-1">
              {displayCombo?.map((k, idx) => (
                <Kbd key={idx}>{k}</Kbd>
              ))}
            </span>
          )}
        </button>

        <Button
          type="button"
          variant="outline"
          size="sm"
          disabled={busy || isDefault}
          onClick={() => void applyShortcut(defaultValue)}
          data-testid="shortcut-reset-button"
          aria-label={t('settings.shortcutReset')}
          title={t('settings.shortcutReset')}
          className="h-8 px-2.5 text-xs"
        >
          <RotateCcw className="h-3.5 w-3.5 mr-1" aria-hidden />
          <span>{t('settings.shortcutReset')}</span>
        </Button>

        <Button
          type="button"
          variant="outline"
          size="sm"
          disabled={busy || isDisabled}
          onClick={() => void applyShortcut('')}
          data-testid="shortcut-disable-button"
          aria-label={t('settings.shortcutDisabled')}
          title={t('settings.shortcutDisabled')}
          className="h-8 px-2.5 text-xs"
        >
          <Ban className="h-3.5 w-3.5 mr-1 text-muted" aria-hidden />
          <span>{t('settings.shortcutDisabled')}</span>
        </Button>
      </div>

      {err && (
        <p role="alert" className="text-xs text-danger font-medium" data-testid="shortcut-error">
          {err}
        </p>
      )}
    </div>
  );
}
