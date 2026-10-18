# telescope.nvim: переносимые механики

> Статус: ЧЕРНОВИК (инкрементальная запись, протокол «артефакт-первым»).
> Дата исследования: 2026-10-14. Зона: nvim-telescope/telescope.nvim (core) + расширения/форки.
> Каждое утверждение — со ссылкой на открытый первоисточник; непроверенное — в «Открытые вопросы».

## Карта возможностей

По `doc/telescope.txt` и README (версия master на дату исследования):

- **Ядро**: Finder → (entry_maker) → Sorter (score) → Picker-окно; prompt, результаты,
  preview — отдельные floating-окна; действия — функции от `prompt_bufnr`.
  Архитектурная схема нарисована в `doc/telescope.txt` (секция OVERVIEW, строки 20–60).
- **53 функции `builtin.*`** (уникальные имена в doc/telescope.txt): файлы (`find_files`, `git_files`, `grep_string`, `live_grep`),
  Vim (`buffers`, `oldfiles`, `help_tags`, `man_pages`, `keymaps`, `registers`, ...),
  LSP (`lsp_references`, `lsp_document_symbols`, `lsp_workspace_symbols`, `diagnostics`, ...),
  git (`git_commits`, `git_status`, `git_branches`, `git_stash`, ...), treesitter,
  `symbols` (JSON-источники символов из `data/telescope-sources/*.json`).
  Полный список — таблицы README «Pickers».
- **6 layout-стратегий**: `horizontal`, `center`, `cursor`, `vertical`, `flex`,
  `bottom_pane`; `layout_config` резолвит размеры (доля `0.5` или абсолют `80`).
  Важно: `ivy` — это НЕ значение `layout_strategy`, а тема `themes.get_ivy()`
  поверх `bottom_pane` (см. механику 2).
- **Previewers**: `vim_buffer_cat/vimgrep/qflist` (дефолт, буферы + treesitter/regex,
  без `bufload`), терминальные `cat/vimgrep/qflist` (внешний `cat`/`bat`),
  интерфейсы `Previewer:new` и `new_buffer_previewer`; глобальный конфиг
  `preview` (mime-проверка, лимиты размера, timeout, hooks).
- **Сортеры**: `get_fuzzy_file`, `get_generic_fuzzy_sorter`, `get_fzy_sorter`
  (дефолт), `get_levenshtein_sorter`, `fuzzy_with_index_bias`; `tiebreak`-функция;
  нативный сортер (fzf-native/fzy-native) подменяет дефолтный.
- **Кастомизация**: таблицы `mappings` по режимам (`i`/`n`), `attach_mappings` на
  пикер, темы-пресеты, `cycle_layout_list`, prompt history, `cache_picker`.
- **Экосистема расширений** (вне ядра): fzf-native (C-порт fzf), frecency, file-browser,
  project, ui-select, undo, live-grep-args — разбираются во втором проходе.

Границы: всё перечисленное — **ядро telescope.nvim** (`doc/telescope.txt`, README);
расширения и форки — отдельные репозитории, у них помечен источник.

## Топ-механики для Lusty

- **layout_strategy + резолвимый layout_config** — 6 стратегий: `horizontal`
  (результаты слева, preview справа, prompt bottom), `vertical` (preview сверху,
  prompt снизу), `center`, `cursor`, `flex` (переключает H/V по `flip_columns`,
  `flip_lines`), `bottom_pane` (для «ivy»-подобных полос). `layout_config`:
  `width`/`height` — доля (0.5) или абсолют (80); `preview_cutoff` отключает
  preview при нехватке колонок/строк (default: horizontal 120, vertical/center/cursor 40);
  `preview_width`/`preview_height`; `prompt_position` top/bottom; `mirror`; `anchor`,
  `anchor_padding`; `cycle_layout_list` + actions `layout.cycle_layout_next/prev`.
  *Переносить*: core — единая модель «стратегия + резолвер долей/абсолютов» и cutoff;
  TUI — grid/long и preview right/left/up/down; GUI — те же доли в CSS/tauri-окне.
  *Сложность*: M. Источник: https://github.com/nvim-telescope/telescope.nvim/blob/master/doc/telescope.txt
  (секции `layout_strategies.*`, `defaults.layout_strategy`, `defaults.layout_config`).

