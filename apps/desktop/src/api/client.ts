import { invoke } from '@tauri-apps/api/core';
import type * as T from './types';
import { mock } from './mock';
const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
import { useApp } from '../lib/store';
const raw = <R>(cmd: string, args?: Record<string, unknown>): Promise<R> => isTauri ? invoke<R>(cmd, args) : (mock as any)[cmd](args ?? {});
/** True for the backend's "vault is locked" error (gui-core GuiState::with). */
export const isLockedError = (e: unknown) => /vault is locked/i.test(typeof e === 'string' ? e : (e as any)?.message ?? String(e));
/** Thrown instead of the raw backend message; the UI switches to the unlock screen. */
export class VaultLockedError extends Error { constructor() { super('Vault locked'); this.name = 'VaultLockedError'; } }
const NO_LOCK_CMDS = new Set(['status', 'unlock', 'unlock_biometric', 'create_account', 'lock']);
const call = <R>(cmd: string, args?: Record<string, unknown>): Promise<R> =>
  Promise.resolve().then(() => raw<R>(cmd, args)).catch((e) => {
    if (!NO_LOCK_CMDS.has(cmd) && isLockedError(e)) { const s = useApp.getState(); if (!s.locked) s.lock(); throw new VaultLockedError(); }
    throw e;
  });
