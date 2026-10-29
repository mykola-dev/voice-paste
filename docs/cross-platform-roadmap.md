# Cross-platform readiness

Статус: 2026-10-29, заплановану Windows-only source isolation виконано. Це карта робіт, не заява про
підтримку macOS/Linux. Контракт функцій: [behavior.md](behavior.md); правила
залежностей: [engineering-principles.md](engineering-principles.md).

## Мета і незмінні правила

Одна логіка диктування, одна модель налаштувань і спільний GPUI UI; мінімальні
платформні адаптери. Зберігаємо функції, а не однакові OS-виклики. Не змінюємо
defaults, особисті prompts/config/models/history чи поведінку Windows під
виглядом рефакторингу. Немає нового універсального PlatformService, plugin
framework, фіктивно успішних заглушок або crate на кожен trait.

GPUI відповідає за UI та частину інтеграцій, не за всю ОС. У pinned GPUI-pre
0.3.10 є clipboard, URL, file picker та application/dock menu API. Це не
гарантія глобальних hotkeys, tray popup, вставлення в іншу програму чи однакової
підтримки backend-ами. Наявності достатнього системного sound API не підтверджено.
Спершу перевіряємо потрібний контракт GPUI, потім придатну спеціалізовану
бібліотеку; за відсутності відповідності залишаємо вузький native adapter.

## Матриця функцій і залежностей

Шляхи adapters нижче відносні до `apps/voicepaste/src/adapters/`.
«Спільний» означає структуру коду, не кваліфікацію на іншій ОС.

| Функція / власник | Що вже спільне / що лишається Windows | Контракт і перевірка при зміні |
| --- | --- | --- |
| Диктування: `crates/core`, `crates/application`, `runtime.rs` | State machine/application відокремлено; target передається як generic `T`, concrete composition ще Windows | 300 ms debounce, busy ignores toggle, session-specific cancel; deterministic tests, потім наскрізна перевірка |
| Hotkeys: `hotkey.rs`, `platform/windows/` | Parser у core, bounded batch policy у `platform/inbox.rs`; registration/hooks/callback bridge Windows | Усі поточні клавіші, right-modifier semantics, Escape swallowing, conflict/save rollback; unit + погоджена реальна клавіатура |
| Audio: `audio.rs` | `cpal`, `rubato`, WAV; реальні devices/backends не кваліфіковано поза Windows | Default mic, 16 kHz mono PCM16, gain лише для UI, cancel/cleanup; unit + погоджений запис |
| STT: `apps/stt-worker`, `worker.rs`, `worker/transport.rs`, `worker/windows.rs`, `crates/protocol` | Спільний pipeline/protocol/readiness policy; private Process володіє Job/child/pipes/readers | VAD/prompts/languages/long audio, CPU/CUDA/Auto, residency, stale/deadline/shutdown; policy/owned-process tests + native inference |
| Вставлення: `paste.rs`, `paste/windows.rs`, application `Paste<T>` | Application переносить opaque target; Windows `FocusTarget` приватно зберігає snapshot, clipboard/SendInput Windows | Capture target до activation, Unicode, три shortcuts, modifier cleanup, text лишається при failed injection; ownership/unit + opt-in real paste |
| Settings: `settings_window.rs`, `settings_theme.rs` | GPUI layout/draft; reveal/foreground та native handle seam Windows | OK/Cancel/Apply, Unicode/IME, focus/caret, geometry, scrolling; GPUI tests + user acceptance на 175% |
| Overlay: `overlay.rs`, `overlay/state.rs`, `overlay/windows/` | GPUI foreground/state/frames; placement/nonactivation/blur Windows | Sharp foreground, rounded blur, alpha, next-show appearance, no activation; unit/native tests + visual acceptance |
| Tray: `tray_icons.rs`, `platform/windows/host.rs`, `platform/menu.rs` | Artwork/state policy; HICON/Shell_NotifyIcon Windows | Idle/open-mouth artwork, exact DPI size, left Settings/right menu; asset/callback tests + manual tray |
| Tray menu: `platform/menu.rs`, `platform/windows.rs` | Typed actions, recording label/enabled policy; TrackPopupMenu Windows | Start/Stop, disabled busy, Settings, Quit, dismissal, target captured before activation, animations continue; policy/ID tests + manual menu |
| Автозапуск: `startup.rs`, `startup/windows.rs` | Налаштування спільне, exact Run-value snapshot/rollback у native adapter | Opt-in, Save transaction/rollback; unit + контрольований adapter test |
| Settings/files: `storage.rs`, `files/windows.rs` | JSON/transaction/path policy спільні; exclusive/stable file sharing і durable replacement native | App-local data, no reseed, prompts verbatim, failure preserves old config/draft; filesystem regression tests |
| Моделі: `models.rs`, `models/http.rs`, `models/http/windows.rs` | Catalog/hashes/resume/range policy спільні; WinHTTP, file primitives і receipt timestamp extraction native | Explicit provisioning, no download from draft/build, integrity/cancel/resume; loopback/filesystem + погоджений real download |
| Статистика: `statistics.rs` | SQLite/schema/query types переважно спільні | No transcript/prompt/audio, stale filters, transactional reset, preserve history; DB/application/UI tests |
| Діагностика: `memory.rs`, `diagnostics.rs`, `launch/windows.rs` | Byte projection/log policy спільні; process probes і console/error UI ізольовані | Host/worker окремо, privacy, no console focus theft; adapter tests + product checks |
| Збірка: `xtask`, build scripts/native assets | Канонічний layout і locks; Windows resource/native packaging | Complete package, native closure/licenses, preserve data; packaging tests + clean machine |

