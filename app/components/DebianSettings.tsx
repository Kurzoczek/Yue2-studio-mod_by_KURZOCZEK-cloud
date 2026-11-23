import React, { useCallback, useEffect, useRef, useState } from 'react';
import { AlertTriangle, Check, FileDown, FolderSearch, Loader2, Thermometer } from 'lucide-react';
import { useI18n } from '../context/I18nContext';
import { LANGUAGE_NAMES, type Language } from '../i18n/translations';

/**
 * The sections ported from the Android app "AI Music Generator and Cover Maker" (Debian build):
 * Language, Memory, Processor, Diagnostics and Save locations. Each control says, right under it, what it does.
 * The service keeps the settings (`/v1/governor`, `/v1/state-dumps`, `/v1/save-locations`).
 */

const CARD = 'rounded-xl border border-zinc-200 p-3 dark:border-white/10';
const LABEL = 'block text-sm font-medium text-zinc-800 dark:text-zinc-100';
const HINT = 'mt-1.5 block text-xs leading-5 text-zinc-500 dark:text-zinc-400';
const WARN = 'mt-1.5 block text-xs leading-5 text-amber-700 dark:text-amber-300';
const CONTROL = 'w-full rounded-lg border border-zinc-300 bg-white px-3 py-2 text-sm text-zinc-900 outline-hidden focus:border-indigo-500 dark:border-zinc-700 dark:bg-zinc-800 dark:text-white';
const BUTTON = 'inline-flex items-center gap-1.5 rounded-lg bg-zinc-900 px-4 py-2 text-sm font-semibold text-white hover:bg-zinc-800 disabled:opacity-50 dark:bg-white dark:text-black dark:hover:bg-zinc-200';
const OUTLINE = 'inline-flex items-center gap-1.5 rounded-lg border border-zinc-300 px-3 py-1.5 text-xs font-semibold text-zinc-700 hover:bg-zinc-100 dark:border-zinc-700 dark:text-zinc-200 dark:hover:bg-white/5';

type T = (key: string) => string;

/** `{name}` placeholders filled in. */
function fill(text: string, values: Record<string, string | number>): string {
  return Object.entries(values).reduce((out, [name, value]) => out.replaceAll(`{${name}}`, String(value)), text);
}

const gb = (mb: number | null | undefined): string => ((mb ?? 0) / 1024).toFixed(1);

/** One page of the service: read, edit, save. */
function usePage<S>(url: string) {
  const [page, setPage] = useState<Record<string, any> | null>(null);
  const [draft, setDraft] = useState<S | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [note, setNote] = useState<string | null>(null);
  const load = useCallback(async () => {
    try {
      const response = await fetch(url);
      const body = await response.json();
      if (!response.ok) throw new Error(body?.error || `HTTP ${response.status}`);
      setPage(body);
      setDraft((current) => current ?? (body.settings as S));
      setError(null);
    } catch (problem) {
      setError(problem instanceof Error ? problem.message : String(problem));
    }
  }, [url]);
  useEffect(() => {
    void load();
    const timer = window.setInterval(() => void load(), 4000);
    return () => window.clearInterval(timer);
  }, [load]);
  const save = useCallback(async (settings: S, t: T) => {
    setSaving(true);
    setNote(null);
    try {
      const response = await fetch(url, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(settings) });
      const body = await response.json();
      if (!response.ok) throw new Error(body?.error || `HTTP ${response.status}`);
      setPage(body);
      setDraft(body.settings as S);
      setNote(body.engine_restarted ? t('dbSavedRestarted') : body.restart_needed ? t('dbSavedRestartLater') : t('dbSaved'));
      setError(null);
    } catch (problem) {
      setError(problem instanceof Error ? problem.message : String(problem));
    } finally {
      setSaving(false);
    }
  }, [url]);
  return { page, draft, setDraft, error, saving, note, save, reload: load };
}

