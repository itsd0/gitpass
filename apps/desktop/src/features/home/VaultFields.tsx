import { useT } from '../../i18n';
import { Input } from '../../ui';
import { VAULT_COLORS, VAULT_ICONS, VaultIcon } from './vaultIcons';

export interface VaultFieldsProps {
  name: string;
  setName: (v: string) => void;
  description: string;
  setDescription: (v: string) => void;
  icon: string;
  setIcon: (v: string) => void;
  color: string;
  setColor: (v: string) => void;
}

export function VaultFields({
  name,
  setName,
  description,
  setDescription,
  icon,
  setIcon,
  color,
  setColor,
}: VaultFieldsProps) {
  const t = useT();

  return (
    <>
      <div className="flex justify-center">
        <VaultIcon icon={icon} color={color} size={56} />
      </div>
      <label className="block text-xs text-muted">
        {t('settings.vaultName')}
        <Input
          className="mt-1"
          autoFocus
          aria-label={t('vault.nameAria')}
          placeholder={t('vault.namePlaceholder')}
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
      </label>
      <label className="block text-xs text-muted">
        {t('settings.vaultDescription')}
        <Input
          className="mt-1"
          aria-label={t('vault.descriptionAria')}
          value={description}
          onChange={(e) => setDescription(e.target.value)}
        />
      </label>
      <div>
        <div className="mb-1 text-xs text-muted">{t('vault.icon')}</div>
        <div className="flex flex-wrap gap-1">
          {Object.entries(VAULT_ICONS).map(([k, I]) => (
            <button
              type="button"
              key={k}
              aria-label={t('vault.iconAria', { icon: k })}
              aria-pressed={icon === k}
              onClick={() => setIcon(k)}
              className={`rounded p-2 ${icon === k ? 'bg-accent' : 'bg-hover hover:bg-hover'}`}
            >
              <I size={16} />
            </button>
          ))}
        </div>
      </div>
      <div>
        <div className="mb-1 text-xs text-muted">{t('vault.color')}</div>
        <div className="flex gap-2">
          {VAULT_COLORS.map((c) => (
            <button
              type="button"
              key={c}
              aria-label={t('vault.colorAria', { color: c })}
              aria-pressed={color === c}
              onClick={() => setColor(c)}
              className={`h-6 w-6 rounded-full transition-transform ${
                color === c ? 'ring-2 ring-fg ring-offset-2 ring-offset-panel scale-110' : 'hover:scale-105'
              }`}
              style={{ background: c }}
            />
          ))}
        </div>
      </div>
    </>
  );
}