- **Ivy-режим = тема + `bottom_pane`, а не отдельная layout_strategy** — в списке
  доступных значений `layout_strategy` в docs нет «ivy»: стратегия `bottom_pane`
  «can be used to create layouts similar to 'ivy'», а удобный пресет даёт
  `themes.get_ivy()`. Поправка к распространённой формулировке «horizontal/vertical/ivy»:
  у telescope это `horizontal` / `vertical` / тема `ivy`.
  *Переносить*: TUI — пресет «нижняя полоса» (prompt сверху, список ниже, высота ~25 строк);
  GUI — preset `ivy`. *Сложность*: S.
  Источник: https://github.com/nvim-telescope/telescope.nvim/blob/master/doc/telescope.txt
  (секции `layout_strategies.bottom_pane`, `themes.get_ivy`).

- **Темы-пресеты как композиция опций** — `themes.get_dropdown(opts)`,
  `themes.get_cursor(opts)`, `themes.get_ivy(opts)` не отдельные механики, а
  «combine several elements of styling»: мержатся в opts пикера, можно выключать
  части (`get_dropdown{previewer=false}`). *Переносить*: core — пресеты из
  `--view list|grid|long` + `LUSTY_*` env; TUI/GUI — один и тот же объект настроек,
  фронтенды только рендерят. *Сложность*: S.
  Источник: https://github.com/nvim-telescope/telescope.nvim/blob/master/doc/telescope.txt
  (секция `telescope.themes`).

- **sorting_strategy + selection_strategy + scroll_strategy** — `sorting_strategy`
  задаёт, в какую сторону «лучшие»: `descending` (default) или `ascending`
  (лучшие сверху). `selection_strategy` управляет курсором после пересортировки:
  `reset` (default), `follow`, `row`, `closest`, `none`. `scroll_strategy`:
  `cycle` (default) или `limit` при выходе за пределы списка.
  *Переносить*: core/serve — режимы сортировки и политика курсора (в Lusty сортировка
  есть, но привязка «выделение ↔ пересортировка» не формализована); TUI — `scroll limit`
  как опция; GUI — те же enum'ы. *Сложность*: S.
  Источник: https://github.com/nvim-telescope/telescope.nvim/blob/master/doc/telescope.txt
  (секции `defaults.sorting_strategy`, `defaults.selection_strategy`, `defaults.scroll_strategy`).

- **Multiselect-реестр действий** — отдельные actions `add_selection`,
  `remove_selection`, `toggle_selection`, `select_all`, `drop_all`, `toggle_all`
  (docs, секция ACTIONS); дефолты `<Tab>`/`<S-Tab>` «toggle selection and move to
  next/prev»; `<M-q>` шлёт выбранные в quickfix, `<C-q>` — нефильтрованные;
  `resume` и `cache_picker` сохраняют мультивыбор между вызовами.
  *Переносить*: TUI — маркеры + toggle-all (в Lusty есть `C-Space`, нет select_all/drop_all);
  serve — расширить протокол (сейчас только пометки клиента); GUI — чекбоксы и batch-действия.
  *Сложность*: M. Источник: https://github.com/nvim-telescope/telescope.nvim/blob/master/doc/telescope.txt
  (секции ACTIONS, `defaults.cache_picker`, `builtin.resume`).

