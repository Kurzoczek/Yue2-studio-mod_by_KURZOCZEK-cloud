**Oryginalny autor:** [timoncool](https://github.com/timoncool)

**Oryginalny program:** [YuE2 Studio](https://github.com/timoncool/YuE2-Studio)

## Polski

## Zmiany względem oryginalnego programu

Zmiany wprowadziłem, aby YuE2 Studio było bardziej przyjazne dla komputerów bez wydajnych kart graficznych oraz z małą ilością miejsca na dysku.

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

## English

**Original author:** [timoncool](https://github.com/timoncool)

**Original program:** [YuE2 Studio](https://github.com/timoncool/YuE2-Studio)

### Changes from the original program

I made these changes to make YuE2 Studio more accessible on computers without powerful graphics cards and with limited disk space.

#### Debian and Linux

- Added scripts to build and install a `.deb` package and launch the studio on Debian.
- Added engine build scripts and a patch to control the number of CPU threads.
- The launcher prevents the studio from running as root and prepares the environment for local engines.

#### Polish language and settings

- Added a Polish translation of the interface, messages, and errors.
- Expanded settings with appearance, folder, and resource-limit options.
- Added a download transfer-speed display.

#### Folders, models, and saves

- Added options to choose the studio data location and move the library and data to another drive.
- Expanded model and engine storage management, including relocation and disk-usage information.
- Added save-location selection and free-space information.

#### Resources, diagnostics, and reliability

- Added CPU and memory usage controls, task activity tracking, and diagnostic exports.
- Improved download and engine-installation handling and task-state recovery.
- Added tests for the new settings, local API, file saving, and installation flows.

#### Local API security

- Added a required access key for the studio's local API. On Unix, the key is stored in a file accessible only to its owner (0600 permissions).
- Strip the key from request URLs and forwarded data so it is not included in saved content or passed to request handlers.
- Kept the API local and usable by the user's configured tools.

## 简体中文

**原作者：** [timoncool](https://github.com/timoncool)

**原始程序：** [YuE2 Studio](https://github.com/timoncool/YuE2-Studio)

### 相对于原版的更改

这些更改旨在让没有高性能显卡、磁盘空间有限的电脑也能更方便地使用 YuE2 Studio。

#### Debian 和 Linux

- 添加了用于构建和安装 `.deb` 软件包以及在 Debian 上启动工作室的脚本。
- 添加了引擎构建脚本，并修补了处理器线程数控制。
- 启动器会阻止以 root 身份运行，并为本地引擎准备运行环境。

#### 波兰语界面和设置

- 添加了波兰语界面、提示和错误信息。
- 扩展了设置，可选择外观、文件夹和资源限制。
- 添加了下载传输速度显示。

#### 文件夹、模型和保存位置

- 添加了工作室数据目录选择，并支持将资料库和数据迁移到其他磁盘。
- 扩展了模型和引擎存储管理，支持迁移并显示磁盘占用情况。
- 添加了保存位置选择和可用磁盘空间信息。

#### 资源管理、诊断和可靠性

- 添加了处理器和内存使用控制、任务活动跟踪以及诊断信息导出。
- 改进了下载、引擎安装和任务状态恢复。
- 为新设置、本地 API、文件保存和安装流程添加了测试。

#### 本地 API 安全

- 为工作室的本地 API 添加了必需的访问密钥。在 Unix 系统中，密钥文件仅允许所有者读取（权限 0600）。
- 从请求 URL 和转发的数据中清除密钥，避免将其保存到内容中或传递给请求处理程序。
- API 仍仅在本机提供，并可供用户配置的工具使用。

## 日本語

**オリジナルの作者：** [timoncool](https://github.com/timoncool)

**オリジナルのプログラム：** [YuE2 Studio](https://github.com/timoncool/YuE2-Studio)

### オリジナル版からの変更点

高性能なグラフィックカードがないパソコンや、ディスク容量が少ない環境でも YuE2 Studio を使いやすくするために変更を加えました。

#### Debian と Linux

- Debian 用の `.deb` パッケージをビルド・インストールするスクリプトと、Studio の起動スクリプトを追加しました。
- エンジンのビルドスクリプトと、CPU スレッド数を制御するパッチを追加しました。
- ランチャーは root での実行を防ぎ、ローカルエンジンの実行環境を準備します。

#### ポーランド語と設定

- インターフェース、メッセージ、エラーのポーランド語訳を追加しました。
- 外観、フォルダー、リソース制限を選べる設定を追加しました。
- ダウンロードの転送速度表示を追加しました。

#### フォルダー、モデル、保存先

- Studio のデータ保存先を選択し、ライブラリーとデータを別のドライブへ移動できるようにしました。
- モデルとエンジンの保存先管理を拡張し、移動とディスク使用量の確認に対応しました。
- ファイルの保存先選択と空き容量の表示を追加しました。

#### リソース、診断、信頼性

- CPU とメモリーの使用制御、タスクの稼働状況、診断情報のエクスポートを追加しました。
- ダウンロード、エンジンのインストール、タスク状態の復旧を改善しました。
- 新しい設定、ローカル API、ファイル保存、インストール処理のテストを追加しました。

#### ローカル API のセキュリティ

- Studio のローカル API に必須のアクセスキーを追加しました。Unix ではキーを所有者だけが読めるファイル（権限 0600）に保存します。
- キーが保存内容に残ったりリクエスト処理に渡ったりしないよう、リクエスト URL と転送データから取り除きます。
- API はローカルのまま維持し、ユーザーが設定したツールから利用できます。

## 한국어

**원작자:** [timoncool](https://github.com/timoncool)

**원본 프로그램:** [YuE2 Studio](https://github.com/timoncool/YuE2-Studio)

### 원본 대비 변경 사항

고성능 그래픽 카드가 없거나 디스크 공간이 부족한 컴퓨터에서도 YuE2 Studio를 더 편리하게 사용할 수 있도록 변경했습니다.

#### Debian 및 Linux

- Debian용 `.deb` 패키지를 빌드·설치하고 스튜디오를 실행하는 스크립트를 추가했습니다.
- 엔진 빌드 스크립트와 CPU 스레드 수를 제어하는 패치를 추가했습니다.
- 실행기는 root 권한으로 스튜디오를 실행하지 못하게 하고 로컬 엔진 환경을 준비합니다.

#### 폴란드어 인터페이스 및 설정

- 인터페이스, 메시지, 오류의 폴란드어 번역을 추가했습니다.
- 모양, 폴더, 리소스 제한을 선택할 수 있도록 설정을 확장했습니다.
- 다운로드 전송 속도 표시를 추가했습니다.

#### 폴더, 모델 및 저장

- 스튜디오 데이터 위치를 선택하고 라이브러리와 데이터를 다른 드라이브로 옮기는 기능을 추가했습니다.
- 모델 및 엔진 저장소 관리를 확장해 위치 이동과 디스크 사용량 확인을 지원합니다.
- 파일 저장 위치 선택과 여유 공간 정보를 추가했습니다.

#### 리소스, 진단 및 안정성

- CPU·메모리 사용량 제어, 작업 활동 추적, 진단 정보 내보내기를 추가했습니다.
- 다운로드, 엔진 설치, 작업 상태 복구 처리를 개선했습니다.
- 새 설정, 로컬 API, 파일 저장 및 설치 흐름에 대한 테스트를 추가했습니다.

#### 로컬 API 보안

- 스튜디오 로컬 API에 필수 접근 키를 추가했습니다. Unix에서는 소유자만 읽을 수 있는 파일(권한 0600)에 키를 저장합니다.
- 키가 저장된 내용에 남거나 요청 처리기에 전달되지 않도록 요청 URL과 전달 데이터에서 제거합니다.
- API는 로컬에서만 제공되며 사용자가 설정한 도구에서 사용할 수 있습니다.

## Русский

**Автор оригинала:** [timoncool](https://github.com/timoncool)

**Оригинальная программа:** [YuE2 Studio](https://github.com/timoncool/YuE2-Studio)

### Изменения относительно оригинала

Я внес эти изменения, чтобы YuE2 Studio было удобнее использовать на компьютерах без мощной видеокарты и с небольшим дисковым пространством.

#### Debian и Linux

- Добавлены скрипты сборки и установки пакета `.deb`, а также запуска студии в Debian.
- Добавлены скрипты сборки движков и патч для управления количеством потоков процессора.
- Программа запуска не позволяет запускать студию от root и подготавливает окружение для локальных движков.

#### Польский язык и настройки

- Добавлен польский перевод интерфейса, сообщений и ошибок.
- Настройки расширены выбором оформления, папок и ограничений ресурсов.
- Добавлено отображение скорости передачи при загрузке.

#### Папки, модели и сохранение

- Добавлен выбор расположения данных студии и перенос библиотеки и данных на другой диск.
- Расширено управление хранилищем моделей и движков: перенос и просмотр занимаемого места.
- Добавлены выбор папки сохранения и информация о свободном месте.

#### Ресурсы, диагностика и надежность

- Добавлены управление нагрузкой на процессор и память, отслеживание активности задач и экспорт диагностических данных.
- Улучшена обработка загрузок, установки движков и восстановления состояния задач.
- Добавлены тесты новых настроек, локального API, сохранения файлов и процессов установки.

#### Безопасность локального API

- Для локального API студии добавлен обязательный ключ доступа. В Unix ключ хранится в файле, доступном только владельцу (права 0600).
- Ключ удаляется из URL запросов и передаваемых данных, чтобы он не попадал в сохраненное содержимое и обработчики запросов.
- API остается локальным и доступен настроенным пользователем инструментам.

## Português

**Autor original:** [timoncool](https://github.com/timoncool)

**Programa original:** [YuE2 Studio](https://github.com/timoncool/YuE2-Studio)

### Alterações em relação ao programa original

Fiz estas alterações para tornar o YuE2 Studio mais acessível em computadores sem placas gráficas potentes e com pouco espaço em disco.

#### Debian e Linux

- Adicionei scripts para compilar e instalar um pacote `.deb` e iniciar o estúdio no Debian.
- Adicionei scripts de compilação dos motores e uma correção para controlar o número de threads do processador.
- O iniciador impede a execução do estúdio como root e prepara o ambiente para os motores locais.

#### Interface e definições em polaco

- Adicionei a tradução para polaco da interface, das mensagens e dos erros.
- Ampliei as definições com opções de aparência, pastas e limites de recursos.
- Adicionei a indicação da velocidade de transferência durante as descargas.

#### Pastas, modelos e ficheiros guardados

- Adicionei a escolha da localização dos dados do estúdio e a possibilidade de mover a biblioteca e os dados para outro disco.
- Ampliei a gestão das localizações dos modelos e motores, com opções para os mover e consultar o espaço ocupado.
- Adicionei a escolha do local de gravação e a indicação do espaço livre.

#### Recursos, diagnóstico e fiabilidade

- Adicionei o controlo da utilização do processador e da memória, o acompanhamento da atividade das tarefas e a exportação de dados de diagnóstico.
- Melhorei o tratamento das descargas, da instalação dos motores e da recuperação do estado das tarefas.
- Adicionei testes para as novas definições, a API local, a gravação de ficheiros e os processos de instalação.

#### Segurança da API local

- Adicionei uma chave de acesso obrigatória à API local do estúdio. Em sistemas Unix, a chave é guardada num ficheiro acessível apenas pelo proprietário (permissões 0600).
- Removo a chave dos URLs dos pedidos e dos dados encaminhados para que não seja incluída no conteúdo guardado nem passada aos handlers.
- A API continua local e disponível para as ferramentas configuradas pelo utilizador.
