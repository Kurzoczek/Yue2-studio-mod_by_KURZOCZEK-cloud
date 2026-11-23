// Debian build: Settings -> Appearance -> Style - standard or anime (on top of light or dark), and the petals.
import React, { useEffect, useState } from 'react';
import { Sparkles } from 'lucide-react';
import { useI18n } from '../context/I18nContext';
import { currentLook, onLookChange, petalsWanted, setLook, setPetals, type Look } from '../services/look';

export const LookSetting: React.FC = () => {
  const { t } = useI18n();
  const [look, setLookState] = useState<Look>(currentLook);
  const [petals, setPetalsState] = useState(petalsWanted);
  useEffect(() => onLookChange(() => { setLookState(currentLook()); setPetalsState(petalsWanted()); }), []);

  const choice = (value: Look, label: React.ReactNode) => (
    <button
      type="button"
      onClick={() => setLook(value)}
      aria-pressed={look === value}
      data-testid={`look-${value}`}
      className={`flex flex-1 items-center justify-center gap-1.5 rounded-lg border-2 px-4 py-3 font-medium transition-colors ${look === value ? 'border-pink-500 bg-pink-500/10 text-pink-600 dark:text-pink-300' : 'border-zinc-300 dark:border-zinc-700'}`}
    >
      {label}
    </button>
  );

  return (
    <div className="space-y-3" data-testid="look-setting">
      <span className="text-sm font-semibold text-zinc-900 dark:text-white">{t('dbLook')}</span>
      <div className="flex gap-3">
        {choice('standard', t('dbLookStandard'))}
        {choice('anime', <><Sparkles size={15} />{t('dbLookAnime')}</>)}
      </div>
      {look === 'anime' && (
        <>
          <p className="text-xs leading-5 text-zinc-500">{t('dbLookHint')}</p>
          <label className="flex items-center gap-2 text-sm text-zinc-700 dark:text-zinc-200">
            <input type="checkbox" checked={petals} onChange={(event) => setPetals(event.target.checked)} className="accent-pink-500" data-testid="look-petals" />
            {t('dbLookPetals')}
          </label>
        </>
      )}
    </div>
  );
};
