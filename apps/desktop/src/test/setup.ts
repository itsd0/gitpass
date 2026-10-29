import '@testing-library/jest-dom/vitest';
// jsdom lacks these
class RO { observe() {} unobserve() {} disconnect() {} }
(globalThis as any).ResizeObserver ??= RO;
Element.prototype.scrollIntoView ??= () => {};

class MemoryStorage {
  private store: Record<string, string> = {};
  get length() { return Object.keys(this.store).length; }
  clear() { this.store = {}; }
  getItem(key: string) { return this.store[key] ?? null; }
  setItem(key: string, value: string) { this.store[key] = String(value); }
  removeItem(key: string) { delete this.store[key]; }
  key(index: number) { return Object.keys(this.store)[index] ?? null; }
}
if (typeof window !== 'undefined') {
  const s = new MemoryStorage();
  Object.defineProperty(window, 'localStorage', { value: s, configurable: true, writable: true });
  Object.defineProperty(globalThis, 'localStorage', { value: s, configurable: true, writable: true });
}