function Choice<V extends string>({ value, options, onChange, label }: { value: V; options: Array<[V, string]>; onChange: (value: V) => void; label: string }) {
  return (
    <div className="mt-2 flex flex-wrap gap-1 rounded-lg bg-zinc-100 p-1 dark:bg-black/30" role="radiogroup" aria-label={label}>
      {options.map(([option, text]) => (
        <button
          key={option}
          type="button"
          role="radio"
          aria-checked={value === option}
          onClick={() => onChange(option)}
          className={`rounded-md px-2.5 py-1.5 text-xs font-semibold transition-all ${value === option ? 'bg-white text-black shadow-xs dark:bg-zinc-700 dark:text-white' : 'text-zinc-500 hover:text-zinc-800 dark:hover:text-zinc-200'}`}
        >
          {text}
        </button>
      ))}
    </div>
  );
}

function Switch({ checked, onChange, label }: { checked: boolean; onChange: (value: boolean) => void; label: string }) {
  return (
    <label className="flex cursor-pointer items-start gap-3">
      <input type="checkbox" checked={checked} onChange={(event) => onChange(event.target.checked)} className="mt-0.5 h-4 w-4 accent-indigo-600" />
      <span className={LABEL}>{label}</span>
    </label>
  );
}

function Range({ label, value, min, max, step, onChange }: { label: string; value: number; min: number; max: number; step: number; onChange: (value: number) => void }) {
  return (
    <label className="block">
      <span className={LABEL}>{label}</span>
      <input type="range" min={min} max={max} step={step} value={value} onChange={(event) => onChange(Number(event.target.value))} className="mt-2 w-full accent-indigo-600" />
    </label>
  );
}

function Footer({ t, saving, note, error, onSave, onDefaults }: { t: T; saving: boolean; note: string | null; error: string | null; onSave: () => void; onDefaults?: () => void }) {
  return (
    <div className="space-y-2">
      <div className="flex flex-wrap items-center gap-2">
        <button type="button" className={BUTTON} disabled={saving} onClick={onSave}>
          {saving ? <Loader2 size={14} className="animate-spin" /> : <Check size={14} />} {t('dbSave')}
        </button>
        {onDefaults && <button type="button" className={OUTLINE} onClick={onDefaults}>{t('dbDefaults')}</button>}
        {note && <span className="text-xs text-emerald-600 dark:text-emerald-400">{note}</span>}
      </div>
      {error && <p className="flex items-center gap-1.5 text-xs text-red-600 dark:text-red-400"><AlertTriangle size={14} />{fill(t('dbLoadFailed'), { error })}</p>}
    </div>
  );
}

function Events({ t, events }: { t: T; events?: Array<{ at: string; kind: string; text: string }> }) {
  return (
    <details className={CARD}>
      <summary className="cursor-pointer text-sm font-medium text-zinc-800 dark:text-zinc-100">{t('dbEvents')}</summary>
      {events && events.length > 0 ? (
        <ul className="mt-2 max-h-48 space-y-1 overflow-y-auto font-mono text-[11px] leading-4 text-zinc-600 dark:text-zinc-400">
          {[...events].reverse().map((entry, index) => (
            <li key={index}>{new Date(entry.at).toLocaleTimeString()} · {entry.text}</li>
          ))}
        </ul>
      ) : (
        <p className={HINT}>{t('dbEventsNone')}</p>
      )}
    </details>
  );
}

// ------------------------------------------------------------------------------------------------ language

export const LanguageSettings: React.FC = () => {
  const { t, language, setLanguage } = useI18n();
  const tt = t as unknown as T;
  return (
    <div className="max-w-lg space-y-3">
      <label className="block">
        <span className={LABEL}>{tt('dbLanguageLabel')}</span>
        <select value={language} onChange={(event) => setLanguage(event.target.value as Language)} className={`${CONTROL} mt-2`}>
          {(Object.entries(LANGUAGE_NAMES) as [Language, string][]).map(([code, name]) => (
            <option key={code} value={code} lang={code}>{name}</option>
          ))}
        </select>
      </label>
      <p className={HINT}>{tt('dbLanguageHint')}</p>
    </div>
  );
};

// ------------------------------------------------------------------------------------------------ governor

