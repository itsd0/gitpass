/** Original illustration: a vault door with a key, drawn with simple shapes. */
export function EmptyIllustration() {
  return (
    <svg width="180" height="140" viewBox="0 0 180 140" fill="none" aria-hidden="true">
      <ellipse cx="90" cy="128" rx="62" ry="6" fill="rgb(var(--hover))" />
      <rect x="40" y="18" width="100" height="100" rx="16" fill="rgb(var(--card))" stroke="rgb(var(--border))" strokeWidth="2" />
      <rect x="52" y="30" width="76" height="76" rx="10" fill="rgb(var(--hover))" />
      <circle cx="90" cy="68" r="22" fill="none" stroke="rgb(var(--accent))" strokeWidth="4" />
      {[0, 60, 120, 180, 240, 300].map((a) => <line key={a} x1="90" y1="68" x2={90 + 22 * Math.cos((a * Math.PI) / 180)} y2={68 + 22 * Math.sin((a * Math.PI) / 180)} stroke="rgb(var(--accent))" strokeWidth="3" strokeLinecap="round" />)}
      <circle cx="90" cy="68" r="6" fill="rgb(var(--accent))" />
      <g transform="translate(128 92) rotate(-30)">
        <circle cx="0" cy="0" r="9" fill="none" stroke="rgb(var(--warn))" strokeWidth="4" />
        <path d="M9 0h22M24 0v6M30 0v5" stroke="rgb(var(--warn))" strokeWidth="4" strokeLinecap="round" />
      </g>
      <circle cx="30" cy="34" r="3" fill="rgb(var(--label))" opacity=".6" /><circle cx="152" cy="28" r="2" fill="rgb(var(--ok))" opacity=".7" />
    </svg>
  );
}
