import { describe, expect, it } from 'vitest';
import { buildWifiPayload, escapeWifi, extractWifiFields, normalizeWifiAuth } from './wifiQr';
import type { ItemDetail } from '../../api/types';

describe('wifiQr string builder and helpers', () => {
  describe('escapeWifi', () => {
    it('escapes backslash, semicolon, comma, colon, and quotes', () => {
      expect(escapeWifi('Office\\Net;1,2:3"Quotes"')).toBe('Office\\\\Net\\;1\\,2\\:3\\"Quotes\\"');
    });

    it('leaves standard characters untouched', () => {
      expect(escapeWifi('Simple-SSID_123')).toBe('Simple-SSID_123');
      expect(escapeWifi('')).toBe('');
    });
  });

  describe('normalizeWifiAuth', () => {
    it('detects WPA variants', () => {
      expect(normalizeWifiAuth('WPA', true)).toBe('WPA');
      expect(normalizeWifiAuth('WPA2-PSK', true)).toBe('WPA');
      expect(normalizeWifiAuth('WPA3', true)).toBe('WPA');
    });

    it('detects WEP', () => {
      expect(normalizeWifiAuth('WEP', true)).toBe('WEP');
      expect(normalizeWifiAuth('wep', true)).toBe('WEP');
    });

    it('detects open networks (nopass, none, open, or missing with no password)', () => {
      expect(normalizeWifiAuth('nopass', true)).toBe('nopass');
      expect(normalizeWifiAuth('none', false)).toBe('nopass');
      expect(normalizeWifiAuth('open', false)).toBe('nopass');
      expect(normalizeWifiAuth('', false)).toBe('nopass');
      expect(normalizeWifiAuth(null, false)).toBe('nopass');
    });

    it('defaults to WPA when password is provided but security is unspecified', () => {
      expect(normalizeWifiAuth(null, true)).toBe('WPA');
      expect(normalizeWifiAuth('', true)).toBe('WPA');
    });
  });

  describe('buildWifiPayload', () => {
    it('builds standard WPA Wi-Fi payload with password and H:false', () => {
      const payload = buildWifiPayload({
        ssid: 'Home-Network',
        password: 'super-secret-password',
        security: 'WPA2',
      });
      expect(payload).toBe('WIFI:T:WPA;S:Home-Network;P:super-secret-password;H:false;;');
    });

    it('escapes special characters in SSID and password', () => {
      const payload = buildWifiPayload({
        ssid: 'Cafe;Guest:Zone,1\\2"3',
        password: 'p;a:s,s\\w"ord',
        security: 'WPA',
      });
      expect(payload).toBe('WIFI:T:WPA;S:Cafe\\;Guest\\:Zone\\,1\\\\2\\"3;P:p\\;a\\:s\\,s\\\\w\\"ord;H:false;;');
    });

    it('formats open networks as T:nopass with no P: field', () => {
      const payloadExplicitNopass = buildWifiPayload({
        ssid: 'Airport-Free-WiFi',
        security: 'nopass',
      });
      expect(payloadExplicitNopass).toBe('WIFI:T:nopass;S:Airport-Free-WiFi;H:false;;');
      expect(payloadExplicitNopass).not.toContain('P:');

      const payloadNone = buildWifiPayload({
        ssid: 'City-Open',
        security: 'none',
        password: '',
      });
      expect(payloadNone).toBe('WIFI:T:nopass;S:City-Open;H:false;;');
      expect(payloadNone).not.toContain('P:');

      const payloadNoPw = buildWifiPayload({
        ssid: 'Public',
        password: null,
      });
      expect(payloadNoPw).toBe('WIFI:T:nopass;S:Public;H:false;;');
      expect(payloadNoPw).not.toContain('P:');
    });

    it('supports the hidden network flag H:true', () => {
      const payload = buildWifiPayload({
        ssid: 'Stealth-Network',
        password: 'secure-password',
        security: 'WPA3',
        hidden: true,
      });
      expect(payload).toBe('WIFI:T:WPA;S:Stealth-Network;P:secure-password;H:true;;');
    });

    it('supports WEP authentication', () => {
      const payload = buildWifiPayload({
        ssid: 'Legacy-AP',
        password: 'wep-key-12345',
        security: 'WEP',
      });
      expect(payload).toBe('WIFI:T:WEP;S:Legacy-AP;P:wep-key-12345;H:false;;');
    });
  });

  describe('extractWifiFields', () => {
    it('extracts SSID, password field, security, and hidden status from item detail', () => {
      const mockDetail: Pick<ItemDetail, 'sections' | 'category'> = {
        category: 'wifi',
        sections: [
          {
            id: 'main',
            title: null,
            fields: [
              { id: 'ssid', label: 'network name', kind: 'text', value: 'Naing-Office', masked: null, diff: null },
              { id: 'password', label: 'wireless password', kind: 'concealed', value: null, masked: '••••••••', diff: null },
              { id: 'security', label: 'security', kind: 'text', value: 'WPA3', masked: null, diff: null },
              { id: 'hidden', label: 'hidden', kind: 'text', value: 'true', masked: null, diff: null },
            ],
          },
        ],
      };

      const extracted = extractWifiFields(mockDetail);
      expect(extracted.ssid).toBe('Naing-Office');
      expect(extracted.ssidField?.id).toBe('ssid');
      expect(extracted.passwordField?.id).toBe('password');
      expect(extracted.security).toBe('WPA3');
      expect(extracted.hidden).toBe(true);
    });

    it('handles missing or alternate field labels cleanly', () => {
      const mockDetail: Pick<ItemDetail, 'sections' | 'category'> = {
        category: 'wifi',
        sections: [
          {
            id: 'main',
            title: null,
            fields: [
              { id: 'f1', label: 'SSID', kind: 'text', value: '   ', masked: null, diff: null },
            ],
          },
        ],
      };

      const extracted = extractWifiFields(mockDetail);
      expect(extracted.ssid).toBe('');
      expect(extracted.passwordField).toBeNull();
      expect(extracted.security).toBe('');
      expect(extracted.hidden).toBe(false);
    });
  });
});
