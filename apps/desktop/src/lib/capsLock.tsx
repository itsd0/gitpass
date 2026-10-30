import { useId, useState } from 'react';
import { ArrowBigUpDash } from 'lucide-react';
import { useT } from '../i18n';

type ModifierEvent = { getModifierState?: (key: string) => boolean };

const capsLockOn = (e: ModifierEvent) => !!e.getModifierState?.('CapsLock');

/**
 * Tracks Caps Lock for one focused password field. Spread `fieldProps` onto the
 * input and render <CapsLockHint> next to it. Detection uses
 * KeyboardEvent.getModifierState('CapsLock') on keydown/keyup and on focus;
 * the hint hides on blur.
 */
export function useCapsLockHint() {
  const id = useId();
  const [capsOn, setCapsOn] = useState(false);
  const [focused, setFocused] = useState(false);
  const show = capsOn && focused;
  const fieldProps = {
    onKeyDown: (e: React.KeyboardEvent<HTMLInputElement>) => setCapsOn(capsLockOn(e as unknown as ModifierEvent)),
    onKeyUp: (e: React.KeyboardEvent<HTMLInputElement>) => setCapsOn(capsLockOn(e as unknown as ModifierEvent)),
    onFocus: (e: React.FocusEvent<HTMLInputElement>) => {
      setFocused(true);
      setCapsOn(capsLockOn(e as unknown as ModifierEvent));
    },
    onBlur: () => {
      setFocused(false);
      setCapsOn(false);
    },
    'aria-describedby': id,
  } as const;
  return { hintId: id, show, fieldProps };
}

/** Reserved-height hint line (no layout shift); the live region announces when the text appears. */
export function CapsLockHint({ id, show }: { id: string; show: boolean }) {
  const t = useT();
  return (
    <p id={id} aria-live="polite" className="mt-1 h-4 text-xs" data-testid="caps-lock-hint">
      {show && (
        <span className="flex items-center gap-1 text-muted">
          <ArrowBigUpDash size={12} aria-hidden />
          {t('common.capsLock')}
        </span>
      )}
    </p>
  );
}
