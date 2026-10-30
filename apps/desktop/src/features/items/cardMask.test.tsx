import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen, fireEvent, act, cleanup } from '@testing-library/react';
import { TooltipProvider } from '../../ui';
import { FieldRow } from './FieldView';
import { api } from '../../api/client';
import { cardLabel, cardMask, groupCardNumber, secretHint } from '../../lib/cardMask';
const ctx = { vaultId: 'v', itemId: 'i' };
afterEach(() => { cleanup(); vi.restoreAllMocks(); });

describe('card number masking', () => {
  it('labels: brand from BIN + last 4 only', () => {
    expect(cardLabel('4000 1234 1234 7575')).toBe('Visa •••• 7575');
    expect(cardLabel('5555555555554444')).toBe('Mastercard •••• 4444');
    expect(cardLabel('9999000011117575')).toBe('•••• 7575');
    expect(cardLabel('123')).toBe('');
    expect(cardMask('4000123412347575')).toBe('•••• •••• •••• 7575');
    expect(groupCardNumber('4000123412347575')).toBe('4000 1234 1234 7575');
  });
  it('detail shows last 4 masked, reveal shows full number grouped in 4s', async () => {
    vi.spyOn(api, 'revealField').mockResolvedValue('4000123412347575');
    render(<TooltipProvider><FieldRow category="credit_card" field={{ id: 'number', label: 'number', kind: 'concealed', value: null, masked: '•••• •••• •••• 7575' }} ctx={ctx} /></TooltipProvider>);
    expect(screen.getByTestId('concealed-value').textContent).toBe('•••• •••• •••• 7575');
    await act(async () => { fireEvent.click(screen.getByLabelText('Reveal number')); });
    expect(screen.getByTestId('concealed-value').textContent).toBe('4000 1234 1234 7575');
  });
  it('CVV stays fully masked', () => {
    render(<TooltipProvider><FieldRow category="credit_card" field={{ id: 'cvv', label: 'verification number', kind: 'concealed', value: null }} ctx={ctx} /></TooltipProvider>);
    expect(screen.getByTestId('concealed-value').textContent).toMatch(/^•+$/);
  });
  it('API key: secretHint boundaries and reveal shows full key without digit grouping', async () => {
    expect(secretHint('')).toBe('••••••••');
    expect(secretHint('short')).toBe('••••••••');
    expect(secretHint('123456')).toBe('12••••••••');
    expect(secretHint('123456789012')).toBe('1234••••••••');
    expect(secretHint('sk-live-0123456789abcdefgh')).toBe('sk-l••••••••efgh');

    const API_KEY = 'sk-live-0123456789abcdefgh';
    vi.spyOn(api, 'revealField').mockResolvedValue(API_KEY);
    render(<TooltipProvider><FieldRow category="api_credential" field={{ id: 'credential', label: 'credential', kind: 'concealed', value: null, masked: secretHint(API_KEY) }} ctx={ctx} /></TooltipProvider>);
    expect(screen.getByTestId('concealed-value').textContent).toBe('sk-l••••••••efgh');
    await act(async () => { fireEvent.click(screen.getByLabelText('Reveal credential')); });
    expect(screen.getByTestId('concealed-value').textContent).toBe(API_KEY);
  });
});
