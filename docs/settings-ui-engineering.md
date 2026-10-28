# Settings UI: правила з вихідного коду Zed

Статус: **обов'язкові правила реалізації та review Settings**, не звіт про
виправлення продукту. Дата: 2026-10-28. Вимога користувача: чітке, адаптивне,
неблимаюче вікно у стилі Zed **One Dark**, основний масштаб Windows **175%**.
Читайте разом із [engineering-principles.md](engineering-principles.md).
Короткі постійні настанови є в кореневому [AGENTS.md](../AGENTS.md).

## 1. Що саме перевірено

Переглянуто відповідні файли **репозиторію Zed**, а не лише опис GPUI чи тему:
коміт [`37faef81f74fc1bb08be9939c4cffc8ede6ced2f`](https://github.com/zed-industries/zed/commit/37faef81f74fc1bb08be9939c4cffc8ede6ced2f),
2026-10-27, `Protect against solargraph LSP violations (#66157)`.
Посилання нижче прив'язані до цього коміту, не до рухомого `main`.

Це source review, не запуск тестів Zed і не аудит усього редактора. З нього
переносимо принципи, а не залежності редактора чи довільні уривки його коду.
Збережений сумісний кандидат нашого UI: `gpui-kit =0.8.1` / `gpui-pre-* =0.3.10`;
він **не тотожний** цьому коміту Zed. API й потрібні backend-виправлення потрібно
звіряти з реальною зафіксованою залежністю. Не змішувати несумісні snapshots.

### Карта вихідних джерел

| Код | Перевірений механізм |
| --- | --- |
| [S1: Windows manifest](https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui/resources/windows/gpui.manifest.xml#L15-L20) | `PerMonitorV2`, не bitmap-масштабування DPI-unaware вікна |
| [S2: Windows window state](https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui_windows/src/window.rs#L127-L145) | `GetDpiForWindow`, окремі logical/device dimensions |
| [S3: native resize / modal loop](https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui_windows/src/events.rs#L221-L315) | Оновлення logical size, renderer resize, callback; обслуговування роботи у size/menu loop |
| [S4: DPI / reentrancy](https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui_windows/src/events.rs#L836-L933) | Звільнення borrow перед зовнішнім викликом; DPI change та синхронні вкладені `WM_SIZE` |
| [S5: Windows frame dispatch](https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui_windows/src/events.rs#L1272-L1392) | Відкладені/coalesced callbacks, draw guard, validation update region, повернення до кадру |
| [S6: render / present](https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui_windows/src/directx_renderer.rs#L330-L390) | `draw → render(scene) → present`; очистка render target не є окремим видимим кадром |
| [S7: renderer resize](https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui_windows/src/directx_renderer.rs#L485-L526) | Перебудова surface/resources лише при потрібній зміні розміру; окрема device-loss обробка |
| [S8: DirectWrite](https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui_windows/src/direct_write.rs#L745-L815) | Glyph rasterization з `scale_factor`, subpixel position та вибором AA mode |
| [S9: settings row layout](https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/settings_ui/src/settings_ui.rs#L1410-L1477) | Flex-рядок, `min_w_0`, width constraints, опис і контроль у спільному layout |
| [S10: settings viewport](https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/settings_ui/src/settings_ui.rs#L3650-L3729) / [page shell](https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/settings_ui/src/settings_ui.rs#L4119-L4153) | Вимірювання за доступною шириною, `ListState` або tracked scroll container, `flex_1/min_h_0` |
| [S11: div clipping/offset](https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui/src/elements/div.rs#L2437-L2449) / [child offset](https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui/src/elements/div.rs#L2082-L2096) | Одна scroll position, content mask і зсув дерева елементів |
| [S12: scrollbar handle](https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/ui/src/components/scrollbar.rs#L1067-L1103) / [thumb drag](https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/ui/src/components/scrollbar.rs#L1681-L1703) | Scrollbar змінює offset того самого scrollable handle |
| [S13: input identity](https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/settings_ui/src/components/input_field.rs#L13-L38) / [state across frames](https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui/src/window.rs#L4422-L4472) | Стабільний unique element ID зберігає стан input та accessibility identity |
| [S14: typography](https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/ui/src/styles/typography.rs#L10-L37) / [page theme](https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/settings_ui/src/settings_ui.rs#L4119-L4129) | UI-font/size tokens та semantic theme colors, не системний stock font |
| [S15: invalidation](https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui/src/window.rs#L2424-L2429) / [frame production](https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui/src/window.rs#L3466-L3487) | Dirty state планує наступний кадр, layout/prepaint/paint не змішуються з input callback |
| [S16: actual resize/reflow test](https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/settings_ui/src/settings_ui.rs#L5758-L5810) | Тест змінює viewport через `simulate_resize`, перевіряє reflow і scroll selection |

## 2. Обов'язкові правила для нашої реалізації

### UI01 — один узгоджений шлях Settings, не саморобний движок із child HWND

Для заміни Settings використовувати узгоджений GPUI/component layout/render/input
шлях із перевіреного сумісного snapshot. Не продовжувати нарощувати стару форму
глобальними invalidation/clipping hacks і не називати її темним перефарбуванням Zed.
Win32 залишається вузьким platform adapter там, де потрібен, а не другим renderer.
Не переносити overlay на інший backend і не відкривати його дослідження знову.
Самі Win32 controls не приречені на ці баги; проблема — наша реалізація та невідповідність
обраному target. Rust як мова не забезпечує layout/DPI/кадри автоматично. [S6, S9–S12]

### UI02 — DPI-коректність до створення вікна

Погоджено 175% як основний acceptance target, а не лише 100/150/200%.
Належна DPI awareness має бути встановлена до створення Settings HWND; перевіряти
**зібраний EXE і живе вікно**, не лише XML у дереві файлів. `GetDpiForWindow`
DPI-unaware вікна повертає 96, це не реальна щільність монітора.
Backend має обробляти `WM_DPICHANGED`, рекомендовані bounds і новий scale factor.
Не додавати один process-DPI виклик без аудиту всіх координат host/overlay. [S1–S4]

### UI03 — logical coordinates та device pixels не змішувати

Layout/збережена geometry — у визначених logical units; surface, растеризація й
ввід ОС перетворюються на platform boundary рівно один раз. При 175% scale = 1.75,
а не округлені 1 або 2. Мінімальний розмір, padding, hitboxes, font metrics і
scrollbar мають використовувати одну scale-модель; DPI change інвалідовує залежні
кеші. Не масштабувати готовий низькороздільний bitmap замість повторного render.
Збережені 16 settings і їхня geometry-семантика залишаються контрактом. [S2, S4, S8]

### UI04 — користувач бачить завершений кадр, не процес стирання

Створити/оновити layout і scene, намалювати у backing surface, потім представити
готовий результат. **Повний redraw backbuffer дозволений**: сам Zed очищає render
target. Заборонений видимий цикл «стерти вікно → по черзі намалювати дітей».
`RDW_ERASE | RDW_ALLCHILDREN` на кожен scroll — не стратегія кадру Settings.
Не лікувати це `LockWindowUpdate`, приховуванням вікна чи окремими bitmap-копіями
кожного контрола. Double buffering окремого елемента не виправляє всю frame pipeline. [S6]

### UI05 — ввід змінює стан; renderer планує кадр

Wheel/drag/resize оновлюють bounded state та dirty flag; backend coalesces роботу
на кадр, не виконує повний синхронний paint для кожного mouse move. Paint не
запускає безмежну рекурсію/повторну інвалідацію. Reentrant draw має відкласти
потрібний кадр без spin-loop, а не втратити його назавжди. Idle Settings не потребує
власного постійного animation timer. Не копіювати політику vsync усього Zed,
якщо достатньо штатного механізму сумісного GPUI backend. [S5, S15]

### UI06 — scroll viewport та спільний offset

Одна позиція скролу для wheel, thumb drag, track/page і keyboard navigation.
Контент малюється з offset і clip mask у видимій області; не пересувати десятки
системних child HWND на кожен крок. Scroll limits залежать від актуально виміряних
content/viewport, clamp оновлюється після resize, reflow і prompt resize. Header
та Save/Cancel лишаються поза scroll viewport. Virtualization потрібна лише за
обсягом контенту; шість секцій самі по собі не виправдовують складний list engine. [S10–S12]

### UI07 — layout від наявного простору, не таблиця координат

Rows/sections описуються flex/constraints, мають shrink/min-size policy та
вимірювані тексти. Scroll body отримує решту висоти (`flex_1/min_h_0`-аналог),
footer не губиться за краєм. При звуженні текст переноситься, контрол може
розтягнутися/стиснутися в погоджених межах; коли необхідно, label/control row
переходить у вертикальний flow. Конкретний breakpoint — наше рішення, не нібито
готова поведінка Zed. Числа для padding/мінімальних розмірів дозволені; абсолютна
таблиця `y=...` як layout усієї форми — ні. Не додавати навігацію/поля редактора,
яких немає в VoicePaste. Explicit 2026-10-28 user exception: master–detail with
left navigation Keys / Transcription / Appearance / System (horizontal tabs were
rejected). Preserve the common draft and Save/Cancel across pages. System owns
residency and the multiline memory report as well as startup/logging. [S9–S10]

### UI08 — resize проходить повну pipeline

Native size change оновлює logical viewport, surface dimensions, layout, scroll
bounds і hit-testing до наступної презентації. Перевіряти normal resize,
maximize/restore, minimize/restore і DPI transition. Resize не повинен вимагати
наступного scroll, щоб оживити layout. Не підміняти тест доставки resize ручним
`form.layout()` — він обходить саме зламану частину. [S3, S7, S16]

### UI09 — жодного borrow/lock через reentrant OS/framework call

Не тримати mutable state borrow під час `DefWindowProc`, `SendMessage`,
`SetWindowPos`, `ShowWindow`, modal dialog чи чужого callback. Вузько зняти snapshot,
звільнити borrow, виконати зовнішню дію; або використати штатне безпечне deferred
оновлення framework. `try_borrow_mut` + default handler — не універсальна обробка:
не можна мовчки втрачати `WM_SIZE`, close, save completion чи input.
Збіг подій coalesce/replay має явну політику. [S4, S5]

Settings tray activation must explicitly reveal a hidden window, not just focus
it. The pinned GPUI Windows `activate` injects synthetic Alt down/up; do not use
that activation path here. Snapshot the HWND in GPUI, release App/RefCell borrows,
then reveal/restore and request foreground without keyboard injection. GUI launch
must not use STARTUPINFO SW_HIDE as a substitute for console suppression.

### UI10 — не покладатися лише на зовнішній message loop

Windows resize, thumb/menu tracking та native dialogs можуть запускати вкладені
message loops. Штатний dispatcher має підтримувати layout/frame delivery і
завершення UI-команд у цих станах, з обмеженням роботи за один callback.
Не додавати конкуруючий UI/render thread, blocking wait або application logic
в `WM_PAINT`. CPU/GPU/model/network/file work залишаються поза UI thread;
Save completion/error не повинні лишати всю форму disabled без виходу. [S3, S5]

### UI11 — чіткість тексту/іконок — частина DPI pipeline

Використовувати якісний text shaper/rasterizer backend з фактичним scale factor,
font metrics і належним AA, vector icons або assets достатньої роздільності.
Не розтягувати `DEFAULT_GUI_FONT`/96-DPI canvas засобами bitmap virtualization.
ClearType не є універсальною вимогою (прозорі поверхні потребують іншого AA);
вимога — чіткий результат на 175%. Font/icon assets локальні, з ліцензіями. [S8, S14]

### UI12 — стан input/focus живе довше одного кадру

Стабільні IDs і owned input state зберігають unsaved draft, caret, selection,
IME composition, focus, prompt height та scroll під час rerender/resize.
Не створювати новий input state з saved config у кожному `render`.
Theme/status/RAM updates не скидають введення. Keyboard Tab/Shift+Tab, видимий
focus і accessibility labels — частина компонента, не опціональна декорація.
Zed може мати іншу persistence-поведінку; **у нас Save/Cancel зберігаються**. [S9, S13]

### UI13 — тема — semantic tokens плюс компоненти/стани

One Dark задає узгоджені поверхні, border/text/accent та стани контролів разом
із typography/spacing. Не вважати JSON палітри готовим скіном довільного UI.
Використовувати єдині tokens, відповідні buttons/inputs/dropdowns і focus/hover/
disabled/error states. Звіряти screenshot з Zed, а також input behavior; GPUI Kit
сам собою не гарантує appearance 1:1. Не імпортувати весь редактор для кольорів;
ліцензії GPUI/Kit, theme, font/icon assets і editor code перевіряти окремо. [S9, S13–S14]

### UI14 — correctness картинки та smoothness — різні перевірки

Статичний screenshot після завершення scroll або збіг з forced redraw доводить
лише фінальні pixels. Він **не доводить** відсутність flicker/blank frames,
неперервний reflow чи responsiveness. Тести мають йти через реальні input/resize
події, а не лише helper-методи. Під час freeze до змін потрібні duration і UI-thread
stack/trace або відтворення; не оголошувати deadlock без доказу. Часовий дефект
перевіряти кадрами/trace і реальною взаємодією, не кількістю unit-тестів. [S16]

## 3. Acceptance-перевірка Settings

- [ ] Реальний зібраний EXE/Settings DPI-aware; 175% — основний запуск.
- [ ] На 100/125/150/175/200% текст та іконки коректні; mixed-DPI move підтримує
      scale, bounds, input і незбережені дані. Матриця не замінює фізичні 175%.
- [ ] Wheel up/down, неперервний thumb drag, track/page, keyboard scroll:
      немає blank flashes, видимого erase, trails чи неузгоджених hitboxes.
- [ ] Edge/corner resize туди-назад і maximize/restore: ширини/переноси/scroll
      limits/footer змінюються без додаткової події; input не обрізається.
- [ ] Prompt resize, Unicode/IME, довгий текст, caret/selection, dropdowns і
      клавіатурна навігація зберігаються під час scroll/resize/status updates.
- [ ] Close/Cancel та save success/failure працюють; pending UI work не губиться
      у resize/scroll/menu loops. No hangs перевірено реальною взаємодією/trace,
      а не `Responding=true` чи screenshot після відновлення.
- [ ] Save/Cancel, hotkey rollback, geometry persistence, validation, defaults
      і next-idle settings application залишилися незмінними.
- [ ] One Dark оцінено на controls/fonts/states, не тільки на background.
- [ ] Виміряно host UI-open/closed RAM та idle CPU; не оголошено GPUI автоматично
      дешевшим за native controls і не переплутано host із STT worker.
- [ ] Докази фіксують input path, DPI, window size і часову поведінку. Неоглянуті
      пункти позначені як неперевірені; user-reported failure переважає старий pass.

Це критерії звичайної реалізації, не новий production approval gate, не новий
overlay probe і не дозвіл на встановлення SDK, завантаження моделей або зупинку
сторонніх застосунків.
