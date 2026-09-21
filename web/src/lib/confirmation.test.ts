import { describe, expect, it } from 'vitest';
import { confirmationFor } from './confirmation';

describe('confirmationFor', () => {
  it('gives downloaded files a clear destructive action', () => {
    expect(confirmationFor('download', 'Arrival')).toEqual({
      title: 'Delete download?',
      message: 'This deletes “Arrival” and its downloaded files from this Mac.',
      confirmLabel: 'Delete download',
    });
  });

  it('uses direct labels for profile, list, and service actions', () => {
    expect(confirmationFor('profile', 'Nia').confirmLabel).toBe('Delete profile');
    expect(confirmationFor('list', 'Weekend').confirmLabel).toBe('Delete list');
    expect(confirmationFor('quit').confirmLabel).toBe('Quit Flix');
  });
});