- **Mappings по режимам + action-объекты** — маппинги задаются таблицей
  `{ i = {...}, n = {...} }`; значением может быть action-функция
  (`function(prompt_bufnr)`), либо `{ action, type = "action"|"command", opts }`;
  `false` отключает дефолт; `attach_mappings = function(prompt_bufnr, map)` даёт
  пикеру добавить/переопределить клавиши; привязки живут только в окне пикера.
  Дефолтные клавиши: `<C-n>/<C-p>`, `j/k`, `H/M/L`, `gg/G`, `<CR>`, `<C-x>/<C-v>/<C-t>`,
  скролл preview `<C-u>/<C-d>/<C-f>/<C-k>`, `<C-/>`/`?` — показать маппинги.
  *Переносить*: TUI — реестр именованных actions вместо switch по клавишам;
  serve — таблица «клавиша → action» для nvim-клиента; GUI — горячие клавиши из того же реестра.
  *Сложность*: M. Источник: https://github.com/nvim-telescope/telescope.nvim/blob/master/doc/telescope.txt
  (секция MAPPINGS) + https://github.com/nvim-telescope/telescope.nvim/blob/master/README.md
  (секция «Default Mappings»).

- **Previewer-контракт и глобальный конфиг `preview`** — базовый `Previewer:new{preview_fn
  = function(self, entry, status)}`; высокоуровневый `new_buffer_previewer{define_preview
  = function(self, entry, status)}` с автоматическим управлением буфером; дефолтные
  `vim_buffer_cat/vimgrep/qflist` читают файл асинхронно (`vim.uv.fs_`), без `bufload`,
  чтобы не тормозить, и подсвечивают treesitter/regex; терминальные previewers
  (`cat/vimgrep/qflist`) — fallback через `cat`/`bat`. Глобальный `preview`: `check_mime_type`,
  `filesize_limit` = 25 MB, `highlight_limit` = 1 MB, `timeout` = 250 ms (защита от
  зависшего preview), hooks `filetype/mime/filesize/timeout`, событие
  `User TelescopePreviewerLoaded` для пост-настройки окна (wrap, number).
  *Переносить*: core — трейт previewer'а (загрузчик + лимиты + timeout);
  TUI — уже есть async/latest-wins, добавить filesize/timeout-политику;
  GUI — те же хуки/лимиты для web-view. *Сложность*: M.
  Источник: https://github.com/nvim-telescope/telescope.nvim/blob/master/doc/telescope.txt
  (секции `defaults.preview`, PREVIEWERS) + README (секция «Previewers»).

- **Entry maker + entry_display: колонки с фиксированной шириной и `remaining`** —
  entry — таблица `value`, `ordinal` (по чему фильтруем), `display` (строка или
  функция, вызываемая при отрисовке), `filename`/`bufnr`/`lnum`/`col` (что открыть),
  `valid` (false = скрыть), плюс highlights `{ {start_col, end_col}, hl_group }`;
  `entry_display.create{ separator, items = { {width=...}, ..., {remaining=true} } }`
  собирает строку из колонок; `create()` рекомендуется вызывать вне `make_display`,
  иначе он пересоздаётся на каждую запись.
  *Переносить*: core — разделение ordinal/display и структурированные колонки
  (meta_line уже близок); TUI long/grid — выравнивание по `remaining`; GUI — таблица.
  *Сложность*: M. Источник: https://github.com/nvim-telescope/telescope.nvim/blob/master/doc/telescope.txt
  (секции MAKE_ENTRY, PICKERS_ENTRY_DISPLAY).

- **Finder → Sorter-число → порядок + `tiebreak` и `prefilter`** — Sorter вызывается
  на каждую запись finder'а и возвращает «дистанцию»; docs указывают дефолт
  и для `file_sorter`, и для `generic_sorter` — `sorters.get_fzy_sorter`
  (README-таблица при этом называет «default sorter for files» `get_fuzzy_file`,
  а «for everything else» — `get_generic_fuzzy_sorter`; расхождение README↔docs,
  см. «Открытые вопросы»); нативный сортер перекрывает дефолт; `prefilter_sorter` оборачивает generic-sorter для
  уточнения уже отфильтрованного (`<C-space>` в `live_grep` — «refine current results with
  fuzzy search»); `tiebreak(current_entry, existing_entry, prompt) -> boolean` решает
  судьбу равных скоров (дефолт — по длине `ordinal`).
  *Переносить*: core — политика tiebreak (сейчас у Lusty — канонический индекс) и
  prefilter-стадия; serve — `C-space`-подобный refine; GUI — тот же скоринг.
  *Сложность*: S/M. Источник: https://github.com/nvim-telescope/telescope.nvim/blob/master/doc/telescope.txt
  (секции `defaults.tiebreak`, `defaults.file_sorter`, `defaults.prefilter_sorter`) +
  README (секция «Sorters»).