interface GovernorSettings {
  ram_auto: boolean;
  ram_limit_mb: number;
  on_memory_limit: 'slow' | 'stop';
  decoder_tiles: 'auto' | 'large' | 'medium' | 'small' | 'tiny';
  threads: number;
  cpu_max_percent: number;
  core_set: 'all' | 'performance' | 'efficiency';
  core_count: number;
  priority: 'low' | 'below_normal' | 'normal' | 'above_normal' | 'high' | 'realtime';
  thermal_protection: boolean;
  thermal_level: 'moderate' | 'severe' | 'critical';
  thermal_throttle_percent: number;
}

export const MemorySettings: React.FC = () => {
  const { t: raw } = useI18n();
  const t = raw as unknown as T;
  const { page, draft, setDraft, error, saving, note, save } = usePage<GovernorSettings>('/v1/governor');
  if (!page || !draft) return <p className="flex items-center gap-2 text-sm text-zinc-500"><Loader2 size={14} className="animate-spin" />{error ? fill(t('dbLoadFailed'), { error }) : t('dbLoading')}</p>;
  const memory = page.memory;
  const set = (change: Partial<GovernorSettings>) => setDraft({ ...draft, ...change });
  const totalMb: number = memory.total_mb || 1024;
  const args: string[] = memory.decoder_args || [];
  return (
    <div className="max-w-2xl space-y-4">
      <div className={CARD}>
        <Switch checked={draft.ram_auto} onChange={(value) => set({ ram_auto: value })} label={fill(t('dbRamAuto'), { gb: gb(memory.auto_limit_mb) })} />
        <span className={HINT}>{fill(t('dbRamAutoHint'), { reserve: gb(memory.reserve_mb) })}</span>
        {!draft.ram_auto && (
          <div className="mt-3">
            <Range
              label={fill(t('dbRamLimit'), { gb: gb(draft.ram_limit_mb), total: gb(totalMb) })}
              value={draft.ram_limit_mb}
              min={512}
              max={Math.max(totalMb, 1024)}
              step={256}
              onChange={(value) => set({ ram_limit_mb: value })}
            />
            <span className={HINT}>{t('dbRamLimitHint')}</span>
          </div>
        )}
        <span className={HINT}>{fill(t('dbRamNow'), { free: gb(memory.available_mb), total: gb(totalMb), swapFree: gb(memory.swap_free_mb), swapTotal: gb(memory.swap_total_mb) })}</span>
        <span className={HINT}>{memory.peak_mb ? fill(t('dbRamPeak'), { peak: gb(memory.peak_mb), at: memory.peak_at ? new Date(memory.peak_at).toLocaleString() : '' }) : t('dbRamPeakNone')}</span>
        {memory.peak_mb > 0 && <span className={memory.fits ? HINT : WARN}>{memory.fits ? t('dbRamFits') : t('dbRamBelow')}</span>}
      </div>

      <div className={CARD}>
        <span className={LABEL}>{t('dbOnLimit')}</span>
        <Choice label={t('dbOnLimit')} value={draft.on_memory_limit} onChange={(value) => set({ on_memory_limit: value })} options={[['slow', t('dbOnLimitSlow')], ['stop', t('dbOnLimitStop')]]} />
        <span className={HINT}>{t(draft.on_memory_limit === 'slow' ? 'dbOnLimitSlowHint' : 'dbOnLimitStopHint')}</span>
        <span className={HINT}>{fill(t('dbScope'), { how: t(page.systemd_scope ? 'dbScopeSystemd' : 'dbScopeNone') })}</span>
      </div>

      <div className={CARD}>
        <span className={LABEL}>{t('dbTiles')}</span>
        <Choice
          label={t('dbTiles')}
          value={draft.decoder_tiles}
          onChange={(value) => set({ decoder_tiles: value })}
          options={(['auto', 'large', 'medium', 'small', 'tiny'] as const).map((value) => [value, t(`dbTiles_${value}`)])}
        />
        <span className={HINT}>{t('dbTilesHint')}</span>
        <span className={HINT}>{args.length ? fill(t('dbTilesNow'), { args: args.join(' ') }) : t('dbTilesNowNone')}</span>
      </div>

      <p className={HINT}>{t('dbMemoryRestart')}</p>
      <Footer t={t} saving={saving} note={note} error={error} onSave={() => void save(draft, t)} onDefaults={() => setDraft({ ...draft, ram_auto: page.defaults.ram_auto, ram_limit_mb: page.defaults.ram_limit_mb, on_memory_limit: page.defaults.on_memory_limit, decoder_tiles: page.defaults.decoder_tiles })} />
      <Events t={t} events={page.events} />
    </div>
  );
};

