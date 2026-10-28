import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, act } from '@testing-library/react';
import { TooltipProvider } from '../../ui';
import { FieldRow, REVEAL_MS } from './FieldView';
import { api } from '../../api/client';
import { useApp } from '../../lib/store';
import type { Field } from '../../api/types';

const SECRET = 'super-secret-FAKE';
const COMPLEX_SECRET = 'aB3!9#';
const ctx = { vaultId: 'v', itemId: 'i' };
const wrap = (f: Field) => render(<TooltipProvider><FieldRow field={f} ctx={ctx} /></TooltipProvider>);

describe('FieldRow', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    useApp.setState({ locked: false });
  });

  it('renders plain text values', () => {
    wrap({ id: 'f1', label: 'username', kind: 'text', value: 'demo' });
    expect(screen.getByText('username')).toBeInTheDocument();
    expect(screen.getByText('demo')).toBeInTheDocument();
  });

  it('concealed never shown without reveal, even if payload wrongly contains a value', () => {
    const spy = vi.spyOn(api, 'revealField');
    const { container } = wrap({ id: 'f2', label: 'password', kind: 'concealed', value: SECRET });
    expect(container.textContent).not.toContain(SECRET);
    expect(screen.getByTestId('concealed-value').textContent).toMatch(/^•+$/);
    expect(spy).not.toHaveBeenCalled();
  });

  it('reveal calls revealField, shows value, then auto-hides after 30s', async () => {
    vi.useFakeTimers();
    const spy = vi.spyOn(api, 'revealField').mockResolvedValue(SECRET);
    wrap({ id: 'f3', label: 'password', kind: 'concealed', value: null });
    await act(async () => { fireEvent.click(screen.getByLabelText('Reveal password')); });
    expect(spy).toHaveBeenCalledWith({ vaultId: 'v', itemId: 'i', fieldId: 'f3' });
    expect(screen.getByTestId('concealed-value').textContent).toBe(SECRET);
    act(() => { vi.advanceTimersByTime(REVEAL_MS + 10); });
    expect(screen.getByTestId('concealed-value').textContent).not.toContain(SECRET);
    vi.useRealTimers();
  });

  it('copy goes through api.copyField (Rust), not the DOM', async () => {
    const spy = vi.spyOn(api, 'copyField').mockResolvedValue(undefined);
    wrap({ id: 'f4', label: 'password', kind: 'concealed', value: null });
    await act(async () => { fireEvent.click(screen.getByLabelText('Copy password')); });
    expect(spy).toHaveBeenCalledWith({ vaultId: 'v', itemId: 'i', fieldId: 'f4' });
  });

  it('non-concealed fields do not show the large type action', () => {
    wrap({ id: 'f_user', label: 'username', kind: 'text', value: 'alice' });
    expect(screen.queryByTestId('large-type-trigger')).toBeNull();
    expect(screen.queryByLabelText(/large type/i)).toBeNull();

    render(<TooltipProvider><FieldRow field={{ id: 'f_note', label: 'notes', kind: 'multiline', value: 'some note' }} ctx={ctx} /></TooltipProvider>);
    expect(screen.queryByTestId('large-type-trigger')).toBeNull();
  });

  it('opening from a concealed field renders one cell per character with position numbers and expected classes', async () => {
    vi.spyOn(api, 'revealField').mockResolvedValue(COMPLEX_SECRET);
    wrap({ id: 'f_pw', label: 'password', kind: 'concealed', value: null });

    const trigger = screen.getByTestId('large-type-trigger');
    expect(trigger).toBeInTheDocument();
    expect(trigger).toHaveAttribute('aria-label', 'Show password in large type');

    await act(async () => {
      fireEvent.click(trigger);
    });

    // Dialog is visible
    expect(screen.getByRole('dialog')).toBeInTheDocument();

    // 6 characters in COMPLEX_SECRET ('aB3!9#')
    const cells = screen.getAllByTestId('large-type-char');
    expect(cells).toHaveLength(6);

    const positions = screen.getAllByTestId('large-type-pos');
    expect(positions).toHaveLength(6);
    expect(positions.map((p) => p.textContent)).toEqual(['1', '2', '3', '4', '5', '6']);

    // Check classification and classes:
    // a -> letter
    expect(cells[0].getAttribute('data-kind')).toBe('letter');
    expect(cells[0].querySelector('span')!.className).toContain('large-type-letter');
    expect(cells[0].querySelector('span')!.className).toContain('text-fg');

    // B -> letter
    expect(cells[1].getAttribute('data-kind')).toBe('letter');

    // 3 -> digit
    expect(cells[2].getAttribute('data-kind')).toBe('digit');
    expect(cells[2].querySelector('span')!.className).toContain('large-type-digit');
    expect(cells[2].querySelector('span')!.className).toContain('text-orange-500');

    // ! -> symbol
    expect(cells[3].getAttribute('data-kind')).toBe('symbol');
    expect(cells[3].querySelector('span')!.className).toContain('large-type-symbol');
    expect(cells[3].querySelector('span')!.className).toContain('text-blue-500');

    // 9 -> digit
    expect(cells[4].getAttribute('data-kind')).toBe('digit');
    expect(cells[4].querySelector('span')!.className).toContain('large-type-digit');
    expect(cells[4].querySelector('span')!.className).toContain('text-orange-500');

    // # -> symbol
    expect(cells[5].getAttribute('data-kind')).toBe('symbol');
    expect(cells[5].querySelector('span')!.className).toContain('large-type-symbol');
    expect(cells[5].querySelector('span')!.className).toContain('text-blue-500');
  });

  it('Esc closes large type and returns focus to the trigger', async () => {
    vi.spyOn(api, 'revealField').mockResolvedValue(SECRET);
    wrap({ id: 'f_pw', label: 'password', kind: 'concealed', value: null });

    const trigger = screen.getByTestId('large-type-trigger');
    trigger.focus();
    expect(document.activeElement).toBe(trigger);

    await act(async () => {
      fireEvent.click(trigger);
    });

    expect(screen.getByRole('dialog')).toBeInTheDocument();

    // Press Escape
    await act(async () => {
      fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
    });

    expect(screen.queryByRole('dialog')).toBeNull();
    expect(document.activeElement).toBe(trigger);
  });

  it('lock event closes the large type view', async () => {
    vi.spyOn(api, 'revealField').mockResolvedValue(SECRET);
    wrap({ id: 'f_pw', label: 'password', kind: 'concealed', value: null });

    await act(async () => {
      fireEvent.click(screen.getByTestId('large-type-trigger'));
    });

    expect(screen.getByRole('dialog')).toBeInTheDocument();

    // Dispatch DOM locked event
    act(() => {
      window.dispatchEvent(new CustomEvent('gitpass:locked'));
    });

    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('store lock closes the large type view', async () => {
    vi.spyOn(api, 'revealField').mockResolvedValue(SECRET);
    wrap({ id: 'f_pw', label: 'password', kind: 'concealed', value: null });

    await act(async () => {
      fireEvent.click(screen.getByTestId('large-type-trigger'));
    });

    expect(screen.getByRole('dialog')).toBeInTheDocument();

    // Trigger store lock
    act(() => {
      useApp.getState().lock();
    });

    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('⌥⌘L shortcut opens large type when the field is focused', async () => {
    const spy = vi.spyOn(api, 'revealField').mockResolvedValue('xyz789!');
    wrap({ id: 'f_shortcut', label: 'API secret', kind: 'concealed', value: null });

    const fieldContainer = screen.getByTestId('concealed-field-f_shortcut');
    fieldContainer.focus();

    await act(async () => {
      fireEvent.keyDown(fieldContainer, { key: 'l', altKey: true, metaKey: true });
    });

    expect(spy).toHaveBeenCalledWith({ vaultId: 'v', itemId: 'i', fieldId: 'f_shortcut' });
    expect(screen.getByRole('dialog')).toBeInTheDocument();

    const cells = screen.getAllByTestId('large-type-char');
    expect(cells).toHaveLength(7);

    // Close with Escape and check focus returns to field container
    await act(async () => {
      fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
    });
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(document.activeElement).toBe(fieldContainer);
  });

  it("shows expired pill and relative hint for expired credit card expiry field", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-05-15T12:00:00Z"));
    render(
      <TooltipProvider>
        <FieldRow
          field={{ id: "expiry", label: "expiry date", kind: "month_year", value: "03/2026" }}
          ctx={ctx}
          category="credit_card"
        />
      </TooltipProvider>
    );
    const pill = screen.getByTestId("expiry-pill");
    expect(pill).toBeInTheDocument();
    expect(pill.textContent).toBe("Expired");
    expect(screen.getByTestId("expiry-hint").textContent).toBe("Expired 2 months ago");
    vi.useRealTimers();
  });

  it("shows expires soon pill and relative hint for expiring api credential expires field", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-05-15T12:00:00Z"));
    render(
      <TooltipProvider>
        <FieldRow
          field={{ id: "expires", label: "expires", kind: "date", value: "2026-05-27" }}
          ctx={ctx}
          category="api_credential"
        />
      </TooltipProvider>
    );
    const pill = screen.getByTestId("expiry-pill");
    expect(pill).toBeInTheDocument();
    expect(pill.textContent).toBe("Expires soon");
    expect(screen.getByTestId("expiry-hint").textContent).toBe("Expires in 12 days");
    vi.useRealTimers();
  });

  it("does not show expiry pill for non-expiry date fields such as birth date", () => {
    render(
      <TooltipProvider>
        <FieldRow
          field={{ id: "birth", label: "birth date", kind: "date", value: "1990-01-01" }}
          ctx={ctx}
          category="identity"
        />
      </TooltipProvider>
    );
    expect(screen.queryByTestId("expiry-pill")).toBeNull();
    expect(screen.queryByTestId("expiry-hint")).toBeNull();
  });
});
