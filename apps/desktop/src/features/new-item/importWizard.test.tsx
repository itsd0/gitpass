import { describe, expect, it, vi } from 'vitest';
import { api } from '../../api/client';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { IMPORT_FORMATS, canPreview, importReducer, initialImport } from './importWizardLogic';
import { ImportWizard } from './ImportWizard';
describe('import wizard reducer', () => {
  it('walks format -> file -> preview -> done and back', () => {
    let s = initialImport('v1'); expect(s.step).toBe('format');
    s = importReducer(s, { type: 'format', format: 'chrome-csv' }); expect(s.step).toBe('file'); expect(canPreview(s)).toBe(false);
    s = importReducer(s, { type: 'path', path: '/tmp/x.csv' }); expect(canPreview(s)).toBe(true);
    s = importReducer(s, { type: 'preview', preview: { parsed: 3, added: 0, duplicates: 1 } }); expect(s.step).toBe('preview');
    expect(importReducer(s, { type: 'back' }).step).toBe('file');
    s = importReducer(s, { type: 'done', result: { parsed: 3, added: 2, duplicates: 1 } }); expect(s.step).toBe('done');
  });
  it('offers all required formats', () => {
    expect(IMPORT_FORMATS.map(f => f.id)).toEqual(['bitwarden-json', 'bitwarden-csv', '1password-csv', '1password-1pux', 'keepass-csv', 'chrome-csv', 'gitpass-json']);
  });
  it('renders dry-run counts then confirms (mock api)', async () => {
    vi.spyOn(api, 'importFile').mockImplementation(async (a: any) => ({ parsed: 24, added: a.dryRun ? 0 : 21, duplicates: 3 }));
    render(<ImportWizard onBack={() => {}} onDone={() => {}} />);
    fireEvent.click(screen.getByText('Bitwarden (.json)'));
    await waitFor(() => expect((screen.getByLabelText('Import vault') as HTMLSelectElement).value).toBe('v_private'));
    fireEvent.change(screen.getByLabelText('Export file path'), { target: { value: '/tmp/export.json' } });
    fireEvent.click(screen.getByText('Preview import'));
    expect(await screen.findByTestId('preview-Parsed')).toHaveProperty('textContent', '24');
    expect(screen.getByTestId('preview-Will add').textContent).toBe('21');
    fireEvent.click(screen.getByText('Confirm import'));
    expect(await screen.findByText(/Imported/)).toBeTruthy();
  });
  it('shows item types, sample items, duplicates and warnings in the preview', async () => {
    vi.spyOn(api, 'listVaults').mockResolvedValue([{ id: 'v1', name: 'Work', description: null, itemCount: 0, shared: false, members: [], icon: null, color: null } as any]);
    const spy = vi.spyOn(api, 'importFile').mockImplementation(async (a: any) => ({ parsed: 3, added: a.dryRun ? 0 : 2, duplicates: 1, duplicateTitles: ['Example Mail'], warnings: ['row 4: skipped empty row'],
      categories: { login: 2, credit_card: 1 }, preview: [{ title: 'Fake Visa', category: 'credit_card', username: null, url: null, hasPassword: false, hasTotp: false, tags: ['Finance'], fields: 5, archived: false },
        { title: 'Example Mail', category: 'login', username: 'alice@example.invalid', url: null, hasPassword: true, hasTotp: true, tags: [], fields: 0, archived: true }] }));
    render(<ImportWizard onBack={() => {}} onDone={() => {}} />);
    fireEvent.click(screen.getByText('1Password (.1pux)'));
    expect((screen.getByLabelText('Export file path') as HTMLInputElement).placeholder).toMatch(/\.1pux$/);
    fireEvent.change(screen.getByLabelText('Export file path'), { target: { value: '/tmp/x.1pux' } });
    await waitFor(() => expect((screen.getByLabelText('Import vault') as HTMLSelectElement).value).toBe('v1'));
    fireEvent.click(screen.getByText(/Preview import/));
    await screen.findByLabelText('Items to import');
    expect(spy).toHaveBeenCalledWith(expect.objectContaining({ format: '1password-1pux', vaultId: 'v1', dryRun: true }));
    expect(screen.getByLabelText('Item types').textContent).toContain('credit card: 1');
    expect(screen.getByText('Example Mail (archived)')).toBeTruthy();
    expect(screen.getByText(/password · one-time code/)).toBeTruthy();
    expect(screen.getByText(/Duplicates skipped: Example Mail/)).toBeTruthy();
    expect(screen.getByLabelText('Import warnings').textContent).toContain('skipped empty row');
  });
});
