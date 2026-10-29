import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, act, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { TooltipProvider } from '../../ui';
import { ItemDetailPane } from './ItemDetail';
import { ItemEditor } from './ItemEditor';
import { FieldRow } from './FieldView';
import { NotesEditorField } from './NotesEditorField';
import { SettingsPage } from '../settings/SettingsPage';
import { searchSettings } from '../settings/searchIndex';
import { useSettings } from '../settings/useSettings';
import { useApp } from '../../lib/store';
import { useItemsUI } from './state';
import { useI18n, translate } from '../../i18n';
import { api } from '../../api/client';
import type { ItemDetail, ItemSummary } from '../../api/types';

const RAW_NOTES = `# My Secret Notes\n\n- [ ] Task 1\n- [x] Task 2\n\nVisit [Example](https://example.com) for details.`;

const mockDetail: ItemDetail = {
  id: 'item-1',
  vaultId: 'v-1',
  category: 'login',
  title: 'Test Login',
  subtitle: 'user@example.com',
  url: 'https://example.com',
  favorite: false,
  archived: false,
  deletedAt: null,
  createdAt: '2026-10-01T00:00:00Z',
  updatedAt: '2026-10-02T00:00:00Z',
  sections: [
    {
      id: 'main',
      title: 'Login',
      fields: [
        { id: 'username', label: 'username', kind: 'text', value: 'user@example.com' },
        { id: 'password', label: 'password', kind: 'concealed', value: null },
      ],
    },
  ],
  notes: RAW_NOTES,
  tags: ['personal'],
  attachments: [],
  history: [],
};

const mockSummary: ItemSummary = {
  id: 'item-1',
  vaultId: 'v-1',
  category: 'login',
  title: 'Test Login',
  subtitle: 'user@example.com',
  url: 'https://example.com',
  favorite: false,
  archived: false,
  updatedAt: '2026-10-02T00:00:00Z',
  tags: ['personal'],
  hasTotp: false,
};

const mockSecureNoteDetail: ItemDetail = {
  ...mockDetail,
  id: 'note-1',
  category: 'secure_note',
  title: 'Private Safe Note',
  sections: [
    {
      id: 'main',
      title: null,
      fields: [
        { id: 'note', label: 'note', kind: 'multiline', value: RAW_NOTES },
      ],
    },
  ],
  notes: null,
};

