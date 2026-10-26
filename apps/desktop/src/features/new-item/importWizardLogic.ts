export const IMPORT_FORMATS = [
  { id: 'bitwarden-json', label: 'Bitwarden (.json)', ext: 'json' }, { id: 'bitwarden-csv', label: 'Bitwarden (.csv)', ext: 'csv' },
  { id: '1password-csv', label: '1Password 8 (.csv)', ext: 'csv' }, { id: '1password-1pux', label: '1Password (.1pux)', ext: '1pux' }, { id: 'keepass-csv', label: 'KeePass (.csv)', ext: 'csv' },
  { id: 'chrome-csv', label: 'Chrome / Chromium (.csv)', ext: 'csv' }, { id: 'gitpass-json', label: 'gitpass (.json)', ext: 'json' },
] as const;
import type { ImportResult } from '../../api/types';
export type ImportStep = 'format' | 'file' | 'preview' | 'done';
export interface ImportState { step: ImportStep; format: string | null; path: string; vaultId: string | null; preview: ImportResult | null; result: ImportResult | null; error: string | null; }
export type ImportAction = { type: 'format'; format: string } | { type: 'path'; path: string } | { type: 'vault'; vaultId: string } | { type: 'preview'; preview: ImportState['preview'] } | { type: 'done'; result: ImportState['result'] } | { type: 'error'; error: string } | { type: 'back' };
export const initialImport = (vaultId: string | null = null): ImportState => ({ step: 'format', format: null, path: '', vaultId, preview: null, result: null, error: null });
export function importReducer(s: ImportState, a: ImportAction): ImportState {
  switch (a.type) {
    case 'format': return { ...s, format: a.format, step: 'file', error: null };
    case 'path': return { ...s, path: a.path, error: null };
    case 'vault': return { ...s, vaultId: a.vaultId };
    case 'preview': return { ...s, preview: a.preview, step: 'preview', error: null };
    case 'done': return { ...s, result: a.result, step: 'done', error: null };
    case 'error': return { ...s, error: a.error };
    case 'back': return { ...s, error: null, step: s.step === 'preview' ? 'file' : s.step === 'file' ? 'format' : s.step };
  }
}
export const canPreview = (s: ImportState) => !!(s.format && s.path.trim() && s.vaultId);
