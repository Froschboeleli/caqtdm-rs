# Parity with caQtDM

Upstream is [caQtDM](https://github.com/caqtdm/caqtdm) at commit `87607049b9a9630fad8bef55656293509803f007` (see `UPSTREAM_REV`). A local clone lives in `upstream/` and is gitignored.

This checklist is the contract for the rewrite. A row is **done** when the Rust code covers the behavior a display author relies on and a test or sample exercises it. **partial** means a usable subset is implemented and the gaps are named. **reconstructed** counts the same as partial: the behavior is implemented from public calls and comments, and it is not verified against code that is absent from the upstream tree. **not started** means there is no runtime behavior yet.

Score: done counts 1, partial and reconstructed count 0.5, not started counts 0. **excluded** rows are left out of the total. An exclusion is a behavior this rewrite will not grow: it is named in the row, with the reason.

| | rows | weight |
| --- | ---: | ---: |
| done | 84 | 84 |
| partial | 5 | 2.5 |
| reconstructed | 1 | 0.5 |
| not started | 2 | 0 |
| excluded | 0 | not scored |
| total | 92 scored | 87 / 92 = **94.6%** |

Fully finished rows are 84 / 92. The weighted score is 87 / 92 (94.6%). A done row is behavior a display author relies on, exercised by a test or a sample. A single assertion that the value is stored is not enough when the painted or written result is what the author sees. Reconstructed counts as one half, same as partial, and is not called done.

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
| caCartesianPlot | Traces from `channel`, `channels`, and `channels_1`..`channels_6`. Zoom windows both axes. `YAxisType` log10 places the sample with log10. The second curve uses `YaxisLimits` pair 2 and a right-hand tick ruler. `Style_N`, `grid`, `gridColor`, `XaxisEnabled`, `YaxisEnabled`, and `scaleColor` change the picture. User or Channel `XaxisLimits` place the sample index in data coordinates. `XaxisType` log10 does the same on a log axis. Omitted `color_N` uses the constructor palette (white, red, yellow, blue, green, magenta). Paint tests check the log pixel, the second curve, a tick, the grid, a user X limit, and log10 X | done |
| caStripPlot | Same traces, log scale, and the right-hand second Y ruler. `timeSpan` keeps the last N samples, and a paint test shows only that window | done |
| caByte | Read-only bit cells | done |
| caTable | One column per channel (`channel`, `channels`, `channels_1`..`channels_6`). A two-channel scalar row is asserted | done |
| caWaveTable | `numberOfRows` by `numberOfColumns`, row-major. A 2x2 grid with user precision 0 is asserted | done |
| caCamera | `channelData` bytes as mono8, rgb888, mono16, or rgba, sized by width and height. rgb888 and mono16 are tested. A gray frame is recolored by `ColorMap`. An RGB frame is left alone | done |
| caCalc | EPICS-style calc on channels A-L. `%P/` runs `PythonCalc(a, b, c, d)` when the `python` feature is on | done |
| caWaterfallPlot | Last N waveforms (`NumberOfTraces`, capped at 64). `ColorMap` is grey, wavelength, hot, heat, jet, or custom stops. Heat and grey pixels are checked. `IntensityScalingMin` / `IntensityScalingMax` are stored; upstream never reads them while drawing | done |
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
| gps | receiver plugin | Default site until a fix arrives (`lat` 47.3769, `lon` 8.5417, `alt` 408, `sats` 8). `ingest_nmea` reads GGA/RMC (`ddmm.mmm`). `ingest_gpsd` reads a TPV or SKY object. `exchange_gpsd` sends `?WATCH=` and returns the first TPV. A write to `lat` is still polled back | done |
| archive | CA, HTTP, HIPA, PRO, SF retrievers | Archiver Appliance `getData.json` and the SF JSON retriever are exercised on the loopback. HIPA and PRO send the `gmv()` argument block and apply the `Valerr` hold-last rule. `pack_log_time` / `exact_time` follow `cvtms_c` / `ExactTime`. The feature `libgmv` (on by default) gates that codec. It is reconstructed from `GetLogShift`, `cvtms_c`, and `ExactTime`. It is not a verified copy of the site library, which is not in the upstream tree | reconstructed |
| modbus | Modbus TCP | MBAP and function codes 1, 2, 3, 4, 5, 6, 15, and 16. Address 40001 is register 0 | done |
| opcua | OPC UA binary | Hello/Acknowledge, a Read of a numeric NodeId, and OpenSecureChannel for Basic128Rsa15, Basic256, Basic256Sha256, Aes128_Sha256_RsaOaep, and Aes256_Sha256_RsaPss (RSA-PSS-SHA256, RSA-OAEP-SHA256, AES-256-CBC, HMAC-SHA256, P_SHA256). A test derives matching keys and round-trips a symmetric chunk for each policy | done |
| bsread | 0MQ bsread | Three-part bsread on length-prefixed TCP, as ZMTP 3.0 frames (NULL, PLAIN, CURVE), and through the system `libzmq.so.5` PUSH/PULL/PUB/SUB ABI. A pyzmq PULL reads a Rust PUSH, and a pyzmq PUSH is read by a Rust PULL | done |
| epics3 | Channel Access | DBR types 0..34 round-trip. The soft IOC serves string, short, float, enum, char, long, and double on create, read, write, and monitor, including waveforms | done |
| epics4 | PV Access / pvxs | Search, scalar get/put/monitor, and `get_nt` / `put_nt` carrying `epics:nt/NTScalar:1.0` on the wire | done |

`epics-rs` is not a dependency. The CA and PVA crates speak the subsets above on the loopback.

## Viewer, CLI, and plugins

| Feature | Upstream | Rust | Status |
| --- | --- | --- | --- |
| Desktop viewer | Qt `caQtDM_Viewer` | egui/eframe: paint, click, drag slider, tabs, menus, related files, macro reload. `action_for` is what the pointer handler calls. A test drives a slider drag, a plot click, a plot drag (both zoom axes), a tab, a menu row, and hit-testing | done |
| Headless render | Qt grab / print | PPM rasterizer. Text uses fontdue. Arimo and Cousine are the selected faces. Noto Sans, Droid Sans Fallback, and a WenQuanYi Micro Hei subset supply Greek, Cyrillic, CJK, and Hangul when the selected face has no glyph. A fully covered pixel is the widget color. Partial coverage is composited onto the pixel already there. Plot zoom, log scale, and the upstream `graphics.edl` pixels are tested | done |
| CLI | `-display`, `-macro`, and many server flags | The upstream flags, including `-x`, `-display`, and `-displayFont` (each accepted and ignored), `-?`, `-slave_server`, `-novnc_readonly` (requires `-novnc`), `-host` (a non-address is ignored), and `-server_port` 65535 mapping the web port to 65534. `-savetoimage` takes no filename and writes PNG. `-print` writes PostScript. `-o` sets the path. The display file is the positional argument | done |
| Macro file | `-macrodefs` | one `KEY=VALUE` per line, then `-macro` overrides | done |
| Shell policy | runs commands, web flag can allow them | dry-run unless `--execute-shell`. `caScriptButton` never runs | done |
| HTTP / WebSocket | `-server`, web launcher | `GET /` and one RFC 6455 text frame on a local port. `-novnc` also speaks RFB 3.8 (raw encoding, security None) on TCP and the same bytes in WebSocket binary frames. `-novnc_readonly` drops pointer and key events. A test reads a red pixel off the RFB socket and checks the version string `RFB 003.008` | done |
| Stylesheets | `-style`, `-stylefile` | `color`, `background-color`, and `font-size` on a widget `styleSheet` | done |
| Single instance | `-attach` | TCP handshake. `run -attach HOST:PORT` sends the display path and does not open a second window | done |
| Qt Designer plugins | `caQtDM_QtControls/plugins` | Qt `.so` plugins are not loaded. `install_designer_plugin` registers a custom class and paints it as a label, LED, frame, or gauge. See `DESIGNER.md`. A registered lamp is painted and the center pixel is checked | done |
| Calc plugin ABI | Qt plugin loader plus Python | C ABI `caqtdm_plugin_abi` / `caqtdm_plugin_eval`, example cdylib adds one | done |
| Font scaling | `fontscalingwidget` | `fontScaleMode` Height, Width, and HeightAndWidth, plus stylesheet `font-size` | done |
| Python visibility | `%P/` expressions | With the default `python` feature, pyo3 embeds CPython and calls `PythonCalc(a, b, c, d)`. If that fails, the same body is sent to `python3`. Nonzero is visible. A function with another name is an error. Without the feature the same text is `CalcError::Unsupported` | done |
| PNG, GIF, SVG images | Qt image readers | PNG and GIF through the `image` crate. SVG subset painted directly | done |
| Camera codecs | `caCamera` | mono8, rgb888, mono16, and rgba. Tests check rgb888 and mono16. `ROI_readType` / `ROI_readmarkerType` draw the marker from `ROI_readChannels`. A drag writes `ROI_writeChannels` with the `Callback_WriteDetectedValues` layout. `caScan2D` joins `channelSAVEDATA_PATH`, `SUBDIR`, and `FILENAME` and loads the 2D MDA detector column | done |
| Print | `-print` | `caqtdm print` writes a raster PostScript page (`%!PS-Adobe-3.0`, `colorimage`). Qt 5 `printPS` uses `QPrinter::NativeFormat` and says PostScript is no longer supported, so there is no upstream PostScript byte stream to match | partial |
| Config dialog | `configDialog` | `caqtdm config` and `-config FILE` load and print control system, macros, CA, and PVA | done |
| Snapshot | `-savetoimage` | Flag with no filename. Writes PNG (`CAQTDM_SCREENSHOT_NAME` or `caQtDM.png`) and stamps `yyyy-MM-dd HH:MM:SS, file` at the top. The stamp is UTC. The PNG is not `QWidget::grab` bytes | partial |
| Designer edit | Qt Designer save | `DisplayEditor` reloads a `.ui` or `.adl` and writes the same bytes back. `set_ui_string` / `set_adl_field` replace one value and leave the surrounding bytes. Upstream `test.ui` round-trips. Qt Designer is not in the tree, and the viewer does not write `.ui`, so a Designer resave is not compared | partial |
| Plot symbols | `QwtSymbol` | `symbol_N` draws a geometric mark. `Rect` is an ellipse, matching `myMarker`. Size is `max(2, round(height * symbolResizeFactor / 50))`. A cross, that ellipse, and the empty diagonal are checked. The mark is not Qwt's antialiased pixmap | partial |
| Time axis | `XaxisType::time` | A UTC `hh:mm:ss` stamp is drawn under the axis and a pixel test sees it. Upstream uses Qt local time and `hh:mm:ss` plus `dd-MM-yy` | partial |
| Plot channels | X and Y vectors, `plotMode`, count, trigger, erase | Scalar history is the Y sample and the index is X. Separate X and Y channels, `PlotNPointsAndStop`, `countNumOrChannel`, `triggerChannel`, and `eraseChannel` are not applied | not started |
| Strip time base | `refreshRate`, `period`, `XaxisSyncGroup`, archive backfill | `timeSpan` keeps the last N samples. The strip does not poll on `refreshRate`, share an X group, or backfill from the archive | not started |

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

## Remaining differences

The table above is the score. This list is the same set of open items, plus limits that sit on a row already marked done and are not scored a second time.

**reconstructed.** HIPA and PRO `libgmv`. The codec follows `GetLogShift`, `cvtms_c`, and `ExactTime`, behind the `libgmv` feature. The site library is not in the upstream tree. That is the only gap that needs non-public code.

**partial.** Print is a raster PostScript page, not `QPrinter` bytes. Snapshot PNG is not `QWidget::grab`, and the caption is UTC. `DisplayEditor` preserves bytes around one edited property; it is not Qt Designer's writer. Plot symbols are geometric, and `Rect` follows `myMarker` by drawing an ellipse; they are not Qwt's antialiased marks. A time axis stamps UTC `hh:mm:ss` and does not draw Qt's local `dd-MM-yy` line. The cartesian grid is a 3-on, 2-off dash, not Qt's `DashLine` pixmap. The plot canvas stays the widget background. The constructor's black canvas, white foreground, and black scale color apply in Qt when a `.ui` omits them.

**not started.** A cartesian trace does not yet take a separate X channel, `plotMode`, `countNumOrChannel`, `triggerChannel`, or `eraseChannel`. A strip plot does not honor `refreshRate`, `period`, `XaxisSyncGroup`, or an archive backfill. `timeSpan` does limit the window.

**not a gap.** `IntensityScalingMin` and `IntensityScalingMax` are stored and upstream never reads them while drawing. MEDM and EDM objects that upstream does not emit are skipped, not replaced with a stand-in. `-x`, `-display`, and `-displayFont` are accepted and ignored, matching upstream's help text. A `.ui` property that only Designer keeps, and that no paint path reads, stays stored and does not change a pixel.

Glyph coverage on done text rows is Arimo, Cousine, Noto Sans Regular, Droid Sans Fallback, and a WenQuanYi Micro Hei subset for Hangul. A pixel at coverage 255 is the widget color. Lower coverage is composited. That is not Qt's FreeType hinter, and it is not every Unicode plane.