describe('Notes safe Markdown feature (#200)', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    useApp.setState({
      selectedItemId: 'item-1',
      editing: false,
      locked: false,
      route: { name: 'items', filter: { kind: 'all' } },
    });
    useItemsUI.setState({ items: [mockSummary], loading: false });
    useSettings.setState({
      settings: {
        autoLockMinutes: 10,
        clipboardClearSecs: 90,
        theme: 'dark',
        language: 'en',
        biometricUnlock: false,
        quickAccessHotkey: 'CmdOrCtrl+Shift+Space',
        hibpCheck: false,
        renderNotesMarkdown: true,
      },
    });
    vi.spyOn(api, 'getItem').mockResolvedValue(mockDetail);
    vi.spyOn(api, 'listVaults').mockResolvedValue([
      { id: 'v-1', name: 'Personal', description: null, itemCount: 1, shared: false, members: [], icon: null, color: '#f97316' },
    ]);
  });

  afterEach(() => {
    useI18n.getState().setLang('en');
  });

  it('renders notes as safe Markdown in item detail when setting is on', async () => {
    render(
      <TooltipProvider>
        <ItemDetailPane />
      </TooltipProvider>
    );

    await waitFor(() => expect(screen.getByRole('heading', { level: 1, name: 'My Secret Notes' })).toBeInTheDocument());
    expect(screen.getByText('Task 1')).toBeInTheDocument();
    expect(screen.getByText('Task 2')).toBeInTheDocument();
    const link = screen.getByRole('link', { name: 'Example' });
    expect(link).toHaveAttribute('href', 'https://example.com');
    expect(link).toHaveAttribute('target', '_blank');
  });

  it('renders notes as plain text in item detail when setting is off', async () => {
    useSettings.setState({
      settings: {
        ...useSettings.getState().settings!,
        renderNotesMarkdown: false,
      },
    });

    render(
      <TooltipProvider>
        <ItemDetailPane />
      </TooltipProvider>
    );

    await waitFor(() => expect(screen.getByTestId('detail-notes')).toBeInTheDocument());
    expect(screen.queryByRole('heading', { level: 1, name: 'My Secret Notes' })).toBeNull();
    // Plain text container has whitespace-pre-wrap
    const container = screen.getByTestId('detail-notes');
    expect(container.textContent).toContain(RAW_NOTES);
  });

  it('copy button on detail notes field copies raw markdown source', async () => {
    const clipboardSpy = vi.fn().mockResolvedValue(undefined);
    Object.assign(navigator, {
      clipboard: { writeText: clipboardSpy },
    });

    render(
      <TooltipProvider>
        <ItemDetailPane />
      </TooltipProvider>
    );

    const copyBtn = await screen.findByTestId('copy-notes-btn');
    fireEvent.click(copyBtn);

    await waitFor(() => expect(clipboardSpy).toHaveBeenCalledWith(RAW_NOTES));
  });

  it('renders Secure Note body in FieldView as markdown when setting is on', () => {
    render(
      <TooltipProvider>
        <FieldRow
          field={{ id: 'note', label: 'note', kind: 'multiline', value: RAW_NOTES }}
          ctx={{ vaultId: 'v-1', itemId: 'note-1' }}
          category="secure_note"
        />
      </TooltipProvider>
    );

    expect(screen.getByRole('heading', { level: 1, name: 'My Secret Notes' })).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'Example' })).toHaveAttribute('href', 'https://example.com');
  });

  it('renders Secure Note body in FieldView as plain text when setting is off', () => {
    useSettings.setState({
      settings: {
        ...useSettings.getState().settings!,
        renderNotesMarkdown: false,
      },
    });

    render(
      <TooltipProvider>
        <FieldRow
          field={{ id: 'note', label: 'note', kind: 'multiline', value: RAW_NOTES }}
          ctx={{ vaultId: 'v-1', itemId: 'note-1' }}
          category="secure_note"
        />
      </TooltipProvider>
    );

    expect(screen.queryByRole('heading', { level: 1, name: 'My Secret Notes' })).toBeNull();
    const fieldContainer = screen.getByTestId('field-note');
    expect(fieldContainer.textContent).toContain('# My Secret Notes');
    expect(fieldContainer.querySelector('.whitespace-pre-wrap')).toBeInTheDocument();
  });

  it('NotesEditorField defaults to Edit mode and renders textarea', () => {
    const onChange = vi.fn();
    render(<NotesEditorField value={RAW_NOTES} onChange={onChange} label="notes" />);

    const textarea = screen.getByRole('textbox', { name: 'notes' });
    expect(textarea).toBeInTheDocument();
    expect(textarea).toHaveValue(RAW_NOTES);

    const editBtn = screen.getByTestId('notes-mode-edit');
    expect(editBtn).toHaveAttribute('aria-checked', 'true');
    expect(editBtn).toHaveClass('bg-accent');

    const previewBtn = screen.getByTestId('notes-mode-preview');
    expect(previewBtn).toHaveAttribute('aria-checked', 'false');
  });

  it('NotesEditorField switches to Preview mode on clicking Preview segment and via arrow keys', async () => {
    const u = userEvent.setup();
    const onChange = vi.fn();
    render(<NotesEditorField value={RAW_NOTES} onChange={onChange} label="notes" />);

    const previewBtn = screen.getByTestId('notes-mode-preview');
    await u.click(previewBtn);

    expect(screen.getByTestId('notes-preview')).toBeInTheDocument();
    expect(screen.getByRole('heading', { level: 1, name: 'My Secret Notes' })).toBeInTheDocument();
    expect(screen.queryByRole('textbox')).toBeNull();

    // Toggle back with ArrowLeft on segment
    fireEvent.keyDown(screen.getByTestId('notes-mode-preview'), { key: 'ArrowLeft' });
    expect(screen.getByRole('textbox', { name: 'notes' })).toBeInTheDocument();
  });

  it('⌘⇧P shortcut toggles preview when textarea is focused', async () => {
    const onChange = vi.fn();
    render(<NotesEditorField value={RAW_NOTES} onChange={onChange} label="notes" />);

    const textarea = screen.getByRole('textbox', { name: 'notes' });
    textarea.focus();

    // Press Cmd+Shift+P
    fireEvent.keyDown(textarea, { key: 'p', shiftKey: true, metaKey: true });

    expect(screen.getByTestId('notes-preview')).toBeInTheDocument();
    expect(screen.getByRole('heading', { level: 1, name: 'My Secret Notes' })).toBeInTheDocument();

    // Press Cmd+Shift+P again while preview container is focused -> returns to edit
    const previewContainer = screen.getByTestId('notes-preview');
    fireEvent.keyDown(previewContainer, { key: 'p', shiftKey: true, metaKey: true });

    await waitFor(() => expect(screen.getByRole('textbox', { name: 'notes' })).toBeInTheDocument());
  });

  it('SettingsPage displays Render notes as Markdown switch in General tab and persists toggle', async () => {
    const setSettingsSpy = vi.spyOn(api, 'setSettings').mockResolvedValue(undefined as any);
    vi.spyOn(api, 'getSettings').mockResolvedValue(useSettings.getState().settings!);

    const u = userEvent.setup();
    render(<SettingsPage initialTab="general" />);

    const sw = await screen.findByTestId('render-notes-markdown-switch');
    expect(sw).toHaveAttribute('aria-checked', 'true');
    expect(screen.getByText('Render notes as Markdown')).toBeInTheDocument();

    await u.click(sw);
    await waitFor(() => expect(setSettingsSpy).toHaveBeenCalled());
    expect(setSettingsSpy.mock.calls[setSettingsSpy.mock.calls.length - 1]![0].settings.renderNotesMarkdown).toBe(false);
  });

  it('searchSettings discovers render notes markdown by keywords notes, markdown, formatting', () => {
    const t = (k: any) => translate('en', k);

    const res1 = searchSettings('markdown', t);
    expect(res1.some((r) => r.id === 'setting-render-notes-markdown')).toBe(true);

    const res2 = searchSettings('notes', t);
    expect(res2.some((r) => r.id === 'setting-render-notes-markdown')).toBe(true);

    const res3 = searchSettings('checklist', t);
    expect(res3.some((r) => r.id === 'setting-render-notes-markdown')).toBe(true);
  });

  it('localizes notes markdown settings and editor strings in Burmese and Thai', () => {
    expect(translate('my', 'settings.renderNotesMarkdown')).toBe('မှတ်စုများကို Markdown အဖြစ် ပြသပါ');
    expect(translate('th', 'settings.renderNotesMarkdown')).toBe('แสดงบันทึกเป็น Markdown');

    expect(translate('my', 'editor.edit')).toBe('ပြင်ဆင်ရန်');
    expect(translate('th', 'editor.edit')).toBe('แก้ไข');

    expect(translate('my', 'editor.preview')).toBe('အကြိုကြည့်ရန်');
    expect(translate('th', 'editor.preview')).toBe('ดูตัวอย่าง');
  });
});
