import { describe, expect, it, vi, afterEach } from 'vitest';
import { cleanup, render, screen, fireEvent, act } from '@testing-library/react';
import {
  ShortcutRecorder,
  DEFAULT_HOTKEY,
  formatShortcut,
  eventToTauriShortcut,
} from './ShortcutRecorder';
import { TooltipProvider } from '../../ui';

afterEach(() => {
  cleanup();
});

describe('formatShortcut and eventToTauriShortcut', () => {
  it('formats hotkeys correctly', () => {
    const formatted = formatShortcut('CommandOrControl+Shift+Space');
    expect(formatted).toContain('Space');
    expect(formatted.length).toBe(3);

    expect(formatShortcut('')).toEqual([]);
    expect(formatShortcut('none')).toEqual([]);
    expect(formatShortcut('disabled')).toEqual([]);
  });

  it('converts keyboard events with modifiers to Tauri shortcut strings', () => {
    // Cmd + Shift + Space
    const e1 = {
      metaKey: true,
      ctrlKey: false,
      altKey: false,
      shiftKey: true,
      code: 'Space',
      key: ' ',
    } as unknown as KeyboardEvent;
    expect(eventToTauriShortcut(e1)).toBe('CommandOrControl+Shift+Space');

    // Cmd + Alt + K
    const e2 = {
      metaKey: true,
      ctrlKey: false,
      altKey: true,
      shiftKey: false,
      code: 'KeyK',
      key: 'k',
    } as unknown as KeyboardEvent;
    expect(eventToTauriShortcut(e2)).toBe('CommandOrControl+Alt+K');

    // Key without modifiers returns null
    const e3 = {
      metaKey: false,
      ctrlKey: false,
      altKey: false,
      shiftKey: false,
      code: 'KeyA',
      key: 'a',
    } as unknown as KeyboardEvent;
    expect(eventToTauriShortcut(e3)).toBeNull();

    // Modifier only returns null
    const e4 = {
      metaKey: true,
      ctrlKey: false,
      altKey: false,
      shiftKey: false,
      code: 'MetaLeft',
      key: 'Meta',
    } as unknown as KeyboardEvent;
    expect(eventToTauriShortcut(e4)).toBeNull();
  });
});

describe('ShortcutRecorder component', () => {
  it('renders current shortcut and allows recording a new combo', async () => {
    const onChange = vi.fn().mockResolvedValue(undefined);
    render(
      <TooltipProvider>
        <ShortcutRecorder value="CommandOrControl+Shift+Space" onChange={onChange} />
      </TooltipProvider>
    );

    const btn = screen.getByTestId('shortcut-recorder-button');
    expect(btn).toBeTruthy();
    expect(btn.textContent).toContain('Space');

    // Click to enter recording mode
    act(() => {
      fireEvent.click(btn);
    });
    expect(btn.className).toContain('ring-accent');
    expect(screen.getByText('Press keys…')).toBeTruthy();

    // Press Escape to cancel
    act(() => {
      fireEvent.keyDown(btn, { key: 'Escape' });
    });
    expect(screen.queryByText('Press keys…')).toBeNull();
    expect(onChange).not.toHaveBeenCalled();

    // Click again and enter keys
    act(() => {
      fireEvent.click(btn);
    });
    act(() => {
      fireEvent.keyDown(btn, {
        key: 'k',
        code: 'KeyK',
        metaKey: true,
        altKey: true,
      });
    });
    expect(btn.textContent).toContain('K');
    expect(screen.getByText('(Enter to save, Esc to cancel)')).toBeTruthy();

    // Press Enter to save
    await act(async () => {
      fireEvent.keyDown(btn, { key: 'Enter' });
    });
    expect(onChange).toHaveBeenCalledWith('CommandOrControl+Alt+K');
  });

  it('Reset button resets hotkey to default combo', async () => {
    const onChange = vi.fn().mockResolvedValue(undefined);
    render(
      <TooltipProvider>
        <ShortcutRecorder value="CommandOrControl+Alt+K" onChange={onChange} />
      </TooltipProvider>
    );

    const resetBtn = screen.getByTestId('shortcut-reset-button');
    expect(resetBtn).not.toBeDisabled();

    await act(async () => {
      fireEvent.click(resetBtn);
    });
    expect(onChange).toHaveBeenCalledWith(DEFAULT_HOTKEY);
  });

  it('Disabled button disables hotkey', async () => {
    const onChange = vi.fn().mockResolvedValue(undefined);
    render(
      <TooltipProvider>
        <ShortcutRecorder value="CommandOrControl+Shift+Space" onChange={onChange} />
      </TooltipProvider>
    );

    const disableBtn = screen.getByTestId('shortcut-disable-button');
    expect(disableBtn).not.toBeDisabled();

    await act(async () => {
      fireEvent.click(disableBtn);
    });
    expect(onChange).toHaveBeenCalledWith('');
  });

  it('shows inline localized error and keeps old shortcut if registration fails', async () => {
    const onChange = vi.fn();
    const registerPlugin = vi.fn().mockRejectedValue(new Error('Shortcut already in use'));
    const unregisterPlugin = vi.fn().mockResolvedValue(undefined);

    render(
      <TooltipProvider>
        <ShortcutRecorder
          value="CommandOrControl+Shift+Space"
          onChange={onChange}
          registerPlugin={registerPlugin}
          unregisterPlugin={unregisterPlugin}
        />
      </TooltipProvider>
    );

    const btn = screen.getByTestId('shortcut-recorder-button');
    act(() => {
      fireEvent.click(btn);
    });

    // Enter conflicting shortcut and press Enter
    act(() => {
      fireEvent.keyDown(btn, {
        key: 'p',
        code: 'KeyP',
        metaKey: true,
        altKey: true,
      });
    });
    await act(async () => {
      fireEvent.keyDown(btn, { key: 'Enter' });
    });

    // Expect inline error displayed
    const err = await screen.findByTestId('shortcut-error');
    expect(err).toBeTruthy();
    expect(err.textContent).toBe('Shortcut is already in use by another application or invalid');

    // onChange is never called with the failing shortcut
    expect(onChange).not.toHaveBeenCalled();
  });
});
