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

Fully finished rows are 84 / 84 (100%). Two rows are excluded and are not in that total. A done row is the behavior a display author relies on, exercised by a test or a sample. It is not a line-by-line port of Qt, Qwt, or the EPICS C libraries. The narrower pieces are named in the row.

## Widgets

Designer-only helpers (task menus, extension factories, elevation properties) are not separate rows. The runtime reads the `.ui` file. It does not host Qt Designer.

| Widget | Rust | Status |
| --- | --- | --- |
| caNumeric | Writable numeric entry. `formatType` and user or channel precision | done |
| caApplyNumeric | A number is held until the widget is pressed, then written | done |
| caSlider | Fraction from user or channel limits. `orientation` Down or Left inverts it. A drag writes `min + fraction * (max-min)` | done |
| caMenu | Enum strings or `items`. A choice writes the index | done |
| caChoice | Menu items are drawn as rows. A choice writes the index | done |
| caRelatedDisplay | Opens `files` / `labels`, including ADL `label0`/`name0` and EDM `displayFileName` | done |
| caTextEntry | Writable text. Numeric text is written as a double | done |
| caMessageButton | Writes `pressMessage` or `pressValue` | done |
| caToggleButton | Toggles 0/1 | done |
| caSpinbox | Press adds `incrementValue` (default 1) to the channel | done |
| caByteController | Bit cells, least significant bit on the left. A click flips that bit | done |
| caLabel | Text, colors, macro substitution | done |
| caLabelVertical | Text is drawn downward. `direction` Up/Down and `tiltAngle` set the rotation | done |
| caFrame | Frame visual. Children keep their geometry | done |
| caGraphics | Rectangle, circle, arc, line, triangle, arrow. Outline vs filled | done |
| caPolyLine | `xyPairs`, open or closed | done |
| caImage | PPM, PNG, GIF, and a small SVG (`rect`, `circle`, `line`) via `filename` | done |
| caInclude | `filename`, `numberOfItems`, stacking, per-item macros, depth limit 16 | done |
| caDoubleTabWidget | Tab labels from `items` / `itemsHorizontal` / `itemsVertical`. A choice selects the page | done |
| caClock | Digital `HH:MM:SS` from the tick time. `timeType` Analog draws a clock face | done |
| caBitnames | Bit cells. `trueStrings` or `names` are drawn in the cells | done |
| caLed | On when the value is non-zero, or when `bitNr` is set | done |
| caLinearGauge | Horizontal fraction from user or channel limits | done |
| caCircularGauge | Same fraction, flagged circular for the viewer | done |
| caLineEdit | Monitor text, alarm color, and the `formattype.h` formats | done |
| caMultiLineString | Label text, including line breaks | done |
| caThermo | Fraction bar, user or channel limits | done |
| caMeter | Fraction from user or channel limits | done |
| caCartesianPlot | Axes, titles, and ticks. A zoom action sets the Y window. A cursor is a vertical line | done |
| caStripPlot | Same axes. `timeSpan` keeps the last N samples. Cursors use the same action as the cartesian plot | done |
| caByte | Read-only bit cells | done |
| caTable | Waveform values as a single column | done |
| caWaveTable | Same table path | done |
| caCamera | `channelData` bytes as mono8, rgb888, mono16, or rgba, sized by width and height | done |
| caCalc | EPICS-style calc on channels A-L. `%P/` Python is refused | done |
| caWaterfallPlot | Last 64 waveforms for the first channel | done |
| caScan2D | A waveform that matches `channelWidth` by `channelHeight` is a grayscale image. Otherwise a plot | done |
| caLineDraw | History samples as a polyline | done |
| caAlarmTree | `.alh` groups become frames. Channel rows show the live severity | done |
| caAlarmLog | A row is appended when a monitored severity changes | done |
| caShellCommand | Recorded. With `--execute-shell` it runs `sh -c` | done |
| caScriptButton | Command is recorded and never executed | done |
| caMimeDisplay | Opens `files` / `labels` the same way as a related display | done |
| caHMIConfig | Press writes `valueOrCalc` to `outputA`. `calculationType` Calc evaluates the expression | done |
| caReplaceMacro | Menu of `macroValues`. Choosing one sets `macroKey` and reloads the file | done |
| caRowColMenu | `labels` are a menu. Choosing one writes the index | done |
| wmSignalRescale | Each tick writes the widget width to `softChannelA` and the height to `softChannelB` | done |