## Послідовність

1. **Поточний пакет:** матриця вище та перша межа `platform.rs`.
   `platform/menu.rs` не залежить від GPUI/Win32; задає порядок усіх пунктів,
   підписи, розділювачі, enabled policy та типізовані команди.
   `platform/windows.rs` лише відображає цей опис, володіє native menu tracking та
   Settings reveal/activation; native IDs перетворюються на команди на цій межі.
   Вигляд меню і порядок викликів не змінено. Native menu/activation виконуються
   поза App/RefCell borrow; HWND не належить helper-у і має жити до повернення.
2. **Другий зріз реалізовано:** hidden host/tray ownership та hotkey registration
   у `platform/windows/host.rs`, callbacks/atomic hook state/thread-local inbox у
   `platform/windows/events.rs`. `platform/inbox.rs` задає bounded FIFO policy
   без OS/GUI dependencies. GPUI coordinator отримує batch через host, не читає
   HWND/Binding fields і не викликає Win32 напряму. Ліміт 8, overflow notification,
   окремі Quit/Cancel slots, coalesced tray/settings flags, 20 ms polling, cleanup
   та shutdown order збережено. Другого message loop немає.
3. **Третій зріз реалізовано:** concrete Windows selection у малому `platform.rs`,
   GPUI coordination у `platform/session.rs`, без Windows module paths у session.
   Це compile-time composition, не новий framework чи готовий порт.
   Application target — generic `T`, а не HWND/`usize`; порти Paste/Transcription
   та Submission/completion зберігають його тип і передають за значенням, без
   Clone/Copy/Default constraints. Windows `paste/windows.rs::FocusTarget` приватно
   володіє лише snapshot, не foreign window. Restore залишається best-effort;
   native API відхиляє невалідні handles, failure означає current-focus fallback.
   Target не серіалізується в IPC/config. Capture відбувається до tray activation.
   Четвертий зріз нижче прибирає native identity snapshots із session.
   Application не залежить від GPUI/Win32; UI не виконує paste/IO напряму.
4. **Четвертий зріз реалізовано:** `platform/windows/ui.rs` ізолює native
   Settings activation intent і overlay lifecycle. Settings повертає тільки GPUI
   handle; HWND extraction більше немає у спільному Settings UI чи session.
   Activation snapshot застосовується після event batch тільки до тієї самої
   відкритої generation. Overlay adapter володіє shell/enabled, виконує native
   mutations поза App/RefCell borrows і зберігає prepare/material/show/fallback,
   hide/reprepare та shutdown ordering. Failure вимикає лише indicator. Немає
   universal platform service, нового trait framework чи fake adapters.
5. **Завершальний source-isolation пакет реалізовано:** private native worker
   process owner, shared bounded response decoder, файлові effects окремо від
   Settings/model transaction policy, WinHTTP окремо від range/resume policy,
   native startup/memory/console/error UI/icon allocation/hotkey/clipboard modules.
   Native dependencies target-scoped; source guard захищає спільні policy/UI
   модулі від прямих native imports/unsafe. Формати IPC/config/receipts, defaults,
   retry/residency та saved prompts не змінено. GPUI activation із synthetic Alt
   і fake-success adapters не додано. Виправлено перелічені нижче lifecycle debts;
   автоматичне приймання й runnable deployment фіксуються окремо нижче.
6. **Порти пізніше, за окремим погодженням:** одна ОС за раз, спочатку повний
   шлях launch → permissions → hotkey → record → transcribe → paste → quit,
   потім повна матриця. macOS permissions, Linux Wayland/X11 і доступність трея,
   GPU backend/native ML packaging досліджуються на реальній цільовій системі.
   CUDA не обіцяємо на всіх ОС; blur fallback залишається translucency. Відмінності
   hotkeys/paste/backend support погоджуємо явно, не прибираємо фічі мовчки.

## Відомі межі й дефекти