- **Builtin-пикеры как каталог «source»-композиций** — каждый пикер — тонкая функция,
  собирающая finder + entry_maker + previewer + actions: `find_files` (fd/find,
  `hidden`, `no_ignore`, `no_ignore_parent`), `live_grep` (`glob_pattern`, `type_filter`,
  `search_dirs`, `grep_open_files`, `additional_args`), `buffers`, `diagnostics`
  (`bufnr`, `severity`, `severity_limit`, `severity_bound`, `workspace`),
  `lsp_references/definitions/type_definitions/document_symbols/workspace_symbols`
  (фильтры `symbols`/`ignore_symbols`, ширины колонок), `git_*`, `help_tags`, `man_pages`,
  `symbols` (JSON-источники). Часть пикеров имеет свой prompt-комплит по `<C-l>`
  (prefilter по типу диагностики/символа).
  *Переносить*: core — абстракция `Source` с общим pipeline (у Lusty пока файлы/буферы);
  serve — новые источники как строки протокола; GUI — селектор источника.
  *Сложность*: L. Источник: https://github.com/nvim-telescope/telescope.nvim/blob/master/README.md
  (секции Pickers) + doc/telescope.txt (BUILTIN).

- **Игнор-политика: `file_ignore_patterns` vs gitignore-инструменты** — глобальная
  таблица Lua-regex, применяется во всех файловых пикерах; docs предупреждают, что это
  ломает `lsp_`-пикеры (например, `node_modules` пропадёт и из LSP-результатов);
  для `find_files`/`live_grep` рекомендуется наоборот полагаться на `fd`/`rg`, которые
  сами уважают `.gitignore`. *Переносить*: core — явное разделение «ignoring для walker'а»
  и «фильтр выдачи LSP/grep» (у Lusty есть skip-dirs; не смешивать с serve-источниками).
  *Сложность*: S. Источник: https://github.com/nvim-telescope/telescope.nvim/blob/master/doc/telescope.txt
  (секция `defaults.file_ignore_patterns`).

- **Prompt history + кэш пикеров** — `history` (файл `stdpath("data")/telescope_history`,
  actions `cycle_history_next/prev`, `cycle_wrap`), `cache_picker` (`num_pickers` default 1,
  `limit_entries` default 1000, `ignore_empty_prompt`) сохраняет результаты и мультивыбор
  между picker'ами; в docs явно о цене: слишком много закэшированных пикеров/записей —
  slowdown и рост RAM. *Переносить*: serve (у Lusty уже есть `F`-журнал и restore query) —
  добавить кэш сессии с лимитами; TUI — history-циклы. *Сложность*: S/M.
  Источник: https://github.com/nvim-telescope/telescope.nvim/blob/master/doc/telescope.txt
  (секции `defaults.history`, `defaults.cache_picker`).

### Расширения/форки (второй проход)

