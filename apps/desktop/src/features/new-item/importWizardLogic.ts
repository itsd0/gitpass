import type { ImportResult } from '../../api/types';

export const IMPORT_FORMATS = [
  { id: 'bitwarden-json', label: 'Bitwarden (.json)', ext: 'json' },
  { id: 'bitwarden-csv', label: 'Bitwarden (.csv)', ext: 'csv' },
  { id: '1password-csv', label: '1Password 8 (.csv)', ext: 'csv' },
  { id: '1password-1pux', label: '1Password (.1pux)', ext: '1pux' },
  { id: 'keepass-csv', label: 'KeePass (.csv)', ext: 'csv' },
  { id: 'chrome-csv', label: 'Chrome / Chromium (.csv)', ext: 'csv' },
  { id: 'gitpass-json', label: 'gitpass (.json)', ext: 'json' },
] as const;

export type ImportFormatId = (typeof IMPORT_FORMATS)[number]['id'];

export type ImportStep = 'file' | 'preview' | 'done';

export interface ImportState {
  step: ImportStep;
  format: string | null;
  path: string;
  vaultId: string | null;
  preview: ImportResult | null;
  result: ImportResult | null;
  error: string | null;
  autoDetected?: boolean;
}

export type ImportAction =
  | { type: 'file'; path: string; format?: string | null; autoDetected?: boolean }
  | { type: 'format'; format: string; autoDetected?: boolean }
  | { type: 'path'; path: string }
  | { type: 'vault'; vaultId: string }
  | { type: 'preview'; preview: ImportResult }
  | { type: 'done'; result: ImportResult }
  | { type: 'error'; error: string }
  | { type: 'back' };

export const initialImport = (vaultId: string | null = null): ImportState => ({
  step: 'file',
  format: null,
  path: '',
  vaultId,
  preview: null,
  result: null,
  error: null,
  autoDetected: false,
});

/** Pure function to detect the import format from filename and optional content/header. */
export function detectImportFormat(filename: string, contentOrHeader?: string): ImportFormatId | null {
  const lowerName = filename.toLowerCase().trim();

  // 1. .1pux files are zip archives
  if (lowerName.endsWith('.1pux') || lowerName.includes('1pux')) {
    return '1password-1pux';
  }

  const content = contentOrHeader?.trim() ?? '';

  // 2. JSON files or JSON content
  if (lowerName.endsWith('.json') || content.startsWith('{')) {
    if (content) {
      try {
        const parsed = JSON.parse(content);
        if (parsed.gitpass_export || parsed.gitpass_encrypted_export) {
          return 'gitpass-json';
        }
        if (parsed.folders || parsed.items || parsed.encrypted !== undefined) {
          return 'bitwarden-json';
        }
      } catch {
        if (content.includes('gitpass_export') || content.includes('gitpass_encrypted_export')) {
          return 'gitpass-json';
        }
        if (content.includes('folders') || content.includes('passwordHistory') || content.includes('organizationId')) {
          return 'bitwarden-json';
        }
      }
    }
    if (lowerName.includes('gitpass')) return 'gitpass-json';
    if (lowerName.includes('bitwarden')) return 'bitwarden-json';
    if (lowerName.endsWith('.json')) return 'bitwarden-json';
  }

  // 3. CSV files or CSV content
  if (lowerName.endsWith('.csv') || content) {
    if (content) {
      const firstLine = content.replace(/^\uFEFF/, '').split(/\r?\n/)[0] ?? '';
      const cols = firstLine.split(',').map((c) => c.replace(/^["']|["']$/g, '').trim().toLowerCase());

      // Bitwarden CSV distinctive headers
      if (cols.some((c) => ['login_username', 'login_uri', 'login_password', 'login_totp', 'reprompt'].includes(c))) {
        return 'bitwarden-csv';
      }

      // KeePass CSV distinctive headers
      if (cols.some((c) => ['group', 'account', 'login name', 'web site', 'comments', 'icon', 'last modified'].includes(c))) {
        return 'keepass-csv';
      }

      // 1Password CSV distinctive headers
      if (cols.some((c) => ['otpauth', 'archived', 'tags'].includes(c)) || (cols.includes('title') && cols.includes('favorite'))) {
        return '1password-csv';
      }

      // Chrome CSV distinctive headers
      if (cols.includes('name') && cols.includes('url') && cols.includes('password')) {
        return 'chrome-csv';
      }
    }

    if (lowerName.includes('bitwarden')) return 'bitwarden-csv';
    if (lowerName.includes('keepass')) return 'keepass-csv';
    if (lowerName.includes('chrome') || lowerName.includes('chromium')) return 'chrome-csv';
    if (lowerName.includes('1password') || lowerName.includes('onepassword')) return '1password-csv';
    if (lowerName.endsWith('.csv')) return '1password-csv';
  }

  return null;
}

export function importReducer(s: ImportState, a: ImportAction): ImportState {
  switch (a.type) {
    case 'file':
      return {
        ...s,
        path: a.path,
        format: a.format !== undefined ? a.format : s.format,
        autoDetected: a.autoDetected ?? false,
        error: null,
      };
    case 'format':
      return { ...s, format: a.format, autoDetected: a.autoDetected ?? false, error: null };
    case 'path': {
      const detected = detectImportFormat(a.path);
      return {
        ...s,
        path: a.path,
        format: detected ?? s.format,
        autoDetected: !!detected,
        error: null,
      };
    }
    case 'vault':
      return { ...s, vaultId: a.vaultId };
    case 'preview':
      return { ...s, preview: a.preview, step: 'preview', error: null };
    case 'done':
      return { ...s, result: a.result, step: 'done', error: null };
    case 'error':
      return { ...s, error: a.error };
    case 'back':
      return {
        ...s,
        error: null,
        step: s.step === 'preview' ? 'file' : s.step,
      };
  }
}

export const canPreview = (s: ImportState) => !!(s.format && s.path.trim() && s.vaultId);
