# nrf24-driver

A lightweight, platform-independent Rust driver for nRF24L01+ compatible 2.4 GHz radio transceivers.

Part of the [LE ROVE](../README.md) railway simulation and physical train project.

## Overview

The goal of this crate is to provide a reusable, `no_std` driver for communicating with nRF24L01+ compatible RF modules over SPI.

The driver is designed to be independent of any specific microcontroller or hardware abstraction layer (HAL), using the [`embedded-hal`](https://github.com/rust-embedded/embedded-hal) traits.

Target platforms include:

- ESP32-C3 — initial development platform
- ESP32-S3
- Raspberry Pi Pico 2 W (RP2350)

**Hardware status:** The initial module is believed to be an nRF24L01+ PA/LNA variant with an external SMA antenna. Its exact chip identity has not yet been independently confirmed.

## Hardware

### RF module

The module uses a 2×4 pin connector with SPI, power and control signals.

![RF module — pin layout](media/rf1.jpeg)

![RF module — additional pin reference](media/rf2.jpeg)

### Pin descriptions

| Pin | Direction (relative to RF module) | Description |
|---|---|---|
| VCC | Power | 3.3 V supply |
| GND | Ground | Common ground |
| CE | Input | Chip Enable, controls RX/TX operation |
| CSN | Input | Active-low SPI chip select |
| SCK | Input | SPI clock |
| MOSI | Input | SPI data from microcontroller |
| MISO | Output | SPI data to microcontroller |
| IRQ | Output | Active-low interrupt, optional |

**Warning:** Do not connect VCC to 5 V. Verify the pinout before powering an unfamiliar module.

### Initial wiring — Seeed XIAO ESP32-C3

| RF module | XIAO ESP32-C3 | Wire color |
|---|---|---|
| VCC | 3V3 | Red |
| GND | GND | Black |
| CE | D2 / GPIO4 | Yellow |
| CSN | D3 / GPIO5 | Purple |
| SCK | D8 / GPIO8 | Blue |
| MOSI | D10 / GPIO10 | Orange |
| MISO | D6 / GPIO21 | Gray |
| IRQ | Not connected | — |

**Note:** MISO was originally connected to GPIO9, but this interfered with the ESP32-C3 boot process. Moving MISO to GPIO21 resolved the issue.

### Planned wiring — Raspberry Pi Pico 2 W (RP2350)

The station firmware (`station-rp2350`) will use the RP2350's SPI0 peripheral to communicate with the nRF24L01+ module.

| RF module | Pico 2 W | Function |
|---|---|---|
| VCC | 3V3 | 3.3 V power supply |
| GND | GND | Common ground |
| CE | GP20 | Radio enable |
| CSN | GP21 | Active-low chip select |
| SCK | GP18 | SPI0 clock |
| MOSI | GP19 | SPI0 TX |
| MISO | GP16 | SPI0 RX |
| IRQ | Not connected | Polling initially |

**Notes:**

- SPI0 will initially operate at 1 MHz, Mode 0.
- CE and CSN will be controlled through GPIO.
- GP16 is reserved for SPI0 RX and must not be used for the green LED from previous Pico experiments.
- Wiring has not yet been physically validated.
- The station will reuse the existing `nrf24-driver` crate through `embedded-hal` interfaces.

## Architecture

The driver will use `embedded-hal` interfaces for SPI and GPIO operations.

```text
          Train / Station Firmware
                    |
          +---------+---------+
          |                   |
   le-rove-protocol      nrf24-driver
   Encode / Decode       Radio control
          |                   |
          |              embedded-hal
          |                   |
          |            Platform-specific HAL
          |                   |
          +---------+---------+
                    |
              RF hardware
```

The driver must not depend on `le-rove-protocol`, `metro-core`, or a specific microcontroller HAL.

Its responsibility is to configure the transceiver and send or receive byte payloads. Application-level message encoding, decoding and railway behavior belong to other crates.

## Development status

### Completed hardware experiments

- [x] Power module from ESP32-C3 at 3.3 V
- [x] Initialize SPI at 500 kHz, Mode 0
- [x] Read `STATUS` register (`0x0E`)
- [x] Read `CONFIG` register (`0x08`)
- [x] Read `RF_CH` original value (`0x02`)
- [x] Write `RF_CH = 0x32` and verify readback
- [x] Restore `RF_CH = 0x02` and verify readback

These experiments confirm SPI register read/write behavior consistent with the nRF24L01+ register interface. They do not establish RF transmission or reception functionality.

### Planned driver milestones

- [x] Implement register read/write operations
- [x] Add host-side unit tests using mock SPI
- [ ] Integrate the driver with ESP32-C3 firmware
- [x] Configure RF operating parameters
- [x] Implement TX FIFO operations
- [ ] Transmit a test payload
- [x] Implement polling reception and RX status-flag handling
- [ ] Validate communication between two physical nodes

## First TX/RX test

Use `RadioConfig::new(50, [0xE7; 5])` and `initialize(&config, &mut delay)`
on **both** nodes, including the transmitter. This selects 1 Mbps, minimum TX
power, five-byte addresses, one-byte CRC, and dynamic payloads on pipe 0.
Initialization explicitly sets and verifies CRC and DPL configuration.
`initialize_tx()` is a legacy diagnostic path, not the initializer for the
current `transmit()` API.

The public receive API is:

```rust,ignore
pub fn receive(&mut self, buffer: &mut [u8])
    -> Result<Option<usize>, RxError<SPI::Error>>
```

After initialization, poll with a 32-byte buffer:

```rust,ignore
let mut buffer = [0u8; 32];
if let Some(len) = radio.receive(&mut buffer)? {
    // Print or decode precisely buffer[..len].
}
```

Each call consumes at most one complete payload, with no allocation or wait for
an RF packet. SPI transactions themselves are synchronous. Keep polling to drain
queued packets while also servicing USB CDC; do not wait for an IRQ edge.

- `Ok(None)`: no packet at the FIFO snapshot.
- `BufferTooSmall { required, available }`: buffer untouched, packet retained;
  retry with a larger buffer.
- `NotReady`: initialization/state/mode rejection without hardware access.
- `InvalidPayloadWidth(width)`: all queued RX packets discarded by recovery;
  the driver remains Ready/RX. Zero is also rejected by this 1–32-byte API.
- `Spi(error)`: the driver becomes Faulted with unknown mode, including errors
  while clearing RX_DR or recovering the FIFO. Do not process buffer contents
  after an error; a failed read may have partially modified them.

RX errors have no GPIO error parameter because receive does not operate CE.
Normal reception preserves CE and mode. Transmission preserves queued RX data.

### Datasheet review

The [Nordic specification](https://www.sparkfun.com/datasheets/Components/SMD/nRF24L01Pluss_Preliminary_Product_Specification_v1_0.pdf)
(§7.3.4, table 20, §8.4 and register-map note c) defines `R_RX_PL_WID=0x60`,
`R_RX_PAYLOAD=0x61`, and `FLUSH_RX=0xE2`. Widths over 32 require flushing.
The RX FIFO holds three packets. RX_DR is an arrival notification, cleared by
writing one; FIFO_STATUS determines whether unread packets remain. The driver
writes only RX_DR, preserving TX_DS/MAX_RT, and checks the FIFO on every call.
A racing arrival remains discoverable even if its notification was cleared.

**No ACK packets in the initial test:** DPL requires ENAA_P0 enabled.
As [Nordic explains](https://devzone.nordicsemi.com/f/nordic-q-a/99192/using-nrf24l01-with-pure-assembler-programing),
use EN_DYN_ACK and `W_TX_PAYLOAD_NOACK=0xB0` to suppress acknowledgments while
retaining dynamic framing. This driver now configures that combination.
`RadioConfig::auto_ack()` remains false: transmitted packets request no ACK,
although EN_AA is nonzero. Both peers must use this configuration.

No `ACTIVATE 0x73` fallback is implemented: [Nordic confirms that genuine
nRF24L01+ ignores ACTIVATE](https://devzone.nordicsemi.com/f/nordic-q-a/29601/nrf24l01-activate-command).
FEATURE/DYNPD readback failure aborts initialization. The module's actual silicon
identity and DPL behavior still require physical verification; no undocumented
clone support is assumed.

FLUSH_RX must not interrupt an ACK transmission. The test uses only NO_ACK
frames from these peers. Receiving ACK-requesting traffic is outside this test
configuration; supporting it needs review of recovery timing before enabling it.

### Integration and hardware limitations

The driver API is sufficient to send an encoded Le Rove frame of 1–32 bytes and
receive its exact bytes. Firmware and protocol files were deliberately untouched.
The existing firmware still needs these integration changes before a two-board run:

- `train-esp32c3`: replace `initialize_tx()` with `initialize()`.
- `station-rp2350`: replace the obsolete `TxConfig` import/use with `RadioConfig`,
  initialize RX, and add receive polling plus USB CDC output.

Use matching channel, address, data rate and payload configuration. Allow the
module supply to stabilize and complete power-on reset before initialization;
its internal 2 ms delay covers radio power-up, not board power sequencing.
Use SPI Mode 0 (start at 1 MHz or below), a properly managed CSN, common ground,
and stable 3.3 V power/decoupling suitable for the PA/LNA module. Wiring, module
identity, supply integrity, and actual over-air reception have not been validated
by these software tests. A successful no-ACK TX does not prove delivery.

## Design principles

- **`no_std`** — No standard-library dependency in the driver
- **Portable** — Compatible with multiple microcontroller HALs
- **Allocation-free** — Fixed-size buffers, no heap required
- **Hardware-independent** — No direct dependencies on `esp-hal` or `rp235x-hal`
- **Testable** — SPI and GPIO interactions can be tested with mocks
- **Small API** — Introduce functionality incrementally as hardware experiments require it

## Building

Run host-side checks from the crate directory:

```bash
cargo check
cargo test
```

Embedded firmware builds are managed by their respective consuming projects, such as `train-esp32c3`.

## License

To be determined.