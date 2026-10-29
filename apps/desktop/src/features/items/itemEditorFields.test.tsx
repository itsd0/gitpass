import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';

import { api } from '../../api/client';
import type { Field, ItemDetail, Section, VaultInfo } from '../../api/types';
import { useI18n } from '../../i18n';
import { defaultRoute, useApp } from '../../lib/store';
import { TooltipProvider, useToasts } from '../../ui';
import { ItemEditor } from './ItemEditor';

const vaults: VaultInfo[] = [{ id: 'vault-1', name: 'Personal', description: null, itemCount: 1, shared: false, members: [] }];
const f = (id: string, label: string, kind: Field['kind'] = 'text', value: string | null = ''): Field => ({ id, label, kind, value });

function makeDetail(sections?: Section[]): ItemDetail {
  return {
    id: 'item-1', vaultId: 'vault-1', category: 'secure_note', title: 'Fake item', subtitle: '', favorite: false, archived: false,
    deletedAt: null, tags: [], updatedAt: 'x', createdAt: 'x',
    sections: sections ?? [{ id: 's1', title: null, fields: [f('f1', 'Alpha'), f('f2', 'Beta'), f('f3', 'Gamma')] }],
    notes: null, attachments: [], history: [], hasTotp: false,
  };
}

const labels = () => screen.getAllByLabelText(/^(Field label|ฟิลด์|ป้ายชื่อช่อง|အကွက်အမည်)$/).map((e) => (e as HTMLInputElement).value);

function renderEditor(detail = makeDetail()) {
  return render(<TooltipProvider><ItemEditor detail={detail} vaults={vaults} /></TooltipProvider>);
}

async function addField(name: string) {
  fireEvent.keyDown(screen.getAllByRole('button', { name: /^(Add field|เพิ่มช่อง|အကွက်ထည့်မည်)$/ })[0], { key: 'Enter', code: 'Enter' });
  fireEvent.click(await screen.findByRole('menuitem', { name }));
}

