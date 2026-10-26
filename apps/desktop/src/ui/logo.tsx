import logoUrl from '../assets/logo.png';
/** gitpass mark: the app icon (orange keyhole + git fork). */
export function Logo({ size = 28, className }: { size?: number; className?: string }) {
  return <img src={logoUrl} width={size} height={size} alt="gitpass" draggable={false} className={['shrink-0 rounded-[22%]', className].filter(Boolean).join(' ')} />;
}
