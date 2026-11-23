import { act } from 'react';
import { createRoot } from 'react-dom/client';
import { afterEach, expect, it, vi } from 'vitest';
import { LookSetting } from './LookSetting';
import { applyLook, setLook } from '../services/look';

vi.mock('../context/I18nContext', () => ({ useI18n: () => ({ language: 'pl', t: (key: string) => key }) }));

afterEach(() => {
  localStorage.clear();
  applyLook();
});

it('puts the anime style on the whole window, with petals that can be turned off, and takes it away', async () => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  const host = document.createElement('div');
  const root = createRoot(host);
  try {
    await act(async () => root.render(<LookSetting />));
    expect(document.documentElement.classList.contains('anime')).toBe(false);
    expect(host.querySelector('[data-testid="look-petals"]')).toBeNull();

    await act(async () => host.querySelector<HTMLButtonElement>('[data-testid="look-anime"]')!.click());
    expect(document.documentElement.classList.contains('anime')).toBe(true);
    expect(localStorage.getItem('look')).toBe('anime');
    const layer = document.getElementById('anime-petals')!;
    expect(layer.getAttribute('aria-hidden')).toBe('true');
    expect(layer.querySelectorAll('.anime-petal').length).toBe(18);

    // the petals go, the style stays
    await act(async () => host.querySelector<HTMLInputElement>('[data-testid="look-petals"]')!.click());
    expect(document.getElementById('anime-petals')).toBeNull();
    expect(document.documentElement.classList.contains('anime')).toBe(true);
    expect(localStorage.getItem('look-petals')).toBe('off');

    await act(async () => host.querySelector<HTMLButtonElement>('[data-testid="look-standard"]')!.click());
    expect(document.documentElement.classList.contains('anime')).toBe(false);
    expect(host.querySelector('[data-testid="look-standard"]')!.getAttribute('aria-pressed')).toBe('true');
  } finally {
    await act(async () => root.unmount());
    vi.unstubAllGlobals();
  }
});

it('comes back at the next start as it was left, and makes one layer of petals however often it is applied', () => {
  setLook('anime');
  document.documentElement.classList.remove('anime');
  applyLook();
  applyLook();
  expect(document.documentElement.classList.contains('anime')).toBe(true);
  expect(document.querySelectorAll('#anime-petals').length).toBe(1);
});
