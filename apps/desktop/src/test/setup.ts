import '@testing-library/jest-dom/vitest';
// jsdom lacks these
class RO { observe() {} unobserve() {} disconnect() {} }
(globalThis as any).ResizeObserver ??= RO;
Element.prototype.scrollIntoView ??= () => {};

// Node 25 defines a dummy getter for `globalThis.localStorage` without Storage methods.
if (typeof (globalThis as any).localStorage?.clear !== 'function') {
  const store = new Map<string, string>();
  const storage = (typeof window !== 'undefined' && window.localStorage && typeof window.localStorage.clear === 'function')
    ? window.localStorage
    : {
        getItem: (k: string) => store.get(k) ?? null,
        setItem: (k: string, v: string) => { store.set(k, String(v)); },
        removeItem: (k: string) => { store.delete(k); },
        clear: () => { store.clear(); },
        key: (i: number) => Array.from(store.keys())[i] ?? null,
        get length() { return store.size; },
      };
  Object.defineProperty(globalThis, 'localStorage', { value: storage, writable: true, configurable: true });
}
