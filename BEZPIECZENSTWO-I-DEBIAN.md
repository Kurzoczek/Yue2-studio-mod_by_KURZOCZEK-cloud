# YuE2 Studio 5.0.0: bezpieczeństwo i dostosowanie do Debiana 13

Przeniesienie poprawek przygotowanych wcześniej dla 4.1.1 na wydanie 5.0.0. Przegląd był celowany, nie objął wszystkich
linii: 5.0 ma około 160 tys. linii Rusta (nowe silniki: natywny ACE-Step, MiniMax, audio.cpp, HOT-Step, RVC, Vevo2,
SoulX-Singer i inne) i interfejs w React. Sprawdzone zostały miejsca, w których zwykle kryje się ryzyko: serwer HTTP
i jego uwierzytelnianie, MCP, uruchamianie programów, zapisywanie plików, klucze API, telemetria, konfiguracja Tauri,
skrypty budowania.

**Poza przeglądem:** zależności z Gita, których nie ma w źródłach (`dub-studio`, `studio-hub-client`, `yue2.cpp`,
`HOT-Step-CPP`, audio.cpp), gotowe zminifikowane pliki (`app/public/midi-editor/assets/*.js`,
`app/public/ffmpeg/ffmpeg-core.wasm`) oraz paczki silników pobierane przez stronę „Silniki i aktualizacje”
(są podpisane, ale ich kod nie był czytany).

## Werdykt

Nie znalazłem niczego, co wyglądałoby na złośliwy kod. Główne ryzyka wynikają z założeń projektu: studio ufało każdemu
programowi na tym samym komputerze, a na polecenie swojego API zapisuje pliki i uruchamia programy. Po poprawkach
lokalne API wymaga klucza. Mimo to **nigdy nie uruchamiaj studia jako root**.

## Ustalenia

1. **Uruchamianie jako root — WYSOKIE.** API potrafi zapisać plik pod wskazaną ścieżką, uruchomić program wskazany jako
   „agent CLI” (`cli_path`) i trzymać dane obok programu. Poprawka: `scripts/yue2-studio-debian` (instalowany jako
   `/usr/local/bin/yue2-studio`) odmawia startu jako root.
2. **Lokalne API bez uwierzytelnienia — ŚREDNIE (naprawione).** Usługa na `127.0.0.1:8791` (REST i MCP) wpuszczała
   każde połączenie z loopbacku. Teraz każde żądanie z tego komputera niesie **klucz lokalnego API**:
   - plik `local-api-key` w folderze danych, prawa 0600, powstaje przy starcie usługi i jest zachowywany;
   - okno dostaje klucz od powłoki Tauri (tylko strony `tauri://localhost`) i dokleja go do adresów usługi
     (`studio_key=`), co obejmuje `<audio>`, obrazy i `EventSource`; adresy z biblioteki dostają go przez `apiUrl`;
   - usługa wycina klucz z adresu i z treści JSON, zanim zobaczy go handler, log, biblioteka albo tagi plików;
   - edytor audio i karta przeglądarki dostają klucz raz w adresie, potem jako ciasteczko `HttpOnly`, `SameSite=Strict`;
   - agenty: nagłówek `Authorization: Bearer <klucz>` albo `X-Studio-Key`; bez klucza działa tylko `/health`;
   - `YUE_LOCAL_API_KEY=...` ustawia własny klucz, `YUE_LOCAL_API_OPEN=1` wyłącza sprawdzanie (tryb deweloperski).

   ```bash
   claude mcp add --transport http yue2-studio http://127.0.0.1:8791/mcp \
     --header "Authorization: Bearer $(yue2-studio --klucz)"
   ```
   Powłoka dołącza klucz tylko do zapytań na port studia (8791), nigdy do innych portów, które sprawdza.
3. **„Dostęp z sieci” — WYSOKIE, gdy włączony (domyślnie wyłączony).** Klucz przez zwykłe HTTP, ciasteczko na rok.
   Zostaw wyłączone; zdalnie używaj tunelu SSH: `ssh -L 8791:127.0.0.1:8791 lukasz@komputer`.
