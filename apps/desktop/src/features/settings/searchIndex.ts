import type { Key } from '../../i18n';

export type TabId = 'general' | 'security' | 'vaults' | 'devices' | 'sync' | 'ssh' | 'about';

export interface SearchableSetting {
  id: string;
  tab: TabId;
  labelKey: Key;
  descriptionKey?: Key;
  keywords?: string[];
}

export const SEARCHABLE_SETTINGS: SearchableSetting[] = [
  // General
  {
    id: 'setting-theme',
    tab: 'general',
    labelKey: 'settings.theme',
    descriptionKey: 'settings.themeHelp',
    keywords: ['appearance', 'dark mode', 'light mode', 'colors'],
  },
  {
    id: 'setting-language',
    tab: 'general',
    labelKey: 'settings.language',
    descriptionKey: 'settings.languageHelp',
    keywords: ['locale', 'translation', 'burmese', 'thai', 'english'],
  },
  {
    id: 'setting-show-website-icons',
    tab: 'general',
    labelKey: 'settings.showWebsiteIcons',
    descriptionKey: 'settings.showWebsiteIconsHelp',
    keywords: ['favicon', 'website icons', 'site icons', 'icons', 'logos', 'monogram'],
  },
  {
    id: 'setting-quick-access-hotkey',
    tab: 'general',
    labelKey: 'settings.quickAccessHotkey',
    descriptionKey: 'settings.quickAccessHotkeyHelp',
    keywords: ['quick access', 'shortcut', 'hotkey', 'global', 'space', 'command'],
  },
  {
    id: 'setting-setup-guide',
    tab: 'general',
    labelKey: 'settings.showSetupGuide',
    descriptionKey: 'settings.showSetupGuideHelp',
    keywords: ['onboarding', 'checklist', 'welcome', 'guide', 'get started', 'setup'],
  },
  // Security
  {
    id: 'setting-autolock',
    tab: 'security',
    labelKey: 'settings.autoLock',
    descriptionKey: 'settings.autoLockHelp',
    keywords: ['timeout', 'idle', 'inactivity', 'sleep'],
  },
  {
    id: 'setting-lock-sleep',
    tab: 'security',
    labelKey: 'settings.lockOnSleep',
    descriptionKey: 'settings.lockOnSleepHelp',
    keywords: ['sleep', 'mac', 'lid', 'suspend', 'power'],
  },
  {
    id: 'setting-lock-screen',
    tab: 'security',
    labelKey: 'settings.lockOnScreenLock',
    descriptionKey: 'settings.lockOnScreenLockHelp',
    keywords: ['screen', 'lock screen', 'ctrl+cmd+q', 'hot corner', 'screensaver'],
  },
  {
    id: 'setting-lock-user-switch',
    tab: 'security',
    labelKey: 'settings.lockOnUserSwitch',
    descriptionKey: 'settings.lockOnUserSwitchHelp',
    keywords: ['fast user switching', 'switch user', 'account', 'session'],
  },
  {
    id: 'setting-remember-generated-passwords',
    tab: 'security',
    labelKey: 'settings.rememberGeneratedPasswords',
    descriptionKey: 'settings.rememberGeneratedPasswordsHelp',
    keywords: ['password generator', 'generator history', 'history', 'passwords', 'generated', 'remember'],
  },
  {
    id: 'setting-clipboard',
    tab: 'security',
    labelKey: 'settings.clipboard',
    descriptionKey: 'settings.clipboardHelp',
    keywords: ['copy', 'clear', 'paste', 'secrets'],
  },
  {
    id: 'setting-biometric',
    tab: 'security',
    labelKey: 'settings.biometric',
    descriptionKey: 'settings.biometricHelp',
    keywords: ['touch id', 'fingerprint', 'face id', 'hardware'],
  },
  {
    id: 'setting-hibp',
    tab: 'security',
    labelKey: 'settings.hibp',
    descriptionKey: 'settings.hibpHelp',
    keywords: ['pwned', 'breach', 'leak', 'compromised', 'watchtower'],
  },
  {
    id: 'setting-password',
    tab: 'security',
    labelKey: 'settings.changePassword',
    descriptionKey: 'settings.changePasswordHelp',
    keywords: ['master password', 'credentials'],
  },
  {
    id: 'setting-rotate-key',
    tab: 'security',
    labelKey: 'settings.rotateKey',
    descriptionKey: 'settings.rotateKeyHelp',
    keywords: ['emergency kit', 'secret key', 'recovery', 'rotate'],
  },
  // Vaults
  {
    id: 'setting-vaults',
    tab: 'vaults',
    labelKey: 'settings.vaults',
    descriptionKey: 'vault.manageHelp',
    keywords: ['rename', 'delete', 'export', 'color', 'icon'],
  },
  // Devices
  {
    id: 'setting-devices',
    tab: 'devices',
    labelKey: 'settings.devices',
    descriptionKey: 'devices.revokeBody',
    keywords: ['revoke', 'registered', 'phone', 'macbook', 'laptop'],
  },
  // Sync
  {
    id: 'setting-sync',
    tab: 'sync',
    labelKey: 'settings.sync',
    descriptionKey: 'sync.remote',
    keywords: ['git', 'github', 'remote', 'repository', 'backup', 'merge', 'disconnect'],
  },
  // SSH Agent
  {
    id: 'setting-ssh-status',
    tab: 'ssh',
    labelKey: 'ssh.status',
    descriptionKey: 'ssh.configHelp',
    keywords: ['socket', 'running', 'agent'],
  },
  {
    id: 'setting-ssh-config',
    tab: 'ssh',
    labelKey: 'ssh.config',
    descriptionKey: 'ssh.configHelp',
    keywords: ['~/.ssh/config', 'identityagent', 'git'],
  },
  {
    id: 'setting-ssh-approve',
    tab: 'ssh',
    labelKey: 'ssh.approve',
    descriptionKey: 'ssh.approveHelp',
    keywords: ['prompt', 'sign', 'confirm', 'authorization'],
  },
  {
    id: 'setting-ssh-cli',
    tab: 'ssh',
    labelKey: 'ssh.cli',
    descriptionKey: 'ssh.cliHelp',
    keywords: ['command line', 'terminal', 'gitpass ssh'],
  },
  // About
  {
    id: 'setting-about-version',
    tab: 'about',
    labelKey: 'about.version',
    descriptionKey: 'about.license',
    keywords: ['version', 'build', 'license'],
  },
  {
    id: 'setting-about-updates',
    tab: 'about',
    labelKey: 'update.title',
    descriptionKey: 'settings.updateHelp',
    keywords: ['version', 'upgrade', 'download', 'check for updates'],
  },
  {
    id: 'setting-about-repo',
    tab: 'about',
    labelKey: 'about.repo',
    descriptionKey: 'settings.repoHelp',
    keywords: ['source code', 'github', 'git'],
  },
  {
    id: 'setting-about-issues',
    tab: 'about',
    labelKey: 'about.issues',
    descriptionKey: 'settings.issuesHelp',
    keywords: ['bugs', 'feedback', 'report', 'github issues'],
  },
];

export interface SearchResult extends SearchableSetting {
  label: string;
  description: string;
  tabName: string;
}

export function searchSettings(
  query: string,
  t: (k: Key, vars?: Record<string, string | number>) => string
): SearchResult[] {
  const q = query.trim().toLowerCase();
  if (!q) return [];
  return SEARCHABLE_SETTINGS.map((item) => {
    const label = t(item.labelKey);
    const description = item.descriptionKey ? t(item.descriptionKey) : '';
    const tabName = t(`settings.${item.tab}` as Key);
    return { ...item, label, description, tabName };
  }).filter((item) => {
    return (
      item.label.toLowerCase().includes(q) ||
      item.description.toLowerCase().includes(q) ||
      item.tabName.toLowerCase().includes(q) ||
      (item.keywords && item.keywords.some((kw) => kw.toLowerCase().includes(q)))
    );
  });
}
