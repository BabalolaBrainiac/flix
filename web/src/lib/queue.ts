import { writable } from 'svelte/store';
import * as api from './api';
import type { PlayCommand, QueueItem } from './types';

export const queueItems = writable<QueueItem[]>([]);

export async function refreshQueue(): Promise<void> {
  const { items } = await api.listQueue();
  queueItems.set(items);
}

export async function addToQueue(command: PlayCommand): Promise<void> {
  await api.addToQueue(command);
  await refreshQueue();
}

export async function playQueuedItem(id: string): Promise<void> {
  await api.playQueued(id);
  queueItems.update((items) => items.filter((item) => item.id !== id));
}

export async function removeQueuedItem(id: string): Promise<void> {
  queueItems.update((items) => items.filter((item) => item.id !== id));
  try {
    await api.removeFromQueue(id);
  } catch (error) {
    await refreshQueue();
    throw error;
  }
}
