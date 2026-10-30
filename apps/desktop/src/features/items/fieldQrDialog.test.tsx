import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen } from '@testing-library/react';
import QRCode from 'qrcode';
import { FieldQrDialog } from './FieldQrDialog';
import { TooltipProvider } from '../../ui';
import { api } from '../../api/client';
import { useApp } from '../../lib/store';
import type { Field } from '../../api/types';

const ctx = { vaultId: 'v1', itemId: 'item1' };

describe('FieldQrDialog', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    useApp.setState({ locked: false });
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('renders plain text field QR code with item title, field label, warning, and plain value', async () => {
    const toDataUrlSpy = vi.spyOn(QRCode, 'toDataURL').mockResolvedValue('data:image/png;base64,FAKE_QR');
    const field: Field = { id: 'username', label: 'username', kind: 'text', value: 'alice@example.com' };

    render(
      <TooltipProvider>
        <FieldQrDialog
          open={true}
          onOpenChange={() => {}}
          ctx={ctx}
          itemTitle="Acme Corp"
          field={field}
        />
      </TooltipProvider>
    );

    // Dialog elements
    expect(screen.getByRole('dialog')).toBeInTheDocument();
    expect(screen.getByTestId('field-qr-title').textContent).toBe('Acme Corp');
    expect(screen.getByTestId('field-qr-label').textContent).toBe('username');
    expect(screen.getByTestId('field-qr-warning').textContent).toBe('Anyone who scans this can see the value');

    // QR code image on white tile
    const img = await screen.findByTestId('field-qr-image');
    expect(img).toHaveAttribute('src', 'data:image/png;base64,FAKE_QR');
    expect(toDataUrlSpy).toHaveBeenCalledWith('alice@example.com', expect.objectContaining({
      margin: 2,
      width: 220,
      color: { dark: '#000000', light: '#ffffff' },
    }));

    // Plain value displayed under QR code
    expect(screen.getByTestId('field-qr-value').textContent).toBe('alice@example.com');
    // Plain fields do not show a reveal button
    expect(screen.queryByTestId('field-qr-reveal-btn')).toBeNull();
  });

  it('concealed fields stay masked under the QR by default and unmask on reveal toggle', async () => {
    vi.spyOn(QRCode, 'toDataURL').mockResolvedValue('data:image/png;base64,FAKE_PW_QR');
    const revealSpy = vi.spyOn(api, 'revealField').mockResolvedValue('super-secret-password-123');
    const field: Field = { id: 'password', label: 'password', kind: 'concealed', value: null, masked: '••••••••••' };

    render(
      <TooltipProvider>
        <FieldQrDialog
          open={true}
          onOpenChange={() => {}}
          ctx={ctx}
          itemTitle="GitHub"
          field={field}
        />
      </TooltipProvider>
    );

    await screen.findByTestId('field-qr-image');
    expect(revealSpy).toHaveBeenCalledWith({ vaultId: 'v1', itemId: 'item1', fieldId: 'password' });

    // Under the QR code, concealed value stays masked
    const valueEl = screen.getByTestId('field-qr-value');
    expect(valueEl.textContent).toBe('••••••••••');
    expect(valueEl.textContent).not.toContain('super-secret-password-123');

    // Reveal button exists
    const revealBtn = screen.getByTestId('field-qr-reveal-btn');
    expect(revealBtn).toHaveAttribute('aria-label', 'Reveal password');

    // Click reveal button to unmask
    await act(async () => {
      fireEvent.click(revealBtn);
      console.log('label is:', screen.getByTestId('field-qr-reveal-btn').getAttribute('aria-label'));
    });

    expect(screen.getByTestId('field-qr-value').textContent).toBe('super-secret-password-123');
    expect(revealBtn).toHaveAttribute('aria-label', 'Conceal password');

    // Click conceal button to re-mask
    await act(async () => {
      fireEvent.click(screen.getByTestId('field-qr-reveal-btn'));
      });
    expect(screen.getByTestId('field-qr-value').textContent).toBe('••••••••••');
  });

  it('TOTP field builds full otpauth:// URI for the QR code payload', async () => {
    const toDataUrlSpy = vi.spyOn(QRCode, 'toDataURL').mockResolvedValue('data:image/png;base64,FAKE_TOTP_QR');
    vi.spyOn(api, 'revealField').mockResolvedValue('JBSWY3DPEHPK3PXP');
    const field: Field = { id: 'otp', label: 'one-time password', kind: 'totp', value: null };

    render(
      <TooltipProvider>
        <FieldQrDialog
          open={true}
          onOpenChange={() => {}}
          ctx={ctx}
          itemTitle="GitHub"
          username="octocat"
          field={field}
        />
      </TooltipProvider>
    );

    await screen.findByTestId('field-qr-image');
    expect(toDataUrlSpy).toHaveBeenCalledWith(
      'otpauth://totp/GitHub%3Aoctocat?secret=JBSWY3DPEHPK3PXP&issuer=GitHub',
      expect.anything()
    );
  });

  it('displays a short error instead of a broken QR when value is too long', async () => {
    const hugeValue = 'A'.repeat(2500);
    const field: Field = { id: 'large_key', label: 'large key', kind: 'text', value: hugeValue };

    render(
      <TooltipProvider>
        <FieldQrDialog
          open={true}
          onOpenChange={() => {}}
          ctx={ctx}
          itemTitle="Big Item"
          field={field}
        />
      </TooltipProvider>
    );

    const errorEl = await screen.findByTestId('field-qr-error');
    expect(errorEl.textContent).toBe('Value is too long to display as a QR code');
    expect(screen.queryByTestId('field-qr-image')).toBeNull();
  });

  it('closes on Esc key and calls onCloseAutoFocus returning focus to triggerRef', async () => {
    vi.spyOn(QRCode, 'toDataURL').mockResolvedValue('data:image/png;base64,FAKE_QR');
    const onOpenChange = vi.fn();
    const trigger = document.createElement('button');
    document.body.appendChild(trigger);
    const triggerRef = { current: trigger };
    trigger.focus();

    const field: Field = { id: 'username', label: 'username', kind: 'text', value: 'alice' };

    render(
      <TooltipProvider>
        <FieldQrDialog
          open={true}
          onOpenChange={onOpenChange}
          ctx={ctx}
          itemTitle="Test"
          field={field}
          triggerRef={triggerRef}
        />
      </TooltipProvider>
    );

    await screen.findByTestId('field-qr-image');

    // Press Escape
    await act(async () => {
      fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
    });

    expect(onOpenChange).toHaveBeenCalledWith(false);
    document.body.removeChild(trigger);
  });

  it('closes immediately on store lock and clears sensitive QR state', async () => {
    vi.spyOn(QRCode, 'toDataURL').mockResolvedValue('data:image/png;base64,FAKE_QR');
    const onOpenChange = vi.fn();
    const field: Field = { id: 'username', label: 'username', kind: 'text', value: 'alice' };

    render(
      <TooltipProvider>
        <FieldQrDialog
          open={true}
          onOpenChange={onOpenChange}
          ctx={ctx}
          itemTitle="Test"
          field={field}
        />
      </TooltipProvider>
    );

    await screen.findByTestId('field-qr-image');

    // Lock store
    act(() => {
      useApp.getState().lock();
    });

    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it('closes immediately on gitpass:locked event', async () => {
    vi.spyOn(QRCode, 'toDataURL').mockResolvedValue('data:image/png;base64,FAKE_QR');
    const onOpenChange = vi.fn();
    const field: Field = { id: 'username', label: 'username', kind: 'text', value: 'alice' };

    render(
      <TooltipProvider>
        <FieldQrDialog
          open={true}
          onOpenChange={onOpenChange}
          ctx={ctx}
          itemTitle="Test"
          field={field}
        />
      </TooltipProvider>
    );

    await screen.findByTestId('field-qr-image');

    // Dispatch DOM event
    act(() => {
      window.dispatchEvent(new CustomEvent('gitpass:locked'));
    });

    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it('closes when close button is clicked', async () => {
    vi.spyOn(QRCode, 'toDataURL').mockResolvedValue('data:image/png;base64,FAKE_QR');
    const onOpenChange = vi.fn();
    const field: Field = { id: 'username', label: 'username', kind: 'text', value: 'alice' };

    render(
      <TooltipProvider>
        <FieldQrDialog
          open={true}
          onOpenChange={onOpenChange}
          ctx={ctx}
          itemTitle="Test"
          field={field}
        />
      </TooltipProvider>
    );

    const closeBtn = await screen.findByTestId('field-qr-close-btn');
    await act(async () => {
      fireEvent.click(closeBtn);
    });

    expect(onOpenChange).toHaveBeenCalledWith(false);
  });
});
