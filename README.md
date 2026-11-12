# EC-Sim

> **Software EtherCAT slave simulator** — run a virtual EtherCAT segment with no hardware.
> Unmodified **IgH EtherCAT Master** and **SOEM** masters discover it and reach **OP**; the built-in ESC core does the dirty work, plugins extend the device catalog.
> 工作名 EC-Sim（最终产品名待定；**产品名与域名不含 "EtherCAT"**）。

**Status:** early development (v0.0.x, pre-alpha). **Latest pre-release: [v0.0.1 — wiring-layer risk closed](https://github.com/archerding/ec-sim/releases/tag/v0.0.1)** (no binaries yet) — see [roadmap](#roadmap).

![status](https://img.shields.io/badge/status-pre--alpha-orange)

---

## Why

EtherCAT slave development without hardware is painful: you need an ESC chip, an EEPROM burn, a master, and 5–15 minutes per debug cycle. Existing options stop at process-data level (IgH's own `libfakeethercat` cannot simulate a bus error) or are locked to a vendor's master and Windows (TwinCAT, acontis).

This project is **a virtual bus + a plugin host + an AI-operable test infrastructure** — not another protocol stack.

## What works today (spikes, with evidence)

| Capability | Status | Evidence |
|---|---|---|
| L2 TAP carries EtherType `0x88A4` both directions | ✅ | [spike-1](docs/spikes/spike-1-tap-bidirectional.md) |
| **Unmodified SOEM** binds the TAP and sends real frames | ✅ | [spike-2](docs/spikes/spike-2-masters-on-tap.md) |
| **Unmodified IgH** (`ec_generic`) binds the TAP, `/dev/EtherCAT0` appears, ~496 frames/s | ✅ | [spike-2](docs/spikes/spike-2-masters-on-tap.md) |
| Virtual slave becomes visible to a master (`ethercat slaves` ≥ 1) | 🚧 in progress | — |

## Architecture (5 layers)

```
L5 control plane   CLI · Web UI · MCP server · Agent Skills · marketplace client
L4 plugin host     declarative device plugins (manifest) + WASM behavior plugins
L3 object dict     ESI → SII + OD + PDO mapping; SDO / CA / SDO Info
L2 ESC core        registers · FMMU · SM · AL state machine · EEPROM · DC · watchdog · mailbox
L1 wire            Linux TAP (flagship) · ESC-over-UDS shim (all platforms / CI)
```

Design rules:
1. ESC and CoE are **built in**; plugins only add object-dictionary declarations and application logic.
2. Plugins **never enter the real-time loop**.
3. One ESC core, two transports — network-level and port-level share identical behavior.

Read the decision record: [technology selection](ethercat-slave-simulator-tech-selection.md).

## Plugin ecosystem

- **Declarative first**: most device plugins are a `plugin.yaml` manifest (identity, SM, PDO, object dictionary) plus a `facts.md` with sources — no code required.
- **Behavior when needed**: CiA402 state machines and vendor-specific protocols run as sandboxed WASM plugins.
- **Marketplace-ready**: closed-source, paid plugins are first-class (Ed25519 signatures, entitlement, online revocation).
- **Reproducible evidence**: every probe script and its last verified result is indexed in [docs/VERIFICATION.md](docs/VERIFICATION.md).
- **Legal discipline**: plugins are *independent re-implementations from public documentation*. No vendor ESI/PDF/firmware is redistributed, identity uses our own Vendor ID, and nothing claims conformance or certification. See [docs/DEVICE-PLUGINS.md](docs/DEVICE-PLUGINS.md).

Starter catalog: [plugins/catalog.yaml](plugins/catalog.yaml).

## Roadmap

- **M1** minimal closed loop: ESC core (registers/SM/FMMU/AL/EEPROM) + CoE expedited SDO + UDS shim + CLI → `ethercat slaves` and `slaveinfo` see the virtual slave.
- **M2** network-level transport (TAP) + Web UI (topology, AL state machine, PDO, frame decode).
- **M3** plugin host (declarative + WASM) + signed packages.
- **M4** MCP server + Agent Skills + test reports.
- **M5** marketplace + LinuxCNC template market, fault injection matrix.

## Repository conventions

Contributors and AI agents: read **[AGENTS.md](AGENTS.md)** first. It defines commit identity, licensing rules, the truthfulness rules, and the shift/backlog protocol. Operational details (servers, credentials) are intentionally **not** stored in this repository.

## Trademark & disclaimer

This is an independent, unofficial project. It is **not affiliated with, endorsed by, or certified by** Beckhoff Automation GmbH, the EtherCAT Technology Group, or any device vendor.
EtherCAT® is a registered trademark of Beckhoff Automation GmbH, Germany.
IgH EtherCAT Master is a trademark of Ingenieurgemeinschaft IgH. LinuxCNC is a trademark of the LinuxCNC project. All other trademarks are property of their respective owners.

Nothing here claims EtherCAT conformance or certification.

## License

Licensed under **Apache-2.0 OR MIT**, at your option. See [LICENSE](LICENSE), [LICENSE-APACHE](LICENSE-APACHE), [LICENSE-MIT](LICENSE-MIT).