4. **Klucze API z prawami 0644 — ŚREDNIE (naprawione).** `openrouter-api-key`, `local-server-api-key` zapisywane z 0600.
5. **Eksport presetu korektora nadpisywał pliki — ŚREDNIE (naprawione).** `equalizer_export` odmawia, gdy plik istnieje.
6. **Telemetria i Studio Hub — PRYWATNOŚĆ.** Program uruchamiający ustawia `DO_NOT_TRACK=1`. Wiadomości z
   `hub.neuro-cartel.com` są dalej pobierane; w 5.0 banery i okienka da się ukryć w Ustawienia → Prywatność.
7. **Usługi chmurowe — INFORMACJA.** `lyrics_find` (lrclib.net, QQ Music, Kugou), OpenRouter, rozpoznawanie w chmurze:
   działają dopiero po użyciu funkcji.
8. **`csp: null`, `withGlobalTauri: true` — NISKIE.** Bez zmian (CSP zepsułoby ffmpeg w WebAssembly i połączenia z usługą).
9. **`audiomass-server.py` i `.go` — NISKIE (usunięte).** Nieużywane serwery deweloperskie na wszystkich interfejsach.
10. **Budowanie przez Dockera — NISKIE (zastąpione).** `scripts/build-debian.sh`: Rust z pakietu `rustup`, Vulkan z
    repozytorium, Node 22 ze sprawdzaną sumą SHA-256.

Nowe trasy ustawień (`/v1/governor`, `/v1/state-dumps`, `/v1/save-locations`) celowo nie są narzędziami MCP: limity
komputera i miejsca zapisu zmienia tylko człowiek (wpisane na listę `NOT_TOOLS` z uzasadnieniem).

## Zmiany pod Debiana

| Plik | Zmiana |
|---|---|
| `rust-toolchain.toml` | `stable` zamiast `stable-x86_64-pc-windows-msvc` |
| `scripts/build-release-linux.sh` | `BUNDLES` (sam `.deb`), sprawdzanie tylko zbudowanych paczek |
| `scripts/build-yue-runtime-linux.sh` | poprawka wątków silnika (`patch-engine-threads.sh`); liczbę kompilacji 5.0 dobiera sam |
| `scripts/build-midi-runtime-linux.sh` | `BUILD_JOBS` |
| `scripts/build-debian.sh`, `scripts/yue2-studio-debian` | budowa w obrazie ext4 na iSCSI jako `lukasz`, instalacja, program uruchamiający |
| `remote.rs`, `credentials.rs`, `mcp.rs`, `saving.rs`, `desktop/src-tauri/src/lib.rs`, `app/services/apiBase.ts` | ustalenia 2, 4, 5; zapis z okna niesie klucz (błąd 401) |
| `governor.rs`, `diagnostics_dump.rs`, `save_locations.rs`, `music-core/src/limits.rs`, `DebianSettings.tsx` | Pamięć, Procesor, Diagnostyka, Miejsca zapisu |
| `app/i18n/*`, komponenty nowych silników | język polski w całym oknie |

Zarządca zasobów obejmuje silnik YuE2 (`yue-server`). Pozostałe silniki 5.0 (natywny ACE-Step, MiniMax, audio.cpp,
HOT-Step) działają jak w oryginale.

Sprawdzenie: `cargo check` usługi i powłoki Tauri; testy usługi 778/778; testy `music-core` 8/8; `music-engine` 22/23
(test `the_cuda_backend_comes_from_its_folder_or_a_developer_build` sprawdza plik `ggml-cuda.dll` z Windows i pada też
na czystym 5.0.0 pod Linuksem); `tsc`; vitest 508/508. Usługa uruchomiona naprawdę: 401 bez klucza, 200 z kluczem,
klucz z prawami 0600, odrzucenie złej ścieżki, zrzut ręczny i zrzut po `kill -9`.

## Uczciwie o sprzęcie (Intel N150, 8 GB RAM)

Autor zaleca kartę NVIDIA z co najmniej 6 GB VRAM. Na zintegrowanym GPU przez Vulkan wokal bywał niezrozumiały, karty
Intela nie były testowane, a sam procesor jest wielokrotnie wolniejszy. Wybierz zestaw **Lekki** (Q5_K_M) albo
**Zrównoważony** (Q6_K); większe przy 8 GB RAM-u są na granicy.
