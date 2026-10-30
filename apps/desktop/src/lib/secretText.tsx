import { cn } from './utils';
import { useColorPasswords } from '../features/settings/colorPasswords';

export type CharKind = 'digit' | 'symbol' | 'letter' | 'space';

/** Class of one character for colour-coding a revealed secret: any Unicode letter is a letter, any decimal digit a digit. */
export function charKind(char: string): CharKind {
  if (/\p{Nd}/u.test(char)) return 'digit';
  if (/[\p{L}\p{M}]/u.test(char)) return 'letter';
  if (/\s/u.test(char)) return 'space';
  return 'symbol';
}

/** Text colour for a character class. Light mode uses blue-600 (>= 4.5:1 on white), dark mode blue-400. */
export function kindClass(kind: CharKind): string {
  switch (kind) {
    case 'digit': return 'secret-digit text-accent';
    case 'symbol': return 'secret-symbol text-blue-600 dark:text-blue-400';
    case 'space': return 'secret-space text-muted';
    default: return 'secret-letter text-fg';
  }
}

/**
 * A revealed secret, one span per character (monospace, digits orange, symbols blue, spaces as a dim ·).
 * textContent is always the exact value, so selecting or asserting on it is unchanged; copy buttons never read the DOM.
 * The whole value is one aria-label so a screen reader reads it as a single string.
 * With the "Colour-code revealed passwords" setting off it renders the plain value.
 */
export function SecretText({ value, className, testId, label }: { value: string; className?: string; testId?: string; label?: string }) {
  const colored = useColorPasswords((s) => s.enabled);
  if (!colored) return <span className={className} data-testid={testId} data-colored="false">{value}</span>;
  return (
    <span className={className} data-testid={testId} data-colored="true" role="img" aria-label={label ?? value}>
      {Array.from(value).map((ch, i) => {
        const kind = charKind(ch);
        return <span key={i} data-kind={kind} className={cn(kindClass(kind), kind === 'space' && "relative inline-block w-[1ch] whitespace-pre before:absolute before:left-0 before:content-['·']")}>{ch}</span>;
      })}
    </span>
  );
}
