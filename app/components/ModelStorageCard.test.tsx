import { act } from 'react';
import { createRoot } from 'react-dom/client';
import { expect, it, vi } from 'vitest';
import { ModelStorageCard } from './ModelStorageCard';

vi.mock('../context/I18nContext', () => ({ useI18n: () => ({ language: 'en', t: (key: string) => key }) }));

it('moves the engines on their own to a folder picked with Browse', async () => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  const storage = {
    directory: '/data', chosen: null, default: '/data', free_bytes: 1e12, total_bytes: 2e12, used_bytes: 30,
    folders: [
      { name: 'models', bytes: 10, elsewhere: false, location: '/data/models' },
      { name: 'music-engines', bytes: 20, elsewhere: false, location: '/data/music-engines' },
      { name: 'music-models', bytes: 0, elsewhere: true, location: '/big/YuE2 Studio models/music-models' },
      { name: 'karaoke', bytes: 0, elsewhere: false, location: '/data/karaoke' },
    ],
  };
  const idle = { running: false, folder: null, done_bytes: 0, total_bytes: 0, moved: [], error: null, engine_error: null };
  const requests = vi.fn(async (url: string, init?: RequestInit) => {
    if (url.endsWith('/v1/storage/pick')) {
      return Response.json({ picked: true, path: '/fast', directory: '/fast/YuE2 Studio models', free_bytes: 1e9, total_bytes: 2e9, copy_bytes: 20, folder: JSON.parse(String(init?.body)).folder });
    }
    return Response.json({ storage, move: idle });
  });
  vi.stubGlobal('fetch', requests);
  const host = document.createElement('div');
  const root = createRoot(host);
  try {
    await act(async () => root.render(<ModelStorageCard />));
    const list = host.querySelector('[data-testid="model-storage-folders"]')!;
    // every folder is listed, also an empty one, so its place can be chosen before the download
    expect(list.textContent).toContain('storageFolder_karaoke');
    expect(list.textContent).toContain('dbStorageEmpty');
    expect(list.textContent).toContain('/big/YuE2 Studio models/music-models');
    const browse = host.querySelector<HTMLButtonElement>('[data-testid="storage-browse-music-engines"]')!;
    await act(async () => browse.click());
    const pick = requests.mock.calls.find(([url]) => String(url).endsWith('/v1/storage/pick'))!;
    expect(JSON.parse(String(pick[1]?.body))).toEqual({ folder: 'music-engines' });
    expect(host.textContent).toContain('dbStoragePickedOne');
    const move = [...host.querySelectorAll('button')].find((button) => button.textContent === 'storageMove')!;
    await act(async () => move.click());
    const put = requests.mock.calls.find(([, init]) => init?.method === 'PUT')!;
    expect(JSON.parse(String(put[1]?.body))).toEqual({ directory: '/fast', folders: ['music-engines'] });
  } finally {
    await act(async () => root.unmount());
    vi.unstubAllGlobals();
  }
});
