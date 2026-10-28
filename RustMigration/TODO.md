# Незавершені задачі

Це лише відкриті задачі та непроведені перевірки, не перелік уже реалізованих
функцій. Не змінювати поведінку/defaults без погодження. Перевірки з мікрофоном,
clipboard/focus, реальним завантаженням моделей чи sign-in узгоджувати окремо.

## Виправлення й оптимізація

- [ ] **Quit/shutdown:** після закриття вікон host/worker іноді залишаються живими
  понад 15 секунд. Зняти bounded diagnostics, виправити ownership/join/shutdown;
  перевірити idle, loading, busy, cancel/error та повторні запуски. Примусове
  завершення windowless-процесу — workaround, не виправлення.
- [ ] **Пам'ять (U08):** виміряти allocations/modules/buffers worker за lifecycle
  stages (CT2, CUDA/cuBLAS/cuDNN, ORT/VAD, tokenizer/frontend, staging/copies).
  Усунути лише виміряне зайве утримання/копії. Порівняти повні Rust/legacy process
  trees у однакових cold, loaded-idle та post-dictation станах, з UI-open cost,
  peaks і окремою VRAM. Однакові модель/options/residency; не trim working set і
  не знижувати точність/VAD. Числовий бюджет погодити після коректного порівняння.

## Portable package і платформи

- [ ] Відтворювана native CT2 4.7.1 CUDA збірка й повна DLL/CRT/ORT/VAD closure
  замість залежності від наявного локального binary cache. Підтвердити чисту
  Windows-машину/driver-only CUDA та CPU route; не встановлювати SDK без дозволу.
- [ ] Повний inventory версій, hashes, licenses/notices/model cards і support assets
  для поширення; поточні third-party notices ще часткові.
- [ ] Qualification `scripts/build-rust.ps1 -Action package -Release` (root BAT launcher): staging/manifest/ZIP
  і draft GitHub Release workflow реалізовано; перевірити clean-machine native
  closure, archive extraction/portable move та збої. Builder відмовляє при наявному
  output, не оновлює активну інсталяцію; автоматичний update/rollback не реалізований.
  GitHub workflow потребує reviewed native bundle URL/SHA-256 та дозволу на поширення.
- [ ] Великий реальний HTTPS download/resume/cancel/failure, cache/integrity та
  offline restart з моделлю. Loopback-тести не закривають реальний network path.
- [ ] Поступово ізолювати решту Win32 adapters за compile-time platform boundaries.
  Linux/macOS — майбутні цілі, не дозвіл реалізовувати порти чи fake-success adapters.
