# VoicePaste — відкриті задачі

Канонічний backlog продукту, перенесений із `RustMigration/TODO.md`.
Оновлено 2026-10-29 після повторного review і підтвердження користувача.
Результати й межі перевірок: [verification](docs/verification-2026-10-29.md).
Контракт: [behavior](docs/behavior.md); defaults і saved prompts не змінювати.

## Надійність: підтверджені залишкові ризики

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

## Відкладено за рішенням користувача

- [ ] **Моделі / реальна мережа:** великий HTTPS download/resume/cancel/failure,
  integrity/cache та offline restart. Наразі пропускаємо. Loopback tests не
  замінюють цей сценарій; provisioning Drop чекає свій потік і не має глобального
  shutdown deadline. Не завантажувати моделі без окремого погодження.
- [ ] **macOS/Linux:** реалізувати й кваліфікувати native adapters, permissions,
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
