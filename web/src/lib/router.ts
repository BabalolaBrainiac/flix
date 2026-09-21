import { writable, get } from 'svelte/store';

export type RouteName =
  | 'discover'
  | 'library'
  | 'downloads'
  | 'reader'
  | 'diagnostics'
  | 'title';

export type LibraryTab = 'lists' | 'status' | 'now-playing';

export interface Route {
  name: RouteName;
  library?: LibraryTab;
  titleId?: string;
}

const LIBRARY_TABS: LibraryTab[] = ['lists', 'status', 'now-playing'];
const SIMPLE_ROUTES: RouteName[] = ['discover', 'downloads', 'reader', 'diagnostics'];
export const DEFAULT_ROUTE: Route = { name: 'discover' };

function safeDecode(value: string): string {
  try {
    return decodeURIComponent(value);
  } catch {
    return value;
  }
}

function parseLibrary(sub: string | undefined): Route {
  const tab = LIBRARY_TABS.find((candidate) => candidate === sub);
  return { name: 'library', library: tab ?? 'lists' };
}

/** Turns a URL path into a route. An unknown path gives the default route. */
export function parsePath(pathname: string): Route {
  const [first, second] = pathname.split('/').filter(Boolean);
  if (first === 'library') return parseLibrary(second);
  if (first === 'title' && second) return { name: 'title', titleId: safeDecode(second) };
  const simple = SIMPLE_ROUTES.find((name) => name === first);
  return simple ? { name: simple } : DEFAULT_ROUTE;
}

/** Turns a route into its URL path. The path never holds the session token. */
export function formatPath(route: Route): string {
  if (route.name === 'library') return `/library/${route.library ?? 'lists'}`;
  if (route.name === 'title') return `/title/${encodeURIComponent(route.titleId ?? '')}`;
  return `/${route.name}`;
}

export function sameRoute(a: Route, b: Route): boolean {
  return formatPath(a) === formatPath(b);
}

interface EntryState {
  index: number;
  scrollY: number;
}

function readState(): EntryState {
  const state = (typeof history !== 'undefined' ? history.state : null) as Partial<EntryState> | null;
  return { index: state?.index ?? 0, scrollY: state?.scrollY ?? 0 };
}

export const currentRoute = writable<Route>(DEFAULT_ROUTE);

// Views stay mounted while hidden, so state such as results and page number
// survives. Only the page scroll position needs saving and restoring.
function restoreScroll(scrollY: number) {
  requestAnimationFrame(() => requestAnimationFrame(() => window.scrollTo(0, scrollY)));
}

function handlePopState() {
  currentRoute.set(parsePath(window.location.pathname));
  restoreScroll(readState().scrollY);
}

/**
 * Starts the router. Returns a function that stops it. Call it once, after
 * the API module has moved the session token out of the address bar.
 */
export function initRouter(): () => void {
  if ('scrollRestoration' in history) history.scrollRestoration = 'manual';
  const route = parsePath(window.location.pathname);
  const state = readState();
  history.replaceState({ ...state }, '', formatPath(route));
  currentRoute.set(route);
  window.addEventListener('popstate', handlePopState);
  return () => window.removeEventListener('popstate', handlePopState);
}

/** Opens a route. It saves the scroll position of the page it leaves. */
export function navigate(route: Route, options: { replace?: boolean } = {}) {
  const from = readState();
  if (sameRoute(get(currentRoute), route)) return;
  if (options.replace) {
    history.replaceState({ index: from.index, scrollY: 0 }, '', formatPath(route));
  } else {
    history.replaceState({ ...from, scrollY: window.scrollY }, '');
    history.pushState({ index: from.index + 1, scrollY: 0 }, '', formatPath(route));
    window.scrollTo(0, 0);
  }
  currentRoute.set(route);
}

/** Goes back one entry when the app made one. Otherwise opens `fallback`. */
export function goBack(fallback: Route) {
  if (readState().index > 0) {
    history.back();
  } else {
    navigate(fallback, { replace: true });
  }
}
