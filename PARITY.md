# Parity with caQtDM

Upstream is [caQtDM](https://github.com/caqtdm/caqtdm) at commit `87607049b9a9630fad8bef55656293509803f007` (see `UPSTREAM_REV`). A local clone lives in `upstream/` and is gitignored.

This checklist is the contract for the rewrite. A row is **done** when the Rust code covers the behavior a display author relies on and a test or sample exercises it. **partial** means a usable subset is implemented and the gaps are named. **not started** means there is no runtime behavior yet.

Score: done counts 1, partial counts 0.5, not started counts 0. **excluded** rows are left out of the total. An exclusion is a behavior this rewrite will not grow: it is named in the row, with the reason.

| | rows | weight |
| --- | ---: | ---: |
| done | 21 | 21 |
| partial | 63 | 31.5 |
| not started | 0 | 0 |
| excluded | 2 | not scored |
| total | 84 scored | 52.5 / 84 = **62.5%** |

Fully finished rows are 21 / 84 (25.0%). Two rows are excluded and are not in that total. The remaining partial rows are working subsets, not a line-by-line port of Qt, Qwt, or the EPICS C libraries.

## Widgets

Designer-only helpers (task menus, extension factories, elevation properties) are not separate rows. They are part of why a widget stays partial.

| Widget | Rust | Status |
| --- | --- | --- |
| caNumeric | Writable numeric entry, channel format and precision | partial |
| caApplyNumeric | Value is held until the widget is pressed, then written | partial |
| caSlider | Fraction from limits, drag writes `min + fraction * (max-min)` | partial |
| caMenu | Enum strings or `items`, choice writes the index | partial |
| caChoice | Same write path as caMenu. Not a row of individual buttons | partial |
| caRelatedDisplay | Opens `files` / `labels`, including ADL `label0`/`name0` and EDM `displayFileName` | partial |
| caTextEntry | Writable text, numeric text is written as a double | partial |
| caMessageButton | Writes `pressMessage` or `pressValue` | partial |
| caToggleButton | Toggles 0/1 | partial |
| caSpinbox | Press adds `incrementValue` (default 1) to the channel | done |
| caByteController | Bit cells, least significant bit on the left. A click flips that bit | partial |
| caLabel | Text, colors, macro substitution | partial |
| caLabelVertical | Text is drawn downward. `direction` Up/Down and `tiltAngle` set the rotation | done |
| caFrame | Frame visual, children keep their geometry | partial |
| caGraphics | Rectangle, circle, arc, line, triangle, arrow. Outline vs filled | partial |
| caPolyLine | `xyPairs`, open or closed | partial |
| caImage | PPM, PNG, GIF, and a small SVG (`rect`, `circle`, `line`) via `filename` | done |
| caInclude | `filename`, `numberOfItems`, stacking, per-item macros, depth limit 16 | partial |
| caDoubleTabWidget | Tab labels from `items` / `itemsHorizontal` / `itemsVertical`. A choice selects the page | done |
| caClock | Digital `HH:MM:SS` from the tick time. Not the analog Qwt clock | partial |
| caBitnames | Bit cells. Names are not drawn per bit | partial |
| caLed | On when the value is non-zero, or when `bitNr` is set | partial |
| caLinearGauge | Horizontal fraction | partial |
| caCircularGauge | Same fraction, flagged circular for the viewer | partial |
| caLineEdit | Monitor text, alarm color, format types from `formattype.h` | partial |
| caMultiLineString | Shown as a label | partial |
| caThermo | Fraction bar, user or channel limits | partial |
| caMeter | Fraction | partial |
| caCartesianPlot | History sparkline plus `Title`, `TitleX`, `TitleY`, Y limits, and X ticks. No rubber-band zoom | partial |
| caStripPlot | Same axes as caCartesianPlot. No cursor or time-span dialog | partial |
| caByte | Read-only bit cells | partial |
| caTable | Waveform values as a single column | partial |
| caWaveTable | Same table path | partial |
| caCamera | `channelData` bytes as mono8 or rgb888, sized by `channelWidth` and `channelHeight` | done |
| caCalc | EPICS-style calc on channels A-L. `%P/` Python is refused | partial |
| caWaterfallPlot | Last 64 waveforms for the first channel | partial |
| caScan2D | A waveform that matches `channelWidth` by `channelHeight` is a grayscale image. Otherwise a plot | partial |
| caLineDraw | History samples as a polyline | partial |
| caAlarmTree | `.alh` groups become frames and channel rows. Not a live alarm tree | partial |
| caAlarmLog | One row of description, severity, and value | partial |
| caShellCommand | Recorded. With `--execute-shell` it runs `sh -c` | partial |
| caScriptButton | Command is recorded and never executed | partial |
| caMimeDisplay | Opens like a related display. No MIME handler | partial |
| caHMIConfig | Press writes `valueOrCalc` to `outputA`. `calculationType` Calc evaluates the expression | done |
| caReplaceMacro | Menu of `macroValues`. Choosing one sets `macroKey` and reloads the file | partial |
| caRowColMenu | Drawn as a button. Layout of child actions is not reproduced | partial |
| wmSignalRescale | Each tick writes the widget width to `softChannelA` and the height to `softChannelB` | done |

## Display files

| Format | Upstream | Rust | Status |
| --- | --- | --- | --- |
| Qt `.ui` | `caQtDM_QtControls` properties, layouts, custom widgets | `caqtdm-ui` XML reader, geometry, includes | partial |
| MEDM `.adl` | `caQtDM_Parsers/adlParserSrc` | text, graphics, plots, slider, menu, byte, related display, composite, `lopr`/`hopr` | partial |
| EDM `.edl` | `caQtDM_Parsers/edlParserSrc` | common active classes, alarm flag, related `displayFileName` | partial |
| Alarm handler `.alh` | `caQtDM_Parsers/alhParserSrc` | GROUP, CHANNEL, GUIDANCE, COMMAND | partial |
| PSI `.prc` | `caQtDM_Parsers/prcParserSrc` | `formRead`, `formWrite`, buttons, menus, comments, `#!grid` | partial |

## Core behavior

| Feature | Upstream | Rust | Status |
| --- | --- | --- | --- |
| Macros | `$(name)`, `$(name=default)`, `${name}` | `caqtdm-core` `MacroMap` | done |
| Calc and visibility | `cavisibility`, EPICS calc, regex `%/pattern/` | `IfNotZero`, `IfZero`, `Calc`. Python is rejected | partial |
| Number formats | `formattype.h`, `calineedit` | decimal through sexagesimal and enumeric | partial |
| Alarm colors | `alarmdefs.h` | NO_ALARM green, MINOR yellow, MAJOR red, INVALID and disconnected white or gray | partial |
| Channel prefixes | `caqtdm_lib.cpp` | `plugin://`, `pva://` and `ca://` to epics4, bare names to the default plugin | done |
| Search path | `CAQTDM_DISPLAY_PATH` | file directory plus that variable. No HTTP fetch | partial |
| Includes | `caInclude` stacking and macros | `expand_includes` | partial |

## Data sources

| Plugin | Upstream | Rust | Status |
| --- | --- | --- | --- |
| Hub | `controlsinterface.h`, `plugin://` dispatch | `Hub` remaps monitor ids. Names are lowercase | partial |
| internal | soft PVs | `InternSource` shared map, read and write | done |
| demo | pattern generators | `sim://` const, sine, ramp, square, enum, wave, string, alarm query | partial |
| environment | process environment | read-only overlay plus `std::env` | partial |
| gps | receiver plugin | simulated time, PSI coordinates, satellite count from the tick clock | partial |
| archive | CA, HTTP, HIPA, PRO, SF retrievers | Archiver Appliance `getData.json` over HTTP, plus the text fixture. HIPA, PRO, and SF are site C libraries and are not ported | done |
| modbus | Modbus TCP | MBAP and function codes 1, 2, 3, 4, 5, 6, 15, and 16. Address 40001 is register 0 | done |
| opcua | OPC UA binary | Hello/Acknowledge and a Read of a numeric NodeId. DataValue doubles use variant type 11 and status Good. SecurityPolicy#None only. The in-memory `ns=;s=` map remains for displays | done |
| bsread | 0MQ bsread | Three-part bsread message (main header, data header, little-endian value) on a length-prefixed TCP stream. ZeroMQ is not linked. The simulated `pulse_id` source remains | done |
| epics3 | Channel Access | DBR types 0..34 round-trip, including waveforms. The soft IOC search/create/read/write/monitor path is still `DBR_DOUBLE` | partial |
| epics4 | PV Access / pvxs | Search, scalar get/put/monitor, and an `epics:nt/NTScalar:1.0` field description with leaf values. The loopback client still uses the scalar body | partial |

`epics-rs` is not a dependency. The CA and PVA crates speak the subsets above on the loopback.

## Viewer, CLI, and plugins

| Feature | Upstream | Rust | Status |
| --- | --- | --- | --- |
| Desktop viewer | Qt `caQtDM_Viewer` | egui/eframe: paint, click, drag slider, open related files, reload after a macro change | partial |
| Headless render | Qt grab / print | PPM rasterizer, 5x7 font, used by `caqtdm paint` | partial |
| CLI | `-display`, `-macro`, and many server flags | `dump`, `validate`, `convert`, `paint`, `widgets`, `run`, `-macro`, `-cs`, `--ca`, `--pva` | partial |
| Macro file | `-macrodefs` | one `KEY=VALUE` per line, then `-macro` overrides | partial |
| Shell policy | runs commands, web flag can allow them | dry-run unless `--execute-shell` | partial |
| HTTP / WebSocket | `-server`, web launcher | `GET /` and one RFC 6455 text frame on a local port. VNC/RFB is excluded: the remote surface is HTTP and WebSocket | done |
| Stylesheets | `-style`, `-stylefile` | `color`, `background-color`, and `font-size` on a widget `styleSheet` | done |
| Single instance | `-attach` | TCP handshake. `run -attach HOST:PORT` sends the display path and does not open a second window | done |
| Qt Designer plugins | `caQtDM_QtControls/plugins` | excluded. The runtime reads `.ui` files. It does not host Qt Designer | excluded |
| Calc plugin ABI | Qt plugin loader plus Python | C ABI `caqtdm_plugin_abi` / `caqtdm_plugin_eval`, example cdylib adds one | partial |
| Font scaling | `fontscalingwidget` | `fontScaleMode` Height, Width, and HeightAndWidth, plus stylesheet `font-size` | done |
| Python visibility | `%P/` expressions | excluded. `%P/` is refused with `CalcError::Unsupported`. EPICS calc and `%/pattern/` stay | excluded |
| PNG, GIF, SVG images | Qt image readers | PNG and GIF through the `image` crate. SVG subset painted directly | done |
| Camera codecs | `caCamera` | mono8 and rgb888. Other pack modes are not decoded | partial |
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

## What is still missing

No row is still marked not started. What remains is the depth inside the partial rows: the rest of each widget's `Q_PROPERTY` behavior (names already round-trip through the `.ui` reader), plot zoom and cursors, the soft IOC serving types other than `DBR_DOUBLE`, the PV Access client using NTScalar on the wire instead of the scalar body, and the parsers' less common object types. Excluded on purpose: Qt Designer plugin binaries, Python `%P/` execution, VNC/RFB, the HIPA/PRO/SF archive C libraries, OPC UA security beyond SecurityPolicy#None, and a linked ZeroMQ stack.
