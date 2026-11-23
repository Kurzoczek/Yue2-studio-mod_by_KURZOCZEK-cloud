import { act } from 'react';
import { createRoot } from 'react-dom/client';
import { expect, it, vi } from 'vitest';
import { ChooseDownloadFolder, DataFolderChooser } from './ChooseFolder';

vi.mock('../context/I18nContext', () => ({ useI18n: () => ({ language: 'pl', t: (key: string) => key }) }));

const button = (host: HTMLElement, label: string) => [...host.querySelectorAll('button')].find((b) => b.textContent?.includes(label))!;

it('moves the model sets to a folder picked right where their path is shown', async () => {
  vi.useFakeTimers({ shouldAdvanceTime: true });
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  let polls = 0;
  const requests = vi.fn(async (url: string, init?: RequestInit) => {
    if (url === '/v1/storage/pick') return Response.json({ picked: true, path: '/media/usb', directory: '/media/usb/YuE2 Studio models', free_bytes: 50e9, copy_bytes: 9.9e9 });
    if (init?.method === 'PUT') return Response.json({ move: { running: true } });
    polls += 1;
    return Response.json({ move: { running: polls < 2, done_bytes: polls * 5e9, total_bytes: 9.9e9, moved: [], error: null } });
  });
  vi.stubGlobal('fetch', requests);
  const moved = vi.fn();
  const host = document.createElement('div');
  const root = createRoot(host);
  try {
    await act(async () => root.render(<ChooseDownloadFolder folder="models" onMoved={moved} />));
    await act(async () => button(host, 'dbChooseFolder').click());
    expect(JSON.parse(String(requests.mock.calls[0][1]?.body))).toEqual({ folder: 'models' });
    expect(host.textContent).toContain('dbChoosePickedCopy');
    await act(async () => { button(host, 'dbChooseMove').click(); await vi.advanceTimersByTimeAsync(2000); });
    const put = requests.mock.calls.find(([, init]) => init?.method === 'PUT')!;
    expect(JSON.parse(String(put[1]?.body))).toEqual({ directory: '/media/usb', folders: ['models'] });
    expect(moved).toHaveBeenCalled();
  } finally {
    await act(async () => root.unmount());
    vi.useRealTimers();
    vi.unstubAllGlobals();
  }
});

it('says why a move failed and keeps the folder where it was', async () => {
  vi.useFakeTimers({ shouldAdvanceTime: true });
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('fetch', vi.fn(async (url: string, init?: RequestInit) => {
    if (url === '/v1/storage/pick') return Response.json({ picked: true, path: '/media/usb', directory: '/media/usb/x', free_bytes: 1, copy_bytes: 2 });
    if (init?.method === 'PUT') return Response.json({ error: 'not enough room' }, { status: 400 });
    return Response.json({});
  }));
  const host = document.createElement('div');
  const root = createRoot(host);
  try {
    await act(async () => root.render(<ChooseDownloadFolder folder="models" />));
    await act(async () => button(host, 'dbChooseFolder').click());
    await act(async () => { button(host, 'dbChooseMove').click(); await vi.advanceTimersByTimeAsync(100); });
    expect(host.querySelector('[role="alert"]')!.textContent).toBe('not enough room');
  } finally {
    await act(async () => root.unmount());
    vi.useRealTimers();
    vi.unstubAllGlobals();
  }
});

it('plans a move of the whole data folder for the next start, and takes it back', async () => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  let pending: string | null = null;
  const requests = vi.fn(async (url: string, init?: RequestInit) => {
    if (url === '/v1/data-folder/pick') return Response.json({ picked: true, path: '/media/usb', target: '/media/usb/yue2-studio', free_bytes: 100e9, size_bytes: 12e9 });
    if (init?.method === 'PUT') pending = JSON.parse(String(init.body)).path ? '/media/usb/yue2-studio' : null;
    return Response.json({ current: '/mnt/iscsi/Modele_ai/yue2-studio', pending, size_bytes: 12e9, launcher: true });
  });
  vi.stubGlobal('fetch', requests);
  const host = document.createElement('div');
  const root = createRoot(host);
  try {
    await act(async () => root.render(<DataFolderChooser />));
    expect(host.querySelector('[data-testid="data-folder-pending"]')).toBeNull();
    await act(async () => button(host, 'dbChooseFolder').click());
    expect(host.textContent).toContain('dbDataPicked');
    await act(async () => button(host, 'dbDataPlan').click());
    const put = requests.mock.calls.find(([, init]) => init?.method === 'PUT')!;
    expect(JSON.parse(String(put[1]?.body))).toEqual({ path: '/media/usb' });
    expect(host.querySelector('[data-testid="data-folder-pending"]')!.textContent).toContain('dbDataPending');
    await act(async () => button(host, 'dbDataUnplan').click());
    expect(host.querySelector('[data-testid="data-folder-pending"]')).toBeNull();
  } finally {
    await act(async () => root.unmount());
    vi.unstubAllGlobals();
  }
});
