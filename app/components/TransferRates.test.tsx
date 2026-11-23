import { act } from 'react';
import { createRoot } from 'react-dom/client';
import { expect, it, vi } from 'vitest';
import { TransferRates, rate } from './TransferRates';

vi.mock('../context/I18nContext', () => ({ useI18n: () => ({ language: 'pl', t: (key: string) => key }) }));

it('reads rates as people do', () => {
  expect(rate(0, 'pl')).toBe('0 B/s');
  expect(rate(830_000, 'pl')).toBe('830 kB/s');
  expect(rate(12_400_000, 'pl')).toBe('12 MB/s');
  expect(rate(5_400_000, 'pl')).toBe('5,4 MB/s');
  expect(rate(1_100_000_000, 'en')).toBe('1.1 GB/s');
});

it('shows the download speed and what each disk reads and writes, asked again each second', async () => {
  vi.useFakeTimers();
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  let download = 2_500_000;
  const requests = vi.fn(async () => Response.json({
    download_bytes_per_second: download,
    disks: [{ device: 'sdb1', mount: '/media/kurzoczek/Nowy', file_system: 'ntfs3', folders: ['music-engines', 'music-models'], read_bytes_per_second: 0, write_bytes_per_second: 24_000_000 }],
  }));
  vi.stubGlobal('fetch', requests);
  const host = document.createElement('div');
  const root = createRoot(host);
  try {
    await act(async () => root.render(<TransferRates />));
    expect(host.querySelector('[data-testid="rate-download"]')!.textContent).toBe('2,5 MB/s');
    expect(host.textContent).toContain('/media/kurzoczek/Nowy');
    expect(host.textContent).toContain('dbRatesWrite 24 MB/s');
    download = 0;
    await act(async () => { vi.advanceTimersByTime(1000); });
    expect(requests).toHaveBeenCalledTimes(2);
    expect(host.querySelector('[data-testid="rate-download"]')!.textContent).toBe('0 B/s');
  } finally {
    await act(async () => root.unmount());
    vi.useRealTimers();
    vi.unstubAllGlobals();
  }
});

it('shows nothing for an answer of another shape', async () => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('fetch', vi.fn(async () => Response.json({ ok: true })));
  const host = document.createElement('div');
  const root = createRoot(host);
  try {
    await act(async () => root.render(<TransferRates />));
    expect(host.querySelector('[data-testid="transfer-rates"]')).toBeNull();
  } finally {
    await act(async () => root.unmount());
    vi.unstubAllGlobals();
  }
});
