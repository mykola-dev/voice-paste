# VoicePaste — відкриті задачі

Канонічний backlog продукту, перенесений із `RustMigration/TODO.md`.
Оновлено 2026-10-31: архітектурний review та залишок macOS-порту.
Результати й межі перевірок: [shared verification](docs/verification-2026-10-31.md)
та [earlier Windows verification](docs/verification-2026-10-29.md).
Контракт: [behavior](docs/behavior.md); defaults і saved prompts не змінювати.

## ПРІОРИТЕТ: один спільний застосунок, не окремий застосунок на кожну ОС

**Обов'язкова архітектура — common domain/application/orchestration/GPUI та
вузькі native adapters, аналог `expect`/`actual`. Не розмножувати сценарії й не
розсипати `if Windows/macOS` або `cfg` по бізнес-логіці та UI. Правило закріплено
в [AGENTS.md](AGENTS.md). Поточний порт ще не відповідає йому повністю.**

- [x] **Об'єднати UI-session coordination.** Прибрати дублювання між
  `apps/voicepaste/src/adapters/platform/session.rs` і
  `apps/voicepaste/src/adapters/platform/macos.rs`: GPUI bootstrap/lifetime,
  runtime events, Settings open/Save/completion, statistics/RAM, overlay updates,
  frame wake/polling, command backpressure та orderly shutdown мають одного
  спільного власника. Залишити за native adapters лише необхідні OS effects;
  не сховати два старі цикли за одним trait або великим platform switch.
  - [x] Перший крок: `adapters/ui_session.rs` спільно володіє GPUI bootstrap,
    platform/global teardown і runtime shutdown, Settings generations/saved state,
    pending Save та stale completions, routing Settings replies, statistics/RAM
    queries/backpressure і frame-wake deadline. Обидві ОС використовують цей код;
    native hotkey staging/commit виконується поза App borrows.
    Локальна Windows-перевірка 2026-10-31: fmt, 157 tests passed / 1 interactive
    ignored, clippy та Debug build `--no-deploy`; runnable folders не оновлено.
    Додано 5 shared regressions. macOS compilation/CI цього кроку ще не виконано.
  - [x] `RuntimeSession`: спільні bounded runtime-event pump (64 replies),
    state/model-status projection та overlay updates; native effects поза App
    borrow. Buffered replies доставляються перед disconnect error. Model-status
    refresh використовує open draft selection і не губиться на full command queue;
    macOS не перезаписує readiness hint backend/menu status на кожному tick.
    Додано 4 shared regressions. Windows: fmt, 173 tests passed / 1 interactive
    ignored, clippy і Debug build `--offline --no-deploy` пройшли; runnable folders
    не оновлено.
    macOS compilation/contracts цього кроку ще не запускались.
  - [x] Обидві ОС виконують один `platform/session.rs`: native intents,
    permission handoff, overlay lifecycle та shutdown. Native hosts не мають
    runtime/UI циклів; вони володіють menu/event tap/hotkey effects. `RecordingInput`
    зберігає початковий permission target, не запускає дубльований Start після
    grant, пріоритезує Cancel і повторює його після backpressure. Quit очікує
    accepted Save completion/registration commit. Додано 5 shared input regressions.
    macOS compilation/CI поточного кроку ще не виконано.
