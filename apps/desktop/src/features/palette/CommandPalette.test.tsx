import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { CommandPalette, PaletteHost } from './CommandPalette';
import { api } from '../../api/client';
import type { ItemDetail, ItemSummary } from '../../api/types';
import { useApp } from '../../lib/store';
import { translate, useI18n } from '../../i18n';
import { useToasts } from '../../ui';

const item = (id: string, title: string, over: Partial<ItemSummary> = {}): ItemSummary => ({
  id, vaultId: 'v1', category: 'login', title, subtitle: `${id}@example.invalid`, favorite: false, archived: false,
  deletedAt: null, tags: [], updatedAt: '2026-10-01T00:00:00Z', createdAt: '2026-10-01T00:00:00Z', url: 'https://example.invalid', ...over,
});
const detail = (base: ItemSummary): ItemDetail => ({ ...base, hasTotp: true, notes: null, attachments: [], history: [], sections: [{ id: 's', title: null, fields: [
  { id: 'fu', label: 'username', kind: 'text', value: 'fake@example.invalid' },
  { id: 'fp', label: 'password', kind: 'concealed', value: 'FAKE-secret-never-in-dom' },
  { id: 'ft', label: 'one-time password', kind: 'totp', value: null },
  { id: 'fw', label: 'website', kind: 'url', value: 'https://example.invalid/login' },
] }] });

const input = () => screen.getByRole('combobox');
const toastText = () => useToasts.getState().list.map((x) => x.title).join();
const waitForItem = (title: string) => waitFor(() => expect(screen.getAllByRole('option').some((o) => o.textContent?.includes(title))).toBe(true));

async function renderPalette(items: ItemSummary[]) {
  vi.spyOn(api, 'listItems').mockResolvedValue(items);
  const onOpenChange = vi.fn();
  render(<CommandPalette open onOpenChange={onOpenChange} />);
  if (items[0]) await waitForItem(items[0].title);
  return onOpenChange;
}

beforeEach(() => {
  useI18n.setState({ lang: 'en' });
  useToasts.setState({ list: [] });
  useApp.setState({ route: { name: 'items', filter: { kind: 'all' } }, selectedItemId: null, multiIds: [], modal: null, editing: false });
});

afterEach(() => { cleanup(); vi.restoreAllMocks(); window.getSelection()?.removeAllRanges(); });

describe('recently used items', () => {
  it('lists the 8 most recently used items above the commands for an empty query', async () => {
    const items = Array.from({ length: 10 }, (_, i) => item(`i${i + 1}`, `Rec ${i + 1}`, { lastUsed: i + 1 }));
    const listItems = vi.spyOn(api, 'listItems').mockResolvedValue([...items].reverse());
    render(<CommandPalette open onOpenChange={() => {}} />);
    await waitForItem('Rec 10');

    const options = screen.getAllByRole('option');
    expect(options.slice(0, 8).map((o) => o.textContent)).toEqual([10, 9, 8, 7, 6, 5, 4, 3].map((n) => expect.stringContaining(`Rec ${n}`)));
    expect(screen.getAllByTestId(/^category-icon-/)).toHaveLength(8);
    expect(screen.queryByText('Recently used')).toBeNull();
    expect(screen.getAllByText('Commands')).toHaveLength(1);
    expect(listItems).toHaveBeenCalledWith({ filter: { kind: 'all' } });
  });

  it('falls back to recently edited items when nothing was used yet', async () => {
    const older = item('i1', 'Older', { updatedAt: '2026-10-01T00:00:00Z' });
    const newer = item('i2', 'Newer', { updatedAt: '2026-10-05T00:00:00Z' });
    await renderPalette([older, newer]);
    expect(screen.getAllByRole('option').slice(0, 2).map((o) => o.textContent)).toEqual([expect.stringContaining('Newer'), expect.stringContaining('Older')]);
  });

  it('ranks matching items below the commands once a query is typed', async () => {
    await renderPalette([item('i1', 'Fake Lock')]);
    fireEvent.change(input(), { target: { value: 'lock' } });
    await waitFor(() => expect(api.listItems).toHaveBeenLastCalledWith({ filter: { kind: 'all', query: 'lock' } }));
    await waitForItem('Fake Lock');
    const headings = screen.getAllByText(/^(Items|Commands)$/).map((h) => h.textContent);
    expect(headings[0]).toBe('Commands');
  });
});

