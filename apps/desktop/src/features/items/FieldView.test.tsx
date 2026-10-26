import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, act } from '@testing-library/react';
import { TooltipProvider } from '../../ui';
import { FieldRow, REVEAL_MS } from './FieldView';
import { api } from '../../api/client';
import type { Field } from '../../api/types';

const SECRET = 'super-secret-FAKE';
const ctx = { vaultId: 'v', itemId: 'i' };
const wrap = (f: Field) => render(<TooltipProvider><FieldRow field={f} ctx={ctx} /></TooltipProvider>);

describe('FieldRow', () => {
  beforeEach(() => { vi.restoreAllMocks(); });
  it('renders plain text values', () => {
    wrap({ id: 'f1', label: 'username', kind: 'text', value: 'demo' });
    expect(screen.getByText('username')).toBeInTheDocument(); expect(screen.getByText('demo')).toBeInTheDocument();
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
});
