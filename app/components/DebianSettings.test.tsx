import { act } from 'react';
import { createRoot } from 'react-dom/client';
import { expect, it, vi } from 'vitest';
import { SaveLocationSettings } from './DebianSettings';

vi.mock('../context/I18nContext', () => ({ useI18n: () => ({ language: 'en', t: (key: string) => key }) }));

it('keeps a folder typed in Save locations without the Save button', async () => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.useFakeTimers();
  let stored = { folders: {} as Record<string, string>, auto_export: false };
  const page = () => ({ settings: stored, kinds: [{ key: 'final', audio: true, folder: stored.folders.final ?? null, writable: true, default: '/home/u/Music/YuE2 Studio' }] });
  const requests = vi.fn(async (_url: string, init?: RequestInit) => {
    if (init?.method === 'PUT') stored = JSON.parse(String(init.body));
    return Response.json(page());
  });
  vi.stubGlobal('fetch', requests);
  const host = document.createElement('div');
  const root = createRoot(host);
  try {
    await act(async () => root.render(<SaveLocationSettings />));
    await act(async () => { await vi.advanceTimersByTimeAsync(10); });
    const input = [...host.querySelectorAll('input')].find((item) => item.getAttribute('placeholder') === 'dbSavingPath' && !item.closest('.space-y-2'))!;
    await act(async () => {
      const setValue = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!;
      setValue.call(input, '/mnt/usb/Muzyka');
      input.dispatchEvent(new Event('input', { bubbles: true }));
    });
    expect(requests.mock.calls.some(([, init]) => init?.method === 'PUT')).toBe(false);
    await act(async () => { await vi.advanceTimersByTimeAsync(1500); });
    const put = requests.mock.calls.find(([, init]) => init?.method === 'PUT')!;
    expect(JSON.parse(String(put[1]?.body)).folders).toEqual({ final: '/mnt/usb/Muzyka' });
    // a path that is not a full one is not saved
    await act(async () => {
      const setValue = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!;
      setValue.call(input, 'Muzyka');
      input.dispatchEvent(new Event('input', { bubbles: true }));
    });
    await act(async () => { await vi.advanceTimersByTimeAsync(1500); });
    expect(requests.mock.calls.filter(([, init]) => init?.method === 'PUT')).toHaveLength(1);
  } finally {
    await act(async () => root.unmount());
    vi.useRealTimers();
    vi.unstubAllGlobals();
  }
});

it('fills a folder from Browse and keeps it', async () => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.useFakeTimers();
  let stored = { folders: {} as Record<string, string>, auto_export: false };
  const page = () => ({ settings: stored, kinds: [{ key: 'midi', audio: false, folder: stored.folders.midi ?? null, writable: true, default: '/home/u/Downloads/YuE2 Studio' }] });
  const requests = vi.fn(async (url: string, init?: RequestInit) => {
    if (url.endsWith('/pick')) return Response.json({ picked: true, path: '/media/usb/MIDI' });
    if (init?.method === 'PUT') stored = JSON.parse(String(init.body));
    return Response.json(page());
  });
  vi.stubGlobal('fetch', requests);
  const host = document.createElement('div');
  const root = createRoot(host);
  try {
    await act(async () => root.render(<SaveLocationSettings />));
    await act(async () => { await vi.advanceTimersByTimeAsync(10); });
    const buttons = [...host.querySelectorAll('button')].filter((item) => item.textContent?.includes('dbSavingBrowse'));
    expect(buttons).toHaveLength(2);
    await act(async () => { buttons[1].click(); await vi.advanceTimersByTimeAsync(10); });
    const pick = requests.mock.calls.find(([url]) => String(url).endsWith('/pick'))!;
    expect(JSON.parse(String(pick[1]?.body))).toEqual({ kind: 'midi', current: null });
    await act(async () => { await vi.advanceTimersByTimeAsync(1500); });
    const put = requests.mock.calls.find(([, init]) => init?.method === 'PUT')!;
    expect(JSON.parse(String(put[1]?.body)).folders).toEqual({ midi: '/media/usb/MIDI' });
  } finally {
    await act(async () => root.unmount());
    vi.useRealTimers();
    vi.unstubAllGlobals();
  }
});
