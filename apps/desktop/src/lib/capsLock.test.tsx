import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { CapsLockHint, useCapsLockHint } from './capsLock';
import { Input, TooltipProvider } from '../ui';

function Field() {
  const caps = useCapsLockHint();
  return (
    <TooltipProvider>
      <div>
        <Input aria-label="Master password" type="password" {...caps.fieldProps} />
        <CapsLockHint id={caps.hintId} show={caps.show} />
      </div>
    </TooltipProvider>
  );
}

const key = (el: HTMLElement, capsLock: boolean, type: 'keydown' | 'keyup' = 'keydown') => {
  const e = new KeyboardEvent(type, { bubbles: true });
  Object.defineProperty(e, 'getModifierState', { value: (k: string) => k === 'CapsLock' && capsLock });
  fireEvent(el, e);
};

describe('CapsLockHint', () => {
  afterEach(cleanup);

  it('shows the hint on keydown with CapsLock on and links it via aria-describedby', () => {
    render(<Field />);
    const input = screen.getByLabelText('Master password');
    fireEvent.focus(input);
    key(input, true);
    expect(screen.getByText('Caps Lock is on')).toBeTruthy();
    expect(input.getAttribute('aria-describedby')).toBe(
      screen.getByTestId('caps-lock-hint').id
    );
  });

  it('hides the hint on keydown with CapsLock off and on blur', () => {
    render(<Field />);
    const input = screen.getByLabelText('Master password');
    fireEvent.focus(input);
    key(input, true);
    expect(screen.getByText('Caps Lock is on')).toBeTruthy();
    key(input, false);
    expect(screen.queryByText('Caps Lock is on')).toBeNull();
    key(input, true);
    expect(screen.getByText('Caps Lock is on')).toBeTruthy();
    fireEvent.blur(input);
    expect(screen.queryByText('Caps Lock is on')).toBeNull();
  });

  it('reserves the hint line even when hidden (no layout shift)', () => {
    render(<Field />);
    const input = screen.getByLabelText('Master password');
    fireEvent.focus(input);
    const hint = screen.getByTestId('caps-lock-hint');
    expect(hint.className).toContain('h-4');
    expect(hint.textContent).toBe('');
  });
});
