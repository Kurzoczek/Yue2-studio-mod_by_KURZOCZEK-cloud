// Debian build: how fast things go now - the downloads coming in, and what each disk the studio keeps its folders
// on reads and writes (a USB disk with the engines, say). Asked of the studio once a second while it is shown.
import React, { useEffect, useState } from 'react';
import { ArrowDownToLine, HardDrive } from 'lucide-react';
import { useI18n } from '../context/I18nContext';

interface Disk {
  device: string;
  mount: string;
  file_system: string;
  folders: string[];
  read_bytes_per_second: number;
  write_bytes_per_second: number;
}

interface Rates {
  download_bytes_per_second: number;
  disks: Disk[];
}

/** A rate, as people read them: 0 B/s, 830 kB/s, 12.4 MB/s, 1.1 GB/s. */
export function rate(bytes: number, language: string): string {
  const units: [number, string][] = [[1e9, 'GB/s'], [1e6, 'MB/s'], [1e3, 'kB/s']];
  for (const [size, unit] of units) {
    if (bytes >= size) {
      const value = bytes / size;
      return `${value.toLocaleString(language, { maximumFractionDigits: value < 10 ? 1 : 0 })} ${unit}`;
    }
  }
  return `${Math.round(bytes)} B/s`;
}

export const TransferRates: React.FC = () => {
  const { t, language } = useI18n();
  const [rates, setRates] = useState<Rates | null>(null);

  useEffect(() => {
    let live = true;
    const ask = async () => {
      try {
        const response = await fetch('/v1/transfer-rates');
        if (!response.ok || !live) return;
        const body = await response.json();
        // only what has the shape of an answer: an older studio, or anything else, shows nothing
        if (body && typeof body.download_bytes_per_second === 'number' && Array.isArray(body.disks)) setRates(body);
      } catch {
        // the studio restarting: the next asking tells
      }
    };
    void ask();
    const timer = window.setInterval(() => void ask(), 1000);
    return () => { live = false; window.clearInterval(timer); };
  }, []);

  if (!rates) return null;
  const folderName = (name: string) => (name === 'data' ? t('dbRatesDataFolder') : t(`storageFolder_${name}` as never));
  return (
    <div className="border-b border-zinc-200 bg-zinc-50 px-6 py-2 text-xs dark:border-white/5 dark:bg-black/20" data-testid="transfer-rates">
      <div className="flex flex-wrap items-center gap-x-4 gap-y-1">
        <span className="flex items-center gap-1.5 font-semibold text-zinc-700 dark:text-zinc-200">
          <ArrowDownToLine size={13} />
          {t('dbRatesDownload')}: <span className="tabular-nums" data-testid="rate-download">{rate(rates.download_bytes_per_second, language)}</span>
        </span>
        {rates.disks.map((disk) => (
          <span key={disk.device} className="flex min-w-0 items-center gap-1.5 text-zinc-600 dark:text-zinc-300" title={`${disk.mount} (${disk.file_system}): ${disk.folders.map(folderName).join(', ')}`}>
            <HardDrive size={13} className="shrink-0" />
            <span className="truncate">{disk.mount} <span className="text-zinc-400">({disk.device})</span></span>
            <span className="shrink-0 tabular-nums">
              {t('dbRatesRead')} {rate(disk.read_bytes_per_second, language)} · {t('dbRatesWrite')} {rate(disk.write_bytes_per_second, language)}
            </span>
          </span>
        ))}
      </div>
    </div>
  );
};
