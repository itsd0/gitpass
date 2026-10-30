import QRCode from 'qrcode';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, act } from '@testing-library/react';
import { TooltipProvider } from '../../ui';
import { FieldRow, REVEAL_MS } from './FieldView';
import { api } from '../../api/client';
import { useApp } from '../../lib/store';
import { useI18n } from '../../i18n';
import type { Field } from '../../api/types';

const SECRET = 'super-secret-FAKE';
const COMPLEX_SECRET = 'aB3!9#';
const ctx = { vaultId: 'v', itemId: 'i' };
const wrap = (f: Field) => render(<TooltipProvider><FieldRow field={f} ctx={ctx} /></TooltipProvider>);

describe('FieldRow', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    useApp.setState({ locked: false });
  });

  it('renders plain text values', () => {
    wrap({ id: 'f1', label: 'username', kind: 'text', value: 'demo' });
    expect(screen.getByText('username')).toBeInTheDocument();
    expect(screen.getByText('demo')).toBeInTheDocument();
  });

  it('concealed never shown without reveal, even if payload wrongly contains a value', () => {
    const spy = vi.spyOn(api, 'revealField');
    const { container } = wrap({ id: 'f2', label: 'password', kind: 'concealed', value: SECRET });
    expect(container.textContent).not.toContain(SECRET);
    expect(screen.getByTestId('concealed-value').textContent).toMatch(/^•+$/);
    expect(spy).not.toHaveBeenCalled();
  });

  it('reveal calls revealField, shows value, then auto-hides after 30s', async () => {
    vi.useFakeTimers();
    const spy = vi.spyOn(api, 'revealField').mockResolvedValue(SECRET);
    wrap({ id: 'f3', label: 'password', kind: 'concealed', value: null });
    await act(async () => { fireEvent.click(screen.getByLabelText('Reveal password')); });
    expect(spy).toHaveBeenCalledWith({ vaultId: 'v', itemId: 'i', fieldId: 'f3' });
    expect(screen.getByTestId('concealed-value').textContent).toBe(SECRET);
    act(() => { vi.advanceTimersByTime(REVEAL_MS + 10); });
    expect(screen.getByTestId('concealed-value').textContent).not.toContain(SECRET);
    vi.useRealTimers();
  });

  it('copy goes through api.copyField (Rust), not the DOM', async () => {
    const spy = vi.spyOn(api, 'copyField').mockResolvedValue(undefined);
    wrap({ id: 'f4', label: 'password', kind: 'concealed', value: null });
    await act(async () => { fireEvent.click(screen.getByLabelText('Copy password')); });
    expect(spy).toHaveBeenCalledWith({ vaultId: 'v', itemId: 'i', fieldId: 'f4' });
  });

  it('non-concealed fields do not show the large type action', () => {
    wrap({ id: 'f_user', label: 'username', kind: 'text', value: 'alice' });
    expect(screen.queryByTestId('large-type-trigger')).toBeNull();
    expect(screen.queryByLabelText(/large type/i)).toBeNull();

    render(<TooltipProvider><FieldRow field={{ id: 'f_note', label: 'notes', kind: 'multiline', value: 'some note' }} ctx={ctx} /></TooltipProvider>);
    expect(screen.queryByTestId('large-type-trigger')).toBeNull();
  });

  it('opening from a concealed field renders one cell per character with position numbers and expected classes', async () => {
    vi.spyOn(api, 'revealField').mockResolvedValue(COMPLEX_SECRET);
    wrap({ id: 'f_pw', label: 'password', kind: 'concealed', value: null });

    const trigger = screen.getByTestId('large-type-trigger');
    expect(trigger).toBeInTheDocument();
    expect(trigger).toHaveAttribute('aria-label', 'Show password in large type');

    await act(async () => {
      fireEvent.click(trigger);
    });

    // Dialog is visible
    expect(screen.getByRole('dialog')).toBeInTheDocument();

    // 6 characters in COMPLEX_SECRET ('aB3!9#')
    const cells = screen.getAllByTestId('large-type-char');
    expect(cells).toHaveLength(6);

    const positions = screen.getAllByTestId('large-type-pos');
    expect(positions).toHaveLength(6);
    expect(positions.map((p) => p.textContent)).toEqual(['1', '2', '3', '4', '5', '6']);

    // Check classification and classes:
    // a -> letter
    expect(cells[0].getAttribute('data-kind')).toBe('letter');
    expect(cells[0].querySelector('span')!.className).toContain('large-type-letter');
    expect(cells[0].querySelector('span')!.className).toContain('text-fg');

    // B -> letter
    expect(cells[1].getAttribute('data-kind')).toBe('letter');

    // 3 -> digit
    expect(cells[2].getAttribute('data-kind')).toBe('digit');
    expect(cells[2].querySelector('span')!.className).toContain('large-type-digit');
    expect(cells[2].querySelector('span')!.className).toContain('text-accent');

    // ! -> symbol
    expect(cells[3].getAttribute('data-kind')).toBe('symbol');
    expect(cells[3].querySelector('span')!.className).toContain('large-type-symbol');
    expect(cells[3].querySelector('span')!.className).toContain('text-blue-600');

    // 9 -> digit
    expect(cells[4].getAttribute('data-kind')).toBe('digit');
    expect(cells[4].querySelector('span')!.className).toContain('large-type-digit');
    expect(cells[4].querySelector('span')!.className).toContain('text-accent');

    // # -> symbol
    expect(cells[5].getAttribute('data-kind')).toBe('symbol');
    expect(cells[5].querySelector('span')!.className).toContain('large-type-symbol');
    expect(cells[5].querySelector('span')!.className).toContain('text-blue-600');
  });

  it('Esc closes large type and returns focus to the trigger', async () => {
    vi.spyOn(api, 'revealField').mockResolvedValue(SECRET);
    wrap({ id: 'f_pw', label: 'password', kind: 'concealed', value: null });

    const trigger = screen.getByTestId('large-type-trigger');
    trigger.focus();
    expect(document.activeElement).toBe(trigger);

    await act(async () => {
      fireEvent.click(trigger);
    });

    expect(screen.getByRole('dialog')).toBeInTheDocument();

    // Press Escape
    await act(async () => {
      fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
    });

    expect(screen.queryByRole('dialog')).toBeNull();
    expect(document.activeElement).toBe(trigger);
  });

  it('lock event closes the large type view', async () => {
    vi.spyOn(api, 'revealField').mockResolvedValue(SECRET);
    wrap({ id: 'f_pw', label: 'password', kind: 'concealed', value: null });

    await act(async () => {
      fireEvent.click(screen.getByTestId('large-type-trigger'));
    });

    expect(screen.getByRole('dialog')).toBeInTheDocument();

    // Dispatch DOM locked event
    act(() => {
      window.dispatchEvent(new CustomEvent('gitpass:locked'));
    });

    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('store lock closes the large type view', async () => {
    vi.spyOn(api, 'revealField').mockResolvedValue(SECRET);
    wrap({ id: 'f_pw', label: 'password', kind: 'concealed', value: null });

    await act(async () => {
      fireEvent.click(screen.getByTestId('large-type-trigger'));
    });

    expect(screen.getByRole('dialog')).toBeInTheDocument();

    // Trigger store lock
    act(() => {
      useApp.getState().lock();
    });

    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('⌥⌘L shortcut opens large type when the field is focused', async () => {
    const spy = vi.spyOn(api, 'revealField').mockResolvedValue('xyz789!');
    wrap({ id: 'f_shortcut', label: 'API secret', kind: 'concealed', value: null });

    const fieldContainer = screen.getByTestId('concealed-field-f_shortcut');
    fieldContainer.focus();

    await act(async () => {
      fireEvent.keyDown(fieldContainer, { key: 'l', altKey: true, metaKey: true });
    });

    expect(spy).toHaveBeenCalledWith({ vaultId: 'v', itemId: 'i', fieldId: 'f_shortcut' });
    expect(screen.getByRole('dialog')).toBeInTheDocument();

    const cells = screen.getAllByTestId('large-type-char');
    expect(cells).toHaveLength(7);

    // Close with Escape and check focus returns to field container
    await act(async () => {
      fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
    });
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(document.activeElement).toBe(fieldContainer);
  });

  it("shows expired pill and relative hint for expired credit card expiry field", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-05-15T12:00:00Z"));
    render(
      <TooltipProvider>
        <FieldRow
          field={{ id: "expiry", label: "expiry date", kind: "month_year", value: "03/2026" }}
          ctx={ctx}
          category="credit_card"
        />
      </TooltipProvider>
    );
    const pill = screen.getByTestId("expiry-pill");
    expect(pill).toBeInTheDocument();
    expect(pill.textContent).toBe("Expired");
    expect(screen.getByTestId("expiry-hint").textContent).toBe("Expired 2 months ago");
    vi.useRealTimers();
  });

  it("shows expires soon pill and relative hint for expiring api credential expires field", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-05-15T12:00:00Z"));
    render(
      <TooltipProvider>
        <FieldRow
          field={{ id: "expires", label: "expires", kind: "date", value: "2026-05-27" }}
          ctx={ctx}
          category="api_credential"
        />
      </TooltipProvider>
    );
    const pill = screen.getByTestId("expiry-pill");
    expect(pill).toBeInTheDocument();
    expect(pill.textContent).toBe("Expires soon");
    expect(screen.getByTestId("expiry-hint").textContent).toBe("Expires in 12 days");
    vi.useRealTimers();
  });

  it("does not show expiry pill for non-expiry date fields such as birth date", () => {
    render(
      <TooltipProvider>
        <FieldRow
          field={{ id: "birth", label: "birth date", kind: "date", value: "1990-01-01" }}
          ctx={ctx}
          category="identity"
        />
      </TooltipProvider>
    );
    expect(screen.queryByTestId("expiry-pill")).toBeNull();
    expect(screen.queryByTestId("expiry-hint")).toBeNull();
  });

  it("shows relative password changed hint under the password field", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-10-28T12:00:00Z"));
    render(
      <TooltipProvider>
        <FieldRow
          field={{ id: "f_pw", label: "password", kind: "concealed", value: null }}
          ctx={ctx}
          category="login"
          passwordChangedAt="2026-10-27T12:00:00Z"
        />
      </TooltipProvider>
    );
    const hint = screen.getByTestId("password-changed-at");
    expect(hint).toBeInTheDocument();
    expect(hint.textContent).toBe("Password changed 1 day ago");
    vi.useRealTimers();
  });

  it("does not show password changed hint when passwordChangedAt is not provided", () => {
    render(
      <TooltipProvider>
        <FieldRow
          field={{ id: "f_pw", label: "password", kind: "concealed", value: null }}
          ctx={ctx}
          category="login"
        />
      </TooltipProvider>
    );
    expect(screen.queryByTestId("password-changed-at")).toBeNull();
  });
});

