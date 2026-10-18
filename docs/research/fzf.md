# fzf: переносимые механики

> Статус: завершён 2026-10-14 (артефакт-первым, инкрементальная запись).
> Зона воркера: junegunn/fzf — man page fzf(1), WIKI, `--layout`/`--height`/`--tmux`,
> preview-window, `--bind`/actions, синтаксис запроса, `--scheme` (по факту с 0.33.0,
> не 0.48+) и `--walker` (0.47–0.48),
> ранжирование (`--tiebreak`, `--no-sort`, `--nth`), `--multi`/маркеры,
> визуал (`--info`, `--pointer`, роли цветов), экосистема (fzf.vim, fzf-git.sh,
> forgit, fzf-tab, zsh4humans), стриминг/производительность.
> Версия fzf на момент отчёта: см. «Источники» и явные пометки версий у механик.

## Карта возможностей

Версии: upstream `master` на дату отчёта — **0.74.5** (верх CHANGELOG); локально
установлен 0.74.4 — использован только для сверки версии, не для сравнения man'ов.
Ссылки ведут на первоисточники: man page, README, CHANGELOG, вики и README
экосистемных проектов.

- **Поиск/запрос**: extended-search по умолчанию (`'`, `^`, `$`, `!`, `|`,
  несколько термов через пробел), smart-case, `--exact`, `--nth/--with-nth/--accept-nth`,
  `--scheme=default|path|history`, `--disabled`.
- **Ранжирование**: `--tiebreak=length,chunk,pathname,begin,end,index`, `--no-sort`,
  `--tac`, `--tail`, `--track`/`--id-nth` (стабильность курсора при `reload`).
- **Компоновка**: `--layout=default|reverse|reverse-list`, `--height` (в т.ч.
  адаптивный `~`), `--min-height`, `--margin/--padding/--border`, `--tmux`/`--popup`.
- **Preview**: `--preview`, `--preview-window=UP|DOWN|LEFT|RIGHT|NEXT:...:hidden|wrap|cycle|follow`,
  placeholders `{} {1} {q} {n} {+} {*} {f}`, позиционные env-переменные.
- **Действия/события**: `--bind` + события (`start`, `load`, `change`, `focus`,
  `result`, `result-final`, ...) + большой реестр действий (`execute`, `reload`,
  `transform*`, `change-preview-window*`, `toggle-sort`, `accept*`, `become`, ...).
- **Мультивыбор**: `--multi`, маркеры, `toggle-all/select-all/clear-multi/change-multi(N)`.
- **Стиль**: `--info=default|right|hidden|inline[:PREFIX]|inline-right[:PREFIX]`,
  `--info-command`, `--pointer`, `--marker`, роли цветов, `--border-label`,
  `--header/--footer`, `--wrap-sign`.
- **Источники/интеграция**: встроенный walker (`--walker*`, с 0.47–0.48),
  shell-интеграция (`fzf --bash|--zsh|--fish`), history-файл, `--listen`
  (HTTP/Unix-socket), экосистема: fzf.vim, fzf-git.sh, forgit, fzf-tab, zsh4humans.

## Топ-механики для Lusty