- **fzf-native: fzf-синтаксис запроса + спаны + кэш паттерна** — C-порт алгоритма fzf
  (README: «only covers the algorithm»), расширение подменяет `file_sorter` и
  `generic_sorter` (`override_file_sorter`/`override_generic_sorter` в setup).
  Поддержан fzf-синтаксис: `sbtrkt` fuzzy, `'wild` — exact, `^music` — prefix,
  `.mp3$` — suffix, `!fire` / `!^music` / `!.mp3$` — инверсии, `|` — OR
  (`^core go$ | rb$ | py$`), `fuzzy = false` — только exact, `case_mode`:
  `smart_case` (default) / `ignore_case` / `respect_case`. В Lua-сортере:
  `scoring_function` возвращает `1 / fzf.get_score`, а `score == 0` → `-1`
  (не матч); распарсенный паттерн кэшируется по строке промпта
  (`state.prompt_cache[prompt]`) — инкрементальная перефильтровка не парсит
  заново; `highlighter` возвращает позиции `fzf.get_pos` (спаны для подсветки);
  при хвостовых `|`, `!`, `\` сбрасывается `_discard_state.filtered` — оператор
  можно дописывать «на лету». Health-check проверяет фиксированные очки
  (`fzf.get_score("src/fzf", "fzf") == 80`, `("fasdzasdf") == 54`) — де-факто
  golden-векторы. README-дисклеймер: возможны отличия от fzf, `normalize` не
  реализован, регистр unicode не работает.
  *Переносить*: core/lusty-fuzzy — операторы `'`, `^`, `$`, `!`, `|` как parse-слой
  над scorer'ом, спаны уже есть, RU→EN сохранить; serve — операторы в строке `Q`;
  TUI/GUI — подсветка спанов; golden-векторы — дополнить эталонами fzf-скорера.
  *Сложность*: L (полный синтаксис с OR/инверсиями), M — для подмножества
  (`'`, `^`, `$`, `!`). Источник: https://github.com/nvim-telescope/telescope-fzf-native.nvim/blob/master/README.md
  и https://github.com/nvim-telescope/telescope-fzf-native.nvim/blob/master/lua/telescope/_extensions/fzf.lua

- **Ties в fzf-native — уточнение** — в README и в `fzf.lua` отдельной обработки
  равенства скоров нет: сортер возвращает `1/score`, а равные значения разрешает
  **ядровый** `tiebreak` (см. механику 9; дефолт — по длине `ordinal`).
  То есть «ties» — это точка расширения ядра telescope, а не особенность
  fzf-native. Для Lusty: если добавлять операторы, политику tiebreak держать
  в core и не прятать в scorer. *Переносить*: core. *Сложность*: S (политика уже
  описана), +M если делать конфигурируемой. Источник:
  https://github.com/nvim-telescope/telescope-fzf-native.nvim/blob/master/lua/telescope/_extensions/fzf.lua
  + https://github.com/nvim-telescope/telescope.nvim/blob/master/doc/telescope.txt
  (`defaults.tiebreak`).

- **frecency (Mozilla-алгоритм, v2 — exponential decay)** — расширение ранжирует
  файлы по «частоте × свежести» (README: implementation of Mozilla Frecency, как в
  адресной строке Firefox). Версия 2.0.0 сменила алгоритм: v1 «count × static
  recency» → v2 «exponential decay» (Rankings сдвигаются, данные мигрируют
  `file_frecency.bin` → `file_frecency_v2.bin`, v1-файл сохраняется для отката).
  Workspace-фильтры: тег в запросе `:workspace_tag:` (по умолчанию `CWD`, плюс
  автоматический `LSP`, если буфер привязан к LSP) сужает выдачу до директорий тега
  и **дополняет** индекс рекурсивным листингом (`fd`/`rg`, при отсутствии — Lua);
  неиндексированные файлы получают score 0 и идут ниже frecent. `<Tab>` после первого
  `:` — омни-комплит тегов (`enable_prompt_mappings`). Очки в списке по умолчанию
  не показываются.
  *Переносить*: core — заменить «счётчик открытий» на decay-формулу и учесть теги
  (у Lusty уже есть `F`-журнал и порядок по depth/frecency); serve — теги как часть
  `Q`/`F`-обмена; GUI — та же формула. *Сложность*: M.
  Источник: https://github.com/nvim-telescope/telescope-frecency.nvim/blob/master/README.md