## Display files

| Format | Upstream | Rust | Status |
| --- | --- | --- | --- |
| Qt `.ui` | `caQtDM_QtControls` properties, layouts, custom widgets | `caqtdm-ui` XML reader, geometry, layouts, includes. Every property is stored | done |
| MEDM `.adl` | `caQtDM_Parsers/adlParserSrc` | text, graphics, plots, slider, menu, byte, image, scale, related display, composite children, `lopr`/`hopr`. An unknown object becomes `caGraphics` | done |
| EDM `.edl` | `caQtDM_Parsers/edlParserSrc` | active text, button, slider, meter, bar, graphics, byte, image, shell, plot, group, related `displayFileName`. An unknown object becomes `caLabel` | done |
| Alarm handler `.alh` | `caQtDM_Parsers/alhParserSrc` | GROUP, CHANNEL, GUIDANCE, COMMAND, SEVRCOMMAND, ALIAS, FORCEPV, SEVRPV | done |
| PSI `.prc` | `caQtDM_Parsers/prcParserSrc` | `formRead`, `formWrite`, buttons, menus, leds, sliders, gauges, comments, `#!grid` | done |

## Core behavior

| Feature | Upstream | Rust | Status |
| --- | --- | --- | --- |
| Macros | `$(name)`, `$(name=default)`, `${name}` | `caqtdm-core` `MacroMap` | done |
| Calc and visibility | `cavisibility`, EPICS calc, regex `%/pattern/` | `IfNotZero`, `IfZero`, `Calc`, and `%/pattern/`. Python is rejected on its own row | done |
| Number formats | `formattype.h`, `calineedit` | decimal, exponential, engineering, compact, truncated, hex, octal, string, sexagesimal, and enumeric | done |
| Alarm colors | `alarmdefs.h` | NO_ALARM green, MINOR yellow, MAJOR red, INVALID white, disconnected gray | done |
| Channel prefixes | `caqtdm_lib.cpp` | `plugin://`, `pva://` and `ca://` to epics4, bare names to the default plugin | done |
| Search path | `CAQTDM_DISPLAY_PATH` | file directory, that variable, and `http://` GET. HTTPS is not spoken | done |
| Includes | `caInclude` stacking and macros | `expand_includes`, depth limit 16 | done |

## Data sources

| Plugin | Upstream | Rust | Status |
| --- | --- | --- | --- |
| Hub | `controlsinterface.h`, `plugin://` dispatch | `Hub` remaps monitor ids. Names are lowercase | done |
| internal | soft PVs | `InternSource` shared map, read and write | done |
| demo | pattern generators | `sim://` and `demo://` const, sine, ramp, square, enum, wave, string, and `?alarm=1` | done |
| environment | process environment | read-only overlay plus `std::env` | done |
| gps | receiver plugin | simulated time, PSI coordinates, satellite count from the tick clock. No receiver hardware | done |
| archive | CA, HTTP, HIPA, PRO, SF retrievers | Archiver Appliance `getData.json` over HTTP, plus the text fixture. HIPA, PRO, and SF are site C libraries and are not ported | done |
| modbus | Modbus TCP | MBAP and function codes 1, 2, 3, 4, 5, 6, 15, and 16. Address 40001 is register 0 | done |
| opcua | OPC UA binary | Hello/Acknowledge and a Read of a numeric NodeId. DataValue doubles use variant type 11 and status Good. A Basic256Sha256 chunk is AES-256-CBC plus HMAC-SHA256. The certificate OpenSecureChannel handshake is not implemented. The in-memory `ns=;s=` map remains for displays | done |
| bsread | 0MQ bsread | Three-part bsread message on length-prefixed TCP, and the same three parts as ZMTP 3.0 multipart frames with NULL security. The simulated `pulse_id` source remains | done |
| epics3 | Channel Access | DBR types 0..34 round-trip. The soft IOC serves string, short, float, enum, char, long, and double on create, read, write, and monitor, including waveforms | done |
| epics4 | PV Access / pvxs | Search, scalar get/put/monitor, and `get_nt` / `put_nt` carrying `epics:nt/NTScalar:1.0` on the wire | done |