export const ProcessorSettings: React.FC = () => {
  const { t: raw } = useI18n();
  const t = raw as unknown as T;
  const { page, draft, setDraft, error, saving, note, save } = usePage<GovernorSettings>('/v1/governor');
  if (!page || !draft) return <p className="flex items-center gap-2 text-sm text-zinc-500"><Loader2 size={14} className="animate-spin" />{error ? fill(t('dbLoadFailed'), { error }) : t('dbLoading')}</p>;
  const cpu = page.cpu;
  const thermal = page.thermal;
  const engine = page.engine;
  const set = (change: Partial<GovernorSettings>) => setDraft({ ...draft, ...change });
  const cores: number = cpu.cores || 1;
  const kindCount = draft.core_set === 'performance' && cpu.hybrid ? cpu.performance.length : draft.core_set === 'efficiency' && cpu.hybrid ? cpu.efficiency.length : cores;
  const raisesPriority = ['above_normal', 'high', 'realtime'].includes(draft.priority);
  const niceOf: Record<string, number> = { low: 19, below_normal: 10, normal: 0, above_normal: -5, high: -10, realtime: -20 };
  return (
    <div className="max-w-2xl space-y-4">
      <div className={CARD}>
        <Range label={fill(t('dbThreads'), { value: draft.threads === 0 ? t('dbThreadsAuto') : draft.threads, cores })} value={draft.threads} min={0} max={Math.max(cores * 2, 2)} step={1} onChange={(value) => set({ threads: value })} />
        <span className={HINT}>{t('dbThreadsHint')}</span>
      </div>

      <div className={CARD}>
        <Range label={fill(t('dbCpuMax'), { value: draft.cpu_max_percent })} value={draft.cpu_max_percent} min={10} max={100} step={10} onChange={(value) => set({ cpu_max_percent: value })} />
        <span className={HINT}>{t('dbCpuMaxHint')}</span>
      </div>

      <div className={CARD}>
        <span className={LABEL}>{t('dbCores')}</span>
        <Choice label={t('dbCores')} value={draft.core_set} onChange={(value) => set({ core_set: value })} options={(['all', 'performance', 'efficiency'] as const).map((value) => [value, t(`dbCores_${value}`)])} />
        <span className={HINT}>{t('dbCoresHint')}</span>
        <span className={HINT}>{cpu.hybrid ? fill(t('dbCoresHybrid'), { performance: cpu.performance.join(','), efficiency: cpu.efficiency.join(',') }) : fill(t('dbCoresFlat'), { count: cores })}</span>
        <div className="mt-3">
          <Range label={fill(t('dbCoreCount'), { value: draft.core_count })} value={draft.core_count} min={0} max={kindCount} step={1} onChange={(value) => set({ core_count: value })} />
          <span className={HINT}>{t('dbCoreCountHint')}</span>
        </div>
      </div>

      <label className={`${CARD} block`}>
        <span className={LABEL}>{t('dbPriority')}</span>
        <select value={draft.priority} onChange={(event) => set({ priority: event.target.value as GovernorSettings['priority'] })} className={`${CONTROL} mt-2`}>
          {(['low', 'below_normal', 'normal', 'above_normal', 'high', 'realtime'] as const).map((value) => (
            <option key={value} value={value}>{t(`dbPriority_${value}`)} (nice {niceOf[value]})</option>
          ))}
        </select>
        <span className={HINT}>{t('dbPriorityHint')}</span>
        {raisesPriority && niceOf[draft.priority] < cpu.lowest_nice && <span className={WARN}>{fill(t('dbPriorityDenied'), { nice: cpu.lowest_nice })}</span>}
      </label>

      <div className={CARD}>
        <Switch checked={draft.thermal_protection} onChange={(value) => set({ thermal_protection: value })} label={t('dbThermal')} />
        <span className={HINT}>{fill(t('dbThermalHint'), { hysteresis: (thermal.pause_at - thermal.resume_at).toFixed(0) })}</span>
        {draft.thermal_protection && (
          <div className="mt-3 space-y-3">
            <div>
              <span className={LABEL}>{t('dbThermalLevel')}</span>
              <Choice label={t('dbThermalLevel')} value={draft.thermal_level} onChange={(value) => set({ thermal_level: value })} options={(['moderate', 'severe', 'critical'] as const).map((value) => [value, t(`dbThermal_${value}`)])} />
            </div>
            <Range label={fill(t('dbThermalThrottle'), { value: draft.thermal_throttle_percent })} value={draft.thermal_throttle_percent} min={5} max={50} step={5} onChange={(value) => set({ thermal_throttle_percent: value })} />
          </div>
        )}
        <span className={`${HINT} flex items-center gap-1`}>
          <Thermometer size={12} />
          {thermal.available ? fill(t('dbThermalNow'), { celsius: Number(thermal.celsius).toFixed(0), sensor: thermal.sensor }) : t('dbThermalNone')}
        </span>
        {thermal.throttled && <span className={WARN}>{t('dbThermalThrottled')}</span>}
      </div>

      <p className={HINT}>{engine ? fill(t('dbEngineNow'), { threads: engine.threads, nice: engine.nice, cpus: engine.cpus, rss: gb(engine.rss_mb) }) : t('dbNotRunning')}</p>
      <p className={HINT}>{t('dbProcessorRestart')} {t('dbOtherProcesses')}</p>
      <Footer t={t} saving={saving} note={note} error={error} onSave={() => void save(draft, t)} onDefaults={() => setDraft({ ...draft, threads: page.defaults.threads, cpu_max_percent: page.defaults.cpu_max_percent, core_set: page.defaults.core_set, core_count: page.defaults.core_count, priority: page.defaults.priority, thermal_protection: page.defaults.thermal_protection, thermal_level: page.defaults.thermal_level, thermal_throttle_percent: page.defaults.thermal_throttle_percent })} />
      <Events t={t} events={page.events} />
    </div>
  );
};