- **file-browser: файловые мутации поверх пикера** — единый finder с двумя режимами
  (`file_browser` / `folder_browser`, переключение `toggle_browser`), действия
  `create`, `create_from_prompt`, `rename`, `move`, `copy`, `remove` — все работают
  в т.ч. на multi-selection; `<bs>/` на пустом промпте — в родительскую папку;
  `toggle_hidden`, `toggle_all`; опционально git-статусы в списке, `hijack_netrw`,
  `depth`/`auto_depth`, `respect_gitignore` при наличии `fd`; multi-selection
  переживает переключение браузеров в рамках сессии. Явное предупреждение README:
  batch-rename/move с взаимозависимыми путями не разрешаются (часть операций упадёт).
  *Переносить*: TUI — операции над выделением (batch), «backspace → parent»;
  core — FS-операции отдельно от UI; GUI — контекстное меню;
  serve — действия, меняющие ФС, делать только по явной команде хоста.
  *Сложность*: L (полный набор мутаций + LSP-интеграция), M — подмножество
  (create/rename/remove). Источник: https://github.com/nvim-telescope/telescope-file-browser.nvim/blob/master/README.md

- **project: список проектов с CRUD и «открыть файл в проекте»** — пикер `project`
  с действиями `d`/`r`/`c` (delete/rename/create), `s` — grep внутри проекта,
  `b` — file_browser, `w` — cd без открытия, `R` — недавний файл, `f`/`<CR>` — файлы
  проекта, `o` — смена cd-scope. Настройки: `base_dirs` (в т.ч. с `max_depth`),
  `ignore_missing_dirs`, `hidden_files`, `order_by` = `asc|desc|recent`,
  `search_by` = `title|path|[both]`, `on_project_selected`, `cd_scope` = tab/window/global.
  *Переносить*: core — модель «проект/корень + история» (сейчас Lusty оперирует root'ом
  CLI); TUI — picker проектов; serve — `R` request «recent in project»; GUI — sidebar.
  *Сложность*: M. Источник: https://github.com/nvim-telescope/telescope-project.nvim/blob/master/README.md

- **ui-select: подмена `vim.ui.select` + спецификация кастомных дисплеев** —
  расширение направляет `vim.ui.select` в telescope (пример README: `code_action()`);
  принимает тему (`get_dropdown{...}`) и `specific_opts` — для каждого «kind»
  можно задать `make_indexed`, `make_displayer`, `make_display`, `make_ordinal`
  либо отключить встроенный кастомный дисплей (`codeactions = false`).
  *Переносить*: serve — источник «выбор из списка, инициированный хостом» (line-протокол
  уже умеет Q; нужен обратный канал выбора); GUI — generic dialog-компонент.
  *Сложность*: S/M. Источник: https://github.com/nvim-telescope/telescope-ui-select.nvim/blob/master/README.md

- **undo: undo-tree как источник, diff-preview, yank/restore-actions** — пикер
  показывает undo-дерево текстовым деревом, fuzzy-запрос матчится по добавлениям
  и удалениям состояний; preview — дифф состояния с контекстом (`vim_diff_opts.ctxlen`
  = `scrolloff`, `delta`, `side_by_side`, `use_custom_command`); действия
  `yank_additions`/`yank_deletions`/`restore`/`yank_larger`; `entry_format`
  `"state #$ID, $STAT, $TIME"`, `saved_only`; рекомендованный layout в README:
  `side_by_side=true`, `layout_strategy="vertical"`, `preview_height=0.8`.
  *Переносить*: core — источник «история/undo» поверх общего pipeline; TUI — diff-preview
  (у Lusty preview уже умеет git diff); GUI — side-by-side дифф. *Сложность*: M.
  Источник: https://github.com/debugloop/telescope-undo.nvim/blob/master/README.md

- **live-grep-args: промпт = argv для ripgrep + `to_fuzzy_refine`** — в промпт
  передаются настоящие аргументы `rg`: `--no-ignore foo`, `"foo bar" bazdir`,
  `"foo" --iglob **/bar/**`, авто-кавычки (`auto_quoting = true`: если промпт не
  начинается с `'`, `"` или `-`, он считается одним аргументом); actions
  `quote_prompt()` / `quote_prompt{postfix=" --iglob "}` / `quote_prompt{postfix=" -t"}`,
  `to_fuzzy_refine` (заморозить текущий список и искать fuzzy только по нему —
  как `<C-space>` в `live_grep`); shortcuts `grep_word_under_cursor`,
  `grep_visual_selection` (+ варианты для current buffer, postfix `-F`, quote/trim).
  *Переносить*: serve — расширение `Q` до «args-режима» (rg-опции в запросе);
  core — проброс `additional_args` в grep-источник; TUI — refine-режим по замороженному
  списку (аналог prefilter). *Сложность*: M.
  Источник: https://github.com/nvim-telescope/telescope-live-grep-args.nvim/blob/master/README.md

