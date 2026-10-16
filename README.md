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
cargo run -p caqtdm-app -- run examples/displays/vacuum.ui -macro SYS=VAC
```

`dump`, `validate`, `convert`, `paint`, and `widgets` do not open a window. `run` opens the egui viewer. `caShellCommand` is recorded and not executed unless you pass `--execute-shell`. `caScriptButton` never runs Python.

Channel names follow caQtDM: `intern://NAME`, `sim://sine:20:4000:40`, `env://VAR`, `pva://NAME`, `ca://NAME`, or a bare name on the default plugin (`-cs epics3` by default). `--ca HOST:PORT` and `--pva HOST:PORT` connect the scalar clients. Without those flags the EPICS plugins stay offline and the simulated plugins still run.

Included displays are searched next to the parent file and in `CAQTDM_DISPLAY_PATH` (colon or semicolon separated).

## Crates

| Crate | Role |
| --- | --- |
| `caqtdm-core` | macros, calc, colors, formats, channel names, values |
| `caqtdm-ui` | `.ui`, `.adl`, `.edl`, `.alh`, `.prc`, includes |
| `caqtdm-datasource` | plugin hub, intern, sim, env, gps, archive fixtures, modbus, opcua, bsread |
| `caqtdm-epics-ca` | Channel Access codec, soft IOC, scalar client (`epics3`) |
| `caqtdm-epics-pva` | PV Access header and scalar get/put/monitor (`epics4`) |
| `caqtdm-widgets` | widget runtime: alarms, visibility, writes, plots |
| `caqtdm-paint` | PPM rasterizer |
| `caqtdm-egui` | desktop viewer |
| `caqtdm-app` | `caqtdm` binary |
| `caqtdm-plugin` | C ABI for a calc plugin |
| `caqtdm-plugin-example` | sample plugin that adds one |

## Limits you will hit

Channel Access here is a scalar double soft server and client (search, create, CTRL read, write, time-double monitor). PV Access speaks the search frame and a documented scalar body, not pvxs `NTScalar` introspection. Archive, OPC UA, Modbus, GPS, and bsread are local stand-ins for the protocols, covered by tests where a loopback is practical. Image widgets decode PPM only.
