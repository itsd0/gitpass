import { describe, expect, it, vi } from 'vitest';
import fs from 'node:fs';
import path from 'node:path';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { api } from '../../api/client';
import { useApp } from '../../lib/store';
import {
  IMPORT_FORMATS,
  canPreview,
  detectImportFormat,
  importReducer,
  initialImport,
} from './importWizardLogic';
import { ImportWizard } from './ImportWizard';

const FIXTURES_DIR = path.resolve(
  __dirname,
  '../../../../../crates/gitpass-core/tests/fixtures/import'
);

describe('detectImportFormat pure function', () => {
  it('detects .1pux from extension', () => {
    expect(detectImportFormat('export.1pux')).toBe('1pux');
    expect(detectImportFormat('/Users/alice/Downloads/export.1pux')).toBe('1pux');
  });

  it('detects generic CSV from fixture header', () => {
    const content = fs.readFileSync(path.join(FIXTURES_DIR, 'generic.csv'), 'utf8');
    expect(detectImportFormat('export.csv', content)).toBe('generic-csv');
  });

  it('detects bitwarden CSV from fixture header', () => {
    const content = fs.readFileSync(path.join(FIXTURES_DIR, 'bitwarden.csv'), 'utf8');
    expect(detectImportFormat('export.csv', content)).toBe('bitwarden-csv');
  });

  it('detects bitwarden JSON from fixture content', () => {
    const content = fs.readFileSync(path.join(FIXTURES_DIR, 'bitwarden.json'), 'utf8');
    expect(detectImportFormat('export.json', content)).toBe('bitwarden-json');
  });

  it('detects chrome CSV from fixture header', () => {
    const content = fs.readFileSync(path.join(FIXTURES_DIR, 'chrome.csv'), 'utf8');
    expect(detectImportFormat('export.csv', content)).toBe('chrome-csv');
  });

  it('detects lastpass CSV from fixture header', () => {
    const content = fs.readFileSync(path.join(FIXTURES_DIR, 'lastpass.csv'), 'utf8');
    expect(detectImportFormat('export.csv', content)).toBe('lastpass-csv');
  });

  it('detects protonpass CSV from fixture header', () => {
    const content = fs.readFileSync(path.join(FIXTURES_DIR, 'protonpass.csv'), 'utf8');
    expect(detectImportFormat('export.csv', content)).toBe('protonpass-csv');
  });

  it('detects keepass CSV from fixture header', () => {
    const content = fs.readFileSync(path.join(FIXTURES_DIR, 'keepass.csv'), 'utf8');
    expect(detectImportFormat('export.csv', content)).toBe('keepass-csv');
  });

  it('detects keepass2 CSV from fixture header', () => {
    const content = fs.readFileSync(path.join(FIXTURES_DIR, 'keepass2.csv'), 'utf8');
    expect(detectImportFormat('export.csv', content)).toBe('keepass-csv');
  });

  it('detects gitpass JSON from shape (plaintext and encrypted)', () => {
    const plain = JSON.stringify({ gitpass_export: 1, items: [] });
    expect(detectImportFormat('export.json', plain)).toBe('gitpass-json');

    const encrypted = JSON.stringify({ gitpass_encrypted_export: 1, envelope: 'abc', salt: 'xyz' });
    expect(detectImportFormat('export.json', encrypted)).toBe('gitpass-json');
  });

  it('detects Apple Passwords CSV from fixture header and distinguishes it from generic CSV', () => {
    const content = fs.readFileSync(path.join(FIXTURES_DIR, 'apple-passwords.csv'), 'utf8');
    expect(detectImportFormat('export.csv', content)).toBe('apple-passwords-csv');
    expect(detectImportFormat('export.csv', 'Title,URL,Username,Password,Notes,OTPAuth\n')).toBe('apple-passwords-csv');
    expect(
      detectImportFormat('export.csv', 'Title,Url,Username,Password,OTPAuth,Favorite,Archived,Tags,Notes\n')
    ).toBe('generic-csv');
    expect(detectImportFormat('export.csv', 'name,url,username,password,note\n')).toBe('chrome-csv');
  });

  it('falls back to Apple Passwords for Passwords.csv / Safari filenames', () => {
    expect(detectImportFormat('/Users/demo/Downloads/Passwords.csv')).toBe('apple-passwords-csv');
    expect(detectImportFormat('Safari Passwords.csv')).toBe('apple-passwords-csv');
  });

  it('ignores apple/safari in parent folders when guessing from the path', () => {
    expect(
      detectImportFormat('/Users/demo/Library/Mobile Documents/com~apple~CloudDocs/export.csv')
    ).toBe('generic-csv');
    expect(detectImportFormat('/Users/demo/Safari/export.csv')).toBe('generic-csv');
    expect(detectImportFormat('/Users/demo/Safari/my-bitwarden.csv')).toBe('bitwarden-csv');
  });

  it('falls back to filename cues when content is missing', () => {
    expect(detectImportFormat('my-bitwarden-export.csv')).toBe('bitwarden-csv');
    expect(detectImportFormat('keepass-passwords.csv')).toBe('keepass-csv');
    expect(detectImportFormat('Chrome Passwords.csv')).toBe('chrome-csv');
    expect(detectImportFormat('lastpass_export.csv')).toBe('lastpass-csv');
    expect(detectImportFormat('proton_pass.csv')).toBe('protonpass-csv');
    expect(detectImportFormat('gitpass-backup.json')).toBe('gitpass-json');
    expect(detectImportFormat('bitwarden-backup.json')).toBe('bitwarden-json');
  });
});