## Best-of конфигурации

- **Layout под задачу**: `find_files({ layout_strategy='vertical', layout_config={width=0.5} })` —
  точечный оверрайд на один вызов (README telescope, «Layout»); dropdown-тема:
  `find_files(themes.get_dropdown{ previewer = false })` (README, «BUILTIN»).
- **fzf-native setup** (README extension): `extensions.fzf = { fuzzy = true,
  override_generic_sorter = true, override_file_sorter = true, case_mode = "smart_case" }`
  + `load_extension('fzf')`.
- **frecency**: `:Telescope frecency workspace=CWD path_display={"shorten"} theme=ivy` —
  комбинируется с любыми telescope-опциями (README frecency, «Usage»).
- **file-browser**: `theme = "ivy"`, `hijack_netrw = true`; запуск из каталога буфера —
  `:Telescope file_browser path=%:p:h select_buffer=true` (README file-browser).
- **undo**: `side_by_side = true`, `layout_strategy = "vertical"`, `preview_height = 0.8` —
  рекомендация автора для широкого превью (README undo, «Configuration»).
- **live_grep_args**: `<C-k>` = `quote_prompt()`, `<C-i>` = `quote_prompt{postfix=' --iglob '}`,
  `<C-space>` = `to_fuzzy_refine` (README live-grep-args, «Configuration»).
- **ui-select**: `["ui-select"] = { themes.get_dropdown{...}, specific_opts = { codeactions = false } }`
  (README ui-select, «Telescope Setup and Configuration»).
- **project**: `base_dirs = { '~/dev/src', {path='~/dev/src3', max_depth=4} }`,
  `order_by = "recent"`, `search_by = {"title","path"}`, `theme = "dropdown"` (README project).

## Антипаттерны / что не тащить

- **`get_levenshtein_sorter`** — в README прямая пометка: «Using Levenshtein distance
  algorithm (don't use :D)»; для 600k записей не вариант (источник: README telescope,
  «Sorters»).
- **Глобальный `file_ignore_patterns`** — ломает `lsp_`-пикеры (node_modules исчезнет
  и из LSP-выдачи); docs сами советуют для файлов/грепа полагаться на gitignore-семантику
  `fd`/`rg`, а не на regex-таблицу (источник: doc/telescope.txt, `defaults.file_ignore_patterns`).
- **`cache_picker` без лимитов** — docs: кэш сохраняет результаты и мультивыбор, но
  «may result in slowdown or increased RAM occupation»; дефолты num_pickers=1,
  limit_entries=1000 (источник: doc/telescope.txt, `defaults.cache_picker`).
- **Preview без лимитов** — фризы на больших/бинарных файлах: в дефолтах
  `filesize_limit=25 MB`, `highlight_limit=1 MB`, `timeout=250 ms`; отключать их —
  сознательный риск (источник: doc/telescope.txt, `defaults.preview`).
- **Терминальные previewers (`cat`/`vimgrep`/`qflist`) как основной путь** — в
  telescope дефолт сместился на `vim_buffer_*`: README объясняет, что буферные
  читают файл асинхронно и не делают `bufload`, «otherwise the speed of the previewer
  would slow down considerably»; запуск `cat`/`bat` на каждое движение курсора — цена
  процесс-на-запрос (у Lusty это уже смягчено latest-wins; не регрессировать).
- **Считать fzf-native паритетом с fzf** — README: «there might be differences in
  matching. I don't guarantee completeness», `normalize` и unicode-case не реализованы;
  при переносе операторов ожидать расхождений и закрывать их golden-векторами.
- **`hijack_netrw = true` по умолчанию** — глобально перехватывает файловый
  менеджер Neovim; в Lusty-core не тащить (это решение хоста/пользователя).
