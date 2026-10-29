import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, act } from '@testing-library/react';
import { TooltipProvider } from '../../ui';
import { FieldRow } from './FieldView';
import { CategoryIcon } from '../../components/CategoryIcon';
import { ItemIcon, catMeta } from './categories';
import { getCategoryLabel } from './categoryLabel';
import { api } from '../../api/client';
import { translate } from '../../i18n';

describe('BankAccount and Server categories', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('renders CategoryIcon for bank_account and server with test ids', () => {
    const { rerender } = render(<CategoryIcon category="bank_account" size={36} />);
    expect(screen.getByTestId('category-icon-bank_account')).toBeInTheDocument();

    rerender(<CategoryIcon category="server" size={36} />);
    expect(screen.getByTestId('category-icon-server')).toBeInTheDocument();
  });

  it('ItemIcon renders CategoryIcon for bank_account and server', () => {
    const { rerender } = render(<ItemIcon item={{ category: 'bank_account', title: 'Chase Checking', subtitle: '•••• 1234' }} />);
    expect(screen.getByTestId('category-icon-bank_account')).toBeInTheDocument();

    rerender(<ItemIcon item={{ category: 'server', title: 'Prod Node', subtitle: 'root@10.0.0.1' }} />);
    expect(screen.getByTestId('category-icon-server')).toBeInTheDocument();
  });

  it('localized category labels in en, my, th', () => {
    const tEn = (k: any) => translate('en', k);
    const tMy = (k: any) => translate('my', k);
    const tTh = (k: any) => translate('th', k);

    expect(getCategoryLabel('bank_account', tEn)).toBe('Bank Account');
    expect(getCategoryLabel('bank_account', tMy)).toBe('ဘဏ်အကောင့်');
    expect(getCategoryLabel('bank_account', tTh)).toBe('บัญชีธนาคาร');

    expect(getCategoryLabel('server', tEn)).toBe('Server');
    expect(getCategoryLabel('server', tMy)).toBe('ဆာဗာ');
    expect(getCategoryLabel('server', tTh)).toBe('เซิร์ฟเวอร์');
  });

  it('catMeta returns valid labels and colors', () => {
    const bank = catMeta('bank_account');
    expect(bank.label).toBe('Bank Account');
    expect(bank.id).toBe('bank_account');

    const srv = catMeta('server');
    expect(srv.label).toBe('Server');
    expect(srv.id).toBe('server');
  });

  it('field row masks account number and reveals on click', async () => {
    const ctx = { vaultId: 'v1', itemId: 'i1' };
    const spy = vi.spyOn(api, 'revealField').mockResolvedValue('123456789012');
    render(
      <TooltipProvider>
        <FieldRow
          field={{ id: 'account_number', label: 'account number', kind: 'concealed', value: null, masked: '•••• •••• •••• 9012' }}
          ctx={ctx}
          category="bank_account"
        />
      </TooltipProvider>
    );

    expect(screen.getByTestId('concealed-value').textContent).toBe('•••• •••• •••• 9012');
    await act(async () => {
      fireEvent.click(screen.getByLabelText('Reveal account number'));
    });
    expect(spy).toHaveBeenCalledWith({ vaultId: 'v1', itemId: 'i1', fieldId: 'account_number' });
    expect(screen.getByTestId('concealed-value').textContent).toBe('1234 5678 9012');
  });
});
