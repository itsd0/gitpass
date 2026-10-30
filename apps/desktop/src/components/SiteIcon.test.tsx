import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { api } from '../api/client';
import { useSettings } from '../features/settings/useSettings';
import { ItemIcon } from '../features/items/categories';
import { SiteIcon, getMonogram } from './SiteIcon';

describe('getMonogram', () => {
  it('returns uppercase first letter of title', () => {
    expect(getMonogram('github.com')).toBe('G');
    expect(getMonogram('Apple ID')).toBe('A');
    expect(getMonogram('1example')).toBe('1');
  });

  it('trims leading whitespace', () => {
    expect(getMonogram('  netflix.com  ')).toBe('N');
  });

  it('returns ? for empty or whitespace-only titles', () => {
    expect(getMonogram('')).toBe('?');
    expect(getMonogram('   ')).toBe('?');
  });

  it('handles multi-byte and unicode characters properly', () => {
    expect(getMonogram('🦊 Firefox')).toBe('🦊');
    expect(getMonogram('ไทย')).toBe('ไ');
  });
});

describe('SiteIcon', () => {
  const dummyFavicon = 'data:image/svg+xml;base64,PHN2Zz48L3N2Zz4=';

  beforeEach(() => {
    vi.restoreAllMocks();
    useSettings.setState({
      settings: {
        autoLockMinutes: 10,
        clipboardClearSecs: 30,
        theme: 'dark',
        language: 'en',
        biometricUnlock: false,
        quickAccessHotkey: 'CmdOrCtrl+Shift+Space',
        hibpCheck: false,
        showWebsiteIcons: false,
      },
      error: null,
    });
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it('renders monogram fallback and makes no network call when showWebsiteIcons is false', () => {
    const spy = vi.spyOn(api, 'faviconUrl');
    render(<SiteIcon url="https://github.com" title="GitHub" size={24} />);

    const monogram = screen.getByTestId('site-icon-monogram');
    expect(monogram).toBeTruthy();
    expect(monogram.textContent).toBe('G');
    expect(monogram.getAttribute('data-status')).toBe('idle');
    expect(monogram.className).toContain('text-accent');
    expect(monogram.className).toContain('bg-accent/15');
    expect(spy).not.toHaveBeenCalled();
  });

  it('renders monogram fallback and makes no network call when url is missing or empty', () => {
    useSettings.setState({
      settings: {
        ...useSettings.getState().settings!,
        showWebsiteIcons: true,
      },
    });
    const spy = vi.spyOn(api, 'faviconUrl');

    const { rerender } = render(<SiteIcon url="" title="GitHub" size={24} />);
    expect(screen.getByTestId('site-icon-monogram').textContent).toBe('G');
    expect(spy).not.toHaveBeenCalled();

    rerender(<SiteIcon url={null} title="GitHub" size={24} />);
    expect(screen.getByTestId('site-icon-monogram').textContent).toBe('G');
    expect(spy).not.toHaveBeenCalled();

    rerender(<SiteIcon url="   " title="GitHub" size={24} />);
    expect(screen.getByTestId('site-icon-monogram').textContent).toBe('G');
    expect(spy).not.toHaveBeenCalled();
  });

  it('shows loading state while fetching and then renders icon on success', async () => {
    useSettings.setState({
      settings: {
        ...useSettings.getState().settings!,
        showWebsiteIcons: true,
      },
    });

    let resolvePromise!: (val: string | null) => void;
    const fetchPromise = new Promise<string | null>((resolve) => {
      resolvePromise = resolve;
    });
    vi.spyOn(api, 'faviconUrl').mockReturnValue(fetchPromise);

    render(<SiteIcon url="https://github.com" title="GitHub" size={24} />);

    // While loading, monogram is shown with loading status
    const loadingMonogram = screen.getByTestId('site-icon-monogram');
    expect(loadingMonogram.getAttribute('data-status')).toBe('loading');
    expect(loadingMonogram.textContent).toBe('G');

    // Resolve successfully
    resolvePromise(dummyFavicon);

    await waitFor(() => {
      const img = screen.getByTestId('site-icon-img');
      expect(img).toBeTruthy();
      expect(img.getAttribute('src')).toBe(dummyFavicon);
      expect(img.getAttribute('alt')).toBe('GitHub');
    });

    expect(screen.queryByTestId('site-icon-monogram')).toBeNull();
  });

  it('falls back to monogram when api.faviconUrl returns null', async () => {
    useSettings.setState({
      settings: {
        ...useSettings.getState().settings!,
        showWebsiteIcons: true,
      },
    });
    vi.spyOn(api, 'faviconUrl').mockResolvedValue(null);

    render(<SiteIcon url="https://github.com" title="GitHub" size={24} />);

    await waitFor(() => {
      const monogram = screen.getByTestId('site-icon-monogram');
      expect(monogram.getAttribute('data-status')).toBe('failed');
      expect(monogram.textContent).toBe('G');
    });
    expect(screen.queryByTestId('site-icon-img')).toBeNull();
  });

  it('falls back to monogram when api.faviconUrl rejects with error', async () => {
    useSettings.setState({
      settings: {
        ...useSettings.getState().settings!,
        showWebsiteIcons: true,
      },
    });
    vi.spyOn(api, 'faviconUrl').mockRejectedValue(new Error('network error'));

    render(<SiteIcon url="https://github.com" title="GitHub" size={24} />);

    await waitFor(() => {
      const monogram = screen.getByTestId('site-icon-monogram');
      expect(monogram.getAttribute('data-status')).toBe('failed');
      expect(monogram.textContent).toBe('G');
    });
  });

  it('falls back to monogram if image fails to load via onError', async () => {
    useSettings.setState({
      settings: {
        ...useSettings.getState().settings!,
        showWebsiteIcons: true,
      },
    });
    vi.spyOn(api, 'faviconUrl').mockResolvedValue('https://example.invalid/broken.ico');

    render(<SiteIcon url="https://example.invalid" title="Broken" size={24} />);

    const img = await screen.findByTestId('site-icon-img');
    expect(img).toBeTruthy();

    fireEvent.error(img);

    await waitFor(() => {
      const monogram = screen.getByTestId('site-icon-monogram');
      expect(monogram.getAttribute('data-status')).toBe('failed');
      expect(monogram.textContent).toBe('B');
    });
    expect(screen.queryByTestId('site-icon-img')).toBeNull();
  });

  it('applies correct dimensions and rounded corners for 24px and 40px', () => {
    const { rerender } = render(<SiteIcon url={null} title="List Item" size={24} />);
    let monogram = screen.getByTestId('site-icon-monogram');
    expect(monogram.style.width).toBe('24px');
    expect(monogram.style.height).toBe('24px');
    expect(monogram.className).toContain('rounded');
    expect(monogram.className).not.toContain('rounded-md');

    rerender(<SiteIcon url={null} title="Detail Item" size={40} />);
    monogram = screen.getByTestId('site-icon-monogram');
    expect(monogram.style.width).toBe('40px');
    expect(monogram.style.height).toBe('40px');
    expect(monogram.className).toContain('rounded-md');
  });

  it('extracts url and title when passed via item prop', () => {
    const spy = vi.spyOn(api, 'faviconUrl');
    render(
      <SiteIcon
        item={{ title: 'Proton Mail', url: 'https://mail.proton.me' }}
        size={24}
      />
    );
    expect(screen.getByTestId('site-icon-monogram').textContent).toBe('P');
    expect(spy).not.toHaveBeenCalled(); // showWebsiteIcons is false
  });
});

describe('ItemIcon category dispatch', () => {
  beforeEach(() => {
    useSettings.setState({
      settings: {
        autoLockMinutes: 10,
        clipboardClearSecs: 30,
        theme: 'dark',
        language: 'en',
        biometricUnlock: false,
        quickAccessHotkey: 'CmdOrCtrl+Shift+Space',
        hibpCheck: false,
        showWebsiteIcons: true,
      },
      error: null,
    });
  });

  it('renders SiteIcon for login items', async () => {
    vi.spyOn(api, 'faviconUrl').mockResolvedValue(null);
    render(
      <ItemIcon
        item={{
          category: 'login',
          title: 'GitHub',
          subtitle: 'sam',
          url: 'https://github.com',
        }}
        size={24}
      />
    );
    await waitFor(() => {
      expect(screen.getByTestId('site-icon-monogram')).toBeTruthy();
    });
  });

  it('keeps CategoryIcon for non-login categories', () => {
    const { rerender } = render(
      <ItemIcon
        item={{
          category: 'credit_card',
          title: 'Visa Platinum',
          subtitle: '•••• 1234',
        }}
        size={24}
      />
    );
    expect(screen.queryByTestId('site-icon-monogram')).toBeNull();

    rerender(
      <ItemIcon
        item={{
          category: 'ssh_key',
          title: 'id_ed25519',
          subtitle: 'sam@mac',
        }}
        size={24}
      />
    );
    expect(screen.queryByTestId('site-icon-monogram')).toBeNull();
  });
});
