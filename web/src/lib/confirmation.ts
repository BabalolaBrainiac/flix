export type ConfirmationAction = 'download' | 'list' | 'profile' | 'quit';

export interface ConfirmationCopy {
  title: string;
  message: string;
  confirmLabel: string;
}

export function confirmationFor(action: ConfirmationAction, itemName?: string): ConfirmationCopy {
  const name = itemName ? `“${itemName}”` : 'this item';

  switch (action) {
    case 'download':
      return {
        title: 'Delete download?',
        message: `This deletes ${name} and its downloaded files from this Mac.`,
        confirmLabel: 'Delete download',
      };
    case 'list':
      return {
        title: 'Delete list?',
        message: `This permanently deletes ${name}. Its titles stay in your library.`,
        confirmLabel: 'Delete list',
      };
    case 'profile':
      return {
        title: 'Delete profile?',
        message: `This permanently deletes ${name} and its saved lists.`,
        confirmLabel: 'Delete profile',
      };
    case 'quit':
      return {
        title: 'Quit Flix?',
        message: 'This stops the local service and active media players.',
        confirmLabel: 'Quit Flix',
      };
  }
}
