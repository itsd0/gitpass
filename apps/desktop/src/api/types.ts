export type ItemCategory = 'login'|'credit_card'|'secure_note'|'identity'|'api_credential'|'document'|'ssh_key'|'wifi'|'passkey'|'bank_account'|'server';
export interface ScannedTotp {
  uri: string;
  secret: string;
  issuer?: string | null;
  account?: string | null;
  digits: number;
  period: number;
  algorithm: string;
}

export type FieldKind = 'text'|'concealed'|'url'|'email'|'phone'|'date'|'month_year'|'totp'|'multiline';
export interface Field { id: string; label: string; kind: FieldKind; value: string | null; /** card numbers only: '•••• •••• •••• 7575' (computed after decryption, display only) */ masked?: string | null; diff?: 'changed' | 'added' | 'removed' | null; } // concealed values are null in summaries/details until reveal
export interface Section { id: string; title: string | null; fields: Field[]; }
export interface Attachment { id: string; name: string; size: number; mime: string; }
export interface ItemSummary { id: string; vaultId: string; category: ItemCategory; title: string; subtitle: string; favorite: boolean; archived: boolean; deletedAt: string|null; tags: string[]; updatedAt: string; createdAt: string; url?: string|null; expiry?: string|null; /** derived `https://<host>/.well-known/change-password` when the URL is usable */ changePasswordUrl?: string|null; ignoredFindings?: string[]; lastUsed?: number; }
export interface IgnoredFinding { itemId: string; vaultId: string; title: string; kind: string; description: string; }
/** Active Watchtower finding shown in the item-detail banner (description comes from the Rust audit). */
export interface FindingInfo { kind: string; description: string; }
export interface ItemDetail extends ItemSummary { version?: number; sections: Section[]; notes: string|null; attachments: Attachment[]; history: {version:number; updatedAt:string}[]; passwordStrength?: number|null; hasTotp: boolean; /** HIBP excluded: the detail view never makes network calls */ findings?: FindingInfo[]; passwordChangedAt?: string|null; }
export interface ItemInput { vaultId: string; category: ItemCategory; title: string; sections: Section[]; notes?: string|null; tags?: string[]; favorite?: boolean; url?: string|null; }
export interface VaultInfo { id: string; name: string; description: string|null; itemCount: number; shared: boolean; members: Member[]; icon?: string|null; color?: string|null; }
export interface Member { fingerprint: string; label: string|null; isSelf: boolean; }
export interface Filter { kind: 'all'|'favorites'|'archive'|'deleted'|'vault'|'tag'|'category'; vaultId?: string; tag?: string; category?: ItemCategory; query?: string; sort?: 'updated'|'title'|'created'|'recent'; }
export interface UnusedItem { itemId: string; days: number; /** true = last use recorded; false = measured from the last edit/creation */ lastUsed: boolean; }
export interface WatchtowerReport { score: number; weak: string[]; reused: string[][]; old: string[]; breached: string[]; missingTotp: string[]; totpAvailable: string[]; passkeyAvailable: string[]; expired: string[]; expiringSoon: string[]; insecureUrls: string[]; duplicates: string[][]; weakSshKeys?: string[]; unused?: UnusedItem[]; weakReasons?: Record<string, string>; ignored: IgnoredFinding[]; checkedAt: string; hibpChecked: boolean; }
export interface DeviceInfo { id: string; name: string; lastSeen: string; revoked: boolean; current: boolean; }
export interface Settings { autoLockMinutes: number; clipboardClearSecs: number; theme: 'dark'|'light'|'system'; language: 'en'|'my'|'th'; biometricUnlock: boolean; quickAccessHotkey: string; /** opt-in HIBP k-anonymity check in Watchtower; default false */ hibpCheck: boolean; /** ask before every SSH agent signature (v0.7) */ sshApproveEachUse?: boolean; lockOnSleep?: boolean; lockOnScreenLock?: boolean; lockOnUserSwitch?: boolean; hideFromScreenRecording?: boolean; generatorMode?: 'random' | 'memorable' | 'pin'; generatorLength?: number; generatorDigits?: boolean; generatorSymbols?: boolean; generatorSeparator?: string; generatorCapitalize?: boolean; showWebsiteIcons?: boolean; renderNotesMarkdown?: boolean; showMenuBarIcon?: boolean; rememberGeneratedPasswords?: boolean; /** Watchtower unused-items threshold in months: 6, 12, 24; 0 = off. Default 12. */ watchtowerUnusedMonths?: number; /** Last seen app version for showing the What's new release sheet once per upgrade */ lastSeenVersion?: string; }
export interface SyncStatus { remote: string|null; lastSync: string|null; ahead: number; behind: number; dirty: boolean; }
export interface Identity { publicKey: string; fingerprint: string; }
export interface Onboarding { extensionInstalled: boolean; emergencyKitSaved: boolean; repoConnected: boolean; imported: boolean; dismissed: boolean; }
export interface UnlockState { unlocked: boolean; vaultPath: string; account: string|null; vaultExists?: boolean; biometricAvailable?: boolean; }
export interface CreateAccountResult { vaultPath: string; secretKey: string; }
export interface GeneratorHistoryEntry {
  id: string;
  value: string;
  createdAt: number;
  source: string;
  host?: string | null;
}

export interface EmergencyKitData { accountId: string; vaultPath: string; remote: string | null; deviceName: string | null; createdAt: string; secretKeyAvailable: boolean; secretKey: string | null; }
export interface ImportPreviewItem { title: string; category: string; username: string | null; url: string | null; hasPassword: boolean; hasTotp: boolean; tags: string[]; fields: number; archived: boolean; }
export interface ImportResult { parsed: number; added: number; duplicates: number; duplicateTitles?: string[]; warnings?: string[]; categories?: Record<string, number>; preview?: ImportPreviewItem[]; }
export interface ExportRequest { vaultId?: string; items?: ItemRef[]; format: 'gitpass-json'|'csv'; encrypted: boolean; passphrase?: string; path: string; }
export interface ExportResult { path: string; items: number; format: string; encrypted: boolean; }
export interface SshKeyCreated { item: ItemSummary; publicKey: string; fingerprint: string; keyType: string; }
export interface SshAgentInfo { socketPath: string; configLine: string; running: boolean; approveEachUse: boolean; }
export type SshActivityResult = 'signed' | 'denied' | 'refused';
/** One SSH agent sign request (metadata only; never the signed data). */
export interface SshActivityEntry {
  time: string; keyTitle: string; fingerprint: string; keyType: string; algorithm: string;
  result: SshActivityResult; client?: string; pid?: number;
}
export interface GitSigningConfig { keyType: string; publicKey: string; signingKey: string; socketPath: string; exportAuthSock: string; allowedSignersFile: string; allowedSignersLine: string; configLines: string; email: string; authSockActive: boolean; }

export interface ItemRef { vaultId: string; itemId: string }
export type BulkAction = 'trash' | 'restore' | 'purge' | 'archive' | 'unarchive' | 'favorite' | 'unfavorite' | 'add_tag' | 'remove_tag' | 'move' | 'copy';
export interface BulkRequest { action: BulkAction; items: ItemRef[]; tag?: string | null; toVaultId?: string | null }
export interface BulkResult { count: number; items: ItemRef[] }
