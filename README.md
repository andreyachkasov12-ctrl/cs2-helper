# CS2 Helper

Одна программа (один .exe), которая по демке CS2 делает:

- **разбор ошибок**: смерти без размена, первые смерти в раундах, флеши в своих и в себя, схема мест смертей;
- **гранаты**: каждый твой бросок с точкой падения и командой `setpos/setang`, чтобы встать на то же место;
- **нарезки киллов**: автоматически находит 2K/3K/4K/ACE, делает `.cfg` для каждого клипа и склеивает записанные ролики в один через ffmpeg.

Результат: HTML-отчёт, который сам открывается в браузере.

## Как пользоваться (когда есть exe)

1. Скачай демку: CS2 → «Смотреть» → «Ваши матчи» → «Скачать». Если файл .bz2, распакуй.
2. Перетащи `.dem` на `cs2-helper.exe`.
3. Введи номер своего игрока в списке. Рядом с демкой появится папка `…_разбор` с `report.html` и папкой `clips`.

### Как записать клипы

1. Открой демку в CS2, запусти запись в OBS.
2. В консоли CS2: `exec clip_01_3K_R5` (имя файла из папки clips, положи .cfg в `game/csgo/cfg`).
3. Остановить запись, когда пройдёт время из `clips.txt`. Повтори для других клипов.
4. Сложи все mp4 в одну папку и запусти: `cs2-helper join папка_с_mp4`. Получится `highlights.mp4` с плавными переходами.

Нужен ffmpeg (ffmpeg.org) в PATH только для склейки.
# CS2 Helper

A single program (one .exe file) that processes CS2 demo files to provide:

- **Error analysis**: deaths without a trade, opening deaths, flashes that hit teammates or yourself, and a map-based visualization of where you died.
- **Grenade analysis**: a breakdown of every grenade you threw, including its landing spot and the specific `setpos`/`setang` commands needed to replicate your exact position.
- **Kill highlights**: automatic detection of 2K/3K/4K/ACE sequences; it generates a `.cfg` file for each clip and uses FFmpeg to stitch the recorded clips together into a single video.

The output is an HTML report that opens automatically in your web browser.

## How to use (once you have the .exe)

1. Download a demo: CS2 → "Watch" → "Your Matches" → "Download". If the file is in `.bz2` format, extract it.
2. Drag and drop the `.dem` file onto `cs2-helper.exe`.
3. Enter your player number from the list provided. A folder named `…_analysis` (or similar) will appear next to the demo file, containing `report.html` and a `clips` folder.

## How to record clips

1. Open the demo in CS2 and start recording in OBS.
2. In the CS2 console, type: `exec clip_01_3K_R5` (use the filename found in the `clips` folder; place the `.cfg` file in your `game/csgo/cfg` directory).
3. Stop the recording once the duration specified in `clips.txt` has elapsed. Repeat this process for the other clips.
4. Place all the resulting `.mp4` files into a single folder and run: `cs2-helper join folder_with_mp4s`. This will generate a `highlights.mp4` file with smooth transitions.

FFmpeg (from ffmpeg.org) must be in your system PATH to use the video-stitching feature.
