**Oryginalny autor:** [timoncool](https://github.com/timoncool)

**Oryginalny program:** [YuE2 Studio](https://github.com/timoncool/YuE2-Studio)

## Zmiany względem oryginalnego programu

### Debian i Linux

- Dodałem budowanie i instalowanie pakietu `.deb` oraz skrypt uruchamiający studio na Debianie.
- Przygotowałem skrypty budowania silników i poprawkę pozwalającą kontrolować liczbę wątków procesora.
- Launcher pilnuje, by studio nie działało jako root, i przygotowuje środowisko dla lokalnych silników.

### Polski interfejs i ustawienia

- Dodałem polskie tłumaczenie interfejsu, komunikatów i błędów.
- Rozszerzyłem ustawienia o wybór wyglądu, folderów oraz limitów zasobów.
- Dodałem widok prędkości transferu podczas pobierania.

### Foldery, modele i zapisy

- Dodałem wybór lokalizacji danych studia oraz przenoszenie biblioteki i danych na inny dysk.
- Rozszerzyłem zarządzanie lokalizacjami modeli i silników; można je przenosić i kontrolować ich zajętość.
- Dodałem wybór miejsca zapisu plików i informacje o wolnym miejscu.

### Zasoby, diagnostyka i niezawodność

- Dodałem kontrolę obciążenia procesora i pamięci, aktywności zadań oraz diagnostyczny eksport informacji.
- Rozszerzyłem obsługę pobierania, instalacji silników i odzyskiwania stanu zadań.
- Dodałem testy dla nowych ustawień, lokalnego API, zapisu plików i przepływów instalacji.

### Bezpieczeństwo lokalnego API

- Dodałem wymagany klucz dostępu do lokalnego API studia. Klucz jest przechowywany z prawami dostępu tylko dla właściciela pliku (0600 w systemach Unix).
- Oczyszczam klucz z adresów żądań i danych przekazywanych dalej, aby nie trafiał do zapisanych treści ani obsługi żądania.
- Zachowałem lokalny charakter API i możliwość jego użycia przez skonfigurowane narzędzia użytkownika.
