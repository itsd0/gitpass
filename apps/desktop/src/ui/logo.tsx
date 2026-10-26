/** gitpass mark: a key-shaped commit graph (original design). */
export function Logo({ size = 28 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 32 32" aria-label="gitpass" role="img">
      <rect width="32" height="32" rx="8" fill="#2E7DF6" />
      <circle cx="11" cy="16" r="5" fill="none" stroke="white" strokeWidth="2.6" />
      <path d="M16 16h10M22 16v4M26 16v3" stroke="white" strokeWidth="2.6" strokeLinecap="round" />
    </svg>
  );
}