describe('Item editor: add field menu, reorder and undo', () => {
  beforeEach(() => {
    vi.spyOn(api, 'passwordStrength').mockResolvedValue(80);
    act(() => {
      useApp.setState({ locked: false, route: defaultRoute, modal: null, editing: true });
      useI18n.setState({ lang: 'en' });
      useToasts.setState({ list: [] });
    });
  });
  afterEach(() => { cleanup(); vi.restoreAllMocks(); });

  it('lists the nine field types in the Add field menu', async () => {
    renderEditor();
    fireEvent.keyDown(screen.getByRole('button', { name: 'Add field' }), { key: 'Enter', code: 'Enter' });
    const items = await screen.findAllByRole('menuitem');
    expect(items.map((i) => i.textContent)).toEqual(['Text', 'Password', 'URL', 'Email', 'Phone', 'Date', 'Month/Year', 'One-time password', 'Multi-line text']);
  });

  it.each([
    ['Password', 'concealed'], ['URL', 'url'], ['Email', 'email'], ['Phone', 'phone'], ['Date', 'date'],
    ['Month/Year', 'month_year'], ['One-time password', 'totp'], ['Multi-line text', 'multiline'], ['Text', 'text'],
  ])('menu item %s adds a %s field with the default label and focuses the label', async (name, kind) => {
    renderEditor();
    await addField(name);
    const rows = screen.getAllByTestId('field-editor');
    expect(rows).toHaveLength(4);
    const row = rows[3];
    expect((within(row).getByLabelText('Field type') as HTMLSelectElement).value).toBe(kind);
    const label = within(row).getByLabelText('Field label') as HTMLInputElement;
    expect(label.value).toBe(name);
    await waitFor(() => expect(document.activeElement).toBe(label));
    expect(label.selectionStart).toBe(0);
    expect(label.selectionEnd).toBe(name.length);
  });

  it('Alt+Down / Alt+Up reorders a field and keeps focus on it', async () => {
    renderEditor();
    const first = screen.getAllByLabelText('Field label')[0] as HTMLInputElement;
    first.focus();
    fireEvent.keyDown(first, { key: 'ArrowDown', altKey: true });
    expect(labels()).toEqual(['Beta', 'Alpha', 'Gamma']);
    expect((document.activeElement as HTMLInputElement).value).toBe('Alpha');
    expect(screen.getByTestId('editor-announce').textContent).toBe('Moved to position 2 of 3');
    fireEvent.keyDown(document.activeElement!, { key: 'ArrowUp', altKey: true });
    expect(labels()).toEqual(['Alpha', 'Beta', 'Gamma']);
    // At the edge nothing moves; a plain arrow key never reorders.
    fireEvent.keyDown(document.activeElement!, { key: 'ArrowUp', altKey: true });
    fireEvent.keyDown(document.activeElement!, { key: 'ArrowDown' });
    expect(labels()).toEqual(['Alpha', 'Beta', 'Gamma']);
  });

  it('reorders by dragging the handle onto another field in the same section', () => {
    renderEditor();
    const handles = screen.getAllByTestId('field-handle');
    const rows = screen.getAllByTestId('field-editor');
    fireEvent.dragStart(handles[0]);
    fireEvent.dragOver(rows[2]);
    expect(rows[2].className).toContain('border-accent');
    fireEvent.drop(rows[2]);
    expect(labels()).toEqual(['Beta', 'Gamma', 'Alpha']);
  });

  it('reorders sections by keyboard on the section handle and by drag', () => {
    renderEditor(makeDetail([
      { id: 's1', title: 'One', fields: [] }, { id: 's2', title: 'Two', fields: [] }, { id: 's3', title: 'Three', fields: [] },
    ]));
    const titles = () => screen.getAllByLabelText('Section title (optional)').map((e) => (e as HTMLInputElement).value);
    const handles = screen.getAllByTestId('section-handle');
    handles[0].focus();
    fireEvent.keyDown(handles[0], { key: 'ArrowDown', altKey: true });
    expect(titles()).toEqual(['Two', 'One', 'Three']);
    const secs = screen.getAllByTestId('section-editor');
    fireEvent.dragStart(screen.getAllByTestId('section-handle')[2]);
    fireEvent.dragOver(secs[0]);
    fireEvent.drop(secs[0]);
    expect(titles()).toEqual(['Three', 'Two', 'One']);
  });

  it('removing a field shows an Undo toast that restores it in place', () => {
    renderEditor();
    fireEvent.click(screen.getAllByRole('button', { name: 'Remove field' })[1]);
    expect(labels()).toEqual(['Alpha', 'Gamma']);
    const toasts = useToasts.getState().list;
    expect(toasts).toHaveLength(1);
    expect(toasts[0].title).toBe('Field removed');
    expect(toasts[0].action?.label).toBe('Undo');
    act(() => { void toasts[0].action!.onClick(); });
    expect(labels()).toEqual(['Alpha', 'Beta', 'Gamma']);
  });

  it('removing a section can be undone at its original position', () => {
    renderEditor(makeDetail([
      { id: 's1', title: 'One', fields: [f('a', 'A')] }, { id: 's2', title: 'Two', fields: [f('b', 'B')] }, { id: 's3', title: 'Three', fields: [] },
    ]));
    fireEvent.click(screen.getAllByRole('button', { name: 'Remove section' })[1]);
    expect(screen.getAllByTestId('section-editor')).toHaveLength(2);
    const t = useToasts.getState().list[0];
    expect(t.title).toBe('Section removed');
    act(() => { void t.action!.onClick(); });
    const secs = screen.getAllByTestId('section-editor');
    expect(secs.map((s) => s.getAttribute('data-section-id'))).toEqual(['s1', 's2', 's3']);
    expect(within(secs[1]).getByLabelText('B value')).toBeTruthy();
  });

  it('One-time password field previews a code for a base32 secret and an otpauth link', async () => {
    renderEditor();
    await addField('One-time password');
    const row = screen.getAllByTestId('field-editor')[3];
    const input = within(row).getByLabelText('One-time password value') as HTMLInputElement;
    fireEvent.change(input, { target: { value: 'GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ' } });
    await waitFor(() => expect(within(row).getByTestId('totp-code').textContent).toMatch(/^\d{6}$/));
    fireEvent.change(input, { target: { value: 'otpauth://totp/Fake:me?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ&digits=8' } });
    await waitFor(() => expect(within(row).getByTestId('totp-code').textContent).toMatch(/^\d{8}$/));
  });

  it('One-time password field shows an inline error for invalid input', async () => {
    renderEditor();
    await addField('One-time password');
    const row = screen.getAllByTestId('field-editor')[3];
    const input = within(row).getByLabelText('One-time password value');
    fireEvent.change(input, { target: { value: 'not a secret!' } });
    expect((await within(row).findByRole('alert')).textContent).toBe('Enter an otpauth:// link or a base32 secret');
    expect(input.getAttribute('aria-invalid')).toBe('true');
    expect(within(row).queryByTestId('totp-code')).toBeNull();
  });

  it.each([
    ['my', 'အကွက်ထည့်မည်', 'စကားဝှက်', 'အပိုင်းထည့်မည်'],
    ['th', 'เพิ่มช่อง', 'รหัสผ่าน', 'เพิ่มส่วน'],
  ] as const)('renders the editor strings in %s', async (lang, addLabel, passwordType, addSection) => {
    act(() => { useI18n.setState({ lang }); });
    renderEditor();
    expect(screen.getByRole('button', { name: addLabel })).toBeTruthy();
    expect(screen.getByRole('button', { name: addSection })).toBeTruthy();
    fireEvent.keyDown(screen.getByRole('button', { name: addLabel }), { key: 'Enter', code: 'Enter' });
    expect(await screen.findByRole('menuitem', { name: passwordType })).toBeTruthy();
    // No English leftovers from the previously hard-coded strings.
    for (const en of ['Add field', 'Add section', 'Section title (optional)', 'Website', 'Tags (comma separated)']) {
      expect(screen.queryByText(en)).toBeNull();
      expect(screen.queryByLabelText(en)).toBeNull();
    }
  });
});
