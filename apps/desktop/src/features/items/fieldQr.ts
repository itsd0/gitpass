import type { Field, ItemCategory } from '../../api/types';

/**
 * Checks if a field is supported for "Show as QR code".
 * Supported fields:
 * - password
 * - username
 * - website / url
 * - one-time-code setup key (totp)
 * - API credential secret (credential)
 * - custom text and concealed fields
 */
export function isQrSupportedField(field: Field, category?: ItemCategory): boolean {
  if (!['text', 'concealed', 'url', 'totp'].includes(field.kind)) {
    return false;
  }

  // 1. Password
  if (field.id === 'password' || field.label.trim().toLowerCase() === 'password') {
    return true;
  }

  // 2. Username
  if (field.id === 'username' || field.label.trim().toLowerCase() === 'username') {
    return true;
  }

  // 3. Website / URL
  if (field.id === 'website' || field.kind === 'url' || field.label.trim().toLowerCase() === 'website') {
    return true;
  }

  // 4. One-time-code setup key
  if (field.kind === 'totp' || field.id === 'otp' || field.id === 'totp') {
    return true;
  }

  // 5. API credential secret
  if (category === 'api_credential' && (field.id === 'credential' || field.kind === 'concealed')) {
    return true;
  }

  // Exclude category-specific standard template fields that are not supported
  if (category === 'credit_card') {
    const cardTemplateIds = ['cardholder', 'type', 'number', 'cvv', 'expiry', 'valid_from', 'pin'];
    if (cardTemplateIds.includes(field.id)) return false;
  }
  if (category === 'bank_account') {
    const bankTemplateIds = ['bank_name', 'account_holder', 'account_type', 'account_number', 'routing_number', 'pin', 'branch_phone', 'branch_address'];
    if (bankTemplateIds.includes(field.id)) return false;
  }
  if (category === 'identity') {
    const idTemplateIds = ['first', 'last', 'birth', 'address', 'email', 'phone'];
    if (idTemplateIds.includes(field.id)) return false;
  }
  if (category === 'ssh_key') {
    const sshTemplateIds = ['private_key', 'public_key', 'fingerprint'];
    if (sshTemplateIds.includes(field.id)) return false;
  }
  if (category === 'wifi') {
    const wifiTemplateIds = ['ssid', 'security'];
    if (wifiTemplateIds.includes(field.id)) return false;
  }

  // 6. Custom text and concealed fields
  return field.kind === 'text' || field.kind === 'concealed';
}

/**
 * Builds an otpauth:// URI from a raw TOTP seed or existing otpauth:// URI.
 */
export function buildTotpUri(rawValue: string, itemTitle?: string, username?: string): string {
  const trimmed = rawValue.trim();
  if (/^otpauth:\/\//i.test(trimmed)) {
    return trimmed;
  }

  const cleanSecret = trimmed.replace(/[\s-]/g, '');
  const title = (itemTitle || 'gitpass').trim();
  const user = username?.trim();
  const label = user ? `${title}:${user}` : title;

  const params = new URLSearchParams();
  params.set('secret', cleanSecret);
  if (title) {
    params.set('issuer', title);
  }

  return `otpauth://totp/${encodeURIComponent(label)}?${params.toString()}`;
}
