/**
 * `gp://<vault>/<item>/<field>` secret references for `gitpass run` / `gitpass inject`.
 * Mirrors crates/gitpass-cli/src/inject.rs (`encode_segment`, `field_segment`): a reference built here
 * resolves to the same field there. A reference names a field; it never contains its value.
 */
const STANDARD_FIELDS = ['password', 'username', 'url', 'notes', 'title', 'totp', 'otp'];

export interface RefField { id: string; label: string }

/** Percent-encode one segment: letters, digits (any script) and `-_.~` stay, everything else is %XX (UTF-8). */
export function encodeSegment(s: string): string {
  let out = '';
  for (const ch of s) {
    if (/[\p{L}\p{N}]/u.test(ch) || '-_.~'.includes(ch)) out += ch;
    else for (const b of new TextEncoder().encode(ch)) out += `%${b.toString(16).toUpperCase().padStart(2, '0')}`;
  }
  return out;
}

/** The field segment the resolver reads back as this field: a standard id as is, else a unique label, else the id. */
export function fieldSegment(field: RefField, fields: RefField[]): string {
  const id = field.id.toLowerCase();
  if (STANDARD_FIELDS.includes(id)) return id;
  const label = field.label.toLowerCase();
  const sameLabel = fields.filter((f) => f.label.toLowerCase() === label).length;
  const shadowed = STANDARD_FIELDS.includes(label);
  return sameLabel === 1 && !shadowed && field.label !== '' ? field.label : field.id;
}

export interface RefTarget {
  /** Vault name, or the vault id when another vault shares the name. */
  vault: string;
  /** Item title, or the item id when the title is empty or another item in the vault shares it. */
  item: string;
  /** Every field of the item, to tell duplicate labels apart. */
  fields: RefField[];
}

export function secretRef(target: RefTarget, field: RefField): string {
  return `gp://${encodeSegment(target.vault)}/${encodeSegment(target.item)}/${encodeSegment(fieldSegment(field, target.fields))}`;
}

/** Name the vault and item by title only while that name is unique, as the resolver needs. */
export function refTarget(
  vault: { id: string; name: string } | undefined,
  vaults: { id: string; name: string }[],
  item: { id: string; title: string },
  siblings: { id: string; title: string }[],
  fields: RefField[],
): RefTarget | null {
  if (!vault) return null;
  const vaultSeg = vault.name && vaults.filter((v) => v.name === vault.name).length === 1 ? vault.name : vault.id;
  const sameTitle = siblings.filter((i) => i.id !== item.id && i.title.toLowerCase() === item.title.toLowerCase()).length;
  const itemSeg = item.title && sameTitle === 0 ? item.title : item.id;
  return { vault: vaultSeg, item: itemSeg, fields };
}
