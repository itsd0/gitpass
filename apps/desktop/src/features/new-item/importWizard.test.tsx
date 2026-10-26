import { describe, expect, it, vi } from 'vitest';
import { api } from '../../api/client';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { IMPORT_FORMATS, canPreview, importReducer, initialImport } from './importWizard';
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
    expect(IMPORT_FORMATS.map(f => f.id)).toEqual(['bitwarden-json', 'bitwarden-csv', '1password-csv', 'keepass-csv', 'chrome-csv', 'gitpass-json']);
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
});
