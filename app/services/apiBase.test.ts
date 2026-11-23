import { afterEach, describe, expect, it, vi } from 'vitest';

async function load(key: string | undefined) {
  vi.resetModules();
  // the desktop window: its pages live on tauri://localhost, the service on loopback
  vi.stubGlobal('location', { protocol: 'tauri:', hostname: 'localhost', origin: 'tauri://localhost' });
  (window as unknown as { __STUDIO_KEY__?: string }).__STUDIO_KEY__ = key;
  return import('./apiBase');
}

afterEach(() => {
  vi.unstubAllGlobals();
  delete (window as unknown as { __STUDIO_KEY__?: string }).__STUDIO_KEY__;
});

describe('the local API key', () => {
  it('rides along in every address of the service', async () => {
    const { apiUrl, withLocalKey } = await load('k e/y');
    expect(apiUrl('/v1/library/media/1')).toBe('http://127.0.0.1:8791/v1/library/media/1?studio_key=k%20e%2Fy');
    expect(apiUrl('/v1/a?v=2')).toBe('http://127.0.0.1:8791/v1/a?v=2&studio_key=k%20e%2Fy');
    expect(apiUrl('http://127.0.0.1:8791/v1/b#t')).toBe('http://127.0.0.1:8791/v1/b?studio_key=k%20e%2Fy#t');
    expect(withLocalKey(apiUrl('/v1/c'))).toBe(apiUrl('/v1/c'));
  });

  it('never goes to another address, and is absent without a key', async () => {
    const { apiUrl, withLocalKey } = await load('key');
    expect(withLocalKey('https://example.com/a.mp3')).toBe('https://example.com/a.mp3');
    expect(withLocalKey('http://127.0.0.1:9999/a')).toBe('http://127.0.0.1:9999/a');
    expect(withLocalKey(undefined)).toBeUndefined();
    expect(apiUrl('blob:abc')).toBe('blob:abc');
    const keyless = await load(undefined);
    expect(keyless.apiUrl('/v1/a')).toBe('http://127.0.0.1:8791/v1/a');
  });
});