- `platform.rs` досі Windows-selected composition, session винесено окремо.
  Settings/overlay native snapshots і shell lifecycle належать `windows/ui.rs`;
  інші native ефекти також ізольовано, але composition лишається Windows-only.
  Compile-time selection/source guard не доводять build/runtime іншої ОС.
- Menu CreatePopupMenu/AppendMenuW/cursor failures тепер повідомляються як error;
  RAII звільняє menu на всіх виходах. TrackPopupMenu zero return і далі трактується
  як dismissal; повний native failure-injection audit не виконано.
- Windows focus snapshot не тримає чуже вікно живим; native handle може стати
  stale/reused. Це існуюче обмеження, не вирішене generic target. Не обіцяємо
  гарантованого повернення фокуса чи OS identity protection.
- Hook bridge досі використовує process-global atomics та один thread-local
  inbox: підтримується один UI-thread host, не кілька інстансів в одному процесі.
  PostMessage failure тепер зберігає callback input у тому самому bounded inbox і
  coalesces warning; це не обіцянка успішного OS posting, unbounded queue чи
  збереження глобального порядку між уже posted та fallback input.
- Host class тепер RAII-owned/unregister-иться, SetTimer failure повертає error,
  DPI size commit відбувається лише після двох успішних icon allocations. Hidden
  class/menu tests не кваліфікують повний repeated host/hook/tray create/drop.
- Process setup тепер установлює owner до fallible pipe/thread setup й reaps child
  на failure. 500 ms shutdown bound стосується post-send wait: synchronous stdin
  write і joins самі по собі не мають hard deadline. Потрібен окремий fault-path
  audit, без обіцянки bounded shutdown лише на підставі простого child fixture.
- Receipt v1 timestamps лишаються Windows-specific cache format. Майбутній порт
  має invalidate/reverify, а не трактувати їх як portable file identity.
- Linux/macOS builds та їхні GPUI backends не перевірені. `cfg` на новому native
  модулі не означає, що весь host уже може компілюватися поза Windows.

## Приймання кожного пакета

`fmt`, `clippy`, workspace tests; потрібні adapter regressions; незмінні defaults
і saved data. Автоматичні тести не доводять real paste, hotkeys, focus, smoothness
або inference. Ризикові інтерактивні перевірки погоджуються окремо. Звіт чітко
відрізняє виконане від запланованого, compilation від portable deployment.

Користувач підтвердив виправлені краї оверлею та роботу, відзначив суб'єктивно
кращу плавність. Це не закриває решту mixed-DPI/alpha/focus/lifecycle матриці.

### Перевірка першого зрізу (2026-10-29)

- Форматування, workspace tests (99 passed, 1 interactive paste ignored), clippy
  та Debug compilation `build --no-deploy` пройшли через локальний hidden runner
  `build/cache/overlay-tools.ps1` / копію xtask (обхід відомого xtask self-lock).
- Додано перевірки всього опису меню для idle/recording/transcribing та взаємного
  перетворення native IDs/typed commands. Наявні hidden callback/reveal regressions
  також пройшли. Інтерактивне меню/focus/paste після зміни не перевірялося.
- `build/debug/` та `build/release/` НЕ оновлено; запущений застосунок не зупиняли.
  Жодних перевірок macOS/Linux, downloads або SDK installs не виконувалося.

### Перевірка другого зрізу (2026-10-29)

- Форматування, workspace tests (101 passed, 1 interactive paste ignored), clippy,
  `git diff --check` і Debug `build --no-deploy` пройшли через той самий hidden
  runner та xtask self-lock workaround.
- Дві нові regression-перевірки: FIFO/overflow/control slots/reset без OS і
  доставка native hotkey/Cancel/Close batch через справжню window procedure.
  Існуючі synchronous Shell callback, tray mapping і hidden Settings reveal
  перевірки перенесено до Windows-модулів; вони також пройшли.
- Host extraction зберігає наявний create/drop, staged hotkey save/rollback,
  polling і порядок GPUI/platform/runtime lifetime. Реальні registration conflicts,
  keyboard hooks/Escape, tray tracking під час запису та exit з loaded worker
  після цього зрізу інтерактивно не перевірялися.
- Runnable `build/debug/` і `build/release/` НЕ оновлено; дані не змінювали,
  застосунок не зупиняли. Порти macOS/Linux не реалізовано й не перевірено.

### Перевірка третього зрізу (2026-10-29)

- Форматування, workspace tests (105 passed, 1 interactive paste ignored), clippy,
  `git diff --check` і Debug `build --no-deploy` пройшли тим самим hidden runner.
- Додано дві application перевірки з нечисловим target без Clone/Copy/Default:
  Start snapshot доходить до matching completion, debounced/Stop/stale targets
  звільняються один раз; cancel/shutdown звільняють recording target без paste.
  Окремо перевірено drop відхиленого/drained target у bounded inbox і Send/no-op
  семантику Windows current-focus snapshot без активації/clipboard.
