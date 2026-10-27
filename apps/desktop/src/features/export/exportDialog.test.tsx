import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { api } from '../../api/client';
import type { ExportResult } from '../../api/types';
import { ExportDialog } from './ExportDialog';

const ok = (format: 'gitpass-json' | 'csv', encrypted: boolean, items = 2): ExportResult => ({ path: `~/Downloads/out.${format === 'csv' ? 'csv' : 'json'}`, items, format, encrypted });
afterEach(() => { cleanup(); vi.restoreAllMocks(); });

describe('ExportDialog', () => {
  it('defaults to encrypted JSON and validates the passphrase', async () => {
    const spy = vi.spyOn(api, 'exportItems').mockResolvedValue(ok('gitpass-json', true));
    const close = vi.fn();
    render(<ExportDialog open onOpenChange={close} vaultId="v1" vaultName="Personal" />);
    const submit = screen.getByRole('button', { name: /Choose file and export/i }) as HTMLButtonElement;
    expect(submit.disabled).toBe(true);
    fireEvent.change(screen.getByLabelText('Export passphrase'), { target: { value: 'short' } });
    expect(screen.getByText('Use at least 8 characters')).toBeTruthy();
    fireEvent.change(screen.getByLabelText('Export passphrase'), { target: { value: 'fake-export-pass' } });
    fireEvent.change(screen.getByLabelText('Confirm passphrase'), { target: { value: 'other-pass' } });
    expect(screen.getByText('Passphrases do not match')).toBeTruthy();
    fireEvent.change(screen.getByLabelText('Confirm passphrase'), { target: { value: 'fake-export-pass' } });
    await waitFor(() => expect(submit.disabled).toBe(false));
    fireEvent.click(submit);
    await waitFor(() => expect(spy).toHaveBeenCalledTimes(1));
    expect(spy).toHaveBeenCalledWith({ req: { vaultId: 'v1', format: 'gitpass-json', encrypted: true, passphrase: 'fake-export-pass', path: '~/Downloads/gitpass-export.json' } });
    expect(close).toHaveBeenCalledWith(false);
  });

  it('requires the explicit acknowledgement for plain CSV', async () => {
    const spy = vi.spyOn(api, 'exportItems').mockResolvedValue(ok('csv', false));
    render(<ExportDialog open onOpenChange={vi.fn()} vaultId="v1" vaultName="Personal" />);
    fireEvent.click(screen.getByRole('radio', { name: /Plain CSV/i }));
    const submit = screen.getByRole('button', { name: /Choose file and export/i }) as HTMLButtonElement;
    expect(submit.disabled).toBe(true);
    expect(screen.getByText(/This file is not encrypted/i)).toBeTruthy();
    fireEvent.click(screen.getByLabelText('I understand this file is unencrypted'));
    await waitFor(() => expect(submit.disabled).toBe(false));
    fireEvent.click(submit);
    await waitFor(() => expect(spy).toHaveBeenCalledTimes(1));
    expect(spy).toHaveBeenCalledWith({ req: { vaultId: 'v1', format: 'csv', encrypted: false, path: '~/Downloads/gitpass-export.csv' } });
  });

  it('exports a selected ref list with its own file name', async () => {
    const spy = vi.spyOn(api, 'exportItems').mockResolvedValue(ok('gitpass-json', true, 2));
    const items = [{ vaultId: 'v1', itemId: 'i1' }, { vaultId: 'v1', itemId: 'i2' }];
    render(<ExportDialog open onOpenChange={vi.fn()} items={items} />);
    expect(screen.getByText('Export selected items')).toBeTruthy();
    expect(screen.getByText('2 selected item(s)')).toBeTruthy();
    fireEvent.change(screen.getByLabelText('Export passphrase'), { target: { value: 'fake-export-pass' } });
    fireEvent.change(screen.getByLabelText('Confirm passphrase'), { target: { value: 'fake-export-pass' } });
    fireEvent.click(screen.getByRole('button', { name: /Choose file and export/i }));
    await waitFor(() => expect(spy).toHaveBeenCalledTimes(1));
    expect(spy).toHaveBeenCalledWith({ req: { items, format: 'gitpass-json', encrypted: true, passphrase: 'fake-export-pass', path: '~/Downloads/gitpass-selected.json' } });
  });
});
