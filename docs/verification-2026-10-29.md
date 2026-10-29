# Windows: повторна перевірка надійності й пакування

Дата: 2026-10-29. Scope: shutdown, native errors, поверхневий memory review,
локальне пакування. Windows interactive acceptance користувач підтвердив;
download/model network і macOS/Linux явно пропущені. Канонічні відкриті задачі
тепер у [TODO.md](../TODO.md); [старий шлях](../RustMigration/TODO.md) збережений.

## 1. Windows interactive acceptance — закрито користувачем

Підтвердження: «по першому пункту вже все окей … дані просто були застарілі».
Не вимагаємо повторювати закриті overlay/Settings/hotkey/tray/focus/paste checks
без нової регресії. Це user acceptance, не новий автоматичний exhaustive DPI run.

## 2. Shutdown — попередні fixes присутні, hard-deadline борг реальний

- `platform/session.rs::run`: platform живе поза Application, teardown globals
  відбувається до dispatcher drop; runtime shutdown після UI. Старий GPUI
  deadlock fix не втрачено. Історичний process test описано в roadmap; цього
  разу GUI/process preview harness не запускали й не розширювали.
- `runtime.rs`: authoritative shutdown flag і nonblocking `try_send(Quit)`;
  full-command-queue regression проходить. Додано regression: full event queue
  не блокує `reliable` після shutdown або receiver disconnect.
- `worker/windows.rs`: graceful і forced owned-child regressions проходять;
  Job закривається до reader joins, process release передує audio/target drop.
- **Не закрито:** synchronous `send(Shutdown)` перед стартом 500-ms wait,
  загалом `send`, forced `wait` і reader joins не мають незалежного deadline.
  Отже «quit deadlock виправлено» не означає «весь shutdown гарантовано bounded».
  Stalled pipe/loaded-native fault injection цього разу не проводили.
- `models.rs::Preparation::drop` теж синхронно чекає provisioning thread;
  cancellation перевіряється між IO, а не миттєво перериває native IO.
  Цей network/fault сценарій залишається у явно відкладеній задачі.

## 3. Native errors — без нової звичайної регресії, є незакриті відмови

- Native menu creation/append/cursor повертають error; Menu RAII прибирає handle.
  Menu IDs/enabled flags/destruction, class register/drop/reregister, posting
  fallback та inbox overflow тести пройшли. Це не actual OS queue exhaustion.
- Storage: розширено реальний filesystem test. Stable read handle (дозволяє read,
  забороняє replacement) провокує
  failure replacement після validation/temp write; після звільнення handle
  початкові bytes не змінені, `.config-*` temp відсутній, retry Save успішний.
  Invalid settings/lock contention також не змінюють config. У fixture startup
  disabled, справжній registry не зачіпається.
- Source review `startup/windows.rs`/`storage.rs`: snapshot exact type/bytes,
  rollback на failed replace, combined rollback error reporting присутні.
  **Actual registry mutation/rollback fault injection не проводили.**
- **Не закрито:** `Shell_NotifyIconW` modify/delete і низка cleanup status results
  ігноруються; збій balloon notification може приховати іншу помилку.
  `TrackPopupMenu` zero досі означає dismissal. Тести normal ownership не
  доводять коректність усіх native failures; ці пункти лишаються в загальному TODO.

## 4. Пам'ять — поверхневий source review, не leak profiler

Користувач не спостерігає витоків. Переглянуто audio/worker owners,
`engine.rs`, `native/bridge.cpp`, `pipeline.rs`, `frontend.rs`, `vad.rs`.
Явних нових leak paths у переглянутих власниках не знайдено:

- recording stream закриває producer до writer join; audio file має owner/Drop;
- process/job/pipes/readers мають owner; handoff queues bounded;
- Engine/Encoded мають native destructors, borrowed lifetime не дозволяє
  Encoded пережити Engine; C++ uses unique_ptr до transfer ownership;
- per-request audio/features/output не накопичуються як resident history;
  resident model/tokenizer/VAD відповідають keep-in-memory policy.

Зауваження лише про **peak/copies**, не про доведений leak: повний WAV у RAM,
VAD output поряд із input, padded waveform і весь mel buffer, window Vec і copy
у C++ encoder. Зміни без вимірювання не робили. Native allocator/CUDA pools,
driver/GPUI allocations і довгий repeated-dictation lifecycle не профілювали;
source review не є доказом відсутності всіх витоків.

## 5. Перевірки й пакування

- `cargo --offline xtask check --offline`: **123 tests passed, 1 interactive paste
  ignored**, fmt і clippy `-D warnings` успішні. Зміни коду цього recheck — лише
  regression tests; production behavior/defaults не змінювали.
- Існуючий повний `build/debug/VoicePaste.exe --check` пройшов із PATH,
  обмеженим System32. Це config/startup check, не inference і не новий ZIP launch.
- `cargo --offline xtask package --offline --no-deploy --release` успішний:
  optimized Release compilation, native inventory/import checks, manifest/ZIP
  verification — **40 files, no user data**. ZIP і SHA-256 оновлено в
  `build/package/VoicePaste-2.0.0-windows-x86_64.zip[.sha256]`.
  `build/release/` НЕ оновлено; `build/debug/` також не змінювали.
- Review `xtask/package.rs`, `docs/native-inputs.md`, release/CI YAML підтверджує
  manifest/hash/required-file/import checks і draft-only publication logic.
  При цьому `.github/workflows/ci-windows.yml` на інших ОС перевіряє build tool,
  а не працездатність портів. CI чи GitHub Release у цій сесії не запускали.
- Наявний native inventory вже містить version/hash/origin/licensePath, і builder
  використовує pinned official inputs. Старе «inventory відсутній» було б хибним;
  full redistribution/legal review, clean-machine inference та CT2 source build
  не випливають із проходження локального builder.
- Publisher preflight захищає від відомих locked/unmanaged paths, але publication
  копіює файли послідовно після звільнення preflight locks. Це не transactional
  updater/rollback при довільному IO failure; уже наявна межа, не нова регресія.

Запущений `build/release/VoicePaste.exe` не зупиняли; runnable папки цим recheck
не оновлюються. Моделі/SDK не завантажували, registry/clipboard/microphone не
зачіпали. Дані й saved prompts не включаються до ZIP.
