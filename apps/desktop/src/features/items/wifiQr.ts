import type { Field, ItemDetail } from '../../api/types';

export interface WifiPayloadOptions {
  ssid: string;
  password?: string | null;
  security?: string | null;
  hidden?: boolean | null;
}

/**
 * Standard ZXing Wi-Fi QR code escaping.
 * Escapes \, ;, ,, :, and " with a leading backslash.
 */
export function escapeWifi(value: string): string {
  return (value || '').replace(/([\\;,:"])/g, '\\$1');
}

/**
 * Normalizes the security type into standard WPA, WEP, or nopass.
 */
export function normalizeWifiAuth(security?: string | null, hasPassword?: boolean): 'WPA' | 'WEP' | 'nopass' {
  const s = (security || '').trim().toLowerCase();
  if (s === 'wep') return 'WEP';
  if (s === 'nopass' || s === 'none' || s === 'open') return 'nopass';
  if (s.includes('wpa')) return 'WPA';
  if (!hasPassword && !s) return 'nopass';
  return 'WPA';
}

/**
 * Builds a standard Wi-Fi QR code payload string:
 * `WIFI:T:<WPA|WEP|nopass>;S:<ssid>;P:<password>;H:<true|false>;;`
 * Open networks omit the `P:<password>;` segment and use `T:nopass`.
 */
export function buildWifiPayload(options: WifiPayloadOptions): string {
  const ssid = options.ssid ?? '';
  const escapedSsid = escapeWifi(ssid);
  const password = options.password ?? '';
  const hasPassword = Boolean(password && password.length > 0);
  const auth = normalizeWifiAuth(options.security, hasPassword);
  const isHidden = options.hidden === true;
  const hiddenFlag = isHidden ? 'true' : 'false';

  let passPart = '';
  if (auth !== 'nopass' && hasPassword) {
    passPart = `P:${escapeWifi(password)};`;
  }

  return `WIFI:T:${auth};S:${escapedSsid};${passPart}H:${hiddenFlag};;`;
}

export interface ExtractedWifiFields {
  ssid: string;
  ssidField: Field | null;
  passwordField: Field | null;
  security: string;
  hidden: boolean;
}

/**
 * Extracts standard Wi-Fi attributes (SSID, password field, security, hidden) from item detail sections.
 */
export function extractWifiFields(detail: Pick<ItemDetail, 'sections' | 'category'>): ExtractedWifiFields {
  const fields = detail.sections.flatMap((s) => s.fields);
  const ssidField = fields.find((f) => f.id === 'ssid' || /^(network name|ssid)$/i.test(f.label)) ?? null;
  const ssid = (ssidField?.value ?? '').trim();
  const passwordField = fields.find((f) => f.id === 'password' || /^(wireless password|password|wifi password)$/i.test(f.label) || f.kind === 'concealed') ?? null;
  const secField = fields.find((f) => f.id === 'security' || /^(security|encryption|security type|auth)$/i.test(f.label)) ?? null;
  const security = (secField?.value ?? '').trim();
  const hiddenField = fields.find((f) => f.id === 'hidden' || /^(hidden|hidden network)$/i.test(f.label)) ?? null;
  const hidden = hiddenField ? /^(true|yes|1)$/i.test((hiddenField.value ?? '').trim()) : false;
  return { ssid, ssidField, passwordField, security, hidden };
}