// ------------------------------------------------------------------------------------------------ diagnostics

interface DumpSettings {
  logs: boolean;
  dump_type: 'none' | 'small' | 'extended' | 'full' | 'automatic';
  on_error: boolean;
  on_unexpected_end: boolean;
  on_pause: boolean;
  keep: number;
}

export const DiagnosticsDumpSettings: React.FC = () => {
  const { t: raw } = useI18n();
  const t = raw as unknown as T;
  const { page, draft, setDraft, error, saving, note, save, reload } = usePage<DumpSettings>('/v1/state-dumps');
  const [dumping, setDumping] = useState(false);
  const [written, setWritten] = useState<string | null>(null);
  const [dumpError, setDumpError] = useState<string | null>(null);
  if (!page || !draft) return <p className="flex items-center gap-2 text-sm text-zinc-500"><Loader2 size={14} className="animate-spin" />{error ? fill(t('dbLoadFailed'), { error }) : t('dbLoading')}</p>;
  const set = (change: Partial<DumpSettings>) => setDraft({ ...draft, ...change });
  const dumpNow = async () => {
    setDumping(true);
    setDumpError(null);
    try {
      const response = await fetch('/v1/state-dumps/now', { method: 'POST' });
      const body = await response.json();
      if (!response.ok) throw new Error(body?.error || `HTTP ${response.status}`);
      setWritten(body.path);
      void reload();
    } catch (problem) {
      setDumpError(problem instanceof Error ? problem.message : String(problem));
    } finally {
      setDumping(false);
    }
  };
  return (
    <div className="max-w-2xl space-y-4">
      <div className={CARD}>
        <Switch checked={draft.logs} onChange={(value) => set({ logs: value })} label={t('dbLogs')} />
        <span className={HINT}>{fill(t('dbLogsHint'), { folder: page.logs_folder ?? '' })}</span>
      </div>

      <div className={CARD}>
        <span className={LABEL}>{t('dbDumpType')}</span>
        <Choice label={t('dbDumpType')} value={draft.dump_type} onChange={(value) => set({ dump_type: value })} options={(['none', 'small', 'extended', 'full', 'automatic'] as const).map((value) => [value, t(`dbDump_${value}`)])} />
        <span className={HINT}>{t(`dbDumpHint_${draft.dump_type}`)}</span>
        {draft.dump_type === 'full' && !page.gcore && <span className={WARN}>{t('dbDumpGcoreMissing')}</span>}
        {draft.dump_type === 'full' && page.ptrace_scope && page.ptrace_scope !== '0' && <span className={WARN}>{fill(t('dbDumpPtrace'), { value: page.ptrace_scope })}</span>}
        {draft.dump_type !== 'none' && (
          <div className="mt-3 space-y-2">
            <span className={LABEL}>{t('dbDumpWhen')}</span>
            <Switch checked={draft.on_error} onChange={(value) => set({ on_error: value })} label={t('dbDumpOnError')} />
            <Switch checked={draft.on_unexpected_end} onChange={(value) => set({ on_unexpected_end: value })} label={t('dbDumpOnEnd')} />
            <Switch checked={draft.on_pause} onChange={(value) => set({ on_pause: value })} label={t('dbDumpOnPause')} />
            <Range label={fill(t('dbDumpKeep'), { value: draft.keep })} value={draft.keep} min={1} max={50} step={1} onChange={(value) => set({ keep: value })} />
          </div>
        )}
        <span className={HINT}>{fill(t('dbDumpFolder'), { folder: page.dumps_folder ?? '' })}</span>
      </div>

      <Footer t={t} saving={saving} note={note} error={error} onSave={() => void save(draft, t)} onDefaults={() => setDraft(page.defaults)} />

      <div className={CARD}>
        <button type="button" className={OUTLINE} disabled={dumping} onClick={() => void dumpNow()}>
          {dumping ? <Loader2 size={14} className="animate-spin" /> : <FileDown size={14} />} {t('dbDumpNow')}
        </button>
        {written && <span className={HINT}>{fill(t('dbDumpWritten'), { path: written })}</span>}
        {dumpError && <span className={WARN}>{dumpError}</span>}
        {page.dumps?.length > 0 && (
          <details className="mt-2">
            <summary className="cursor-pointer text-xs text-zinc-600 dark:text-zinc-300">{t('dbDumpList')} ({page.dumps.length})</summary>
            <ul className="mt-1 space-y-0.5 font-mono text-[11px] text-zinc-500">
              {page.dumps.map((dump: { name: string; bytes: number }) => <li key={dump.name}>{dump.name} · {(dump.bytes / 1024).toFixed(0)} kB</li>)}
            </ul>
          </details>
        )}
      </div>
    </div>
  );
};

