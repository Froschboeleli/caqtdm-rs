# caQtDM, rewritten in Rust

A display manager for EPICS control screens. It reads Qt Designer `.ui` files and the MEDM `.adl`, EDM `.edl`, alarm-handler `.alh`, and PSI `.prc` formats, subscribes to process variables, and draws the widgets.

This is a Rust port of [caQtDM](https://github.com/caqtdm/caqtdm) aimed at feature parity. The upstream tree used as the reference is commit `87607049b9a9630fad8bef55656293509803f007`. Clone it into `upstream/` if you want the C++ sources beside this repo (that directory is gitignored). What is finished, partial, or missing is listed in [PARITY.md](PARITY.md).

License: GPL-3.0-or-later, the same license as caQtDM.

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
cargo run -p caqtdm-app -- print examples/displays/vacuum.ui -macro SYS=VAC -o /tmp/vacuum.ppm
cargo run -p caqtdm-app -- config
cargo run -p caqtdm-app -- run examples/displays/vacuum.ui -macro SYS=VAC
```

`dump`, `validate`, `convert`, `paint`, `print`, `config`, and `widgets` do not open a window. `run` opens the egui viewer. `run -attach HOST:PORT` sends the file to a process that is already listening. `caShellCommand` is recorded and not executed unless you pass `--execute-shell`. `caScriptButton` never runs. A `%P/` visibility expression runs `PythonCalc` through `python3` when the `python` feature is on (it is on by default). Custom `.ui` classes register through `install_designer_plugin`; see [DESIGNER.md](DESIGNER.md).

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

Channel Access round-trips DBR types 0 through 34. The soft IOC serves every native type on create, read, write, and monitor, including waveforms. PV Access keeps the scalar get/put/monitor body, and `get_nt` / `put_nt` send `epics:nt/NTScalar:1.0` on the wire. Archive speaks Archiver Appliance `getData.json`, the HIPA and PRO `gmv()` argument block (the time words follow `cvtms_c` / `ExactTime`; `libgmv` itself is not in the upstream tree), and the SF JSON retriever. Display search fetches `http://` and `https://` (rustls). OPC UA speaks Hello, Acknowledge, Read, and OpenSecureChannel for Basic128Rsa15, Basic256, Basic256Sha256, Aes128_Sha256_RsaOaep, and Aes256_Sha256_RsaPss. Headless text uses Arimo or Cousine, then Noto Sans and Droid Sans Fallback for missing glyphs. A fully covered pixel is the widget color. Partial coverage is composited. That is not Qt's FreeType hinter. RFB 3.8 and `-novnc` (WebSocket) serve the painted framebuffer. `-novnc_readonly` drops pointer and key events. A plot with a second Y scale draws a right-hand tick ruler. Modbus speaks function codes 1, 2, 3, 4, 5, 6, 15, and 16. bsread messages move as length-prefixed TCP, as ZMTP 3.0 frames (NULL, PLAIN, CURVE), and through `libzmq.so.5` when that library is present (tested against pyzmq). Images decode PPM, PNG, GIF, and a small SVG subset. Camera bytes decode mono8, rgb888, mono16, and rgba. Plot zoom windows both axes. Unrecognized MEDM and EDM objects are skipped. `caqtdm print` writes a PPM. `caqtdm config` and `-attach` cover the operator file and the single-instance handshake. GPS starts at a fixed site and updates from NMEA GGA/RMC or a gpsd TPV/SKY watch. `-x` is accepted and does not change the display. See `PARITY.md` for the score and for the properties that are stored but not drawn.
