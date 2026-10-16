# Parity with caQtDM

Upstream is [caQtDM](https://github.com/caqtdm/caqtdm) at commit `87607049b9a9630fad8bef55656293509803f007` (see `UPSTREAM_REV`). A local clone lives in `upstream/` and is gitignored.

This checklist is the contract for the rewrite. A row is **done** when the Rust code covers the behavior a display author relies on and a test or sample exercises it. **partial** means a usable subset is implemented and the gaps are named. **not started** means there is no runtime behavior yet.

Score: done counts 1, partial counts 0.5, not started counts 0.

| | rows | weight |
| --- | ---: | ---: |
| done | 3 | 3 |
| partial | 71 | 35.5 |
| not started | 12 | 0 |
| total | 86 | 38.5 / 86 = **44.8%** |

Fully finished rows are 3 / 86 (3.5%). Most of the display manager is a working subset, not a line-by-line port of Qt, Qwt, or the EPICS C libraries.

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
| caSpinbox | Writable entry. No spinner buttons | partial |
| caByteController | Bit cells, least significant bit on the left. A click flips that bit | partial |
| caLabel | Text, colors, macro substitution | partial |
| caLabelVertical | Drawn as a normal label. Text is not rotated | partial |
| caFrame | Frame visual, children keep their geometry | partial |
| caGraphics | Rectangle, circle, arc, line, triangle, arrow. Outline vs filled | partial |
| caPolyLine | `xyPairs`, open or closed | partial |
| caImage | PPM P3/P6 via `filename`, stretched into the widget. No PNG, GIF, or SVG | partial |
| caInclude | `filename`, `numberOfItems`, stacking, per-item macros, depth limit 16 | partial |
| caDoubleTabWidget | Drawn as a frame. Tabs do not switch | partial |
| caClock | Digital `HH:MM:SS` from the tick time. Not the analog Qwt clock | partial |
| caBitnames | Bit cells. Names are not drawn per bit | partial |
| caLed | On when the value is non-zero, or when `bitNr` is set | partial |
| caLinearGauge | Horizontal fraction | partial |
| caCircularGauge | Same fraction, flagged circular for the viewer | partial |
| caLineEdit | Monitor text, alarm color, format types from `formattype.h` | partial |
| caMultiLineString | Shown as a label | partial |
| caThermo | Fraction bar, user or channel limits | partial |
| caMeter | Fraction | partial |
| caCartesianPlot | One sparkline per channel from the session history (240 samples) | partial |
| caStripPlot | Same history plot. No Qwt axes, cursors, or time span dialog | partial |
| caByte | Read-only bit cells | partial |
| caTable | Waveform values as a single column | partial |
| caWaveTable | Same table path | partial |
| caCamera | Placeholder text. No image codec | partial |
| caCalc | EPICS-style calc on channels A-L. `%P/` Python is refused | partial |
| caWaterfallPlot | Last 64 waveforms for the first channel | partial |
| caScan2D | Drawn with the cartesian history. No 2D image | partial |
| caLineDraw | History samples as a polyline | partial |
| caAlarmTree | `.alh` groups become frames and channel rows. Not a live alarm tree | partial |
| caAlarmLog | One row of description, severity, and value | partial |
| caShellCommand | Recorded. With `--execute-shell` it runs `sh -c` | partial |
| caScriptButton | Command is recorded and never executed | partial |
| caMimeDisplay | Opens like a related display. No MIME handler | partial |
| caHMIConfig | | not started |
| caReplaceMacro | Menu of `macroValues`. Choosing one sets `macroKey` and reloads the file | partial |
| caRowColMenu | Drawn as a button. Layout of child actions is not reproduced | partial |
| wmSignalRescale | | not started |

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
| archive | CA, HTTP, HIPA, PRO, SF retrievers | local `channel` / `time_ms,value,severity` fixtures. No network archive | partial |
| modbus | Modbus TCP | MBAP, function codes 3, 6, and 16. Address 40001 is register 0 | partial |
| opcua | OPC UA binary | in-memory node ids such as `ns=2;s=...` | partial |
| bsread | 0MQ bsread | simulated `pulse_id` and `:wf`. No ZeroMQ | partial |
| epics3 | Channel Access | scalar `DBR_DOUBLE` soft IOC: search, create, read CTRL, write, time-double monitor | partial |
| epics4 | PV Access / pvxs | search layout plus a scalar get/put/monitor body. Not NTScalar introspection | partial |

`epics-rs` is not a dependency. The CA and PVA crates speak the subsets above on the loopback.

## Viewer, CLI, and plugins

| Feature | Upstream | Rust | Status |
| --- | --- | --- | --- |
| Desktop viewer | Qt `caQtDM_Viewer` | egui/eframe: paint, click, drag slider, open related files, reload after a macro change | partial |
| Headless render | Qt grab / print | PPM rasterizer, 5x7 font, used by `caqtdm paint` | partial |
| CLI | `-display`, `-macro`, and many server flags | `dump`, `validate`, `convert`, `paint`, `widgets`, `run`, `-macro`, `-cs`, `--ca`, `--pva` | partial |
| Macro file | `-macrodefs` | one `KEY=VALUE` per line, then `-macro` overrides | partial |
| Shell policy | runs commands, web flag can allow them | dry-run unless `--execute-shell` | partial |
| HTTP / VNC / WebSocket | `-server`, `-novnc`, web launcher | | not started |
| Stylesheets | `-style`, `-stylefile` | | not started |
| Single instance | `-attach` | | not started |
| Qt Designer plugins | `caQtDM_QtControls/plugins` | classes are recognized in `.ui` files. No Designer binary | not started |
| Calc plugin ABI | Qt plugin loader plus Python | C ABI `caqtdm_plugin_abi` / `caqtdm_plugin_eval`, example cdylib adds one | partial |
| Font scaling | `fontscalingwidget` | | not started |
| Python visibility | `%P/` expressions | refused with `CalcError::Unsupported` | not started |
| PNG, GIF, SVG images | Qt image readers | | not started |
| Camera codecs | `caCamera` | | not started |
| Print | `-print` | | not started |
| Config dialog | `configDialog` | | not started |

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

The unfinished rows above, plus the depth inside every partial row: the rest of each widget's `Q_PROPERTY` set, Qwt plot axes and zoom, tab pages, rotated labels, real Channel Access arrays and the full DBR catalog, pvxs field descriptions, archive network protocols, OPC UA and Modbus beyond the tested subset, image codecs, and the web/VNC front end.