export const api = {
  status: () => call<T.UnlockState>('status'),
  unlock: (a:{vaultPath:string; password:string; secretKey?:string|null}) => call<T.UnlockState>('unlock', a),
  lock: () => call<void>('lock'),
  createAccount: (a:{vaultPath:string; password:string; saveSecretKey:boolean}) => call<T.CreateAccountResult>('create_account', a),
  unlockBiometric: (a:{vaultPath:string}) => call<T.UnlockState>('unlock_biometric', a),
  listVaults: () => call<T.VaultInfo[]>('list_vaults'),
  updateVault: (a:{vaultId:string; name:string; description?:string|null; icon?:string|null; color?:string|null}) => call<T.VaultInfo>('update_vault', a),
  /** permanent; confirmName must equal the vault's name */
  deleteVault: (a:{vaultId:string; confirmName:string}) => call<void>('delete_vault', a),
  createVault: (a:{name:string; description?:string|null; icon?:string|null; color?:string|null}) => call<T.VaultInfo>('create_vault', a),
  showQuickAccess: () => call<void>('show_quick_access', {}),
  listItems: (a:{filter:T.Filter}) => call<T.ItemSummary[]>('list_items', a),
  getItem: (a:{vaultId:string; itemId:string}) => call<T.ItemDetail>('get_item', a),
  revealField: (a:{vaultId:string; itemId:string; fieldId:string}) => call<string>('reveal_field', a),
  copyField: (a:{vaultId:string; itemId:string; fieldId:string}) => call<void>('copy_field', a),
  totpNow: (a:{vaultId:string; itemId:string}) => call<{code:string; remaining:number; period:number}>('totp_now', a),
  createItem: (a:{input:T.ItemInput}) => call<T.ItemSummary>('create_item', a),
  updateItem: (a:{itemId:string; input:T.ItemInput}) => call<T.ItemSummary>('update_item', a),
  setFavorite: (a:{vaultId:string; itemId:string; favorite:boolean}) => call<void>('set_favorite', a),
  archiveItem: (a:{vaultId:string; itemId:string; archived:boolean}) => call<void>('archive_item', a),
  deleteItem: (a:{vaultId:string; itemId:string}) => call<void>('delete_item', a),       // soft delete
  restoreItem: (a:{vaultId:string; itemId:string}) => call<void>('restore_item', a),
  purgeItem: (a:{vaultId:string; itemId:string}) => call<void>('purge_item', a),
  /** permanently delete every item in Recently Deleted in one commit; returns the count */
  emptyTrash: () => call<number>('empty_trash'),
  /** multi-select bulk action; one vault commit (move = two) */
  bulkItems: (a:{req:T.BulkRequest}) => call<T.BulkResult>('bulk_items', a),
  moveItem: (a:{vaultId:string; itemId:string; toVaultId:string}) => call<T.ItemSummary>('move_item', a),
  duplicateItem: (a:{vaultId:string; itemId:string}) => call<T.ItemSummary>('duplicate_item', a),
  itemVersion: (a:{vaultId:string; itemId:string; version:number}) => call<T.ItemDetail>('item_version', a),
  addAttachment: (a:{vaultId:string; itemId:string; path:string}) => call<T.Attachment>('add_attachment', a),
  saveAttachment: (a:{vaultId:string; itemId:string; attachmentId:string; destPath:string}) => call<void>('save_attachment', a),
  listTags: () => call<string[]>('list_tags'),
  renameTag: (a: { old: string; new: string }) => call<T.BulkResult>('rename_tag', a),
  deleteTag: (a: { name: string }) => call<T.BulkResult>('delete_tag', a),
  generatePassword: (a:{length:number; symbols:boolean; digits:boolean; words?:boolean; pin?:boolean}) => call<string>('generate_password', a),
  copyPassword: (a:{password:string}) => call<void>('copy_password', a),
  passwordStrength: (a:{password:string}) => call<number>('password_strength', a),
  watchtower: (a:{hibp:boolean}) => call<T.WatchtowerReport>('watchtower', a),
  ignoreFinding: (a:{vaultId:string; itemId:string; kind:string}) => call<void>('ignore_finding', a),
  unignoreFinding: (a:{vaultId:string; itemId:string; kind:string}) => call<void>('unignore_finding', a),
  importFile: (a:{vaultId:string; format:string; path:string; dryRun:boolean}) => call<T.ImportResult>('import_file', a),
  exportItems: (a:{req:T.ExportRequest}) => call<T.ExportResult>('export_items', a),
  identity: () => call<T.Identity>('identity'),
  /** v0.7: new Ed25519 key in a new SSH Key item (private key never reaches the webview) */
  sshGenerate: (a:{vaultId:string; title:string; comment?:string|null}) => call<T.SshKeyCreated>('ssh_generate', a),
  /** v0.7: import an OpenSSH private key (decrypted with passphrase if encrypted) */
  sshImport: (a:{vaultId:string; title:string; privateKey:string; passphrase?:string|null}) => call<T.SshKeyCreated>('ssh_import', a),
  sshAgentInfo: () => call<T.SshAgentInfo>('ssh_agent_info'),
  /** fingerprint of a pasted recipient key (same parser as share_vault); for out-of-band verification */
  fingerprintOf: (a:{publicKey:string}) => call<string>('fingerprint_of', a),
  listMembers: (a:{vaultId:string}) => call<T.Member[]>('list_members', a),
  shareVault: (a:{vaultId:string; publicKey:string; label?:string|null}) => call<T.Member>('share_vault', a),
  unshareVault: (a:{vaultId:string; fingerprint:string}) => call<void>('unshare_vault', a),
  acceptShare: (a:{path:string}) => call<T.VaultInfo>('accept_share', a),
  listDevices: () => call<T.DeviceInfo[]>('list_devices'),
  revokeDevice: (a:{id:string; rotateCredentials:boolean}) => call<void>('revoke_device', a),
  getSettings: () => call<T.Settings>('get_settings'),
  setSettings: (a:{settings:T.Settings}) => call<T.Settings>('set_settings', a),
  changePassword: (a:{current:string; next:string}) => call<void>('change_password', a),
  rotateSecretKey: () => call<{emergencyKit:string}>('rotate_secret_key'),
  syncStatus: () => call<T.SyncStatus>('sync_status'),
  sync: (a:{merge:boolean}) => call<{message:string}>('sync', a),
  /** `gitpass remote add <url>`: git init, .gitignore, commit encrypted files, push -u origin main. Uses existing git credentials. */
  remoteConnect: (a:{url:string}) => call<T.SyncStatus>('remote_connect', a),
  remoteDisconnect: () => call<T.SyncStatus>('remote_disconnect'),
  /** Opens https://github.com/new in the default browser (fixed URL). */
  openNewRepoPage: () => call<void>('open_new_repo_page'),
  /** Opens the item's `https://<host>/.well-known/change-password` page in the default browser (URL derived in Rust). */
  openChangePassword: (a:{vaultId:string; itemId:string}) => call<void>('open_change_password', a),
  onboarding: () => call<T.Onboarding>('onboarding'),
  setOnboarding: (a:{patch:Partial<T.Onboarding>}) => call<T.Onboarding>('set_onboarding', a),
  emergencyKit: () => call<string>('emergency_kit'),
  /** Structured kit; secretKey is null unless just created/rotated or `password` (master password) is given. */
  emergencyKitData: (a: { password?: string } = {}) => call<T.EmergencyKitData>('emergency_kit_data', a),   // returns HTML/text kit WITHOUT secret key unless user just rotated
  faviconUrl: (a:{url:string}) => call<string|null>('favicon_url', a), // local cache/data URL; never leaks vault data
};
