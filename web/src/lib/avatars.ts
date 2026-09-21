// Reuses icons other views already import (Film, Sparkles) where it fits,
// so the avatar library adds as little to the bundle as it can.
import { Bot, Cat, Crown, Dog, Flame, Film, Gamepad2, Ghost, Heart, Rocket, Sparkles, Zap } from 'lucide-svelte';

export interface AvatarColor {
  key: string;
  label: string;
  color: string;
}

export const AVATAR_COLORS: AvatarColor[] = [
  { key: 'amber', label: 'Amber', color: '#f59e0b' },
  { key: 'emerald', label: 'Emerald', color: '#10b981' },
  { key: 'cobalt', label: 'Cobalt', color: '#3b82f6' },
  { key: 'purple', label: 'Purple', color: '#a855f7' },
  { key: 'rose', label: 'Rose', color: '#f43f5e' },
  { key: 'slate', label: 'Slate', color: '#64748b' },
];

export interface AvatarIcon {
  key: string;
  label: string;
  icon: typeof Cat;
}

export const AVATAR_ICONS: AvatarIcon[] = [
  { key: 'cat', label: 'Cat', icon: Cat },
  { key: 'dog', label: 'Dog', icon: Dog },
  { key: 'ghost', label: 'Ghost', icon: Ghost },
  { key: 'rocket', label: 'Rocket', icon: Rocket },
  { key: 'flame', label: 'Flame', icon: Flame },
  { key: 'zap', label: 'Lightning', icon: Zap },
  { key: 'gamepad', label: 'Gamepad', icon: Gamepad2 },
  { key: 'film', label: 'Film reel', icon: Film },
  { key: 'crown', label: 'Crown', icon: Crown },
  { key: 'heart', label: 'Heart', icon: Heart },
  { key: 'bot', label: 'Robot', icon: Bot },
  { key: 'sparkles', label: 'Sparkles', icon: Sparkles },
];

const DEFAULT_COLOR = AVATAR_COLORS[0];

export interface ParsedAvatar {
  colorKey: string;
  color: string;
  iconKey: string | null;
}

/**
 * An `avatar_key` is either a plain color name from before the avatar
 * library existed ("amber"), or "color:icon" ("amber:rocket"). Both forms
 * are read here, so a profile created before this feature still renders -
 * as its stored color with initials, since it has no icon half.
 */
export function parseAvatarKey(key: string): ParsedAvatar {
  const [colorKey, iconKey] = (key || '').split(':');
  const color = AVATAR_COLORS.find((entry) => entry.key === colorKey) ?? DEFAULT_COLOR;
  return { colorKey: color.key, color: color.color, iconKey: iconKey || null };
}

export function avatarKeyOf(colorKey: string, iconKey: string | null): string {
  return iconKey ? `${colorKey}:${iconKey}` : colorKey;
}

export function iconFor(iconKey: string | null): typeof Cat | null {
  return AVATAR_ICONS.find((entry) => entry.key === iconKey)?.icon ?? null;
}
