export type ItemCategory = 'login'|'credit_card'|'secure_note'|'identity'|'api_credential'|'document'|'ssh_key'|'wifi';
export type FieldKind = 'text'|'concealed'|'url'|'email'|'phone'|'date'|'month_year'|'totp'|'multiline';
export interface Field { id: string; label: string; kind: FieldKind; value: string | null; } // concealed values are null in summaries/details until reveal
export interface Section { id: string; title: string | null; fields: Field[]; }
export interface Attachment { id: string; name: string; size: number; mime: string; }
export interface ItemSummary { id: string; vaultId: string; category: ItemCategory; title: string; subtitle: string; favorite: boolean; archived: boolean; deletedAt: string|null; tags: string[]; updatedAt: string; createdAt: string; url?: string|null; }
export interface ItemDetail extends ItemSummary { sections: Section[]; notes: string|null; attachments: Attachment[]; history: {version:number; updatedAt:string}[]; passwordStrength?: number|null; hasTotp: boolean; }
export interface ItemInput { vaultId: string; category: ItemCategory; title: string; sections: Section[]; notes?: string|null; tags?: string[]; favorite?: boolean; url?: string|null; }
export interface VaultInfo { id: string; name: string; description: string|null; itemCount: number; shared: boolean; members: Member[]; icon?: string|null; color?: string|null; }
export interface Member { fingerprint: string; label: string|null; isSelf: boolean; }
export interface Filter { kind: 'all'|'favorites'|'archive'|'deleted'|'vault'|'tag'|'category'; vaultId?: string; tag?: string; category?: ItemCategory; query?: string; sort?: 'updated'|'title'|'created'; }
export interface WatchtowerReport { score: number; weak: string[]; reused: string[][]; old: string[]; breached: string[]; missingTotp: string[]; checkedAt: string; hibpChecked: boolean; }
export interface DeviceInfo { id: string; name: string; lastSeen: string; revoked: boolean; current: boolean; }
export interface Settings { autoLockMinutes: number; clipboardClearSecs: number; theme: 'dark'|'light'|'system'; language: 'en'|'my'|'th'; biometricUnlock: boolean; quickAccessHotkey: string; /** opt-in HIBP k-anonymity check in Watchtower; default false */ hibpCheck: boolean; }
export interface SyncStatus { remote: string|null; lastSync: string|null; ahead: number; behind: number; dirty: boolean; }
export interface Identity { publicKey: string; fingerprint: string; }
export interface Onboarding { extensionInstalled: boolean; emergencyKitSaved: boolean; repoConnected: boolean; imported: boolean; dismissed: boolean; }
export interface UnlockState { unlocked: boolean; vaultPath: string; account: string|null; vaultExists?: boolean; biometricAvailable?: boolean; }
export interface CreateAccountResult { vaultPath: string; secretKey: string; }