describe('import wizard reducer', () => {
  it('walks file -> preview -> done and back', () => {
    let s = initialImport('v1');
    expect(s.step).toBe('file');
    expect(canPreview(s)).toBe(false);

    s = importReducer(s, { type: 'path', path: '/tmp/bitwarden.csv' });
    expect(s.format).toBe('bitwarden-csv');
    expect(s.autoDetected).toBe(true);
    expect(canPreview(s)).toBe(true);

    s = importReducer(s, { type: 'format', format: 'chrome-csv' });
    expect(s.format).toBe('chrome-csv');

    s = importReducer(s, {
      type: 'preview',
      preview: { parsed: 3, added: 0, duplicates: 1 },
    });
    expect(s.step).toBe('preview');

    expect(importReducer(s, { type: 'back' }).step).toBe('file');

    s = importReducer(s, {
      type: 'done',
      result: { parsed: 3, added: 2, duplicates: 1 },
    });
    expect(s.step).toBe('done');
  });

  it('detects .1pif from filename and content', () => {
    expect(detectImportFormat('export.1pif')).toBe('1pif');
    expect(detectImportFormat('data.1pif')).toBe('1pif');
    expect(detectImportFormat('random.txt', '***uuid***\n{"typeName":"webforms.WebForm"}')).toBe('1pif');
  });

  it('offers all required formats including Apple Passwords / Safari', () => {
    expect(IMPORT_FORMATS.map((f) => f.id)).toEqual([
      'apple-passwords-csv',
      'bitwarden-json',
      'bitwarden-csv',
      'generic-csv',
      '1pux',
      '1pif',
      'keepass-csv',
      'chrome-csv',
      'lastpass-csv',
      'protonpass-csv',
      'gitpass-json',
    ]);
  });
});

