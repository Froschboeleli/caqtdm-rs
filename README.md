# caQtDM, rewritten in Rust

A display manager for EPICS control screens. It reads Qt Designer `.ui` files and the MEDM `.adl`, EDM `.edl`, alarm-handler `.alh`, and PSI `.prc` formats, subscribes to process variables, and draws the widgets.

This is a Rust port of [caQtDM](https://github.com/caqtdm/caqtdm), written by Anton Mezger at the Paul Scherrer Institute and released under GPL-3.0-or-later. The upstream tree used as the reference is commit `87607049b9a9630fad8bef55656293509803f007`. Clone it into `upstream/` if you want the C++ sources beside this repo (that directory is gitignored). What is finished, partial, or missing is listed in [PARITY.md](PARITY.md).

License: GPL-3.0-or-later, the same terms as caQtDM. The full text is in [LICENSE](LICENSE). Faces in `crates/caqtdm-paint/fonts/` keep their own licenses: Arimo, Cousine, and Noto Sans are SIL Open Font License 1.1, and Droid Sans Fallback and WenQuanYi Micro Hei are Apache-2.0.

## Build

Rust 1.85 or newer (the committed `rust-toolchain.toml` selects stable).

```bash
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

Included displays are searched next to the parent file and in `CAQTDM_DISPLAY_PATH` (colon or semicolon separated).

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

Channel Access round-trips DBR types 0 through 34. The soft IOC serves every native type on create, read, write, and monitor, including waveforms. PV Access keeps the scalar get/put/monitor body, and `get_nt` / `put_nt` send `epics:nt/NTScalar:1.0` on the wire. Archive speaks Archiver Appliance `getData.json` and the SF JSON retriever. The HIPA and PRO archive follows `GetLogShift` in the public retrieval sources. `libNewLogRPC.a` is not in the tree, so the loopback frame only carries those arguments. The `libgmv` feature is on by default. `-print` writes the portrait A4 PDF `printPS` asks Qt 5 to produce and hands it to `lp`, `lpr`, or the Windows print verb. Display search fetches `http://` and `https://` (rustls). OPC UA speaks Hello, Acknowledge, Read, and OpenSecureChannel for Basic128Rsa15, Basic256, Basic256Sha256, Aes128_Sha256_RsaOaep, and Aes256_Sha256_RsaPss. Headless text uses Arimo or Cousine, then Noto Sans, Droid Sans Fallback, and a WenQuanYi subset for Hangul. A fully covered pixel is the widget color. Partial coverage is composited. That is not Qt's FreeType hinter. `-display` and `-displayFont` are accepted and ignored, matching upstream. RFB 3.8 and `-novnc` (WebSocket) serve the painted framebuffer. `-novnc_readonly` drops pointer and key events. A plot with a second Y scale draws a right-hand tick ruler. Modbus speaks function codes 1, 2, 3, 4, 5, 6, 15, and 16. bsread messages move as length-prefixed TCP, as ZMTP 3.0 frames (NULL, PLAIN, CURVE), and through `libzmq.so.5` when that library is present (tested against pyzmq). Images decode PPM, PNG, GIF, and a small SVG subset. Camera bytes decode mono8, rgb888, mono16, and rgba. Plot zoom windows both axes. Unrecognized MEDM and EDM objects are skipped. `caqtdm print` writes the portrait A4 PDF described above. `caqtdm -savetoimage` writes a PNG and does not take a filename. `caqtdm config` and `-attach` cover the operator file and the single-instance handshake. GPS starts at a fixed site and updates from NMEA GGA/RMC or a gpsd TPV/SKY watch. `-x` is accepted and does not change the display. See `PARITY.md` for the score and for the properties that are stored but not drawn.

## Known limitations

The HIPA and PRO archive plugin is reimplemented from the public `hipaRetrieval.c`, `proRetrieval.c`, and the two plugin headers. Those files call `gmv()` in the proprietary PSI library `libNewLogRPC` (`libNewLogRPC_64.a` on 64-bit). PSI never released that library or its source. The Rust code follows every step those public files define (the time window, `ExactTime`, the `Valerr` hold-last rule, and the server names the plugins set). The loopback frame only carries those arguments. It is not the library's wire protocol, so this build cannot talk to a real PSI archive server.

Closing that gap needs one of three things: the `libNewLogRPC` / `libNewLogRPC_64.a` binary, its source, or a wire-protocol specification. A capture of traffic from a real PSI archive server would also do, if the request and reply bytes can be matched to `GetLogShift`.

Without a real installation, two further checks stay unverified. Channel Access and PV Access are tested against the in-process soft IOC and loopback clients, not against live EPICS process variables on a site network. Display parsing and painting are tested against the samples in this repo and, when `upstream/` is present, against the upstream examples (`test.ui`, `graphics.edl`, `prcshowcase.prc`, and the `.ui` files in that tree). They have not been compared with a facility's production `.adl` or `.ui` set.