describe('FieldRow i18n + a11y (#182)', () => {
  beforeEach(() => { vi.restoreAllMocks(); });
  afterEach(() => { useI18n.getState().setLang('en'); });

  it('strength meter localizes label and accessible name in Burmese', () => {
    useI18n.getState().setLang('my');
    const { container } = render(
      <TooltipProvider><FieldRow field={{ id: 'f_pw', label: 'password', kind: 'concealed', value: null }} ctx={ctx} strength={85} /></TooltipProvider>
    );
    const meter = screen.getByRole('meter');
    expect(meter).toHaveAttribute('aria-valuenow', '85');
    expect(meter).toHaveAttribute('aria-label', 'စကားဝှက် အားကောင်းမှု: အလွန်ကောင်း');
    expect(meter.textContent).toContain('အလွန်ကောင်း');
    // Same bar layout and colours: all 5 bars lit bg-ok for the top bucket.
    expect(meter.querySelectorAll('.bg-ok')).toHaveLength(5);
    expect(container.querySelector('[data-testid="strength-meter"]')).toBe(meter);
  });

  it('strength meter localizes label and accessible name in Thai', () => {
    useI18n.getState().setLang('th');
    render(
      <TooltipProvider><FieldRow field={{ id: 'f_pw', label: 'password', kind: 'concealed', value: null }} ctx={ctx} strength={65} /></TooltipProvider>
    );
    const meter = screen.getByRole('meter');
    expect(meter).toHaveAttribute('aria-valuenow', '65');
    expect(meter).toHaveAttribute('aria-label', 'ความแข็งแกร่งของรหัสผ่าน: ดี');
    expect(meter.textContent).toContain('ดี');
    // 4 of 5 bars lit for the Good bucket; the last bar stays bg-active.
    expect(meter.querySelectorAll('.bg-ok')).toHaveLength(4);
    expect(meter.querySelectorAll('.bg-active')).toHaveLength(1);
  });

  it('URL open action localizes its accessible name in Burmese and Thai', () => {
    useI18n.getState().setLang('my');
    const { unmount } = render(
      <TooltipProvider><FieldRow field={{ id: 'f_url', label: 'website', kind: 'url', value: 'https://example.invalid' }} ctx={ctx} /></TooltipProvider>
    );
    expect(screen.getByLabelText('လင့်ခ်ဖွင့်ရန်')).toHaveAttribute('href', 'https://example.invalid');
    unmount();
    useI18n.getState().setLang('th');
    render(
      <TooltipProvider><FieldRow field={{ id: 'f_url', label: 'website', kind: 'url', value: 'https://example.invalid' }} ctx={ctx} /></TooltipProvider>
    );
    expect(screen.getByLabelText('เปิดลิงก์')).toHaveAttribute('href', 'https://example.invalid');
  });

  it('TOTP ring is aria-hidden and the remaining seconds are exposed once on the code, localized', async () => {
    vi.spyOn(api, 'totpNow').mockResolvedValue({ code: '123456', remaining: 25, period: 30 });
    useI18n.getState().setLang('my');
    const { container } = render(
      <TooltipProvider><FieldRow field={{ id: 'f_totp', label: 'one-time password', kind: 'totp', value: null }} ctx={ctx} /></TooltipProvider>
    );
    const code = await screen.findByLabelText('123456 25 စက္ကန့် ကျန်သေးသည်');
    expect(code.textContent).toBe('123 456');
    // Ring and ticking seconds text are hidden from assistive tech: no live announcements.
    expect(container.querySelector('svg')).toHaveAttribute('aria-hidden', 'true');
    expect(screen.getByText('25s')).toHaveAttribute('aria-hidden', 'true');
    expect(container.querySelector('[aria-live]')).toBeNull();
    expect(container.querySelector('[aria-label$="seconds remaining"]')).toBeNull();
  });

  it('TOTP remaining label localizes in Thai', async () => {
    vi.spyOn(api, 'totpNow').mockResolvedValue({ code: '123456', remaining: 12, period: 30 });
    useI18n.getState().setLang('th');
    render(
      <TooltipProvider><FieldRow field={{ id: 'f_totp', label: 'one-time password', kind: 'totp', value: null }} ctx={ctx} /></TooltipProvider>
    );
    const code = await screen.findByLabelText('123456 เหลืออีก 12 วินาที');
    expect(code.textContent).toBe('123 456');
  });

  it('copies a secret reference (not the value) without touching copyField', async () => {
    const write = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', { value: { writeText: write }, configurable: true });
    const copyField = vi.spyOn(api, 'copyField').mockResolvedValue(undefined);
    const fields = [{ id: 'username', label: 'username' }, { id: 'password', label: 'password' }];
    const target = { vault: 'My Vault', item: 'Prod DB', fields };
    const pw: Field = { id: 'password', label: 'password', kind: 'concealed', value: null };
    render(<TooltipProvider><FieldRow field={pw} ctx={ctx} refTarget={target} /></TooltipProvider>);
    await act(async () => { fireEvent.click(screen.getByLabelText('Copy secret reference for password')); });
    expect(write).toHaveBeenCalledWith('gp://My%20Vault/Prod%20DB/password');
    expect(copyField).not.toHaveBeenCalled();
  });

  it('shows no reference action without a target', () => {
    wrap({ id: 'username', label: 'username', kind: 'text', value: 'demo' });
    expect(screen.queryByLabelText(/secret reference/i)).toBeNull();
  });
});