- **Extended-search синтаксис** — режим включён по умолчанию; термы через пробел
  (AND), `'foo` — exact-match (не fuzzy), `^foo`/`foo$` — якоря начала/конца
  строки (тоже exact), `'foo'` — exact-boundary (underscore считается границей
  слова, такие совпадения ранжируются ниже), `!foo` — исключение (exact по
  умолчанию), `|` — OR (`^core go$ | rb$ | py$`); `\ ` экранирует пробел;
  `--exact` делает exact-режим дефолтным, а `'` в нём — escape обратно во fuzzy.
  Что переносить: парсер в `lusty-fuzzy` (ядро; в т.ч. подсветка спанов для
  exact/anchored термов), подсказки операторов в TUI, прокидывание строки запроса
  через `serve` без потерь, поле поиска в GUI. Сложность **M**.
  Источник: [man fzf(1), EXTENDED SEARCH](https://github.com/junegunn/fzf/blob/master/man/man1/fzf.1)
- **Ранжирование: tiebreak + no-sort + scheme** — `--tiebreak` — список
  критериев (`length`, `chunk`, `pathname`, `begin`, `end`, `index`;
  `pathname` добавлен в 0.59.0); `index` допускается только последним и неявно добавляется, дефолт
  `length,index`; при `end` fzf сканирует строки с конца. `--no-sort` отдаёт
  результаты в порядке входного потока (для `--tac`, стримов, истории).
  `--scheme=path` даёт бонус только символам после разделителя пути и (с 0.59.0)
  ставит `--tiebreak=pathname,length`; `--scheme=history` отключает бонусы и
  ставит `--tiebreak=index`. Что переносить: расширить стратегии в
  `crates/lusty-fuzzy`/core rank (`pathname`/`begin`/`end`/`length`), тумблер
  «sort/no-sort» и выбор схемы в TUI/GUI; в `serve` — опциональные флаги.
  Сложность **M**. Источник:
  [man, SEARCH](https://github.com/junegunn/fzf/blob/master/man/man1/fzf.1);
  версии: [CHANGELOG 0.33.0 (scheme)](https://github.com/junegunn/fzf/blob/master/CHANGELOG.md),
  [CHANGELOG 0.59.0 (pathname)](https://github.com/junegunn/fzf/blob/master/CHANGELOG.md)
- **Layout-режимы и адаптивная высота** — `--layout=default` (список снизу,
  промпт сверху), `reverse` (список сверху), `reverse-list` (список сверху,
  промпт снизу); `--height` задаёт высоту панели вместо full-screen, поддерживает
  проценты, `~`-адаптивность и отрицательные значения; `--min-height`,
  `--margin/--padding/--border` управляют геометрией. Что переносить: панельная
  вёрстка вместо fullscreen (список + preview как панели), адаптивная высота
  для TUI; в GUI — те же три стратегии как CSS-раскладки.
  Сложность **S**. Источник:
  [man, LAYOUT](https://github.com/junegunn/fzf/blob/master/man/man1/fzf.1)
- **`--tmux`/`--popup`** — запуск fzf во всплывающей панели tmux/Zellij
  (по умолчанию `center,50%`), требует tmux 3.3+ или Zellij 0.44+; опция
  игнорируется вне tmux/Zellij. С 0.71.0 `--popup` — новое имя, `--tmux` —
  алиас; с 0.74.0 на tmux 3.7+ это не модальная floating-панель (можно
  переключаться между окнами, двигать мышью), native border используется по
  умолчанию. Что переносить: сам механизм — не в TUI Lusty (у нас нет tmux-слоя),
  но идея «окно пикера поверх хоста заданной геометрии» релевантна для
  Tauri-GUI (окно без рамки, позиция/размер в %); в `serve` — не тащить.
  Сложность **L** (интеграция с tmux), идея для GUI — **S**. Источник:
  [man, DISPLAY MODE](https://github.com/junegunn/fzf/blob/master/man/man1/fzf.1);
  версии: [CHANGELOG 0.71.0/0.74.0](https://github.com/junegunn/fzf/blob/master/CHANGELOG.md)
- **Preview-window: полная семантика окна** — позиции `up|down|left|right|next`
  (next — со стороны списка, зависит от layout); `:hidden` — окно скрыто, команда
  выполняется после `toggle-preview`; размер в `%`; `wrap`, `wrap-word`, `follow`
  (автоскролл вниз, как `tail -f`), `cycle` (циклический скролл), `noinfo`
  (скрыть offset), border-стили (`border-line`, `border-rounded`, ...),
  `+SCROLL[OFFSETS][/DENOM]` (стартовый скролл, в т.ч. по полю строки) и
  `~HEADER_LINES` (закреплённый заголовок); размер `0` — окно невидимо, но
  команда выполняется в фоне. Placeholders: `{}` (текущая строка), `{N}`
  (поле), `{q}` (запрос), `{n}` (индекс), `{+}` (выбранные), `{*}`
  (все совпадения), `{f}` (временный файл против ARG_MAX), флаги `r` (raw,
  без квотирования) и `s` (сохранять пробелы); env `FZF_PREVIEW_LINES/COLUMNS/TOP/LEFT`.
  С 0.24.0 рендерится частичный вывод до завершения команды (CSI 2J — для
  «вечно обновляющегося» превью). Что переносить: почти всё — в модуль preview
  Lusty (позиция/размер/wrap/cycle/hidden/latest-wins), в `serve` (параметры
  окна), в GUI (панель с фиксированным заголовком, follow для логов); `{+}` и
  `{f}` особенно полезны для batch-действий. Сложность **M**.
  Источник: [man, PREVIEW WINDOW](https://github.com/junegunn/fzf/blob/master/man/man1/fzf.1)
- **Реестр действий `--bind` (+ события)** — привязка клавиш/событий к действиям,
  композиция через `+` (`execute(...)+abort`); события: `start`, `load`, `change`,
  `focus`, `result`, `result-final`, `resize`, клики/скролл и т.д.;
  действия: `execute/execute-silent` (запуск и возврат), `become` (замена
  процесса fzf командой), `reload/reload-sync` (+ `transform`-версии), `accept`,
  `accept-or-print-query`, `print-query`, `toggle-sort`, `change-preview[-window]`,
  `change-multi(...)`, `toggle-preview`, `toggle-track`, `enable-search/disable-search`
  и десятки навигационных. Что переносить: реестр действий — основа будущего
  keymap TUI Lusty; подмножество `execute/reload/transform/accept/toggle-sort/
  change-preview-window` — в line-протокол `serve` (как команды), в GUI — как
  команды/шорткаты; `become` полезен в standalone, но в `serve` его нет.
  Сложность **M** (ядро реестра), **L** (паритет по количеству действий).
  Источник: [man, KEY/EVENT BINDINGS + AVAILABLE ACTIONS](https://github.com/junegunn/fzf/blob/master/man/man1/fzf.1)
- **Мультивыбор и маркеры** — `--multi` включает выбор нескольких строк;
  действия `toggle`, `toggle-all`, `select-all`, `deselect`, `deselect-all`,
  `clear-multi`, `change-multi(N)` (лимит или динамическое вкл/выкл), маркер
  рисуется `--marker`; в preview/execute placeholders `{+}`/`{+f}` отдают
  выбранное списком, `{+n}` — индексы; `--multi --bind 'enter:become(vim {+})'`
  — корректная передача выбранного с пробелами. Что переносить: TUI-модель
  мультивыбора (маркер, toggle-all, счётчик), `serve` — batch-accept,
  GUI — multi-select и batch-действия. Сложность **S/M**.
  Источник: [man, AVAILABLE ACTIONS](https://github.com/junegunn/fzf/blob/master/man/man1/fzf.1),
  [README, become(...)](https://github.com/junegunn/fzf/blob/master/README.md)
- **Визуальные роли и info-строка** — `--info=default|right|hidden|inline[:PREFIX]|inline-right[:PREFIX]`;
  `--info-command=CMD` — внешний генератор info-строки (запускается синхронно и
  блокирует UI; `$FZF_INFO` — исходный текст); `--pointer`, `--marker`;
  цветовые роли (`fg`, `bg`, `hl`/`hl+`, `query`, `prompt`, `pointer`, `marker`,
  `info`, `header`, `border`, `label`, `preview-*`, `disabled`, `nth` (только
  атрибуты), `spinner`, `gutter`); `--border-label`, `--header/--footer`.
  Что переносить: тема TUI Lusty (роли вместо хардкода), GUI-тема из
  `theme.toml`; info-строка `matched/total` + состояние загрузки; счётчик
  выбранного при `--multi`. Сложность **S**.
  Источник: [man, GLOBAL STYLE](https://github.com/junegunn/fzf/blob/master/man/man1/fzf.1)
- **Стриминг, async и цена опций** — fzf читает вход потоково и перефильтровывает
  на каждый ввод; `reload`/`transform` асинхронны, `--sync` переключает на
  синхронную загрузку, `--tail N` ограничивает память на бесконечном стриме
  (`tail -f *.log | fzf --tail 100000 --tac --no-sort --exact`); README прямо
  перечисляет тормозящие опции: `--ansi` замедляет начальный скан, `--nth` и
  `--with-nth` — токенизация каждой строки, regexp-delimiter дешевле заменить
  plain строкой. Что переносить: потоковый листинг/ре-ранк в `lusty-core`,
  окна и лимиты памяти (600k записей), «latest-wins» для async preview;
  в GUI — те же лимиты на стороне Rust, не в JS. Сложность **M**.
  Источник: [man, SEARCH](https://github.com/junegunn/fzf/blob/master/man/man1/fzf.1),
  [README, Performance](https://github.com/junegunn/fzf/blob/master/README.md)
- **Встроенный walker и его опции** — с 0.47.0 fzf заменил внешний `find` на
  встроенный параллельный walker (библиотека `charlievieth/fastwalk`), с 0.48.0
  появились опции: `--walker=file,dir,follow,hidden` (дефолт `file,follow,hidden`),
  `--walker-root=DIR`, `--walker-skip=DIRS` (дефолт `.git,node_modules`).
  Walker используется только standalone-бинарником, когда `$FZF_DEFAULT_COMMAND`
  не задан и вход — TTY; при stdin/walker игнорируется. Побочные эффекты
  (breaking в 0.47): hidden-файлы листятся, hidden-каталоги — нет; порядок
  обхода при параллельном обходе нестабилен. Что переносить: абстракция
  source/обхода в `lusty-core` (наборы skip-паттернов, follow/hidden-флаги,
  параллелизм), в GUI — те же настройки в UI. Сложность **M**.
  Источник: [man, DIRECTORY TRAVERSAL](https://github.com/junegunn/fzf/blob/master/man/man1/fzf.1),
  [CHANGELOG 0.47.0/0.48.0](https://github.com/junegunn/fzf/blob/master/CHANGELOG.md)
- **История запросов и frecency-совместимость** — `--history=FILE` загружает
  историю запросов и сохраняет её при выходе, `--history-size=N` (дефолт 1000,
  файл усекается), CTRL-N/CTRL-P автоматически перебиндиваются на
  `next-history/prev-history`; `--scheme=history` + `--tiebreak=index` дают
  хронологический порядок без сдвига бонусами скоринга. Что переносить:
  история запросов в TUI/serve, совместимая с идеей frecency в Lusty-кэше;
  в GUI — та же модель. Сложность **S**.
  Источник: [man, HISTORY](https://github.com/junegunn/fzf/blob/master/man/man1/fzf.1)
- **`--listen`: внешнее управление пикером** — fzf поднимает HTTP-сервер
  на `[ADDR:]PORT` или Unix-сокете (`--listen[=SOCKET]`): POST выполняет
  действия, GET отдаёт состояние (JSON, экспериментально; `limit`/`offset`);
  для не-localhost обязателен `FZF_API_KEY` в заголовке `x-api-key`, а
  `--listen-unsafe` разрешает удалённое исполнение команд.
  Это архитектурный аналог `lusty serve`: сравнить протоколы (HTTP+JSON status
  payload vs строгий line-протокол; безопасность: bind только на loopback,
  явный opt-in для исполнения). Что переносить: идею машиночитаемого статуса
  (`progress`, query, matches) в `serve`/GUI-IPC; сам HTTP-сервер не тащить.
  Сложность **M**. Источник:
  [man, SCRIPTING](https://github.com/junegunn/fzf/blob/master/man/man1/fzf.1)

### Экосистемные механики

- **fzf.vim: `fzf#wrap` + sink-контракт** — каждая команда плагина внутри
  вызывает `fzf#wrap`, который применяет глобальные опции; конфиг — словарь
  `g:fzf_vim`; `preview_window` — массив вида `['right,50%', 'ctrl-/']`, где
  первый элемент — аргументы `--preview-window`, второй — клавиша toggle,
  поддерживается условная раскладка `['hidden,right,50%,<70(up,40%)', 'ctrl-/']`
  (спрятать, показать справа 50%, а при терминале уже 70 колонок — сверху 40%);
  команды можно переопределять через `fzf#vim#files(...)` и т.п.; кастомная
  fuzzy-completion — `fzf#vim#complete` с `source`/`options`/`prefix`/`reducer`.
  С 0.74.x Vim-плагин больше не блокирует редактор: `fzf#run` асинхронно
  возвращает пустой список, результат отдают `sink`/`sinklist`/`exit`.
  Что переносить: sink-абстракцию в `serve` (accept → действие хост-редактора,
  batch — список), toggle preview как команду протокола, условную раскладку
  preview по ширине окна — в TUI Lusty и GUI. Сложность **M**.
  Источник: [fzf.vim README](https://github.com/junegunn/fzf.vim),
  версии: [CHANGELOG 0.74.4 (Vim plugin)](https://github.com/junegunn/fzf/blob/master/CHANGELOG.md)
- **fzf-git.sh: контекстные git-источники под одним префиксом** — `CTRL-G` +
  буква: файлы/ветки/теги/remote/хеши коммитов/stash/reflog/worktree/
  `for-each-ref`; files-режим работает и вне репозитория (fallback на файловый
  поиск fzf). Выбранные объекты «печатаются» в исходную панель, а tmux-вариант
  биндит те же команды на `PREFIX g` и работает в любой панели; внутри fzf
  TAB — мультивыбор, `CTRL-/` — смена раскладки preview, `CTRL-O` — открыть
  объект в браузере; опции меняются переопределением `_fzf_git_fzf`.
  Что переносить: `source`-абстракцию (файлы/git-объекты) и панель действий
  в GUI, команды источников в `serve` (git-объекты для Neovim); паттерн
  «префикс + одна буква на объект» — в keymap Lusty. Сложность **M**.
  Источник: [fzf-git.sh README](https://github.com/junegunn/fzf-git.sh)
- **forgit: действие-на-команду + per-command опции + цепочка пейджеров** —
  ~28 интерактивных git-команд (add/diff/log/show/stash/blame/checkout/rebase/
  worktree/...), у каждой свои `FORGIT_<CMD>_FZF_OPTS` и `FORGIT_<CMD>_GIT_OPTS`,
  а просмотр идёт через пейджер с фолбэками (`FORGIT_DIFF_PAGER` →
  `git config pager.diff` → `FORGIT_PAGER` → `cat`). Что переносить: реестр
  «действий по источнику» (git add/checkout/log как действия Lusty), per-source
  переопределения опций, единая цепочка preview/pager; в GUI — список действий
  для выбранного элемента. Сложность **S/M**.
  Источник: [forgit README](https://github.com/wfxr/forgit)
- **fzf-tab: fzf как фронтенд completion'а** — Tab перехватывает результат
  compsys и показывает его в fzf: группы (`descriptions` + F1/F2), Ctrl-Space —
  мультивыбор, `/` — continuous completion для глубоких путей, per-context
  zle-zstyle (`:fzf-tab:complete:cd:*` → preview через `eza`), tmux-popup
  через `ftb-tmux-popup`; для скорости цветоцитирования есть бинарный модуль.
  Что переносить: идею «пикер как UI выбора кандидатов» — в serve-режим для
  Neovim и в GUI-дропдаун; per-source preview-настройки. Сложность **M**.
  Источник: [fzf-tab README](https://github.com/Aloxaf/fzf-tab)
- **zsh4humans: сквозной fzf-UX без ручной настройки** — история по `CTRL-R`
  через fzf, инкрементальный Up/Down-поиск по подстроке в уже набранном тексте,
  completions поверх fzf; в README документированы те же операторы запроса
  (`^music .mp3$ sbtrkt !fire`, `|`), что и в fzf. Caveats проекта: не для
  vi-биндингов и «очень мало документации» о своих опциях. Что переносить:
  UX-инвариант «один язык запросов во всех пиках» и предсказуемые дефолты;
  кода — почти ничего. Сложность **S**.
  Источник: [zsh4humans README](https://github.com/romkatv/zsh4humans)
- **Двухфазная фильтрация через `reload` (rg + fzf)** — в ADVANCED.md fzf
  запускается с `--disabled`, а `start:reload`/`change:reload` перезапускают
  `rg --column ... {q}` (поиск целиком у внешней утилиты, `{q}` — текущий
  запрос; `sleep 0.1` — debounce от лишних процессов); `alt-enter` через
  `unbind(change,alt-enter)+enable-search+clear-query` переключает на
  fuzzy-режим fzf; preview `bat'а` скроллится к совпадению
  (`up,60%,border-bottom,+{2}+3/3,~3`), `enter:become(vim {1} +{2})` открывает
  файл на нужной строке. Что переносить: перезагрузку источника по изменению
  запроса с debounce в `lusty-core`/`serve`, переключение «точный поиск ↔ fuzzy»
  в TUI/GUI, preview с якорем на совпавшую строку. Сложность **M**.
  Источник: [ADVANCED.md, interactive Ripgrep launcher](https://github.com/junegunn/fzf/blob/master/ADVANCED.md)

## Best-of конфигурации

Примеры из первоисточников (в скобках — чему учит):

1. **Popup с graceful degradation** — README рекомендует в `$FZF_DEFAULT_OPTS`:
   `--height 40% --popup bottom,40% --layout reverse --border top` — «open in a
   popup if on tmux or Zellij, otherwise use --height mode».
   ([README](https://github.com/junegunn/fzf/blob/master/README.md))
2. **Приоритет `--tmux` над `--height`** — `fzf --height 70% --tmux 70%`:
   `--tmux` указан позже и выигрывает внутри tmux, вне tmux молча игнорируется
   и остаётся `--height`. ([ADVANCED.md](https://github.com/junegunn/fzf/blob/master/ADVANCED.md))
3. **Боевой набор fzf-git.sh** — переопределяемая `_fzf_git_fzf()`:
   `--height 50% --tmux 90%,70% --layout reverse --multi --min-height 20+
   --no-separator --header-border horizontal --border-label-pos 2
   --color 'label:blue'`. ([fzf-git.sh README](https://github.com/junegunn/fzf-git.sh))
4. **Условный preview в fzf.vim** —
   `let g:fzf_vim.preview_window = ['hidden,right,50%,<70(up,40%)', 'ctrl-/']`:
   preview спрятан, открывается по `ctrl-/`; справа 50%, а на узком терминале
   (<70 колонок) — сверху 40%. Дефолт — `['right,50%', 'ctrl-/']`.
   ([fzf.vim README](https://github.com/junegunn/fzf.vim))
5. **Типовая конфигурация fzf-tab** — `format '[%d]'` для групп, `list-colors`
   из `LS_COLORS`, `menu no` (чтобы fzf-tab поймал однозначный префикс),
   preview для `cd` через `eza`, свои флаги
   `zstyle ':fzf-tab:*' fzf-flags --color=fg:1,fg+:2 --bind=tab:accept`,
   `switch-group '<' '>'`. ([fzf-tab README](https://github.com/Aloxaf/fzf-tab))
6. **Стрим логов с ограничением памяти** —
   `tail -f *.log | fzf --tail 100000 --tac --no-sort --exact`.
   ([man, SEARCH](https://github.com/junegunn/fzf/blob/master/man/man1/fzf.1))
7. **Interactive ripgrep launcher** — `--disabled` + `start:reload`/`change:reload`
   с `{q}` и `sleep 0.1`, preview `bat` c `+{2}+3/3,~3`, `enter:become(vim {1} +{2})`.
   ([ADVANCED.md](https://github.com/junegunn/fzf/blob/master/ADVANCED.md))
8. **Классический конвейер-меню** — `ps -ef | fzf | awk '{print $2}' | xargs kill`:
   fzf как чистый Unix-фильтр (тоже LESSON: встраиваемость важнее фреймворка).
   ([ADVANCED.md](https://github.com/junegunn/fzf/blob/master/ADVANCED.md))

## Антипаттерны / что не тащить

- **Опции, замедляющие скан, в дефолтах.** README прямо предупреждает: `--ansi`
  замедляет начальный скан, `--nth` и `--with-nth` требуют токенизации каждой
  строки (regexp-delimiter дешевле заменить plain). man добавляет: `--with-nth`
  хранит оригинальные строки в памяти, «only use --with-nth when you actually
  need field transformation». В Lusty на 600k записей — не включать по умолчанию.
  ([README](https://github.com/junegunn/fzf/blob/master/README.md),
  [man](https://github.com/junegunn/fzf/blob/master/man/man1/fzf.1))
- **Внешние команды в `--info-command`.** Команда запускается синхронно и
  блокирует UI; в Lusty статус-строка не должна ждать внешний процесс
  (делать async с отбрасыванием устаревших ответов).
  ([man](https://github.com/junegunn/fzf/blob/master/man/man1/fzf.1))
- **Расчёт на `--tmux` вне tmux.** Опция молча игнорируется; для предсказуемости
  комбинировать с `--height` (см. Best-of #2). Для Lusty — не завязывать GUI/TUI
  на наличие внешнего мультиплексора.
  ([ADVANCED.md](https://github.com/junegunn/fzf/blob/master/ADVANCED.md))
- **Адаптивная высота на стриме.** man перечисляет ограничения `--height=~...`:
  «fzf cannot start until the input ends or exceeds the height, so a stream that
  stays below it will delay the first render»; плюс несовместимость с top/bottom
  margin/padding в процентах и ошибки размера на multi-line items. Для Lusty с
  потоковым обходом большого дерева адаптивную высоту по умолчанию не включать.
  ([man, LAYOUT](https://github.com/junegunn/fzf/blob/master/man/man1/fzf.1))
- **Открытый `--listen`.** Для не-localhost обязателен `FZF_API_KEY`, а
  `--listen-unsafe` разрешает удалённое исполнение команд. `lusty serve` должен
  оставаться stdio/Unix-socket с локальным доступом; сетевой режим — только
  явный opt-in с токеном. ([man, SCRIPTING](https://github.com/junegunn/fzf/blob/master/man/man1/fzf.1))
- **Копирование community-примеров без теста.** Вики-страница examples сама
  несёт дисклеймер: «The examples are maintained by the community and are not
  thoroughly tested». Брать из вики идеи, но проверять у себя.
  ([wiki examples](https://github.com/junegunn/fzf/wiki/examples))
- **Глобальный конфиг, протекающий в потребителя.** fzf-tab по умолчанию не
  следует `FZF_DEFAULT_OPTS`, потому что «some flags break this plugin»
  (Aloxaf/fzf-tab#455). Вывод для Lusty: `serve`/GUI не должны зависеть от
  пользовательского глобального конфига, который может сломать протокол;
  параметры передаются явно. ([fzf-tab README](https://github.com/Aloxaf/fzf-tab))
- **Конфликты клавиш с хостом.** fzf-git.sh предупреждает: `CTRL-B` не сработает,
  если это tmux-prefix; `CTRL-S` заморозит терминал при flow control
  (лечится `stty -ixon`); обход — комбинации `CTRL-G *{key}*`. Для Lusty:
  префиксные биндинги вместо одиночных хоткеев, конфигурируемость, подсказки.
  ([fzf-git.sh README](https://github.com/junegunn/fzf-git.sh))
- **Безопасность shell-обёрток.** В 0.74.5 пришлось чинить `fzf-tmux`
  (eval размера layout через bash arithmetic → инъекция) и zsh-completion
  (FIFO в предсказуемом `/tmp`); вывод: не eval'ить аргументы/env, временные
  файлы — в приватном `mktemp -d`. Релевантно аналогам `serve`-сокетов и wrapper'ов.
  ([CHANGELOG 0.74.5](https://github.com/junegunn/fzf/blob/master/CHANGELOG.md))
- **Безоглядный `follow` по симлинкам.** До 0.71.0 `--walker=follow` мог
  истощить ресурсы на симлинке, ведущем к предку корня (пример из CHANGELOG:
  Wine `z:` → `/`). В Lusty — защита от циклов/ancestor при follow.
  ([CHANGELOG 0.71.0](https://github.com/junegunn/fzf/blob/master/CHANGELOG.md))
- **Grid/multi-column тут нет.** В man/README 0.74.5 список одноколоночный,
  grid-режим отсутствует; механику grid для Lusty брать из отчётов по
  telescope/skim/fzf-lua, а не из fzf.
  ([man](https://github.com/junegunn/fzf/blob/master/man/man1/fzf.1))

## Открытые вопросы

- **Версионная сверка.** Отчёт построен по `master` (0.74.5); локально — 0.74.4.
  Полный man локального бинарника не сверялся, только заголовок CHANGELOG и
  `fzf --version`; возможны мелкие расхождения по свежим опциям.
- **`--listen` API.** Детали GET-эндпоинтов и status-payload (включая `positions`
  в матчах, добавленные в 0.71.0) прочитаны фрагментарно; нужно ли Lusty
  перенимать JSON-статус в `serve` — решение не принято.
- **`--track`/`--id-nth` (0.71.0)** — семантика известна только по CHANGELOG;
  применимость к перезагрузкам `serve` (стабильность курсора при `reload`)
  не проверена.
- **Tauri-GUI.** Какие из window-механик (`--tmux` popup/floating, позиция и
  размер в %) воспроизводимы средствами WM/Tauri без tmux — не исследовано.
- **Производительность.** Конкретных цифр про 600k записей в fzf-источниках нет;
  переносить имеет смысл лимиты (`--tail`) и осторожность с `--nth/--with-nth`,
  но замеры для Lusty нужны свои.
- **Async статус.** Делать ли аналог `--info-command` асинхронным (в fzf он
  синхронный и блокирует UI) — открыто.

## Источники (URL + дата)

Все открыты 2026-10-14 (HTTP 200), кроме отмеченного 404:

1. fzf man page (master, 0.74.5) — https://raw.githubusercontent.com/junegunn/fzf/master/man/man1/fzf.1
2. fzf README (master) — https://raw.githubusercontent.com/junegunn/fzf/master/README.md
3. fzf CHANGELOG (master; версии 0.33.0, 0.47.0, 0.48.0, 0.59.0, 0.71.0, 0.74.x) — https://raw.githubusercontent.com/junegunn/fzf/master/CHANGELOG.md
4. fzf ADVANCED.md (master) — https://raw.githubusercontent.com/junegunn/fzf/master/ADVANCED.md
5. fzf wiki «examples» (community-maintained) — https://github.com/junegunn/fzf/wiki/examples
   (raw `.md` версия отдала 404, страница читалась в HTML)
6. fzf.vim README — https://raw.githubusercontent.com/junegunn/fzf.vim/master/README.md
7. fzf-git.sh README — https://raw.githubusercontent.com/junegunn/fzf-git.sh/master/README.md
8. forgit README — https://raw.githubusercontent.com/wfxr/forgit/master/README.md
9. fzf-tab README — https://raw.githubusercontent.com/Aloxaf/fzf-tab/master/README.md
10. zsh4humans README — https://raw.githubusercontent.com/romkatv/zsh4humans/master/README.md

Локальная проверка (не веб-источник): `fzf 0.74.4 (refs/tags/v0.74.4)` в PATH.
Бюджет: 11 HTTP-запросов, включая повторный запрос вики после 404 (содержательных
источников — 10).
