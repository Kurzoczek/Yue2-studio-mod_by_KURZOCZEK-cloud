#!/bin/bash
# Poprawka liczby wątków silnika (acestep.cpp / yue2.cpp, plik src/backend.h).
# Oryginał liczy na połowie rdzeni ("hyperthreading"), co na procesorach bez HT (np. Intel N150: 4 rdzenie,
# 4 wątki) daje tylko 2 wątki. Po poprawce:
#   STUDIO_CPU_THREADS=N  -> dokładnie N wątków (ustawia to studio: Ustawienia -> Procesor -> Wątki),
#   brak zmiennej         -> wszystkie rdzenie.
# Użycie: patch-engine-threads.sh <katalog źródeł silnika>
set -euo pipefail

if [ "$(id -u)" -eq 0 ]; then
    echo "[BŁĄD] Uruchom poprawkę jako zwykły użytkownik, nie root." >&2
    exit 1
fi

src="${1:?użycie: patch-engine-threads.sh KATALOG_ŹRÓDEŁ}"
header="$src/src/backend.h"
if [ ! -f "$header" ]; then
    echo "[UWAGA] $header nie istnieje - poprawka wątków pominięta (silnik użyje połowy rdzeni)" >&2
    exit 0
fi
if grep -q 'studio_cpu_threads' "$header"; then
    echo "[OK] poprawka wątków już jest w $header"
    exit 0
fi
if ! grep -q 'std::thread::hardware_concurrency() / 2' "$header"; then
    echo "[UWAGA] w $header nie ma wyrażenia 'hardware_concurrency() / 2' - poprawka wątków pominięta" >&2
    exit 0
fi
tmp="$(mktemp)"
cat > "$tmp" <<'EOT'
// --- studio: liczba wątków procesora (STUDIO_CPU_THREADS, domyślnie wszystkie rdzenie) ---
#ifndef STUDIO_CPU_THREADS_DEFINED
#define STUDIO_CPU_THREADS_DEFINED
#include <cstdlib>
#include <thread>
static inline unsigned studio_cpu_threads() {
    const char * e = std::getenv("STUDIO_CPU_THREADS");
    int n = e ? std::atoi(e) : 0;
    if (n > 0) return (unsigned) n;
    unsigned h = std::thread::hardware_concurrency();
    return h > 0 ? h : 1;
}
#endif
EOT
sed 's#std::thread::hardware_concurrency() / 2#studio_cpu_threads()#' "$header" >> "$tmp"
mv "$tmp" "$header"
echo "[OK] poprawka wątków w $header (STUDIO_CPU_THREADS)"
