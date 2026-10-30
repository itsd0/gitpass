import type { RotationInfo } from "../../api/types";
import type { Key } from "../../i18n";

/** "Change password every…" choices, in days (mirrors core `ROTATION_DAYS`). */
export const ROTATION_CHOICES = [30, 90, 180, 365] as const;

export type TranslateFn = (key: Key, vars?: Record<string, string | number>) => string;

/** Label for the "Due for rotation" / "Rotate in N days" pill; `null` when nothing is due. */
export function rotationLabel(rotation: RotationInfo | null | undefined, t: TranslateFn): string | null {
  if (!rotation) return null;
  if (rotation.state === "due") return t("rotation.due");
  return rotation.days === 1 ? t("rotation.soonDay") : t("rotation.soonDays", { n: rotation.days });
}
