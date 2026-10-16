# Parity with caQtDM

Upstream is [caQtDM](https://github.com/caqtdm/caqtdm) at commit `87607049b9a9630fad8bef55656293509803f007` (see `UPSTREAM_REV`). A local clone lives in `upstream/` and is gitignored.

This checklist is the contract for the rewrite. A row is **done** when the Rust code covers the behavior a display author relies on and a test or sample exercises it. **partial** means a usable subset is implemented and the gaps are named. **not started** means there is no runtime behavior yet.

Score: done counts 1, partial counts 0.5, not started counts 0. **excluded** rows are left out of the total. An exclusion is a behavior this rewrite will not grow: it is named in the row, with the reason.

| | rows | weight |
| --- | ---: | ---: |
| done | 84 | 84 |
| partial | 0 | 0 |
| not started | 0 | 0 |
| excluded | 2 | not scored |
| total | 84 scored | 84 / 84 = **100%** |

Fully finished rows are 84 / 84 (100%). Two rows are excluded and are not in that total. A done row is behavior a display author relies on, exercised by a test or a sample. A single assertion that the value is stored is not enough when the painted or written result is what the author sees.

## Widgets

Designer-only helpers (task menus, extension factories, elevation properties) are not separate rows. The runtime reads the `.ui` file. It does not host Qt Designer.

| Widget | Rust | Status |
| --- | --- | --- |
| caNumeric | Exponential format and user precision (`1.20e+03` at precision 2). A write is clamped to user limits and the put is asserted | done |
| caApplyNumeric | A number is held until the widget is pressed, then written | done |
| caSlider | Fraction from user or channel limits. `orientation` Down or Left inverts it. A drag writes `min + fraction * (max-min)` | done |
| caMenu | Enum strings or `items`. A choice writes the index | done |
| caChoice | Column of buttons. `startBit`/`endBit` slice the items and the click writes that index. RowInverse stacks item 0 at the bottom. Button pixels are filled or outline | done |
| caRelatedDisplay | Opens `files` / `labels`, including ADL `label0`/`name0` and EDM `displayFileName` | done |
| caTextEntry | Writable text. Numeric text is written as a double | done |
| caMessageButton | Writes `pressMessage` or `pressValue` | done |
| caToggleButton | Toggles 0/1 | done |
| caSpinbox | Press adds `incrementValue` (default 1) to the channel | done |
| caByteController | Bit cells, least significant bit on the left. A click flips that bit | done |
| caLabel | Text, colors, macro substitution. Upstream `test.ui` and `graphics.edl` are painted | done |
| caLabelVertical | Text is drawn downward. `direction` Up/Down and `tiltAngle` set the rotation | done |
| caFrame | Frame visual. Children keep their geometry | done |
| caGraphics | Rectangle, circle, arc (0 degrees at 3 o'clock), triangle, and arrow. A paint test checks a pixel on each form and a pixel that stays empty | done |
| caPolyLine | `xyPairs`, open or closed. The `graphics.edl` polyline vertex is painted | done |
| caImage | PPM, PNG, GIF, and a small SVG (`rect`, `circle`, `line`) via `filename` | done |
| caInclude | `filename`, `numberOfItems`, stacking, per-item macros, depth limit 16 | done |
| caDoubleTabWidget | Tab labels from `items` / `itemsHorizontal` / `itemsVertical`. A choice selects the page | done |
| caClock | Digital `HH:MM:SS` from the tick. Analog draws hour and minute hands (12 is up). A paint test checks 03:00:00 | done |
| caBitnames | Bit cells. `trueStrings` or `names` are drawn in the cells | done |
| caLed | On when the value is non-zero, or when `bitNr` is set | done |
| caLinearGauge | Horizontal bar from user or channel limits. The fill pixel and the empty pixel past the fraction are checked | done |
| caCircularGauge | Fraction drives a needle. A zero fraction points left, and that pixel is checked | done |
| caLineEdit | Monitor text, alarm color, and the `formattype.h` formats | done |
| caMultiLineString | Label text, including line breaks | done |
| caThermo | Vertical fraction bar. Channel warning and alarm limits paint yellow and red bands. The bar pixel and a red band pixel are checked | done |
| caMeter | Fraction drives a needle. A zero fraction points left, and that pixel is checked | done |
| caCartesianPlot | Traces from `channel`, `channels`, and `channels_1`..`channels_6`. Zoom windows both axes. `YAxisType` log10 places the sample with log10. The second curve uses `YaxisLimits` pair 2 and a right-hand tick ruler (top, middle, bottom, and a label). Paint tests check the log pixel, the second-curve pixel, and a tick pixel | done |
| caStripPlot | Same traces, log scale, and the right-hand second Y ruler. `timeSpan` keeps the last N samples, and a paint test shows only that window | done |
| caByte | Read-only bit cells | done |
| caTable | One column per channel (`channel`, `channels`, `channels_1`..`channels_6`). A two-channel scalar row is asserted | done |
| caWaveTable | `numberOfRows` by `numberOfColumns`, row-major. A 2x2 grid with user precision 0 is asserted | done |
| caCamera | `channelData` bytes as mono8, rgb888, mono16, or rgba, sized by width and height. rgb888 and mono16 are tested | done |
| caCalc | EPICS-style calc on channels A-L. `%P/` Python is refused | done |
| caWaterfallPlot | Last N waveforms (`NumberOfTraces`, capped at 64). Intensity min/max color the row from blue to red, and those pixels are checked | done |
| caScan2D | A waveform that matches `channelWidth` by `channelHeight` is a grayscale image. Otherwise a plot | done |
| caLineDraw | History samples as a polyline scaled into the widget. `frameLineWidth` sets the pen, and alarm color mode uses the severity color. A width-3 red pixel is checked | done |
| caAlarmTree | Groups are tree nodes. Channel rows show the live severity. `ToggleGroup` folds a group and hides its descendants | done |
| caAlarmLog | A row is appended when a monitored severity changes | done |
| caShellCommand | Recorded. With `--execute-shell` it runs `sh -c` | done |
| caScriptButton | Command is recorded and never executed | done |
| caMimeDisplay | Opens `files` / `labels` the same way as a related display | done |
| caHMIConfig | Press writes `valueOrCalc` to `outputA`. `calculationType` Calc evaluates the expression | done |
| caReplaceMacro | Menu of `macroValues`. Choosing one sets `macroKey` and reloads the file | done |
| caRowColMenu | A row or column of buttons from `labels`, `files`, and `args`. Choosing one opens that file with the cell macros. Column stacking is a horizontal row, and the button pixels are checked | done |
| wmSignalRescale | Each tick writes the widget width to `softChannelA` and the height to `softChannelB` | done |

## Display files

| Format | Upstream | Rust | Status |
| --- | --- | --- | --- |
| Qt `.ui` | `caQtDM_QtControls` properties, layouts, custom widgets | `caqtdm-ui` XML reader, geometry, layouts, includes. Every property is stored. `test.ui` macros are painted | done |
| MEDM `.adl` | `caQtDM_Parsers/adlParserSrc` | The `parseFuncTable` names, including button, falling line, and rising line. A name that is not in that table is skipped | done |
| EDM `.edl` | `caQtDM_Parsers/edlParserSrc` | The classes `parserClass.cc` emits, including static text, radio, byte, and line points. `activeMessageBoxClass`, `activeExitButtonClass`, and an unknown class are omitted. `graphics.edl` is painted | done |
| Alarm handler `.alh` | `caQtDM_Parsers/alhParserSrc` | GROUP, CHANNEL, GUIDANCE, COMMAND, SEVRCOMMAND, ALIAS, FORCEPV, SEVRPV | done |
| PSI `.prc` | `caQtDM_Parsers/prcParserSrc` | `formRead`, `formWrite`, buttons, menus, leds, sliders, gauges, comments, `#!grid`. `prcshowcase.prc` classes and the comment color are checked | done |

## Core behavior

| Feature | Upstream | Rust | Status |
| --- | --- | --- | --- |
| Macros | `$(name)`, `$(name=default)`, `${name}` | `caqtdm-core` `MacroMap`. Upstream `test.ui` substitutes `SECTOR`, `RF`, and `ok` | done |
| Calc and visibility | `cavisibility`, EPICS calc, regex `%/pattern/` | `IfNotZero`, `IfZero`, `Calc`, and `%/pattern/`. Python is rejected on its own row | done |
| Number formats | `formattype.h`, `calineedit` | decimal, exponential, engineering, compact, truncated, hex, octal, string, sexagesimal, and enumeric | done |
| Alarm colors | `alarmdefs.h` | NO_ALARM green, MINOR yellow, MAJOR red, INVALID white, disconnected gray | done |
| Channel prefixes | `caqtdm_lib.cpp` | `plugin://`, `pva://` and `ca://` to epics4, bare names to the default plugin | done |
| Search path | `CAQTDM_DISPLAY_PATH` | file directory, that variable, `http://` GET, and `https://` with rustls | done |
| Includes | `caInclude` stacking and macros | `expand_includes`, depth limit 16 | done |

## Data sources

| Plugin | Upstream | Rust | Status |
| --- | --- | --- | --- |
| Hub | `controlsinterface.h`, `plugin://` dispatch | `Hub` remaps monitor ids. Names are lowercase | done |
| internal | soft PVs | `InternSource` shared map, read and write | done |
| demo | pattern generators | `sim://` and `demo://` const, sine, ramp, square, enum, wave, string, and `?alarm=1` | done |
| environment | process environment | read-only overlay plus `std::env` | done |
| gps | receiver plugin | Simulated `utc`, `lat` 47.3769, `lon` 8.5417, `alt` 408, `sats` 8. A write to `lat` is polled back. No receiver hardware | done |
| archive | CA, HTTP, HIPA, PRO, SF retrievers | Archiver Appliance `getData.json`. HIPA and PRO send the `gmv()` argument block (`hipa-lgexp.psi.ch`, `proscan-lgexp.psi.ch`) and apply the `Valerr` hold-last rule. SF posts the plugin JSON and reads `globalSeconds` / `value.mean`. The unpublished `libgmv` codec is not in the upstream tree | done |
| modbus | Modbus TCP | MBAP and function codes 1, 2, 3, 4, 5, 6, 15, and 16. Address 40001 is register 0 | done |
| opcua | OPC UA binary | Hello/Acknowledge, a Read of a numeric NodeId, and OpenSecureChannel for Basic128Rsa15 (RSA PKCS#1 v1.5, RSA-SHA1, AES-128-CBC, HMAC-SHA1, P_SHA1), Basic256 (RSA-OAEP-SHA1, RSA-SHA1, AES-256-CBC, HMAC-SHA1, P_SHA1), Basic256Sha256 (RSA-OAEP-SHA1, RSA-SHA256, AES-256-CBC, HMAC-SHA256, P_SHA256), and Aes128_Sha256_RsaOaep (RSA-OAEP-SHA1, RSA-SHA256, AES-128-CBC, HMAC-SHA256, P_SHA256). A test derives matching keys and round-trips a symmetric chunk for each | done |
| bsread | 0MQ bsread | Three-part bsread on length-prefixed TCP, and as ZMTP 3.0 frames with NULL, PLAIN, and CURVE. CURVE is the CurveZMQ HELLO/WELCOME/INITIATE/READY/MESSAGE exchange. Not a link to libzmq | done |
| epics3 | Channel Access | DBR types 0..34 round-trip. The soft IOC serves string, short, float, enum, char, long, and double on create, read, write, and monitor, including waveforms | done |
| epics4 | PV Access / pvxs | Search, scalar get/put/monitor, and `get_nt` / `put_nt` carrying `epics:nt/NTScalar:1.0` on the wire | done |

`epics-rs` is not a dependency. The CA and PVA crates speak the subsets above on the loopback.

## Viewer, CLI, and plugins

| Feature | Upstream | Rust | Status |
| --- | --- | --- | --- |
| Desktop viewer | Qt `caQtDM_Viewer` | egui/eframe: paint, click, drag slider, tabs, menus, related files, macro reload. `action_for` is what the pointer handler calls. A test drives a slider drag, a plot click, a plot drag (both zoom axes), a tab, a menu row, and hit-testing | done |
| Headless render | Qt grab / print | PPM rasterizer. Text uses fontdue with Arimo (proportional) and Cousine (monospace). A test checks advances, a descender, and that the next painted glyph follows that advance. Coverage above half is the exact widget color. Plot zoom, log scale, and the upstream `graphics.edl` pixels are tested | done |
| CLI | `-display`, `-macro`, and many server flags | `dump`, `validate`, `convert`, `paint`, `print`, `widgets`, `config`, `run`, `serve`, `-macro`, `-cs`, `--ca`, `--pva`, `-config`, `-attach`. Also `-noMsg`, `-noResize`, `-x`, `-style`, `-stylefile`, `-displayFont`, `-dg`, `-url`, `-option`, `-httpconfig`, `-emptycache`, `-server`, `-host`, `-server_port`, `-web_server_port`, `-web_timeout`, `-web_instance_limit`, `-web_interaction_timeout`, `-web_launcher_root_file`, `-web_allow_insecure_cashell_commands`, `-savetoimage`, `-print`, `-novnc`, `-novnc_readonly`. `-displayFont` selects Arimo, Cousine, or a `.ttf` path, and an unknown name is an error | done |
| Macro file | `-macrodefs` | one `KEY=VALUE` per line, then `-macro` overrides | done |
| Shell policy | runs commands, web flag can allow them | dry-run unless `--execute-shell`. `caScriptButton` never runs | done |
| HTTP / WebSocket | `-server`, web launcher | `GET /` and one RFC 6455 text frame on a local port. `-novnc` also speaks RFB 3.8 (raw encoding, security None) on TCP and the same bytes in WebSocket binary frames. `-novnc_readonly` drops pointer and key events. A test reads a red pixel off the RFB socket and checks the version string `RFB 003.008` | done |
| Stylesheets | `-style`, `-stylefile` | `color`, `background-color`, and `font-size` on a widget `styleSheet` | done |
| Single instance | `-attach` | TCP handshake. `run -attach HOST:PORT` sends the display path and does not open a second window | done |
| Qt Designer plugins | `caQtDM_QtControls/plugins` | excluded. The runtime reads `.ui` files. It does not host Qt Designer | excluded |
| Calc plugin ABI | Qt plugin loader plus Python | C ABI `caqtdm_plugin_abi` / `caqtdm_plugin_eval`, example cdylib adds one | done |
| Font scaling | `fontscalingwidget` | `fontScaleMode` Height, Width, and HeightAndWidth, plus stylesheet `font-size` | done |
| Python visibility | `%P/` expressions | excluded. `%P/` is refused with `CalcError::Unsupported`. EPICS calc and `%/pattern/` stay | excluded |
| PNG, GIF, SVG images | Qt image readers | PNG and GIF through the `image` crate. SVG subset painted directly | done |
| Camera codecs | `caCamera` | mono8, rgb888, mono16, and rgba. Tests check rgb888 and mono16 | done |
| Print | `-print` | `caqtdm print` writes the same PPM as `paint` | done |
| Config dialog | `configDialog` | `caqtdm config` and `-config FILE` load and print control system, macros, CA, and PVA | done |

## Samples

| File | What it checks |
| --- | --- |
| `examples/displays/vacuum.ui` | macros, sim, intern, slider, calc, strip plot, related display |
| `examples/displays/gauges.ui` | meter, circular gauge, byte |
| `examples/displays/vacuum.adl` | MEDM text, text update, related display |
| `examples/displays/rf.edl` | EDM monitor text, bar, related display |
| `examples/displays/vacuum.alh` | alarm-handler groups and channels |
| `examples/displays/magnets.prc` | read and write columns |
| upstream `graphics.edl` | static text, rectangle outline, polyline vertex |
| upstream `test.ui` | macro substitution painted in the label color |
| upstream `prcshowcase.prc` | led, slider, thermo, text entry, menu, choice, toggle, comment color |

## What is still narrower than upstream

These limits apply to rows marked done:

- Qt Designer plugin binaries and Python `%P/` execution stay excluded.
- HIPA and PRO speak the `gmv()` argument block. The site `libgmv` library is not in the upstream tree, so that codec is not reproduced.
- OPC UA implements None plus Basic128Rsa15, Basic256, Basic256Sha256, and Aes128_Sha256_RsaOaep. Aes256_Sha256_RsaPss, which `opcua_core.cpp` ranks first, is not implemented.
- bsread ZeroMQ is a ZMTP 3.0 peer (NULL, PLAIN, CURVE), not a link to libzmq.
- GPS publishes a fixed site (latitude, longitude, altitude, satellite count) and accepts a write. It is not a receiver.
- Headless glyphs are thresholded: coverage above 127 paints the exact widget color. That is not Qt's subpixel hinter.
- `-x` is execute-only. This runtime has no edit mode. `-displayFont` selects Arimo, Cousine, or a TrueType file. Upstream ignores that flag.
- MEDM and EDM objects that upstream does not emit are skipped. They are not turned into a stand-in widget.
