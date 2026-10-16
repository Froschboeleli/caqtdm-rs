# Parity with caQtDM

Upstream is [caQtDM](https://github.com/caqtdm/caqtdm) at commit `87607049b9a9630fad8bef55656293509803f007` (see `UPSTREAM_REV`). A local clone lives in `upstream/` and is gitignored.

This checklist is the contract for the rewrite. A row is **done** when the Rust code covers the behavior a display author relies on and a test or sample exercises it. **partial** means a usable subset is implemented and the gaps are named. **not started** means there is no runtime behavior yet.

Score: done counts 1, partial counts 0.5, not started counts 0. **excluded** rows are left out of the total. An exclusion is a behavior this rewrite will not grow: it is named in the row, with the reason.

| | rows | weight |
| --- | ---: | ---: |
| done | 64 | 64 |
| partial | 20 | 10 |
| not started | 0 | 0 |
| excluded | 2 | not scored |
| total | 84 scored | 74 / 84 = **88.1%** |

Fully finished rows are 64 / 84 (76.2%). Two rows are excluded and are not in that total. A done row is behavior a display author relies on, exercised by a test or a sample. A single assertion that the value is stored is not enough when the painted or written result is what the author sees.

## Widgets

Designer-only helpers (task menus, extension factories, elevation properties) are not separate rows. The runtime reads the `.ui` file. It does not host Qt Designer.

| Widget | Rust | Status |
| --- | --- | --- |
| caNumeric | Exponential format and user precision. The entry is marked writable, and that write is not asserted | partial |
| caApplyNumeric | A number is held until the widget is pressed, then written | done |
| caSlider | Fraction from user or channel limits. `orientation` Down or Left inverts it. A drag writes `min + fraction * (max-min)` | done |
| caMenu | Enum strings or `items`. A choice writes the index | done |
| caChoice | Items are painted as text rows and a choice uses the menu write path. Not a column of buttons, and stacking is not asserted | partial |
| caRelatedDisplay | Opens `files` / `labels`, including ADL `label0`/`name0` and EDM `displayFileName` | done |
| caTextEntry | Writable text. Numeric text is written as a double | done |
| caMessageButton | Writes `pressMessage` or `pressValue` | done |
| caToggleButton | Toggles 0/1 | done |
| caSpinbox | Press adds `incrementValue` (default 1) to the channel | done |
| caByteController | Bit cells, least significant bit on the left. A click flips that bit | done |
| caLabel | Text, colors, macro substitution. Upstream `test.ui` and `graphics.edl` are painted | done |
| caLabelVertical | Text is drawn downward. `direction` Up/Down and `tiltAngle` set the rotation | done |
| caFrame | Frame visual. Children keep their geometry | done |
| caGraphics | Rectangle outline and circle form are tested. Arc, triangle, and arrow paint as a rectangle | partial |
| caPolyLine | `xyPairs`, open or closed. The `graphics.edl` polyline vertex is painted | done |
| caImage | PPM, PNG, GIF, and a small SVG (`rect`, `circle`, `line`) via `filename` | done |
| caInclude | `filename`, `numberOfItems`, stacking, per-item macros, depth limit 16 | done |
| caDoubleTabWidget | Tab labels from `items` / `itemsHorizontal` / `itemsVertical`. A choice selects the page | done |
| caClock | Digital `HH:MM:SS` from the tick time. Analog is a circle, not clock hands | partial |
| caBitnames | Bit cells. `trueStrings` or `names` are drawn in the cells | done |
| caLed | On when the value is non-zero, or when `bitNr` is set | done |
| caLinearGauge | Horizontal fraction from user or channel limits. The bar fill is not checked in a raster test | partial |
| caCircularGauge | Same fraction, and the rasterizer draws a needle. The needle pixel is not checked | partial |
| caLineEdit | Monitor text, alarm color, and the `formattype.h` formats | done |
| caMultiLineString | Label text, including line breaks | done |
| caThermo | Fraction bar from user or channel limits. No alarm bands, and the bar pixel is not checked | partial |
| caMeter | Fraction drives a needle. The needle pixel is not checked | partial |
| caCartesianPlot | Traces from `channel` and `channels`, titles, Y limits, X ticks. Zoom windows both axes, and a paint test checks both. A cursor is a vertical line. No log scale and no second Y axis | partial |
| caStripPlot | Same trace list. `timeSpan` keeps the last N samples. Zoom and cursor are the cartesian actions. The strip window is not painted in a test | partial |
| caByte | Read-only bit cells | done |
| caTable | Waveform values as a single column | partial |
| caWaveTable | Same single-column path as caTable | partial |
| caCamera | `channelData` bytes as mono8, rgb888, mono16, or rgba, sized by width and height. rgb888 and mono16 are tested | done |
| caCalc | EPICS-style calc on channels A-L. `%P/` Python is refused | done |
| caWaterfallPlot | Last 64 waveforms for the first channel. The test only checks that a row exists | partial |
| caScan2D | A waveform that matches `channelWidth` by `channelHeight` is a grayscale image. Otherwise a plot | done |
| caLineDraw | History samples as a polyline. Pen width and color mode are ignored | partial |
| caAlarmTree | `.alh` groups become frames. Channel rows show the live severity. Not an expandable tree | partial |
| caAlarmLog | A row is appended when a monitored severity changes | done |
| caShellCommand | Recorded. With `--execute-shell` it runs `sh -c` | done |
| caScriptButton | Command is recorded and never executed | done |
| caMimeDisplay | Opens `files` / `labels` the same way as a related display | done |
| caHMIConfig | Press writes `valueOrCalc` to `outputA`. `calculationType` Calc evaluates the expression | done |
| caReplaceMacro | Menu of `macroValues`. Choosing one sets `macroKey` and reloads the file | done |
| caRowColMenu | `labels` are a menu. Choosing one writes the index. Not a row and column of child widgets | partial |
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
| gps | receiver plugin | UTC from the tick is tested. Coordinates and satellite count are computed and not asserted. No receiver hardware | partial |
| archive | CA, HTTP, HIPA, PRO, SF retrievers | Archiver Appliance `getData.json`. HIPA and PRO send the `gmv()` argument block (`hipa-lgexp.psi.ch`, `proscan-lgexp.psi.ch`) and apply the `Valerr` hold-last rule. SF posts the plugin JSON and reads `globalSeconds` / `value.mean`. The unpublished `libgmv` codec is not in the upstream tree | done |
| modbus | Modbus TCP | MBAP and function codes 1, 2, 3, 4, 5, 6, 15, and 16. Address 40001 is register 0 | done |
| opcua | OPC UA binary | Hello/Acknowledge, a Read of a numeric NodeId, and Basic256Sha256 OpenSecureChannel: X.509, RSA-OAEP-SHA1, RSA-SHA256, then P_SHA256 into the AES-256-CBC and HMAC-SHA256 chunk. Other policies are not implemented | done |
| bsread | 0MQ bsread | Three-part bsread on length-prefixed TCP, and as ZMTP 3.0 frames with NULL, PLAIN, and CURVE. CURVE is the CurveZMQ HELLO/WELCOME/INITIATE/READY/MESSAGE exchange. Not a link to libzmq | done |
| epics3 | Channel Access | DBR types 0..34 round-trip. The soft IOC serves string, short, float, enum, char, long, and double on create, read, write, and monitor, including waveforms | done |
| epics4 | PV Access / pvxs | Search, scalar get/put/monitor, and `get_nt` / `put_nt` carrying `epics:nt/NTScalar:1.0` on the wire | done |

`epics-rs` is not a dependency. The CA and PVA crates speak the subsets above on the loopback.

## Viewer, CLI, and plugins

| Feature | Upstream | Rust | Status |
| --- | --- | --- | --- |
| Desktop viewer | Qt `caQtDM_Viewer` | egui/eframe: paint, click, drag slider, tabs, menus, related files, macro reload. A plot drag sets both zoom axes and a click places a cursor. No test drives the egui event | partial |
| Headless render | Qt grab / print | PPM rasterizer and a 5x7 font. Plot zoom on both axes and the upstream `graphics.edl` pixels are tested. Widget text is not Qt metrics | partial |
| CLI | `-display`, `-macro`, and many server flags | `dump`, `validate`, `convert`, `paint`, `print`, `widgets`, `config`, `run`, `-macro`, `-cs`, `--ca`, `--pva`, `-config`, `-attach`. The rest of the server flags are absent | partial |
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

Twenty scored rows are partial. The gaps are in those rows. These limits also apply to rows marked done:

- Qt Designer plugin binaries, Python `%P/` execution, and VNC/RFB are excluded.
- HIPA and PRO speak the `gmv()` argument block. The site `libgmv` library is not in the upstream tree, so that codec is not reproduced.
- OPC UA security here is Basic256Sha256 OpenSecureChannel plus the symmetric chunk. The other policies in `opcua_core.cpp` are not implemented.
- bsread ZeroMQ is a ZMTP 3.0 peer (NULL, PLAIN, CURVE), not a link to libzmq.
- GPS is a simulated clock, not a receiver.
- Plot zoom windows both axes for one Y scale. There is no log axis.
- MEDM and EDM objects that upstream does not emit are skipped. They are not turned into a stand-in widget.