- [x] **Об'єднати lifecycle підготовки моделей.** Звірити `adapters/models.rs`
  та `adapters/models/macos.rs`, винести спільні integrity/publication,
  partial/resume, progress, cancellation/ownership і failure rules. HTTP/native
  filesystem primitives та GGML/CT2 catalogs лишаються окремими там, де потрібно;
  не уніфікувати різні моделі або backend policies штучно.
  - [x] `adapters/model_preparation.rs`: один власник provisioning thread,
    bounded terminal result, cancellation/join, progress snapshots та chunked
    SHA-256. Обидва adapters використовують його; stream/file progress лишаються
    явними transport policies. Terminal result одноразовий; network activity
    скидається після success/error/panic. Додано 8 спільних регресій без мережі.
    Windows 2026-10-31: fmt, 165 tests passed / 1 interactive ignored, clippy і
    Debug build `--offline --no-deploy` пройшли; runnable folders не оновлено.
    macOS compilation/CI цих змін ще не виконано.
  - [x] Exact-size/SHA-256 verification і verified publication мають спільну
    реалізацію: non-file rejection, sync staging, cancellation перед commit,
    backup попереднього corrupt completed artifact. macOS offline promotion,
    download publication і VAD copy використовують ті самі гарантії; native
    hard-link/rename та directory sync ізольовано у files adapter. Додано 4 shared
    publication regressions і 2 macOS-only contracts; розширено macOS offline test.
    Windows 2026-10-31: fmt, 169 tests passed / 1 interactive ignored, clippy і
    Debug build `--offline --no-deploy` пройшли; runnable folders не оновлено.
    macOS compilation/contracts цього кроку ще не запускались.
  - [x] Partial/resume має спільний сценарій: offline complete-partial recovery,
    resume offset, reset corrupt complete/oversized staging лише за дозволеної
    мережі, verification/publication після transport та retention після interruption.
    Виправлено пропущену publication після macOS download у попередніх робочих
    змінах; додано 5 shared regressions без мережі. Native staging permissions,
    cache locking/receipts і download approval лишаються окремими boundaries.
    Publication вимагає cache writer lock і не гарантує
    crash-atomic transaction для всього multi-file model або захист від hostile writer.
- [x] **Аудит решти платформних розгалужень і дублів.** Перевірити Settings,
  storage/config save, startup, CLI/composition, runtime та worker boundaries.
  Спільні рішення/валідація/GPUI мають залишатися common; платформні відмінності
  подавати через вузькі контракти й явні дані/політики. `cfg` для native modules
  і dependencies дозволений; OS switches усередині сценаріїв — не заміна дизайну.
  - [x] Settings rendering більше не перемикає layout/text за OS: explicit profile
    дані задають native permission notice, startup/memory/backend hints та ваговий
    artifact для спільного GPUI. Додано catalog/profile contract на обох hosts.
  - [x] Config staging/write/sync/cleanup та Save startup→publish→rollback мають
    спільних власників; snapshots/Run value/LaunchAgent і native replacement
    залишаються native. Failed staging creation не видаляє чужий colliding file.
  - [x] Worker memory probing та private temp-file permissions вибрані на module
    boundary; shared worker більше не містить цих OS branches.
  - [x] `bootstrap.rs` володіє CLI routing, model preparation, offline file
    transcription і runtime/session startup. Native composition визначає paths
    та dependencies. Наявні --check/batch-WAV/stdout contracts задані даними;
    file-mode на обох ОС перевіряє pinned artifacts offline, ніколи не завантажує.
    Виявлений Windows integrity bypass не скопійовано; виправлення явно записано
    в CHANGELOG. CLI parser regressions перевіряють обидва contracts і Unicode paths.
  - [x] Оновлено [verification evidence](docs/verification-2026-10-31.md); старий CI
    `b2664b6` зібрав Release/package і пройшов clippy, але 2 нові shared UI fixtures
    використали Windows defaults на macOS (70 host tests passed / 2 failed).
    Fixtures виправлено на host-selected defaults. `30df76d` пройшов macOS
    Release/package/signatures, clippy, 75 host і 3 worker tests у CI 41226390965.
- [x] **Критерій завершення refactor.** Обидві ОС використовують один session
  coordinator і спільні lifecycle policies; native/backend differences мають
  конкретне обґрунтування. Додати спільні регресії Save/Cancel, stale completions,
  queue saturation, shutdown та provisioning failures; перевірити адаптери обох
  ОС, fmt/clippy/tests/build/package. Зберегти Windows behavior, prompts/data,
  macOS permissions і заборону silent fallback. Оновити architecture docs та
  перевірити код на дублювання, а не лише отримати зелений CI.
  Source consolidation і автоматичні contracts виконано; Windows Release package
  `--offline --no-deploy` verified (40 files, no user data). Device acceptance
  нижче залишається відкритим і не випливає з цих перевірок.

## Надійність: підтверджені залишкові ризики

