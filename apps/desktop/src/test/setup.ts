import '@testing-library/jest-dom/vitest';
// jsdom lacks these
class RO { observe() {} unobserve() {} disconnect() {} }
(globalThis as any).ResizeObserver ??= RO;