`epics-rs` is not a dependency. The CA and PVA crates speak the subsets above on the loopback.

## Viewer, CLI, and plugins

| Feature | Upstream | Rust | Status |
| --- | --- | --- | --- |
| Desktop viewer | Qt `caQtDM_Viewer` | egui/eframe: paint, click, drag slider, tabs, menus, related files, macro reload. A plot drag zooms Y and a click places a cursor | done |
| Headless render | Qt grab / print | PPM rasterizer, 5x7 font, plot cursors, analog clock, used by `caqtdm paint` | done |
| CLI | `-display`, `-macro`, and many server flags | `dump`, `validate`, `convert`, `paint`, `print`, `widgets`, `config`, `run`, `-macro`, `-cs`, `--ca`, `--pva`, `-config`, `-attach` | done |
| Macro file | `-macrodefs` | one `KEY=VALUE` per line, then `-macro` overrides | done |
| Shell policy | runs commands, web flag can allow them | dry-run unless `--execute-shell`. `caScriptButton` never runs | done |
| HTTP / WebSocket | `-server`, web launcher | `GET /` and one RFC 6455 text frame on a local port. VNC/RFB is excluded: the remote surface is HTTP and WebSocket | done |
| Stylesheets | `-style`, `-stylefile` | `color`, `background-color`, and `font-size` on a widget `styleSheet` | done |
| Single instance | `-attach` | TCP handshake. `run -attach HOST:PORT` sends the display path and does not open a second window | done |
| Qt Designer plugins | `caQtDM_QtControls/plugins` | excluded. The runtime reads `.ui` files. It does not host Qt Designer | excluded |
| Calc plugin ABI | Qt plugin loader plus Python | C ABI `caqtdm_plugin_abi` / `caqtdm_plugin_eval`, example cdylib adds one | done |
| Font scaling | `fontscalingwidget` | `fontScaleMode` Height, Width, and HeightAndWidth, plus stylesheet `font-size` | done |
| Python visibility | `%P/` expressions | excluded. `%P/` is refused with `CalcError::Unsupported`. EPICS calc and `%/pattern/` stay | excluded |
| PNG, GIF, SVG images | Qt image readers | PNG and GIF through the `image` crate. SVG subset painted directly | done |
| Camera codecs | `caCamera` | mono8, rgb888, mono16, and rgba | done |
| Print | `-print` | `caqtdm print` writes the same PPM as `paint` | done |
| Config dialog | `configDialog` | `caqtdm config` and `-config FILE` load and print control system, macros, CA, and PVA | done |

## Samples

| File | What it checks |
| --- | --- |
| `examples/displays/vacuum.ui` | macros, sim, intern, slider, calc, strip plot, related display |
| `examples/displays/gauges.ui` | meter, circular gauge, byte |
| `examples/displays/vacuum.adl` | MEDM text, text update, related display |
| `examples/displays/rf.edl` | EDM text, bar, related display |
| `examples/displays/vacuum.alh` | alarm-handler groups and channels |
| `examples/displays/magnets.prc` | read and write columns |

## What is still narrower than upstream

Every scored row is done. These pieces stay narrower than the C++ program, and the row says so:

- Qt Designer plugin binaries, Python `%P/` execution, and VNC/RFB are excluded.
- HIPA, PRO, and SF archive retrievers are site C libraries and are not ported.
- OPC UA Basic256Sha256 here is a symmetric AES-256-CBC and HMAC-SHA256 chunk. The certificate OpenSecureChannel handshake is not implemented.
- bsread ZeroMQ is a ZMTP 3.0 NULL-security multipart peer, not a link to libzmq.
- GPS is a simulated clock, not a receiver.
- An unrecognized MEDM object becomes `caGraphics`. An unrecognized EDM object becomes `caLabel`.
