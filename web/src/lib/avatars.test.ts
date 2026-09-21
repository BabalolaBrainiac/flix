import { describe, expect, it } from 'vitest';
import { AVATAR_COLORS, AVATAR_ICONS, avatarKeyOf, iconFor, parseAvatarKey } from './avatars';

describe('parseAvatarKey', () => {
  it('reads a plain color key from before the avatar library existed', () => {
    expect(parseAvatarKey('emerald')).toEqual({ colorKey: 'emerald', color: '#10b981', iconKey: null });
  });

  it('reads a color and an icon from a combined key', () => {
    expect(parseAvatarKey('amber:rocket')).toEqual({ colorKey: 'amber', color: '#f59e0b', iconKey: 'rocket' });
  });

  it('falls back to the default color for an unknown or empty key', () => {
    expect(parseAvatarKey('nonsense').colorKey).toBe(AVATAR_COLORS[0].key);
    expect(parseAvatarKey('').colorKey).toBe(AVATAR_COLORS[0].key);
  });
});

describe('avatarKeyOf', () => {
  it('combines a color and an icon, or omits the icon half when there is none', () => {
    expect(avatarKeyOf('rose', 'ghost')).toBe('rose:ghost');
    expect(avatarKeyOf('rose', null)).toBe('rose');
  });

  it('round-trips through parseAvatarKey', () => {
    const key = avatarKeyOf('cobalt', 'star');
    expect(parseAvatarKey(key)).toEqual({ colorKey: 'cobalt', color: '#3b82f6', iconKey: 'star' });
  });
});

describe('iconFor', () => {
  it('finds every listed icon by key', () => {
    for (const entry of AVATAR_ICONS) {
      expect(iconFor(entry.key)).toBe(entry.icon);
    }
  });

  it('returns null for no icon or an unknown key', () => {
    expect(iconFor(null)).toBeNull();
    expect(iconFor('not-a-real-icon')).toBeNull();
  });
});
