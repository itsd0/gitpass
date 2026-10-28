import { describe, it, expect } from 'vitest';
import fs from 'node:fs';
import path from 'node:path';

function lum(rgb: [number, number, number]): number {
  const c = rgb
    .map((v) => v / 255)
    .map((v) => (v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4));
  return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
}

function contrast(c1: [number, number, number], c2: [number, number, number]): number {
  const [hi, lo] = [lum(c1), lum(c2)].sort((a, b) => b - a);
  return (hi + 0.05) / (lo + 0.05);
}

describe('styles.css theme tokens and contrast', () => {
  const css = fs.readFileSync(path.resolve(__dirname, 'styles.css'), 'utf8');

  it('defines the gitpass orange accent tokens for light and dark themes', () => {
    // Light theme under :root
    expect(css).toMatch(/--accent:\s*194\s+65\s+12;/);
    // Dark theme under .dark
    expect(css).toMatch(/--accent:\s*242\s+140\s+40;/);

    // No legacy blue accents remain
    expect(css).not.toContain('26 115 232');
    expect(css).not.toContain('46 125 246');
    expect(css).not.toContain('#1a73e8');
  });

  it('meets WCAG contrast requirements for accent colors and text', () => {
    const lightAccent: [number, number, number] = [194, 65, 12];
    const darkAccent: [number, number, number] = [242, 140, 40];
    const white: [number, number, number] = [255, 255, 255];
    const lightBg: [number, number, number] = [248, 248, 250];
    const darkBg: [number, number, number] = [30, 30, 32];

    // White text on light accent background (e.g. primary buttons, Unlock button) >= 4.5:1
    const whiteOnLightAccent = contrast(white, lightAccent);
    expect(whiteOnLightAccent).toBeGreaterThanOrEqual(4.5);

    // Light accent on light background (e.g. links, icons) >= 4.5:1
    const lightAccentOnBg = contrast(lightAccent, lightBg);
    expect(lightAccentOnBg).toBeGreaterThanOrEqual(4.5);

    // Dark accent on dark background (e.g. links, focus rings, selected rows) >= 4.5:1
    const darkAccentOnBg = contrast(darkAccent, darkBg);
    expect(darkAccentOnBg).toBeGreaterThanOrEqual(4.5);
  });
});