- [ ] **Overlay cold-start double height (user report, still open).** Перший
  показ після запуску застосунку виглядає зависоким, як на скріншоті з подвоєною
  висотою; повторний показ нормальний. Перехід між моніторами після останнього
  виправлення нормальний. Підозра: стартова геометрія/перший paint дає viewport
  вище канонічних 100 DIP, а blur брав висоту з HWND без обмеження панеллю.
  Виправлення-кандидат: blur/material обмежено 100 DIP висоти, `show()` гріє
  прихований swapchain, звіряє viewport з канонічними пикселями поточного DPI і
  відновлює його через звичайний `WM_SIZE` до показу material. Hidden/native
  тести цього не доводять; потрібне живе visual acceptance першого показу.

- [ ] **Overlay refresh-rate acceptance.** Slide/fade більше не семплюється
  20-мс host polling: foreground семплює monotonic timeline на GPUI frame,
  coalesced wake доставляє той самий sample native material поза App borrow.
  Next-frame callbacks читають refresh_interval поточного дисплея; тривалості
  220/160 мс і швидкість waveform/pulse/bars не залежать від кількості кадрів.
  Потрібне вимірювання фактичного render/present cadence на 120 Гц та mixed
  120/60 Гц: pinned Windows backend має DWM-wide vsync; unit tests цього не доводять.
  Tray artwork не змінювали; гарантований FPS Explorer не заявляємо.

- [ ] **Overlay нижні кути / зайвий напівпрозорий контур.** Користувач повторно
  відкрив visual acceptance після slide/fade. Read-only діагностика живого Debug:
  outer `(3379,30)-(3745,180)`, client origin `(3390,30)`, size `344×139`, DPI 144;
  blur follows client `344×139`, а GPUI панель має висоту 100 DIP = 150 px.
  Backend створює PopUp зі style 0; Windows додає WS_CAPTION, GPUI прибирає
  лише верхній non-client inset. DWMNCRP_DISABLED не прибирає бокові/нижній inset.
  Hidden regression зі style 0 відтворила помилку до виправлення. Windows adapter
  тепер задає справжній frameless WS_POPUP до показу. Потрібне візуальне
  підтвердження; не зупиняти запис і не замінювати запущений Debug для оновлення.

- [ ] **Overlay slide/fade regression (user report).** Після появи панель їде
  назад і стає невидимою, хоча запис триває. Попередні unit/adapter checks не
  доводять коректність живої анімації; visual acceptance повторно відкрито.
  Користувач підтвердив збій у Debug, після якого повернувся на старий Release.
  Виправлення: нерухомий viewport до краю work area, GPUI translate/opacity і
  відповідний Composition offset/opacity замість покадрового руху HWND та
  перерахунку anchor з анімованих координат. Потрібне повторне visual acceptance;
  hidden/native tests не доводять відсутність live-збою.
  Не зупиняти активний запис для відтворення або оновлення.

- [ ] **Worker IO/shutdown deadline.** GPUI dispatcher teardown і full-command-
  queue shutdown уже виправлені; це інші дефекти. У
  `apps/voicepaste/src/adapters/worker/windows.rs` 500 ms починаються лише після
  synchronous Shutdown write. `send`, forced `wait` і reader `join` не мають
  незалежної межі. Перевірити stalled stdin, успадковані pipes і завислий loaded
  worker; зберегти kill-owned-tree-before-join та audio release після process.
  Наявні graceful/forced child tests проходять, але не моделюють усі ці відмови.
- [ ] **Native error reporting/cleanup.** Create/Append menu, cursor, timer,
  posting fallback і class ownership уже виправлені/покриті регресіями.
  `Shell_NotifyIconW(NIM_MODIFY/NIM_DELETE)` і частина cleanup results досі
  ігноруються; відмова сповіщення може приховати первинну помилку.
  TrackPopupMenu zero трактується як dismissal. Окремо перевірити failure paths
  Save/startup rollback; не змінювати справжній Run value без погодження.

## Поширення: не плутати готовий builder із повною кваліфікацією

- [ ] Чиста Windows: CPU та driver-only CUDA, реальна inference з packaged
  ORT/CRT, перенесення/розпакування portable. Локальна перевірка hashes/imports/
  ZIP не доводить clean-machine або hardware compatibility.
- [ ] Підтвердити фактичний GitHub Actions build і публічний Release. Version tag
  автоматично публікує реліз після перевірки checksums і завантаження assets;
  review YAML не є запуском CI. Публікацію погоджено користувачем.
