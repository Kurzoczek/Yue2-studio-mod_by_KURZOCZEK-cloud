// Debian build: the studio's style on top of light or dark - "standard", or "anime": the whole window recoloured
// (night sakura in dark, pastel sakura in light), softer corners, a glow on the main buttons, and, if wanted, sakura
// petals drifting down. Everything is CSS on the <html> element (see src-styles.css, `html.anime`), so no
// component of the studio changes; the choice is kept in this browser's storage, like light and dark.

export type Look = 'standard' | 'anime';

const LOOK_KEY = 'look';
const PETALS_KEY = 'look-petals';
const PETALS_ID = 'anime-petals';
const PETAL_COUNT = 18;

const read = (key: string): string | null => {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
};

const write = (key: string, value: string) => {
  try {
    localStorage.setItem(key, value);
  } catch {
    // a window without storage keeps the choice until it closes
  }
};

export function currentLook(): Look {
  return read(LOOK_KEY) === 'anime' ? 'anime' : 'standard';
}

/** Petals are on unless turned off; they never fall for someone who asked the system for less motion. */
export function petalsWanted(): boolean {
  return read(PETALS_KEY) !== 'off';
}

const listeners = new Set<() => void>();

/** Hears every change of the look, so a settings page shows what is on. */
export function onLookChange(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function setLook(look: Look) {
  write(LOOK_KEY, look);
  applyLook();
}

export function setPetals(on: boolean) {
  write(PETALS_KEY, on ? 'on' : 'off');
  applyLook();
}

/** Puts the chosen look on the window: the class the styles hang on, and the petals. Safe to call again. */
export function applyLook() {
  if (typeof document === 'undefined') return;
  const anime = currentLook() === 'anime';
  document.documentElement.classList.toggle('anime', anime);
  const existing = document.getElementById(PETALS_ID);
  if (anime && petalsWanted()) {
    if (!existing) document.body.appendChild(petals());
  } else {
    existing?.remove();
  }
  listeners.forEach((listener) => listener());
}

/** A layer of petals over the window: it takes no clicks, and each petal has its own place, size, speed and sway. */
function petals(): HTMLElement {
  const layer = document.createElement('div');
  layer.id = PETALS_ID;
  layer.setAttribute('aria-hidden', 'true');
  for (let index = 0; index < PETAL_COUNT; index += 1) {
    const petal = document.createElement('span');
    petal.className = 'anime-petal';
    // spread evenly across the width, varied by a fixed pattern (the same each time: no flicker on a new layer)
    const spread = (index * 37) % 100;
    const size = 8 + ((index * 7) % 9);
    petal.style.left = `${spread}%`;
    petal.style.width = `${size}px`;
    petal.style.height = `${Math.round(size * 0.75)}px`;
    petal.style.animationDuration = `${11 + ((index * 5) % 9)}s, ${3 + (index % 4)}s`;
    petal.style.animationDelay = `${-((index * 13) % 17)}s, ${-(index % 3)}s`;
    petal.style.opacity = `${0.45 + ((index * 3) % 5) / 10}`;
    layer.appendChild(petal);
  }
  return layer;
}
