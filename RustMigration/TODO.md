# Незавершені задачі

Це лише відкриті задачі та непроведені перевірки, не перелік уже реалізованих
функцій. Не змінювати поведінку/defaults без погодження. Перевірки з мікрофоном,
clipboard/focus, реальним завантаженням моделей чи sign-in узгоджувати окремо.

## Виправлення й оптимізація

- [ ] **Quit/shutdown qualification:** deadlock при drop GPUI Kit text cleanup
  локалізовано native stack: async-channel/EventListener повторно брав mutex після
  знищення dispatcher. Platform тепер живе довше Application/globals; звичайний
  `cx.quit()` збережений, без process::exit/kill workaround. Короткий процесний
  тест `tests/quit-process.Tests.ps1` відтворив старий дефект і пройшов після фіксу
  без моделі/мережі/мікрофона. Loading, busy, cancel/error та loaded-worker exit
  залишаються окремою qualification; успішний preview-тест їх не доводить.
- [ ] **Пам'ять (U08):** виміряти allocations/modules/buffers worker за lifecycle
  stages (CT2, CUDA/cuBLAS, ORT/VAD, tokenizer/frontend, staging/copies).
  Усунути лише виміряне зайве утримання/копії. Порівняти повні Rust/legacy process
  trees у однакових cold, loaded-idle та post-dictation станах, з UI-open cost,
  peaks і окремою VRAM. Однакові модель/options/residency; не trim working set і
  не знижувати точність/VAD. Числовий бюджет погодити після коректного порівняння.

## Portable package і платформи

- [ ] Відтворювана native CT2 4.7.1 CUDA збірка й повна DLL/CRT/ORT/VAD closure
  замість залежності від наявного локального binary cache. Підтвердити чисту
  Windows-машину/driver-only CUDA та CPU route; не встановлювати SDK без дозволу.
  `cargo xtask prepare` готує DLL, matching headers і notices з офіційних hash-pinned
  архівів (`xtask/native-inputs.json`), CRT — з установленого VS. CT2/OpenMP
  байт-ідентичні раніше перевіреному runtime. Це не CT2 source-build; нові stable
  ORT 1.25.1 та CRT потребують inference qualification. Виявлені старі build-дефекти:
  неправильна назва VS redist version-файлу та припущення, що AVX512 BSD notice
  містить слово Permission; у xtask виправлено, оригінальні notices збережено.
- [ ] Повний inventory версій, hashes, licenses/notices/model cards і support assets
  для поширення; поточні third-party notices ще часткові.
- [ ] Qualification `cargo xtask package --release` (root BAT launcher): staging/manifest/ZIP
  і draft GitHub Release workflow реалізовано; перевірити clean-machine native
  closure, archive extraction/portable move та збої. Builder відмовляє при
  заблокованих файлах, не зупиняє активну інсталяцію; автоматичний update/rollback
  не реалізований. `--no-deploy` дозволяє зробити ZIP без оновлення runnable folder.
  GitHub workflow використовує той самий BAT, без private native bundle/approval
  variables. Реальна GitHub binary-збірка та публікація ще мають бути підтверджені.
  Локальний root BAT із `--check --no-deploy --offline` пройшов: workspace
  fmt/tests/clippy, 12 build-tool regressions, ZIP із 40 файлами та SHA-256.
  Із розпакованого ZIP пройшли host `--check`, worker startup і ORT/VAD session init
  з tokenizer без weights (очікувана помилка model.bin); це не повна inference
  qualification і не перевірка чистої Windows. Запущену інсталяцію не оновлювали.
  Канонічний layout: `build/debug/`, `build/release/`, `build/package/` для ZIP/checksum,
  `build/cache/` для всіх технічних inputs/tools/target/staging. Старі діагностичні
  файли збережені в `build/cache/previous-build/`, особисті `data/` не переносились.
- [ ] Великий реальний HTTPS download/resume/cancel/failure, cache/integrity та
  offline restart з моделлю. Loopback-тести не закривають реальний network path.
- [ ] Поступово ізолювати решту Win32 adapters за compile-time platform boundaries.
  Linux/macOS — майбутні цілі, не дозвіл реалізовувати порти чи fake-success adapters.