- [ ] Повний redistribution review: Rust/UI/native licenses, artwork rights,
  model cards і застосовні VS Distributable Code terms. Hash/version/origin/
  license-path inventory уже є, але не замінює юридичну перевірку.
- [ ] Відтворювана CT2 4.7.1 CUDA **source build**, якщо це вимога релізу.
  Поточний builder отримує hash-pinned офіційні binaries/headers; це працюючий
  acquisition pipeline, не власна CT2 source build. Не встановлювати SDK без дозволу.
- [ ] Якщо знадобиться automatic update/rollback: нинішній publisher не є
  транзакцією на весь каталог; після preflight збій IO може залишити часткове
  оновлення. Locked-destination rejection/data preservation уже тестуються.

## macOS: authorized development on `macOS-2`

Поточний стан: recording/runtime, single-key controls/Escape, GPUI Settings/overlay,
whisper.cpp/Metal worker, approved GGML/VAD provisioning, Cmd+V та statistics
підключені. Для `dc90085` [CI 41187550557](https://github.com/mykola-dev/voice-paste/actions/runs/41187550557)
пройшов macOS Release/package/signature, clippy та host contracts. Windows:
152 tests passed, 1 interactive ignored, clippy/Debug compile; runnable folders
не оновлювали (`--no-deploy`). Це **не** завершення архітектури або реальна
inference/device acceptance. Користувач підтвердив лише запуск ранішого локально
зібраного Rust preview на corporate macOS 26.7.1. Downloaded CI apps там заблоковані.

- [ ] **Спочатку архітектурні виправлення вище**, потім фінальна автоматична
  перевірка й один спільний ручний прогін, без проміжних slice-test прохань.
- [x] **Фінальні commits/CI.** Локальні `258c4f6` (ambiguous GGML rejection та
  запуск worker contracts у CI) і `24e6dbb` (combined acceptance guide) pushed
  разом зі shared-session/provisioning/bootstrap refactor. Windows: 189 tests
  passed / 1 interactive ignored, fmt/clippy і Debug build `--offline --no-deploy`.
  Runnable folders не оновлено. Фінальний code commit `30df76d` пройшов
  [CI 41226390965](https://github.com/mykola-dev/voice-paste/actions/runs/41226390965):
  portable contracts, 75 macOS host **і 3 worker** tests, Release bundle/package/signatures
  та clippy.
  Старий зелений CI не доводить коректність наступних змін.
- [ ] **Повний локальний build/update на корпоративному Mac.** Зберегти Settings,
  явно Quit і відкласти стару `.app`, оновити `macOS-2`, зібрати штатним script.
  Не видаляти Application Support/legacy config, не змінювати Gatekeeper/MDM,
  не вимагати платного Developer ID. Локальний запуск не доводить довіру до
  downloaded ad-hoc artifacts; production distribution потребує окремого рішення.
- [ ] **Реальна доставка моделей.** Лише після явного дозволу перевірити повні
  FP16 large-v2/large-v3/large-v3-turbo та Silero: HTTPS, progress, cancel/resume,
  hashes, corrupted partial/cache, offline restart і corporate-network failures.
    Усі три повні GGML моделі й Silero завантажені локально, size/SHA-256 перевірені.
    Погоджена заміна: macOS — окремий GitHub Release `Models`/`models`, raw parts
    зі спільним download/assembly lifecycle; Windows — попередній HTTPS source.
    Git LFS fallback прибрано на обох платформах. [Models assets](https://github.com/mykola-dev/voice-paste/releases/tag/models)
    опубліковані: усі remote sizes/digests збігаються з локальними; публічні range
    downloads усіх шести частин і повний VAD перевірені локально. `a918028` пройшов
    [macOS CI 41333232913](https://github.com/mykola-dev/voice-paste/actions/runs/41333232913),
    Windows fmt/clippy/208 tests (1 interactive ignored), Debug пакет оновлено.
    Меню, TCC recovery, generation-scoped Cancel і progress/error UI реалізовані;
    corporate Mac delivery та реальна device/visual acceptance ще відкриті.
    Offline import ще не реалізований.
  Джерело доказів: [model delivery](docs/macos-model-delivery.md).
- [ ] **Справжня inference і M1+ qualification.** Записати chip/RAM/macOS;
  перевірити Metal-only без CPU fallback і явний CPU, усі погоджені моделі,
  beam/VAD, English/Ukrainian/bilingual/auto, custom prompt, silence та >30 s audio.
  Виміряти latency, окремий host/worker RAM і memory pressure, keep-in-memory
  on/off та unload/reload. Не зменшувати модель/beam/VAD/precision заради RAM.
  Поточна збірка на одному runner не кваліфікує M1 або обидві підтримувані macOS.
- [ ] **TCC, recording keys та focus/paste.** Microphone/Accessibility allow,
  deny/retry; Right Control/Right Option/F8/F12, repeats/chords, key replacement,
  Escape/stale session, busy toggles. Перевірити Unicode, рівно один Cmd+V,
  original-app restoration/current-focus fallback, manual clipboard on failure
  та відсутність paste після shutdown. Target зараз app/PID, не стабільна AX
  window identity; цю межу не видавати за гарантію повернення в те саме вікно.
- [ ] **GPUI/native visual acceptance.** Settings resize/scroll/DPI/focus/IME,
  draft identity, Save/Apply/Cancel, prompts та statistics reset; overlay
  entry/exit/reversal/waveform/busy без focus theft/flicker, mixed displays,
  placement/clipping і translucent fallback. Поточний macOS blur — public
  `NSVisualEffectView` + white tint/16-DIP clipping, не доведений еквівалент
  Windows Gaussian kernel. Потрібне візуальне підтвердження користувача.
- [ ] **Lifecycle/startup/data.** Перевірити login enable/disable та реальний
  logout/login; Quit під час recording/download/inference/pending Save,
  microphone-first stop, owned-child cleanup, persistence і statistics migration.
  **Abrupt-crash temporary-recording recovery не реалізовано**: окремо визначити
  безпечне відновлення/прибирання без видалення активних файлів іншого instance.
- [ ] **Підсумкова acceptance/evidence.** Пройти один
  [combined local-device checklist](docs/macos-preview.md), записати результати
  й залишкові дефекти, оновити docs відповідно до фактичної архітектури. Не
  називати порт завершеним через CI. Не merge/push master і не публікувати macOS
  release без окремого запиту. Linux лишається поза scope.

## Відкладено за рішенням користувача

- [ ] **Моделі / реальна мережа:** великий HTTPS download/resume/cancel/failure,
  integrity/cache та offline restart. Наразі пропускаємо. Loopback tests не
  замінюють цей сценарій; provisioning Drop чекає свій потік і не має глобального
  shutdown deadline. Не завантажувати моделі без окремого погодження.
- [ ] **Linux:** реалізувати й кваліфікувати native adapters, permissions,
  GPUI/CPAL backends і native ML packaging лише за окремим дозволом. Source
  isolation завершена, але портів немає. Receipt-v1 timestamps Windows-specific:
  invalidate/reverify, не reinterpret. Windows focus restore лишається best-effort;
  snapshot не захищає від HWND reuse.

## Не блокери / лише за виміряною потребою

- [ ] **Пам'ять:** користувач не спостерігає витоків; поверхневий ownership review
  явного leak не виявив. Можливі оптимізації peak RAM: повний WAV, VAD output,
  padded waveform/features і копія в native encoder. Виміряти перед змінами;
  не trim working set і не погіршувати accuracy/VAD. Повний Rust/legacy benchmark
  та числовий бюджет зараз не є умовою завершення міграції.

## Закрито / не повертати в backlog без нової регресії

- Windows interactive acceptance (overlay/Settings/hotkeys/tray/focus/paste):
  користувач прямо підтвердив «по першому пункту вже все окей» 2026-10-29.
  Це user acceptance, не твердження про окремий автоматичний прогін кожного DPI.
- Запланована Windows source isolation; native resource ownership boundaries.
- Виправлення GPUI quit deadlock, command-queue shutdown, partial worker setup,
  native menu construction cleanup, window-class cleanup, timer error reporting,
  failed-post fallback, DPI icon commit і Windows xtask self-rebuild lock.
- Локальні build/package механізми, manifest/ZIP/checksum/import/inventory checks
  реалізовані. Решту release qualification перелічено вище окремо.