describe('item shortcuts in the palette', () => {
  it.each([
    ['password', { key: 'c', ctrlKey: true }, 'fp', 'Password copied'],
    ['username', { key: 'C', ctrlKey: true, shiftKey: true }, 'fu', 'Username copied'],
    ['one-time code', { key: 'c', ctrlKey: true, altKey: true }, 'ft', 'One-time code copied'],
  ])('copies the %s through the API, toasts and closes the palette', async (_label, init, fieldId, toast) => {
    const base = item('i1', 'Fake Mail');
    vi.spyOn(api, 'getItem').mockResolvedValue(detail(base));
    const copyField = vi.spyOn(api, 'copyField').mockResolvedValue(undefined);
    const onOpenChange = await renderPalette([base]);
    fireEvent.keyDown(input(), init);
    await waitFor(() => expect(copyField).toHaveBeenCalledWith({ vaultId: 'v1', itemId: 'i1', fieldId }));
    expect(onOpenChange).toHaveBeenCalledWith(false);
    await waitFor(() => expect(toastText()).toContain(toast));
  });

  it.each([
    ['⌘O', { key: 'o', ctrlKey: true }],
    ['⌘⇧O (item shortcut alias)', { key: 'O', ctrlKey: true, shiftKey: true }],
  ])('opens the item website with %s and closes the palette', async (_label, init) => {
    const base = item('i1', 'Fake Mail');
    vi.spyOn(api, 'getItem').mockResolvedValue(detail(base));
    const click = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => {});
    const onOpenChange = await renderPalette([base]);
    fireEvent.keyDown(input(), init);
    await waitFor(() => expect(click).toHaveBeenCalled());
    expect((click.mock.contexts[0] as HTMLAnchorElement).href).toBe('https://example.invalid/');
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it('opens the item on Enter (current behaviour)', async () => {
    const onOpenChange = await renderPalette([item('i1', 'Fake Mail')]);
    fireEvent.keyDown(input(), { key: 'Enter' });
    expect(onOpenChange).toHaveBeenCalledWith(false);
    expect(useApp.getState().route).toEqual({ name: 'items', filter: { kind: 'all' } });
    expect(useApp.getState().selectedItemId).toBe('i1');
  });

  it('shows the available shortcut hints for the selected item', async () => {
    await renderPalette([item('i1', 'Fake Mail')]);
    for (const key of ['copyPassword', 'copyUsername', 'copyTotp', 'openWebsite'] as const) expect(screen.getByText(translate('en', `itemKeys.${key}`))).toBeTruthy();
    expect(screen.getByText(translate('en', 'palette.openItem'))).toBeTruthy();
    expect(screen.getByText(translate('en', 'palette.actions'))).toBeTruthy();
    expect(document.body.textContent).not.toContain('FAKE-secret-never-in-dom');
  });
});

describe('item actions submenu', () => {
  it('opens with → or ⌘K, navigates with the keyboard and goes back with Esc / ←', async () => {
    await renderPalette([item('i1', 'Fake Mail')]);
    fireEvent.keyDown(input(), { key: 'ArrowRight' });
    expect(screen.getByText(translate('en', 'palette.actionsFor', { title: 'Fake Mail' }))).toBeTruthy();
    expect(screen.getAllByRole('option')).toHaveLength(4);
    expect(screen.getAllByRole('option')[0]).toHaveAttribute('aria-selected', 'true');
    fireEvent.keyDown(input(), { key: 'ArrowDown' });
    expect(screen.getAllByRole('option')[1]).toHaveAttribute('aria-selected', 'true');

    fireEvent.keyDown(input(), { key: 'Escape' });
    expect(screen.queryByText(translate('en', 'palette.actionsFor', { title: 'Fake Mail' }))).toBeNull();
    await waitForItem('Fake Mail');

    fireEvent.keyDown(input(), { key: 'k', ctrlKey: true });
    expect(screen.getByText(translate('en', 'palette.actionsFor', { title: 'Fake Mail' }))).toBeTruthy();
    fireEvent.keyDown(input(), { key: 'ArrowLeft' });
    expect(screen.queryByText(translate('en', 'palette.actionsFor', { title: 'Fake Mail' }))).toBeNull();
  });

  it('keeps the palette open when ⌘K opens the actions from the input', async () => {
    vi.spyOn(api, 'listItems').mockResolvedValue([item('i1', 'Fake Mail')]);
    render(<PaletteHost />);
    fireEvent.keyDown(window, { key: 'k', ctrlKey: true });
    await waitForItem('Fake Mail');
    fireEvent.keyDown(input(), { key: 'k', ctrlKey: true });
    expect(screen.getByText(translate('en', 'palette.actionsFor', { title: 'Fake Mail' }))).toBeTruthy();
    expect(screen.getByRole('combobox')).toBeTruthy();
  });

  it('runs the selected action with Enter and closes the palette', async () => {
    const base = item('i1', 'Fake Mail');
    vi.spyOn(api, 'getItem').mockResolvedValue(detail(base));
    const copyField = vi.spyOn(api, 'copyField').mockResolvedValue(undefined);
    const onOpenChange = await renderPalette([base]);
    fireEvent.keyDown(input(), { key: 'ArrowRight' });
    fireEvent.keyDown(input(), { key: 'ArrowDown' });
    fireEvent.keyDown(input(), { key: 'Enter' });
    await waitFor(() => expect(copyField).toHaveBeenCalledWith({ vaultId: 'v1', itemId: 'i1', fieldId: 'fu' }));
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });
});

describe('palette localization', () => {
  it.each(['my', 'th'] as const)('renders group headings, item actions and hints in %s', async (lang) => {
    useI18n.setState({ lang });
    const base = item('i1', 'Fake Mail');
    vi.spyOn(api, 'getItem').mockResolvedValue(detail(base));
    await renderPalette([base]);

    expect(screen.getByText(translate(lang, 'palette.commands'))).toBeTruthy();
    expect(screen.queryByText('Recently used')).toBeNull();
    expect(screen.queryByText('Commands')).toBeNull();

    fireEvent.keyDown(input(), { key: 'ArrowRight' });
    expect(screen.getByText(translate(lang, 'palette.actionsFor', { title: base.title }))).toBeTruthy();
    for (const key of ['copyPassword', 'copyUsername', 'copyTotp', 'openWebsite'] as const) expect(screen.getByText(translate(lang, `itemKeys.${key}`))).toBeTruthy();
    expect(screen.getByText(translate(lang, 'palette.back'))).toBeTruthy();
    expect(screen.queryByText('Copy password')).toBeNull();
  });
});
