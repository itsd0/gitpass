import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, render, screen } from '@testing-library/react';
import { api } from '../../api/client';
import type { VaultInfo } from '../../api/types';
import { useI18n } from '../../i18n';
import { HomePage } from './HomePage';
import { DeleteVaultDialog } from './VaultDialogs';

const PRIVATE: VaultInfo = { id: 'v1', name: 'Work', description: null, itemCount: 3, shared: false, members: [], icon: 'vault', color: '#64748b' };
const SHARED: VaultInfo = { id: 'v2', name: 'Team', description: null, itemCount: 1, shared: true, members: [], icon: 'vault', color: '#64748b' };
const DISMISSED = { extensionInstalled: false, emergencyKitSaved: false, repoConnected: true, imported: false, dismissed: true };

afterEach(() => { cleanup(); vi.restoreAllMocks(); useI18n.getState().setLang('en'); });

describe('home i18n (#177)', () => {
  it('renders vault cards localized in Burmese, with plural-aware counts', async () => {
    vi.spyOn(api, 'listVaults').mockResolvedValue([PRIVATE, SHARED]);
    vi.spyOn(api, 'onboarding').mockResolvedValue(DISMISSED);
    useI18n.getState().setLang('my');
    render(<HomePage />);
    expect(await screen.findByText('သော့ခတ်သေတ္တာအားလုံး')).toBeTruthy();
    expect(screen.getByText('သင့်သော့ခတ်သေတ္တာအားလုံးရှိ item များကို တစ်နေရာတည်းတွင် အတူတကွ ကြည့်ရှုပါ။')).toBeTruthy();
    expect(screen.getByText('item 3 ခု')).toBeTruthy();
    expect(screen.getByText('item 1 ခု')).toBeTruthy();
    expect(screen.getByText('သင်တစ်ဦးတည်းသာ ဤသော့ခတ်သေတ္တာရှိ item များကို ကြည့်ရှုပြီး ပြင်ဆင်နိုင်ပါသည်။')).toBeTruthy();
    expect(screen.getByText('မျှဝေထားသော သော့ခတ်သေတ္တာ။')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'သော့ခတ်သေတ္တာအားလုံး ကို ဖွင့်ရန်' })).toBeTruthy();
    expect(screen.getAllByRole('button', { name: 'သော့ခတ်သေတ္တာ ဆက်တင်များ' })).toHaveLength(2);
    expect(screen.getAllByRole('button', { name: 'သော့ခတ်သေတ္တာ ဖွင့်ရန်' })).toHaveLength(3);
    expect(screen.getByRole('heading', { name: 'သော့ခတ်သေတ္တာများ' })).toBeTruthy();
  });

  it('renders vault cards localized in Thai', async () => {
    vi.spyOn(api, 'listVaults').mockResolvedValue([PRIVATE]);
    vi.spyOn(api, 'onboarding').mockResolvedValue(DISMISSED);
    useI18n.getState().setLang('th');
    render(<HomePage />);
    expect(await screen.findByText('ห้องนิรภัยทั้งหมด')).toBeTruthy();
    expect(screen.getByText('3 รายการ')).toBeTruthy();
    expect(screen.getByText('เฉพาะคุณเท่านั้นที่ดูและแก้ไขรายการในห้องนิรภัยนี้ได้')).toBeTruthy();
  });

  it('localizes the delete-vault confirm-name aria label', () => {
    useI18n.getState().setLang('th');
    render(<DeleteVaultDialog vault={PRIVATE} isLast={false} onOpenChange={() => {}} onDeleted={() => {}} />);
    expect(screen.getByLabelText('ยืนยันชื่อห้องนิรภัย')).toBeTruthy();
  });
});