// ------------------------------------------------------------------------------------------------ save locations

interface SaveSettings {
  folders: Record<string, string>;
  auto_export: boolean;
}

export const SaveLocationSettings: React.FC = () => {
  const { t: raw } = useI18n();
  const t = raw as unknown as T;
  const { page, draft, setDraft, error, saving, note, save } = usePage<SaveSettings>('/v1/save-locations');
  const [all, setAll] = useState('');
  const [browsing, setBrowsing] = useState<string | null>(null);
  const [browseError, setBrowseError] = useState<string | null>(null);
  // Saved by itself a moment after every change (a folder typed, OK, Default, the switch): before, a change was
  // kept only by the Save button at the bottom, and closing the window without it lost every folder.
  const saved = useRef<string | null>(null);
  useEffect(() => {
    if (!page || !draft) return;
    if (saved.current === null) saved.current = JSON.stringify(page.settings);
    const text = JSON.stringify(draft);
    if (text === saved.current) return;
    if (Object.values(draft.folders).some((folder) => folder.trim() && !folder.trim().startsWith('/'))) return;
    const timer = window.setTimeout(() => {
      saved.current = text;
      void save(draft, t);
    }, 1200);
    return () => window.clearTimeout(timer);
  }, [draft, page, save, t]);
  if (!page || !draft) return <p className="flex items-center gap-2 text-sm text-zinc-500"><Loader2 size={14} className="animate-spin" />{error ? fill(t('dbLoadFailed'), { error }) : t('dbLoading')}</p>;
  const kinds: Array<{ key: string; audio: boolean; folder: string | null; writable: boolean | null; default: string | null }> = page.kinds;
  const music = kinds.find((kind) => kind.key === 'final')?.default ?? '';
  const downloads = kinds.find((kind) => kind.key === 'midi')?.default ?? '';
  // from the draft as it is when the change lands: "Browse…" answers after its dialog closes, and what changed
  // meanwhile is kept
  const setFolder = (key: string, folder: string) => setDraft((current) => {
    if (!current) return current;
    const folders = { ...current.folders };
    if (folder.trim()) folders[key] = folder; else delete folders[key];
    return { ...current, folders };
  });
  const badPath = Object.values(draft.folders).some((folder) => folder.trim() && !folder.trim().startsWith('/'));
  /** "Browse…": the system's folder dialog, opened where the field points; `kind` empty = one folder for everything. */
  const browse = async (kind: string, current: string, done: (folder: string) => void) => {
    setBrowsing(kind || '*');
    setBrowseError(null);
    try {
      const response = await fetch('/v1/save-locations/pick', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ kind, current: current || null }) });
      const body = await response.json();
      if (!response.ok) throw new Error(body?.error || `HTTP ${response.status}`);
      if (body.picked && typeof body.path === 'string') done(body.path);
    } catch (problem) {
      setBrowseError(fill(t('dbSavingBrowseFailed'), { error: problem instanceof Error ? problem.message : String(problem) }));
    } finally {
      setBrowsing(null);
    }
  };
  const browseButton = (kind: string, current: string, done: (folder: string) => void) => (
    <button type="button" className={OUTLINE} disabled={browsing !== null} onClick={() => void browse(kind, current, done)}>
      {browsing === (kind || '*') ? <Loader2 size={14} className="animate-spin" /> : <FolderSearch size={14} />} {t('dbSavingBrowse')}
    </button>
  );
  return (
    <div className="max-w-2xl space-y-4">
      <p className={HINT}>{fill(t('dbSavingInfo'), { music, downloads })}</p>
      <p className={HINT}>{t('dbSavingAutoSaved')}</p>

      <div className={CARD}>
        <Switch checked={draft.auto_export} onChange={(value) => setDraft({ ...draft, auto_export: value })} label={t('dbSavingAuto')} />
        <span className={HINT}>{t('dbSavingAutoHint')}</span>
      </div>

      <div className={`${CARD} space-y-2`}>
        <span className={LABEL}>{t('dbSavingAll')}</span>
        <div className="flex gap-2">
          <input value={all} onChange={(event) => setAll(event.target.value)} placeholder={t('dbSavingPath')} className={CONTROL} />
          {browseButton('', all, setAll)}
          <button type="button" className={OUTLINE} onClick={() => all.trim() && setDraft({ ...draft, folders: Object.fromEntries(kinds.map((kind) => [kind.key, all.trim()])) })}>OK</button>
          <button type="button" className={OUTLINE} onClick={() => setDraft({ ...draft, folders: {} })}>{t('dbSavingAllReset')}</button>
        </div>
      </div>

      {kinds.map((kind) => {
        const chosen = draft.folders[kind.key] ?? '';
        return (
          <div key={kind.key} className={CARD}>
            <span className={LABEL}>{t(`dbCat_${kind.key}`)}</span>
            <div className="mt-2 flex gap-2">
              <input value={chosen} onChange={(event) => setFolder(kind.key, event.target.value)} placeholder={t('dbSavingPath')} className={CONTROL} />
              {browseButton(kind.key, chosen, (folder) => setFolder(kind.key, folder))}
              {chosen && <button type="button" className={OUTLINE} onClick={() => setFolder(kind.key, '')}>{t('dbSavingReset')}</button>}
            </div>
            <span className={HINT}>{chosen ? fill(t('dbSavingCustom'), { path: chosen }) : fill(t('dbSavingDefault'), { path: kind.default ?? '' })}</span>
            {kind.folder && kind.folder === chosen && kind.writable === false && <span className={WARN}>{t('dbSavingNoAccess')}</span>}
          </div>
        );
      })}

      {browseError && <p className={WARN}>{browseError}</p>}
      {badPath && <p className={WARN}>{t('dbSavingBadPath')}</p>}
      <Footer t={t} saving={saving} note={note} error={error} onSave={() => void save(draft, t)} />
    </div>
  );
};
