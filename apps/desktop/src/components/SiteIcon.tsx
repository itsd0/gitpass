import { useEffect, useState } from 'react';
import { api } from '../api/client';
import { useSettings } from '../features/settings/useSettings';
import { cn } from '../lib/utils';

export interface SiteIconProps {
  url?: string | null;
  title?: string;
  size?: number;
  className?: string;
  item?: { title: string; url?: string | null };
}

export function getMonogram(title: string): string {
  const t = title.trim();
  if (!t) return '?';
  return Array.from(t)[0].toUpperCase();
}

export function SiteIcon({ url: directUrl, title: directTitle, size = 24, className, item }: SiteIconProps) {
  const title = item?.title ?? directTitle ?? '';
  const url = item ? item.url : directUrl;
  const showWebsiteIcons = useSettings((s) => s.settings?.showWebsiteIcons ?? false);

  const [iconSrc, setIconSrc] = useState<string | null>(null);
  const [status, setStatus] = useState<'idle' | 'loading' | 'success' | 'failed'>('idle');

  useEffect(() => {
    let active = true;
    if (!showWebsiteIcons || !url || !url.trim()) {
      setIconSrc(null);
      setStatus('idle');
      return;
    }

    setStatus('loading');
    api.faviconUrl({ url })
      .then((src) => {
        if (!active) return;
        if (src) {
          setIconSrc(src);
          setStatus('success');
        } else {
          setIconSrc(null);
          setStatus('failed');
        }
      })
      .catch(() => {
        if (!active) return;
        setIconSrc(null);
        setStatus('failed');
      });

    return () => {
      active = false;
    };
  }, [url, showWebsiteIcons]);

  const monogram = getMonogram(title);
  const fontSize = Math.max(10, Math.round(size * 0.45));
  const roundedClass = size <= 28 ? 'rounded' : 'rounded-md';

  if (status === 'success' && iconSrc) {
    return (
      <div
        className={cn(
          'flex shrink-0 items-center justify-center overflow-hidden bg-card text-foreground',
          roundedClass,
          className
        )}
        style={{ width: size, height: size }}
        data-testid="site-icon-image-wrapper"
      >
        <img
          src={iconSrc}
          alt={title}
          width={size}
          height={size}
          className="h-full w-full object-contain"
          onError={() => {
            setStatus('failed');
            setIconSrc(null);
          }}
          data-testid="site-icon-img"
        />
      </div>
    );
  }

  return (
    <div
      role="img"
      aria-label={title}
      className={cn(
        'flex shrink-0 select-none items-center justify-center font-semibold text-accent bg-accent/15',
        roundedClass,
        className
      )}
      style={{
        width: size,
        height: size,
        fontSize: `${fontSize}px`,
        lineHeight: 1,
      }}
      data-testid="site-icon-monogram"
      data-status={status}
    >
      {monogram}
    </div>
  );
}
