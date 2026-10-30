import { describe, it, expect } from 'vitest';
import { encodeSegment, fieldSegment, refTarget, secretRef } from './secretRef';

const f = (id: string, label: string) => ({ id, label });

describe('secretRef', () => {
  it('encodes like the CLI resolver expects', () => {
    expect(encodeSegment('Dev')).toBe('Dev');
    expect(encodeSegment('My Vault')).toBe('My%20Vault');
    expect(encodeSegment('a/b')).toBe('a%2Fb');
    expect(encodeSegment('100%')).toBe('100%25');
    expect(encodeSegment('{x}')).toBe('%7Bx%7D');
    expect(encodeSegment('Café')).toBe('Café');
    expect(encodeSegment('a.b-c_d~e')).toBe('a.b-c_d~e');
    expect(decodeURIComponent(encodeSegment('x y/z%1 \u{1F600}'))).toBe('x y/z%1 \u{1F600}');
  });

  it('uses standard names, then unique labels, then ids', () => {
    const fields = [f('username', 'username'), f('password', 'password'), f('f1', 'API Key'), f('f2', 'Region'), f('f3', 'region'), f('f4', 'Password')];
    expect(fieldSegment(fields[1], fields)).toBe('password');
    expect(fieldSegment(fields[2], fields)).toBe('API Key');
    expect(fieldSegment(fields[3], fields)).toBe('f2');
    expect(fieldSegment(fields[4], fields)).toBe('f3');
    expect(fieldSegment(fields[5], fields)).toBe('f4');
  });

  it('builds a reference for a name with spaces and slashes', () => {
    const fields = [f('username', 'username'), f('password', 'password'), f('f1', 'API Key')];
    const target = refTarget({ id: 'v1', name: 'My Vault' }, [{ id: 'v1', name: 'My Vault' }], { id: 'i1', title: 'Prod DB/primary' }, [], fields)!;
    expect(secretRef(target, fields[1])).toBe('gp://My%20Vault/Prod%20DB%2Fprimary/password');
    expect(secretRef(target, fields[2])).toBe('gp://My%20Vault/Prod%20DB%2Fprimary/API%20Key');
  });

  it('falls back to ids when vault names or item titles are ambiguous', () => {
    const vaults = [{ id: 'v1', name: 'Dev' }, { id: 'v2', name: 'Dev' }];
    const t = refTarget(vaults[0], vaults, { id: 'i1', title: 'Postgres' }, [{ id: 'i2', title: 'postgres' }], [])!;
    expect(t.vault).toBe('v1');
    expect(t.item).toBe('i1');
    expect(refTarget(undefined, vaults, { id: 'i1', title: 'x' }, [], [])).toBeNull();
    const own = refTarget(vaults[0], [vaults[0]], { id: 'i1', title: 'Postgres' }, [{ id: 'i1', title: 'Postgres' }], [])!;
    expect(own.item).toBe('Postgres');
  });
});