describe('ImportWizard component', () => {
  it('shows 3-step header and highlights current step in orange', async () => {
    render(<ImportWizard onBack={() => {}} onDone={() => {}} />);
    await screen.findByRole('heading', { name: /Migrate data into gitpass/i });

    const stepList = screen.getByLabelText(/Import progress/i);
    expect(stepList.textContent).toContain('1. Choose file');
    expect(stepList.textContent).toContain('2. Preview');
    expect(stepList.textContent).toContain('3. Done');

    const step1Span = screen.getByText(/1\. Choose file/);
    expect(step1Span.className).toContain('text-accent');
  });

  it('supports drag-and-drop with dashed orange border when dragging', async () => {
    render(<ImportWizard onBack={() => {}} onDone={() => {}} />);
    const dropZone = screen.getByTestId('drop-zone');

    // Drag enter
    fireEvent.dragEnter(dropZone);
    expect(dropZone.className).toContain('border-accent');
    expect(dropZone.className).toContain('border-dashed');

    // Drop file
    const file = new File(['name,url,username,password\nmail,http://a,u,p'], 'chrome.csv', {
      type: 'text/csv',
    });
    fireEvent.drop(dropZone, {
      dataTransfer: { files: [file] },
    });

    await waitFor(() => {
      expect((screen.getByLabelText('Format') as HTMLSelectElement).value).toBe('chrome-csv');
    });
    expect(screen.getByText('Auto-detected')).toBeTruthy();
  });

  it('walks choose-file -> preview -> confirm import using mock API', async () => {
    const importSpy = vi.spyOn(api, 'importFile').mockImplementation(async (a: any) => ({
      parsed: 24,
      added: a.dryRun ? 0 : 21,
      duplicates: 3,
      categories: { login: 20, credit_card: 4 },
      preview: [
        {
          title: 'Example Mail',
          category: 'login',
          username: 'alice@example.invalid',
          url: 'https://mail.example.invalid',
          hasPassword: true,
          hasTotp: true,
          tags: ['Work'],
          fields: 4,
          archived: false,
        },
      ],
    }));

    const onDone = vi.fn();
    render(<ImportWizard onBack={() => {}} onDone={onDone} />);

    // Vaults load
    await waitFor(() => {
      expect((screen.getByLabelText('Import into vault') as HTMLSelectElement).value).toBe('v_private');
    });

    // Enter file path
    fireEvent.change(screen.getByLabelText('Export file path'), {
      target: { value: '/tmp/bitwarden.json' },
    });

    // Auto-detected bitwarden-json
    expect((screen.getByLabelText('Format') as HTMLSelectElement).value).toBe('bitwarden-json');

    // Preview button clicked
    fireEvent.click(screen.getByText('Preview import'));

    // Step 2 is active
    expect(await screen.findByTestId('preview-Parsed')).toHaveProperty('textContent', '24');
    expect(screen.getByTestId('preview-Will add').textContent).toBe('21');
    expect(screen.getByLabelText('Item types').textContent).toContain('login: 20');
    expect(screen.getByText('Example Mail')).toBeTruthy();

    const step2Span = screen.getByText(/2\. Preview/);
    expect(step2Span.className).toContain('text-accent');

    // Confirm import
    fireEvent.click(screen.getByText('Confirm import'));

    // Step 3 Done view
    expect(await screen.findByText(/Import complete/i)).toBeTruthy();
    expect(screen.getByText(/Imported 21 of 24 items/i)).toBeTruthy();
    expect(screen.getByText('View imported items')).toBeTruthy();
    expect(screen.getByText('Close')).toBeTruthy();

    const step3Span = screen.getByText(/3\. Done/);
    expect(step3Span.className).toContain('text-accent');

    // Click View imported items
    const routeSpy = vi.spyOn(useApp.getState(), 'setRoute');
    fireEvent.click(screen.getByText('View imported items'));
    expect(routeSpy).toHaveBeenCalledWith(
      expect.objectContaining({
        name: 'items',
        filter: expect.objectContaining({ vaultId: 'v_private' }),
      })
    );
    expect(onDone).toHaveBeenCalled();
  });

  it('supports keyboard navigation: Enter advances, Esc closes, focus moves to heading', async () => {
    vi.spyOn(api, 'importFile').mockImplementation(async (a: any) => ({
      parsed: 5,
      added: a.dryRun ? 0 : 5,
      duplicates: 0,
    }));

    const onDone = vi.fn();
    render(<ImportWizard onBack={() => {}} onDone={onDone} />);

    await waitFor(() => {
      expect((screen.getByLabelText('Import into vault') as HTMLSelectElement).value).toBe('v_private');
    });

    const heading = screen.getByRole('heading', { name: /Migrate data into gitpass/i });
    expect(document.activeElement).toBe(heading);

    // Enter path
    fireEvent.change(screen.getByLabelText('Export file path'), {
      target: { value: '/tmp/export.1pux' },
    });

    // Press Enter to advance to preview
    fireEvent.keyDown(heading, { key: 'Enter', code: 'Enter' });

    await screen.findByTestId('preview-Parsed');
    expect(document.activeElement).toBe(heading);

    // Press Enter to confirm import
    fireEvent.keyDown(heading, { key: 'Enter', code: 'Enter' });

    await screen.findByText(/Import complete/i);
    expect(document.activeElement).toBe(heading);

    // Press Enter to close done step
    fireEvent.keyDown(heading, { key: 'Enter', code: 'Enter' });
    expect(onDone).toHaveBeenCalled();
  });

  it('closes on Escape key press', () => {
    const onDone = vi.fn();
    render(<ImportWizard onBack={() => {}} onDone={onDone} />);
    const heading = screen.getByRole('heading', { name: /Migrate data into gitpass/i });
    fireEvent.keyDown(heading, { key: 'Escape', code: 'Escape' });
    expect(onDone).toHaveBeenCalled();
  });

  it('does not intercept Enter when focus is on a button or select', async () => {
    const importSpy = vi.spyOn(api, 'importFile').mockResolvedValue({
      parsed: 5,
      added: 5,
      duplicates: 0,
    });
    importSpy.mockClear();
    const onBack = vi.fn();
    render(<ImportWizard onBack={onBack} onDone={() => {}} />);

    await waitFor(() => {
      expect((screen.getByLabelText('Import into vault') as HTMLSelectElement).value).toBe('v_private');
    });

    // Enter path
    fireEvent.change(screen.getByLabelText('Export file path'), {
      target: { value: '/tmp/export.1pux' },
    });

    const backButton = screen.getByRole('button', { name: 'Back' });
    const btnEvt = new KeyboardEvent('keydown', { key: 'Enter', code: 'Enter', bubbles: true, cancelable: true });
    backButton.dispatchEvent(btnEvt);

    expect(btnEvt.defaultPrevented).toBe(false);
    expect(importSpy).not.toHaveBeenCalled();

    const select = screen.getByLabelText('Format');
    const selectEvt = new KeyboardEvent('keydown', { key: 'Enter', code: 'Enter', bubbles: true, cancelable: true });
    select.dispatchEvent(selectEvt);

    expect(selectEvt.defaultPrevented).toBe(false);
    expect(importSpy).not.toHaveBeenCalled();

    // Check child inside button (e.g. svg icon in back button)
    const icon = backButton.querySelector('svg');
    if (icon) {
      const iconEvt = new KeyboardEvent('keydown', { key: 'Enter', code: 'Enter', bubbles: true, cancelable: true });
      icon.dispatchEvent(iconEvt);
      expect(iconEvt.defaultPrevented).toBe(false);
      expect(importSpy).not.toHaveBeenCalled();
    }
  });

  it('imports Apple Passwords / Safari with auto-detect and the macOS export hint', async () => {
    const spy = vi.spyOn(api, 'importFile').mockImplementation(async (a: any) => ({
      parsed: 4,
      added: a.dryRun ? 0 : 3,
      duplicates: 1,
    }));
    render(<ImportWizard onBack={() => {}} onDone={() => {}} />);
    await waitFor(() => {
      expect((screen.getByLabelText('Import into vault') as HTMLSelectElement).value).toBe('v_private');
    });
    expect(screen.getByRole('option', { name: 'Apple Passwords / Safari (.csv)' })).toBeTruthy();
    expect(screen.queryByTestId('format-hint')).toBeNull();

    const file = new File(['Title,URL,Username,Password,Notes,OTPAuth\nMail,https://a.example,u,p,,'], 'Passwords.csv', {
      type: 'text/csv',
    });
    fireEvent.drop(screen.getByTestId('drop-zone'), { dataTransfer: { files: [file] } });
    await waitFor(() => {
      expect((screen.getByLabelText('Format') as HTMLSelectElement).value).toBe('apple-passwords-csv');
    });
    expect(screen.getByText('Auto-detected')).toBeTruthy();
    expect(screen.getByTestId('format-hint').textContent).toMatch(
      /Passwords app \(File → Export All Passwords…\) or Safari \(File → Export → Passwords…\)/
    );

    fireEvent.click(screen.getByText('Preview import'));
    expect(await screen.findByTestId('preview-Parsed')).toHaveProperty('textContent', '4');
    expect(spy).toHaveBeenCalledWith(expect.objectContaining({ format: 'apple-passwords-csv', dryRun: true }));
  });
});