- **Batch-операции с взаимозависимыми путями** (file-browser) — README явно
  предупреждает, что такие move/rename не разрешаются; для Lusty либо запрещать
  конфликтующие пути заранее, либо делать операции последовательно с проверкой.
- **Формулировка «layout_strategy = ivy»** — неверна для telescope: `ivy` — тема,
  стратегия — `bottom_pane` (см. механику 2); копировать конфиги «как в блогах»
  без сверки с docs не стоит.

## Открытые вопросы

- Есть ли в ядре telescope (без fzf-native) операторы `'`, `^`, `$`, `!`, `|`?
  В `doc/telescope.txt` они не описаны; заявлены в README fzf-native как поддержанный
  fzf-синтаксис. Поведение дефолтного fzy-сортера с этими токенами не проверялось.
- Как дефолтный fzy-lua сортер трактует регистр, привязку к началу строки и
  не-ASCII (RU-раскладка)? `lua/telescope/sorters.lua` не открывался (лимит источников) —
  у Lusty RU→EN уже есть, паритет не подтверждён.
- Какой сортер в ядре реально активен по умолчанию: docs объявляют
  `get_fzy_sorter` для `file_sorter` и `generic_sorter`, README-таблица —
  `get_fuzzy_file`/`get_generic_fuzzy_sorter`. Противоречие README↔docs не
  разрешено (нужен `lua/telescope/sorters.lua`, вне лимита источников).
- Нужна ли отдельная конфигурируемая политика ties (`tiebreak`) в Lusty, или
  канонического индекса достаточно? В fzf-native обработки ties нет; сравнение
  `1/score` может давать равные значения — вопрос к продовым данным.
- Мультивыбор за пределами nvim: telescope оперирует им внутри процесса; как
  кодировать набор в `serve`-протоколе Lusty (batch open/remove) — ответа в
  первоисточниках нет.
- Производительность на больших репо: единственный измеренный ориентир в
  источниках — self-reported benchmark fzf-native на 240 201 файловой строке
  (README расширения, картинки wiki; вендорское число без аблаций);
  отдельного гайда «telescope на 600k» не открывалось (вне лимита источников).
- `preview_cutoff` имеет смысл в TUI (нет места — прячем preview); переносить ли
  авто-отключение в ресайзящийся Tauri-GUI — вопрос дизайна Lusty, источника нет.

## Источники (URL + дата)

Все страницы открыты 2026-10-14 (файлы скачаны из ветки `master`, если не указано иное).

1. https://github.com/nvim-telescope/telescope.nvim/blob/master/doc/telescope.txt (ядро: defaults, layout, mappings, actions, previewers, builtin)
2. https://github.com/nvim-telescope/telescope.nvim/blob/master/README.md (обзор, pickers, default mappings, previewers/sorters/layout)
3. https://github.com/nvim-telescope/telescope-fzf-native.nvim/blob/master/README.md (синтаксис, API, benchmark, дисклеймер)
4. https://github.com/nvim-telescope/telescope-fzf-native.nvim/blob/master/lua/telescope/_extensions/fzf.lua (1/score, кэш паттерна, highlighter, health-scores)
5. https://github.com/nvim-telescope/telescope-frecency.nvim/blob/master/README.md (Mozilla-алгоритм, v2 decay, workspace-теги)
6. https://github.com/nvim-telescope/telescope-file-browser.nvim/blob/master/README.md (мутации, multi-select, опции)
7. https://github.com/nvim-telescope/telescope-project.nvim/blob/master/README.md (CRUD проектов, base_dirs, order_by)
8. https://github.com/nvim-telescope/telescope-ui-select.nvim/blob/master/README.md (vim.ui.select, specific_opts)
9. https://github.com/debugloop/telescope-undo.nvim/blob/master/README.md (undo-tree, delta/side_by_side, yank/restore)
10. https://github.com/nvim-telescope/telescope-live-grep-args.nvim/blob/master/README.md (rg-args в промпте, auto_quoting, to_fuzzy_refine)
