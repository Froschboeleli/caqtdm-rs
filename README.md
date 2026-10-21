# caQtDM, rewritten in Rust

A display manager for EPICS control screens. It reads Qt Designer `.ui` files and the MEDM `.adl`, EDM `.edl`, alarm-handler `.alh`, and PSI `.prc` formats, subscribes to process variables, and draws the widgets.

This is a Rust port of [caQtDM](https://github.com/caqtdm/caqtdm), written by Anton Mezger at the Paul Scherrer Institute and released under GPL-3.0-or-later. The upstream tree used as the reference is commit `87607049b9a9630fad8bef55656293509803f007`. Clone it into `upstream/` if you want the C++ sources beside this repo (that directory is gitignored). What is finished, partial, or missing is listed in [PARITY.md](PARITY.md).

License: GPL-3.0-or-later, the same terms as caQtDM. The full text is in [LICENSE](LICENSE). Faces in `crates/caqtdm-paint/fonts/` keep their own licenses: Arimo, Cousine, and Noto Sans are SIL Open Font License 1.1, and Droid Sans Fallback and WenQuanYi Micro Hei are Apache-2.0.

## Build

Rust 1.85 or newer (the committed `rust-toolchain.toml` selects stable). PyO3 0.30 is what the `python` feature links. It accepts CPython 3.9 through 3.15, including 3.14, so `cargo build --release` does not need `PYO3_USE_ABI3_FORWARD_COMPATIBILITY`. The feature is on by default.

```bash
cargo build --release
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
```

## Run

```bash
cargo run -p caqtdm-app -- dump examples/displays/vacuum.ui -macrodefs macros.txt
cargo run -p caqtdm-app -- validate examples/displays/vacuum.ui -macro SYS=VAC
cargo run -p caqtdm-app -- convert examples/displays/rf.edl
cargo run -p caqtdm-app -- paint examples/displays/vacuum.ui -macro SYS=VAC -o /tmp/vacuum.ppm
cargo run -p caqtdm-app -- print examples/displays/vacuum.ui -macro SYS=VAC -o /tmp/vacuum.ps
cargo run -p caqtdm-app -- examples/displays/vacuum.ui -savetoimage -o /tmp/vacuum.png
cargo run -p caqtdm-app -- config
cargo run -p caqtdm-app -- run examples/displays/vacuum.ui -macro SYS=VAC
```

`dump`, `validate`, `convert`, `paint`, `print`, `config`, and `widgets` do not open a window. `run` opens the egui viewer. `run -attach HOST:PORT` sends the file to a process that is already listening. `caShellCommand` is recorded and not executed unless you pass `--execute-shell`. `caScriptButton` never runs. A `%P/` visibility expression runs `PythonCalc` inside an embedded interpreter (pyo3) when the `python` feature is on, and falls back to `python3` if that fails. The feature is on by default. Custom `.ui` classes register through `install_designer_plugin`. `DisplayEditor` reloads a `.ui` or `.adl` and can replace one property without rewriting the rest of the file. See [DESIGNER.md](DESIGNER.md).

Channel names follow caQtDM: `intern://NAME`, `sim://sine:20:4000:40`, `env://VAR`, `pva://NAME`, `ca://NAME`, or a bare name on the default plugin (`-cs epics3` by default). `--ca HOST:PORT` and `--pva HOST:PORT` connect the scalar clients. Without those flags the EPICS plugins stay offline and the simulated plugins still run.

Display files are resolved the way `searchFile::findFile` does. The name is tried as given (the working directory, or an absolute path). If that file is not there, each directory in `CAQTDM_DISPLAY_PATH` is tried. The separator is `:` on Unix and `;` on Windows. The directory of the display that is already open is the last fallback, which is how an include sitting next to its parent is found after the path has been searched. `-url DIR` adds one more directory. The same search is used for the file you pass on the command line, for `caInclude`, and for a related display.

A missing include does not stop the display. The message is `sorry, could not load include file <name>`, and the rest of the file still opens. A missing `.adl` or `.edl` is tried again as `.ui`. Nested includes stop at 49 levels, or when a file includes itself.

```bash
CAQTDM_DISPLAY_PATH=/opt/displays:/opt/includes ./target/release/caqtdm S_OP_overview.ui
./target/release/caqtdm upstream/caQtDM_Tests/tests.ui
```

## Crates

| Crate | Role |
| --- | --- |
| `caqtdm-core` | macros, calc, colors, formats, channel names, values |
| `caqtdm-ui` | `.ui`, `.adl`, `.edl`, `.alh`, `.prc`, includes |
| `caqtdm-datasource` | plugin hub, intern, sim, env, gps, archive HTTP, modbus, OPC UA, bsread |
| `caqtdm-epics-ca` | Channel Access codec, DBR catalog, soft IOC, client (`epics3`) |
| `caqtdm-epics-pva` | PV Access search, scalar get/put/monitor, and NTScalar (`epics4`) |
| `caqtdm-widgets` | widget runtime: alarms, visibility, writes, plots |
| `caqtdm-paint` | PPM rasterizer |
| `caqtdm-egui` | desktop viewer |
| `caqtdm-app` | `caqtdm` binary |
| `caqtdm-plugin` | C ABI for a calc plugin |
| `caqtdm-plugin-example` | sample plugin that adds one |

## Limits you will hit

Channel Access round-trips DBR types 0 through 34. The soft IOC serves every native type on create, read, write, and monitor, including waveforms. PV Access keeps the scalar get/put/monitor body, and `get_nt` / `put_nt` send `epics:nt/NTScalar:1.0` on the wire. Archive speaks Archiver Appliance `getData.json` and the SF JSON retriever. The HIPA and PRO archive follows `GetLogShift` in the public retrieval sources. `libNewLogRPC.a` is not in the tree, so the loopback frame only carries those arguments. The `libgmv` feature is on by default. `-print` writes the portrait A4 PDF `printPS` asks Qt 5 to produce and hands it to `lp`, `lpr`, or the Windows print verb. Display search fetches `http://` and `https://` (rustls). OPC UA speaks Hello, Acknowledge, Read, and OpenSecureChannel for Basic128Rsa15, Basic256, Basic256Sha256, Aes128_Sha256_RsaOaep, and Aes256_Sha256_RsaPss. Headless text uses Noto Sans (regular and bold, the face a stock Linux desktop gives Qt), or Arimo or Cousine with a synthesized bold, then Droid Sans Fallback and a WenQuanYi subset for Hangul. A fully covered pixel is the widget color. Partial coverage is composited. That is not Qt's FreeType hinter. `-display` and `-displayFont` are accepted and ignored, matching upstream. RFB 3.8 and `-novnc` (WebSocket) serve the painted framebuffer. `-novnc_readonly` drops pointer and key events. A plot with a second Y scale draws a right-hand tick ruler. Modbus speaks function codes 1, 2, 3, 4, 5, 6, 15, and 16. bsread messages move as length-prefixed TCP, as ZMTP 3.0 frames (NULL, PLAIN, CURVE), and through `libzmq.so.5` when that library is present (tested against pyzmq). Images decode PPM, PNG, GIF, and a small SVG subset. Camera bytes decode mono8, rgb888, mono16, and rgba. Plot zoom windows both axes. Unrecognized MEDM and EDM objects are skipped. `caqtdm print` writes the portrait A4 PDF described above. `caqtdm -savetoimage` writes a PNG and does not take a filename. `caqtdm config` and `-attach` cover the operator file and the single-instance handshake. GPS starts at a fixed site and updates from NMEA GGA/RMC or a gpsd TPV/SKY watch. `-x` is accepted and does not change the display. See `PARITY.md` for the score and for the properties that are stored but not drawn.

## Known limitations

The HIPA and PRO archive plugin is reimplemented from the public `hipaRetrieval.c`, `proRetrieval.c`, and the two plugin headers. Those files call `gmv()` in the proprietary PSI library `libNewLogRPC` (`libNewLogRPC_64.a` on 64-bit). PSI never released that library or its source. The Rust code follows every step those public files define (the time window, `ExactTime`, the `Valerr` hold-last rule, and the server names the plugins set). The loopback frame only carries those arguments. It is not the library's wire protocol, so this build cannot talk to a real PSI archive server.

Closing that gap needs one of three things: the `libNewLogRPC` / `libNewLogRPC_64.a` binary, its source, or a wire-protocol specification. A capture of traffic from a real PSI archive server would also do, if the request and reply bytes can be matched to `GetLogShift`.

Channel Access and PV Access are tested against the in-process soft IOC and loopback clients, not against live EPICS process variables on a site network. Display parsing and painting are tested against the samples in this repo and, when `upstream/` is present, against the upstream examples. They have also been compared with the PSI SwissFEL operator overview (`S_OP_overview.ui` with its included displays, which are PSI-internal and not in this tree) against the original C++ caQtDM 4.9.0 built from the reference commit and grabbed under Xvfb without a control system. The headless render of that panel matches the Qt grab in window size, include stacking, z-order, visibility rules, fonts and fitted font sizes, label alignment, group-box title chips, LED gradients, check boxes, line-edit frames, caGraphics pens, and the Qwt canvas frames, scales, and ticks; what remains differs by one or two pixels (the overlap of two transparent archive plots, Noto Sans digit widths against the Arial substitute Qt picked, and sub-pixel glyph positions). Each behavior fixed on the way has a regression test on a synthetic display in this tree.

The original C++ caQtDM from the reference commit was built outside the source tree with Qt 5.15.13, Qwt 6.1.4 (`libqwt-qt5`), and Debian `epics-dev`. That package ships pvAccess headers without the pvAccess libraries, so the epics4 plugin was not linked. The viewer is `caQtDM` V4.9.0. `scripts/compare-displays.sh` paints the Rust display and, when `CAQTDM_BIN` and `LD_LIBRARY_PATH` point at that build, grabs the display window under Xvfb. The version splash is not the grab. Output goes to `artifacts/compare/` (gitignored).

That run wrote 39 side-by-side grabs. The window size matches for `tests.ui` (1090×756), `test.ui` (800×600), `numeric.ui` (1973×987), and `examples/displays/vacuum.ui` (640×360). `graphics.edl` crashes the Qt binary on this build, so it has a Rust snapshot only.

On that run, `tests.ui` opens in both programs at 1090×756. A missing include is a warning (`sorry, could not load include file recursive_bad.ui`) and the window still opens. The outer `currentIndex` is 1, so the monitors page is the one on screen. `QTabWidget::tab > QWidget > QWidget` fills that page with wheat `(245, 222, 179)`, which is the dominant color in both grabs. The central widget stays stylesheet blue `(58, 94, 171)` in the margin, `caLineEdit` is light yellow, and `caTable` is cornsilk. `examples/displays/vacuum.ui` is 640×360 in both. The Rust plot fills its widget with the plot background; Qwt leaves a gray margin and draws the trace inside the scale. A label with no `fontScaleMode` uses WidthAndHeight, so the Rust glyphs are larger than Qt's point size when the widget is tall.

What the Rust raster still does not match in Qt: FreeType hinting, stylesheet sub-controls (`::groove`, slider handles, `qlineargradient`), and the exact Fusion metrics of a tab bar. The page under a north tab bar is inset by 28 pixels and the chips are flat gray (`#d8d8d8`, selected `#fafafa`) with dark text. A long strip scrolls so the current tab stays visible. Inactive tab pages are hidden and do not enlarge the snapshot. A label with no `background` is transparent so the parent shows through.
