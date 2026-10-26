import { describe, expect, it } from 'vitest';
import { detectBrand, formatCardNumber, formatExpiry, luhnValid } from './card';
describe('card', () => {
  it('detects brands', () => {
    expect(detectBrand('4111 1111 1111 1111')).toBe('visa');
    expect(detectBrand('5500000000000004')).toBe('mastercard');
    expect(detectBrand('2221000000000009')).toBe('mastercard');
    expect(detectBrand('378282246310005')).toBe('amex');
    expect(detectBrand('6011111111111117')).toBe('discover');
    expect(detectBrand('3530111333300000')).toBe('jcb');
    expect(detectBrand('30569309025904')).toBe('diners');
    expect(detectBrand('9999')).toBe('unknown');
  });
  it('formats numbers by brand', () => {
    expect(formatCardNumber('4111111111111111')).toBe('4111 1111 1111 1111');
    expect(formatCardNumber('378282246310005999')).toBe('3782 822463 10005');
    expect(formatCardNumber('41a1-1')).toBe('4111');
  });
  it('luhn + expiry', () => {
    expect(luhnValid('4111 1111 1111 1111')).toBe(true);
    expect(luhnValid('4111 1111 1111 1112')).toBe(false);
    expect(formatExpiry('3', '2029')).toBe('03/2029');
    expect(formatExpiry('', '2029')).toBe('');
  });
});
