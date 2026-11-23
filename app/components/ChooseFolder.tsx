// Debian build: a folder the studio shows (the model sets, its own data) is no longer only shown - it can be
// changed right where it is shown.
//  * ChooseDownloadFolder: one of the download folders (the model sets: "models"), moved at once, as in
//    Settings -> Models -> Where the models are kept, with the same checks.
//  * DataFolderChooser: the whole data folder; the library is open while the studio runs, so the move is
//    written down and done by the program that starts the studio, before its next start.
import React, { useEffect, useState } from 'react';
import { FolderSearch, Loader2, X } from 'lucide-react';
import { useI18n } from '../context/I18nContext';

const BUTTON = 'inline-flex items-center gap-1 rounded-lg border border-zinc-300 px-2 py-1 text-[11px] font-medium text-zinc-600 hover:border-pink-400 hover:text-pink-600 disabled:opacity-40 dark:border-white/15 dark:text-zinc-300';
const PRIMARY = 'inline-flex items-center gap-1 rounded-lg bg-pink-600 px-2 py-1 text-[11px] font-semibold text-white hover:bg-pink-500 disabled:opacity-40';

const gigabytes = (bytes: number, language: string) => `${(bytes / 1e9).toLocaleString(language, { maximumFractionDigits: 1 })} GB`;

async function answer(response: Response) {
  const body = await response.json().catch(() => ({}));
  if (!response.ok) throw new Error(body?.error ?? body?.message ?? `HTTP ${response.status}`);
  return body;
}

interface Picked { path: string; directory: string; free_bytes: number; copy_bytes: number }

/** Change where a download folder ("models") is kept, from wherever its path is shown. */
export const ChooseDownloadFolder: React.FC<{ folder: string; onMoved?: () => void }> = ({ folder, onMoved }) => {
  const { t, language } = useI18n();
  const [picked, setPicked] = useState<Picked | null>(null);
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);

  const choose = async () => {
    setError(null);
    setBusy(true);
    try {
      const body = await answer(await fetch('/v1/storage/pick', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ folder }) }));
      if (body.picked) setPicked(body);
    } catch (failure) {
      setError(failure instanceof Error ? failure.message : String(failure));
    } finally {
      setBusy(false);
    }
  };

  const move = async () => {
    if (!picked) return;
    setError(null);
    setBusy(true);
    setProgress(0);
    try {
      await answer(await fetch('/v1/storage', { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ directory: picked.path, folders: [folder] }) }));
      // how far it has come, until it is done
      for (;;) {
        await new Promise((resolve) => setTimeout(resolve, 700));
        const now = await answer(await fetch('/v1/storage'));
        const job = now.move;
        if (job?.total_bytes > 0) setProgress(Math.min(100, Math.round((job.done_bytes / job.total_bytes) * 100)));
        if (!job?.running) {
          const failed = [job?.error, ...(job?.moved ?? []).map((entry: { error?: string | null }) => entry.error)].filter(Boolean);
          if (failed.length > 0) throw new Error(failed.join('; '));
          break;
        }
      }
      setPicked(null);
      onMoved?.();
    } catch (failure) {
      setError(failure instanceof Error ? failure.message : String(failure));
    } finally {
      setBusy(false);
      setProgress(null);
    }
  };

  return (
    <span className="inline-flex flex-col gap-1">
      <span className="inline-flex flex-wrap items-center gap-1.5">
        <button type="button" onClick={() => void choose()} disabled={busy} className={BUTTON} data-testid={`choose-folder-${folder}`}>
          {busy && progress !== null ? <Loader2 size={13} className="animate-spin" /> : <FolderSearch size={13} />}
          {progress !== null ? `${t('dbChooseMoving')} ${progress}%` : t('dbChooseFolder')}
        </button>
      </span>
      {picked && progress === null && (
        <span className="flex flex-wrap items-center gap-1.5 text-[11px] leading-4 text-zinc-600 dark:text-zinc-300">
          <span>
            {picked.copy_bytes > 0
              ? t('dbChoosePickedCopy').replace('{folder}', picked.directory).replace('{copy}', gigabytes(picked.copy_bytes, language)).replace('{free}', gigabytes(picked.free_bytes, language))
              : t('dbChoosePicked').replace('{folder}', picked.directory)}
          </span>
          <button type="button" onClick={() => void move()} disabled={busy} className={PRIMARY}>{t('dbChooseMove')}</button>
          <button type="button" onClick={() => setPicked(null)} disabled={busy} className={BUTTON}><X size={12} />{t('dbChooseCancel')}</button>
        </span>
      )}
      {error && <span role="alert" className="text-[11px] leading-4 text-rose-600 dark:text-rose-300">{error}</span>}
    </span>
  );
};

interface DataStatus { current: string; pending: string | null; size_bytes: number; launcher: boolean }

/** Change the whole data folder: written down now, moved before the next start. */
export const DataFolderChooser: React.FC = () => {
  const { t, language } = useI18n();
  const [status, setStatus] = useState<DataStatus | null>(null);
  const [picked, setPicked] = useState<{ target: string; free_bytes: number; size_bytes: number; path: string } | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void fetch('/v1/data-folder').then(answer).then((body) => { if (body && typeof body.current === 'string') setStatus(body); }).catch(() => undefined);
  }, []);

  const run = async (work: () => Promise<void>) => {
    setError(null);
    setBusy(true);
    try {
      await work();
    } catch (failure) {
      setError(failure instanceof Error ? failure.message : String(failure));
    } finally {
      setBusy(false);
    }
  };
  const choose = () => run(async () => {
    const body = await answer(await fetch('/v1/data-folder/pick', { method: 'POST' }));
    if (body.picked) setPicked(body);
  });
  const plan = (path: string | null) => run(async () => {
    setStatus(await answer(await fetch('/v1/data-folder', { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ path }) })));
    setPicked(null);
  });

  return (
    <div className="mt-2 space-y-1.5" data-testid="data-folder-chooser">
      <button type="button" onClick={() => void choose()} disabled={busy} className={BUTTON}>
        <FolderSearch size={13} />{t('dbChooseFolder')}
      </button>
      {picked && (
        <p className="flex flex-wrap items-center gap-1.5 text-[11px] leading-4 text-zinc-600 dark:text-zinc-300">
          <span>{t('dbDataPicked').replace('{folder}', picked.target).replace('{size}', gigabytes(picked.size_bytes, language)).replace('{free}', gigabytes(picked.free_bytes, language))}</span>
          <button type="button" onClick={() => void plan(picked.path)} disabled={busy} className={PRIMARY}>{t('dbDataPlan')}</button>
          <button type="button" onClick={() => setPicked(null)} disabled={busy} className={BUTTON}><X size={12} />{t('dbChooseCancel')}</button>
        </p>
      )}
      {status?.pending && (
        <p className="flex flex-wrap items-center gap-1.5 rounded-md bg-amber-500/10 p-2 text-[11px] leading-4 text-amber-700 dark:text-amber-300" data-testid="data-folder-pending">
          <span>{t('dbDataPending').replace('{folder}', status.pending)}{!status.launcher && ` ${t('dbDataLauncher')}`}</span>
          <button type="button" onClick={() => void plan(null)} disabled={busy} className={BUTTON}><X size={12} />{t('dbDataUnplan')}</button>
        </p>
      )}
      {error && <p role="alert" className="text-[11px] leading-4 text-rose-600 dark:text-rose-300">{error}</p>}
    </div>
  );
};