describe('FieldRow Show as QR code (#346)', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    useApp.setState({ locked: false });
    vi.spyOn(QRCode, 'toDataURL').mockResolvedValue('data:image/png;base64,FAKE_QR');
  });

  it('renders QR code trigger for supported fields (password, username, website, totp, api credential, custom text, custom concealed)', () => {
    const pw: Field = { id: 'password', label: 'password', kind: 'concealed', value: null };
    const { unmount: u1 } = render(<TooltipProvider><FieldRow field={pw} ctx={ctx} category="login" /></TooltipProvider>);
    expect(screen.getByTestId('show-qr-trigger')).toBeInTheDocument();
    u1();

    const user: Field = { id: 'username', label: 'username', kind: 'text', value: 'alice' };
    const { unmount: u2 } = render(<TooltipProvider><FieldRow field={user} ctx={ctx} category="login" /></TooltipProvider>);
    expect(screen.getByTestId('show-qr-trigger')).toBeInTheDocument();
    u2();

    const site: Field = { id: 'website', label: 'website', kind: 'url', value: 'https://example.com' };
    const { unmount: u3 } = render(<TooltipProvider><FieldRow field={site} ctx={ctx} category="login" /></TooltipProvider>);
    expect(screen.getByTestId('show-qr-trigger')).toBeInTheDocument();
    u3();

    const totp: Field = { id: 'otp', label: 'one-time password', kind: 'totp', value: null };
    const { unmount: u4 } = render(<TooltipProvider><FieldRow field={totp} ctx={ctx} category="login" /></TooltipProvider>);
    expect(screen.getByTestId('show-qr-trigger')).toBeInTheDocument();
    u4();

    const apiCred: Field = { id: 'credential', label: 'credential', kind: 'concealed', value: null };
    const { unmount: u5 } = render(<TooltipProvider><FieldRow field={apiCred} ctx={ctx} category="api_credential" /></TooltipProvider>);
    expect(screen.getByTestId('show-qr-trigger')).toBeInTheDocument();
    u5();

    const customText: Field = { id: 'sec_code', label: 'Security Code', kind: 'text', value: 'ABC123' };
    const { unmount: u6 } = render(<TooltipProvider><FieldRow field={customText} ctx={ctx} category="login" /></TooltipProvider>);
    expect(screen.getByTestId('show-qr-trigger')).toBeInTheDocument();
    u6();

    const customConcealed: Field = { id: 'secret_pin', label: 'Secret PIN', kind: 'concealed', value: null };
    const { unmount: u7 } = render(<TooltipProvider><FieldRow field={customConcealed} ctx={ctx} category="login" /></TooltipProvider>);
    expect(screen.getByTestId('show-qr-trigger')).toBeInTheDocument();
    u7();
  });

  it('does not render QR code trigger for unsupported fields (notes, credit card number/cvv, date)', () => {
    const note: Field = { id: 'note', label: 'note', kind: 'multiline', value: 'some note' };
    const { unmount: u1 } = render(<TooltipProvider><FieldRow field={note} ctx={ctx} category="secure_note" /></TooltipProvider>);
    expect(screen.queryByTestId('show-qr-trigger')).toBeNull();
    u1();

    const ccNum: Field = { id: 'number', label: 'number', kind: 'concealed', value: null };
    const { unmount: u2 } = render(<TooltipProvider><FieldRow field={ccNum} ctx={ctx} category="credit_card" /></TooltipProvider>);
    expect(screen.queryByTestId('show-qr-trigger')).toBeNull();
    u2();

    const ccCvv: Field = { id: 'cvv', label: 'verification number', kind: 'concealed', value: null };
    const { unmount: u3 } = render(<TooltipProvider><FieldRow field={ccCvv} ctx={ctx} category="credit_card" /></TooltipProvider>);
    expect(screen.queryByTestId('show-qr-trigger')).toBeNull();
    u3();

    const birth: Field = { id: 'birth', label: 'birth date', kind: 'date', value: '1990-01-01' };
    const { unmount: u4 } = render(<TooltipProvider><FieldRow field={birth} ctx={ctx} category="identity" /></TooltipProvider>);
    expect(screen.queryByTestId('show-qr-trigger')).toBeNull();
    u4();
  });

  it('right click context menu displays Show as QR code and opens dialog', async () => {
    vi.spyOn(api, 'revealField').mockResolvedValue('pw123');
    const pw: Field = { id: 'password', label: 'password', kind: 'concealed', value: null };
    render(<TooltipProvider><FieldRow field={pw} ctx={ctx} category="login" itemTitle="My Account" /></TooltipProvider>);

    const row = screen.getByTestId('field-password');
    fireEvent.contextMenu(row, { clientX: 50, clientY: 50 });

    const menu = await screen.findByTestId('field-context-menu');
    expect(menu).toBeInTheDocument();

    const qrItem = screen.getByTestId('menu-show-qr');
    expect(qrItem).toBeInTheDocument();
    expect(qrItem.textContent).toBe('Show as QR code');

    await act(async () => {
      fireEvent.click(qrItem);
    });

    expect(screen.getByRole('dialog')).toBeInTheDocument();
    expect(screen.getByTestId('field-qr-title').textContent).toBe('My Account');

    // Close with Escape
    await act(async () => {
      fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
    });
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('field more actions dropdown menu has Show as QR code item', async () => {
    const user: Field = { id: 'username', label: 'username', kind: 'text', value: 'alice' };
    render(<TooltipProvider><FieldRow field={user} ctx={ctx} category="login" /></TooltipProvider>);

    const moreBtn = screen.getByTestId('field-more-actions-username');
    await act(async () => {
      fireEvent.keyDown(moreBtn, { key: 'Enter', code: 'Enter' });
    });

    const menu = await screen.findByTestId('field-action-menu');
    expect(menu).toBeInTheDocument();
    expect(screen.getByTestId('menu-show-qr')).toBeInTheDocument();
  });
});