- Жодних protocol/config/defaults змін; worker залишає target у host-side pending
  submission, а не надсилає в native процес. Clipboard retries, shortcuts та
  current-focus fallback збережено. Інтерактивна вставка/focus після рефакторингу
  не перевірялася; успішні unit/hidden tests її не доводять.
- Runnable `build/debug/` і `build/release/` НЕ оновлено, процеси не зупиняли,
  models/data/prompts не змінювали. macOS/Linux адаптерів або кваліфікації немає.

### Перевірка четвертого зрізу (2026-10-29)

- Форматування, workspace tests (107 passed, 1 interactive paste ignored), clippy,
  `git diff --check` і Debug `build --no-deploy` пройшли через hidden runner.
- Дві нові deterministic перевірки охоплюють усі вісім комбінацій
  active/pending-show/visible: повторний show, приховування та geometry sync.
  Наявні hidden shell/nonactivation/backdrop/Settings reveal і shared Settings
  draft/save/cancel regressions також пройшли. Це не fault-injection тест усієї
  lifecycle sequence й не доказ відсутності reentrancy/focus/visual проблем.
- Native show/position/material operations залишилися поза App/entity/runtime
  borrows; нормальне закриття detach/drop shell передує GPUI foreground close.
  На cancellation збережено попередній Shell Drop без foreground mutation.
- Інтерактивне Settings reveal/close-during-save, overlay blur/corners/animation,
  mixed-DPI та наскрізне dictation/paste/loaded-worker shutdown після цього зрізу
  не перевірялися. User acceptance залишається відкритим.
- `build/debug/` і `build/release/` НЕ оновлено. Процеси не зупиняли; config,
  prompts, models і session data не змінювали; портів macOS/Linux немає.

### Завершальний source-isolation пакет і приймання (2026-10-29)

- Звичайний `cargo --offline xtask build --check --offline` пройшов без copied
  runner: fmt, **122 workspace tests passed, 1 interactive paste ignored**, clippy
  з `-D warnings`, Debug compilation і публікація повної portable папки.
  Nested build не перебудовує running xtask; workspace test/clippy його включають.
  Додано окрему regression-перевірку вибору Cargo packages.
- Нові перевірки: source-level native boundary guard; exact resumed Content-Range;
  response FIFO/overflow/disconnect і EOF/malformed frames; matching/stale
  generation/request/session, invalid readiness/CUDA policy, timeout/release і
  cleanup owned audio; graceful/forced owned-test-child exit + reader join.
  Child fixture — test EXE з CREATE_NO_WINDOW, не справжній ML worker. У звичайному
  запуску fixture інертний; він не використовує GUI, microphone чи clipboard.
- Native menu count/IDs/enabled flags і RAII destruction перевірено без показу
  меню. Перевірено register/drop/reregister owned window class. Posting fallback
  перевіряється deterministic failure-branch injection, не заявою про фактичне
  вичерпання Windows message queue. Timer/DPI allocation failure paths не мають
  повної fault-injection qualification.
- Порівняно з попереднім зрізом, явно виправлено error/lifecycle defects:
  process owner до fallible pipe setup, menu build errors + cleanup, callback
  fallback/warning, owned class unregister, SetTimer error, DPI-size commit після
  successful allocation, Windows xtask self-rebuild lock. Звичайні user flows,
  labels, shortcuts, IPC/config/receipt formats і public defaults збережено.
- **Оновлено `build/debug/`**, його config-only `VoicePaste.exe --check` пройшов.
  Це не launch tray/Settings, recording, native inference чи real paste acceptance.
  Активний процес знайдено лише у `build/release/`; Release не зупиняли й не
  оновлювали. Існуючий `data/` не включався до пакування/архівів і зберігається
  builder-ом; models/prompts не змінювали, speech downloads/SDK installs не було.
  ZIP не перевидавався цим `build`.

## Межа завершення цього етапу

Завершено заплановану **source isolation Windows host**: shared policy/UI не
імпортують native API, конкретні resources/effects мають Windows-selected owners,
збережено існуючі application contracts і data formats. Це не завершення всього
[загального TODO](../TODO.md) і не підтримка іншої ОС.

Оновлення після повторного review: користувач підтвердив Windows interactive
acceptance, попередні відкриті UI acceptance-пункти більше не актуальні.
Hard-deadline/fault-path native IO/cleanup і clean-machine redistribution
залишаються окремими задачами; network downloads і майбутні OS implementations
відкладено. Поточні статуси: [TODO](../TODO.md), докази:
[повторна перевірка](verification-2026-10-29.md). Історичні записи вище
не є актуальним списком незавершеної роботи.
