/**
 * Where the studio service lives.
 *
 * In the packaged desktop application the UI is served from Tauri's asset
 * protocol, so a relative `/setup/status` never reaches the service — it
 * resolves against the asset host and comes back as the HTML shell, which the
 * caller then fails to parse as JSON. Development is different again: the Vite
 * server proxies those paths.
 *
 * One rule covers both: address the service directly on loopback. The service
 * answers with permissive CORS, so this works from the desktop shell, from the
 * dev server, and from a plain browser tab.
 */

const DEFAULT_BASE = 'http://127.0.0.1:8791';

/** Paths owned by the studio service; everything else is left untouched. */
const SERVICE_PREFIXES = ['/v1/', '/setup/', '/engine/', '/health'];

function resolveBase(): string {
  const override = (import.meta as { env?: Record<string, string | undefined> }).env?.VITE_STUDIO_API_BASE;
  if (override) return override.replace(/\/$/, '');
  // Only the desktop window's own pages live on another host than the service.
  // A browser - the dev server, which proxies these paths, or the service
  // itself handing the page to another computer, on whatever port a router
  // forwards - keeps requests same-origin.
  if (typeof location !== 'undefined' && /^https?:$/.test(location.protocol) && location.hostname !== 'tauri.localhost') return '';
  return DEFAULT_BASE;
}

export const API_BASE = resolveBase();

/**
 * The local API key the desktop shell hands its own pages (window.__STUDIO_KEY__). Every request of the window to
 * the service carries it in its address (`studio_key=`), which also covers <audio src>, EventSource and downloads
 * that cannot send a header; the service takes it out before any handler or log sees it. A page the service itself
 * serves (a browser tab, the audio editor) has no such value and is signed in by a cookie instead.
 */
const LOCAL_KEY: string | undefined = typeof window !== 'undefined'
  ? (window as unknown as { __STUDIO_KEY__?: string }).__STUDIO_KEY__
  : undefined;

function isServicePath(path: string): boolean {
  return SERVICE_PREFIXES.some((prefix) => path === prefix || path.startsWith(prefix));
}

/** The address with the local key added, when it is one of the service's and carries none yet. */
export function withLocalKey<T extends string | undefined>(url: T): T {
  if (!url || !LOCAL_KEY || !API_BASE || !url.startsWith(`${API_BASE}/`)) return url;
  if (/[?&]studio_key=/.test(url)) return url;
  const hash = url.indexOf('#');
  const [head, tail] = hash >= 0 ? [url.slice(0, hash), url.slice(hash)] : [url, ''];
  return `${head}${head.includes('?') ? '&' : '?'}studio_key=${encodeURIComponent(LOCAL_KEY)}${tail}` as T;
}

export function apiUrl(path: string): string {
  if (!API_BASE) return path;
  if (!path.startsWith('/')) return withLocalKey(path);
  return withLocalKey(`${API_BASE}${path}`);
}

/**
 * Rewrites service-owned relative requests once, centrally, so every caller can
 * keep using readable paths and no screen can be left behind pointing at the
 * wrong host.
 */
export function installApiBase(): void {
  if (!API_BASE || typeof window === 'undefined') return;
  const original = window.fetch.bind(window);
  window.fetch = (input: RequestInfo | URL, init?: RequestInit) => {
    if (typeof input === 'string' && input.startsWith('/') && isServicePath(input)) {
      return original(apiUrl(input), init);
    }
    if (typeof input === 'string') {
      return original(withLocalKey(input), init);
    }
    if (input instanceof URL) {
      return original(withLocalKey(input.toString()), init);
    }
    if (input instanceof Request && input.url.startsWith(location.origin)) {
      const path = input.url.slice(location.origin.length);
      if (isServicePath(path)) return original(new Request(apiUrl(path), input));
    }
    if (input instanceof Request && withLocalKey(input.url) !== input.url) {
      return original(new Request(withLocalKey(input.url), input));
    }
    return original(input, init);
  };
}
