import { describe, expect, it } from 'vitest';
import { compareSemver, determineWhatsNewAction, parseSemver } from './semver';

describe('semver', () => {
  it('parses version strings correctly', () => {
    expect(parseSemver('0.6.5')).toEqual({ major: 0, minor: 6, patch: 5, prerelease: null });
    expect(parseSemver('v1.2.3')).toEqual({ major: 1, minor: 2, patch: 3, prerelease: null });
    expect(parseSemver('0.7.0-beta.1')).toEqual({ major: 0, minor: 7, patch: 0, prerelease: 'beta.1' });
    expect(parseSemver('2')).toEqual({ major: 2, minor: 0, patch: 0, prerelease: null });
    expect(parseSemver('invalid')).toEqual({ major: 0, minor: 0, patch: 0, prerelease: null });
  });

  it('compares versions accurately', () => {
    expect(compareSemver('0.6.5', '0.6.4')).toBeGreaterThan(0);
    expect(compareSemver('0.7.0', '0.6.5')).toBeGreaterThan(0);
    expect(compareSemver('1.0.0', '0.9.9')).toBeGreaterThan(0);
    expect(compareSemver('0.6.5', '0.6.5')).toBe(0);
    expect(compareSemver('v0.6.5', '0.6.5')).toBe(0);
    expect(compareSemver('0.6.4', '0.6.5')).toBeLessThan(0);
    expect(compareSemver('0.6.5', '0.6.5-beta.1')).toBeGreaterThan(0);
    expect(compareSemver('0.6.5-beta.1', '0.6.5')).toBeLessThan(0);
    expect(compareSemver('0.6.5-beta.2', '0.6.5-beta.1')).toBeGreaterThan(0);
  });

  it('determines action on fresh install: record_silent', () => {
    expect(determineWhatsNewAction('0.6.5', null)).toBe('record_silent');
    expect(determineWhatsNewAction('0.6.5', undefined)).toBe('record_silent');
    expect(determineWhatsNewAction('0.6.5', '')).toBe('record_silent');
    expect(determineWhatsNewAction('0.6.5', '   ')).toBe('record_silent');
  });

  it('determines action on upgrade: show', () => {
    expect(determineWhatsNewAction('0.6.5', '0.6.4')).toBe('show');
    expect(determineWhatsNewAction('0.7.0', '0.6.5')).toBe('show');
    expect(determineWhatsNewAction('1.0.0', '0.9.9')).toBe('show');
    expect(determineWhatsNewAction('0.6.5', '0.6.5-beta.1')).toBe('show');
  });

  it('determines action on same version or downgrade: ignore', () => {
    expect(determineWhatsNewAction('0.6.5', '0.6.5')).toBe('ignore');
    expect(determineWhatsNewAction('v0.6.5', '0.6.5')).toBe('ignore');
    expect(determineWhatsNewAction('0.6.4', '0.6.5')).toBe('ignore');
    expect(determineWhatsNewAction('0.5.0', '0.6.5')).toBe('ignore');
  });
});
