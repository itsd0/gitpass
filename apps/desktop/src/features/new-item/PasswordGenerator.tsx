import { useEffect, useState } from 'react';
import { RefreshCw, Wand2 } from 'lucide-react';
import { api } from '../../api/client';
import { Button } from '../../ui';
export function StrengthMeter({ password }: { password: string }) {
  const [score, setScore] = useState<number | null>(null);
  useEffect(() => { let live = true; if (!password) { setScore(null); return; } const t = setTimeout(() => api.passwordStrength({ password }).then(s => live && setScore(s)).catch(() => live && setScore(null)), 150); return () => { live = false; clearTimeout(t); }; }, [password]);
  if (score === null) return null;
  const label = score < 40 ? 'Weak' : score < 70 ? 'Fair' : score < 90 ? 'Good' : 'Excellent';
  const color = score < 40 ? 'bg-danger' : score < 70 ? 'bg-warn' : score < 90 ? 'bg-lime-500' : 'bg-emerald-500';
  return (<div className="mt-1 flex items-center gap-2" aria-label={`Password strength ${label}`}>
    <div className="h-1.5 flex-1 rounded bg-hover"><div className={`h-1.5 rounded ${color}`} style={{ width: `${score}%` }} /></div>
    <span className="w-16 text-right text-xs text-muted">{label}</span></div>);
}
export function PasswordGenerator({ onUse }: { onUse: (pw: string) => void }) {
  const [open, setOpen] = useState(false);
  const [length, setLength] = useState(20); const [symbols, setSymbols] = useState(true); const [digits, setDigits] = useState(true); const [words, setWords] = useState(false);
  const [pw, setPw] = useState(''); const [err, setErr] = useState<string | null>(null);
  const gen = () => api.generatePassword({ length, symbols, digits, words }).then(p => { setPw(p); setErr(null); }).catch(e => setErr(String(e)));
  useEffect(() => { if (open) gen(); }, [open, length, symbols, digits, words]); // eslint-disable-line
  return (<div className="relative">
    <Button type="button" size="sm" variant="ghost" aria-label="Generate password" onClick={() => setOpen(o => !o)}><Wand2 size={14} /> Generate</Button>
    {open && (<div role="dialog" aria-label="Password generator" className="absolute right-0 z-20 mt-1 w-80 rounded-lg border border-border bg-panel p-4 shadow-xl">
      <div className="flex items-center gap-2"><code className="flex-1 break-all rounded bg-bg px-2 py-1.5 font-mono text-sm">{pw || '…'}</code>
        <Button type="button" size="icon" variant="ghost" aria-label="Regenerate" onClick={gen}><RefreshCw size={14} /></Button></div>
      <StrengthMeter password={pw} />
      {err && <p className="mt-1 text-xs text-danger">{err}</p>}
      <label className="mt-3 block text-xs text-muted">{words ? 'Characters (approx.)' : 'Length'}: {length}
        <input type="range" min={8} max={64} value={length} onChange={e => setLength(+e.target.value)} className="w-full accent-accent" /></label>
      <div className="mt-2 grid grid-cols-3 gap-2 text-xs">
        <label className="flex items-center gap-1"><input type="checkbox" checked={words} onChange={e => setWords(e.target.checked)} /> Words</label>
        <label className="flex items-center gap-1"><input type="checkbox" checked={digits} disabled={words} onChange={e => setDigits(e.target.checked)} /> Digits</label>
        <label className="flex items-center gap-1"><input type="checkbox" checked={symbols} disabled={words} onChange={e => setSymbols(e.target.checked)} /> Symbols</label></div>
      <div className="mt-3 flex justify-end gap-2"><Button type="button" size="sm" variant="ghost" onClick={() => setOpen(false)}>Cancel</Button>
        <Button type="button" size="sm" variant="default" disabled={!pw} onClick={() => { onUse(pw); setOpen(false); }}>Use password</Button></div>
    </div>)}</div>);
}
