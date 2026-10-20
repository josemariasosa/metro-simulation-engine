# nrf24-driver

A portable, allocation-free `no_std` Rust driver for nRF24L01+ transceivers.
It uses `embedded-hal` 1.0 traits and does not depend on a particular MCU HAL.
The driver is part of the [Le Rove project](../README.md), but this crate
transmits and receives raw byte payloads; application framing belongs to the
consumer.

## Overview

The public workflow is receive-first. [`Nrf24::initialize`] configures the
radio and starts listening on RX pipe 0. [`Nrf24::transmit`] synchronously
switches RX → TX → RX for one payload; on success it returns with RX listening
again. [`Nrf24::receive`] polls FIFO status and reads at most one payload into
a caller-owned buffer. It does not wait for a packet or use the IRQ pin.

Initialization fixes the address width to five bytes, CRC to one byte, and
dynamic payloads to 1–32 bytes on pipe 0. TX always uses the nRF24L01+'s
selective NO_ACK command: it requests no acknowledgment and performs no
automatic retransmission. The driver still enables pipe-0 auto-ack in the
receiver configuration because the radio requires EN_AA for dynamic payload
lengths. These are separate settings: receiving an ACK-requesting packet may
still cause an ACK even though this driver's own outgoing packets use NO_ACK.

All SPI, CE, and delay behavior is supplied by the consumer's HAL. SPI
transactions are synchronous, and critical failures put the software driver
in a fail-stop state. Mocks and successful compilation do not validate a
physical module or prove over-the-air delivery.

## Implemented features

| Feature | Status and coverage |
|---|---|
| SPI register read/write and multi-byte address access | Implemented; mock SPI tests cover command bytes and address transfers. |
| RF channel 0–125 | Implemented; invalid-channel and register-configuration tests. |
| Shared five-byte TX and RX pipe-0 address | Implemented; initialization writes and verifies both addresses. |
| 250 kbit/s, 1 Mbit/s, and 2 Mbit/s rates | Implemented; RF_SETUP encoding tests. |
| Four nominal TX power levels | Implemented; RF_SETUP encoding tests. |
| One-byte CRC | Set and verified by receive-first initialization. |
| Dynamic Payload Length on pipe 0 | Configured and verified; receive tests cover valid widths through 32 bytes. |
| Selective NO_ACK transmission | Implemented with `W_TX_PAYLOAD_NOACK`; `FEATURE.EN_DYN_ACK` is configured. |
| Auto ACK configuration | Receiver pipe 0 has EN_AA enabled for DPL compatibility; outgoing calls do not request ACK. No auto-ack policy setter exists. |
| Initialization and register verification | Implemented; tests cover configuration, verification failure, state transitions, and timing calls. |
| RX → TX → RX switching | Implemented; tests cover successful restoration and failures. |
| TX FIFO handling and payload transmission | Implemented; TX FIFO is flushed before each send and completion is polled. |
| RX FIFO inspection and dynamic-width read | Implemented; tests cover empty, queued, and multiple payloads. |
| Payload reception and buffer validation | Implemented; tests cover exact copy length, untouched tail, undersized buffers, and maximum length. |
| Invalid-width recovery | Invalid widths trigger `FLUSH_RX`; tests cover zero and values above 32, successful recovery, and SPI failures. Recovery discards all queued RX payloads. |
| Error and fail-stop handling | Implemented; mock tests exercise SPI failures, state rejection, mode restoration, and faulting. |
| Mocked hardware unit tests | Present using `embedded-hal-mock`; they test software transactions, not actual radio electrical or RF behavior. |
| Physical over-the-air validation | Not yet established for this driver's TX/RX behavior. |

The matrix describes implementation and test coverage, not a claim that the
validation commands below have passed in every environment or that the driver
has been tested on physical radios.

## Public API

The consumer supplies:

- An `embedded_hal::spi::SpiDevice` which owns/manages CSN. Configure its bus
  for SPI Mode 0, MSB first, at no more than the nRF24L01+'s 10 MHz limit.
- A separate CE `embedded_hal::digital::OutputPin`.
- An `embedded_hal::delay::DelayNs` implementation for startup, radio settling,
  and bounded TX polling delays.

`Nrf24::new` takes ownership of the SPI device and CE pin without accessing
hardware. Call `initialize` once with a `RadioConfig`; successful initialization
leaves the radio listening. Use a 32-byte receive buffer to accept every
supported payload size. A shorter buffer is valid, but if it is too small the
oldest payload remains queued and can be retried with a larger one.

```rust,ignore
let config = RadioConfig::new(50, [0xE7; 5])?;
let mut radio = Nrf24::new(spi_device, ce_pin);
radio.initialize(&config, &mut delay)?;

let payload = [0x01, 0x02, 0x03];
radio.transmit(&payload, &mut delay)?;

let mut buffer = [0u8; 32];
if let Some(length) = radio.receive(&mut buffer)? {
    process(&buffer[..length]);
}
```

The snippet assumes application-provided variables and error handling. It is
not a board-specific pin map or a claim of hardware validation. See the
crate-level documentation and the public API comments for detailed contracts.

### State and errors

The driver starts in `RadioState::Unknown`. Successful `initialize` changes it
to `Ready` in RX mode. `Faulted` means the hardware state is uncertain; the
radio data methods and initializers reject further use. Call `release` to
recover ownership of the SPI device and CE pin, then reset/reinitialize the
hardware as appropriate for the board. `release` itself performs no hardware
reset or pin change.

Invalid TX length, `receive` with no packet, an undersized buffer, and a
successfully recovered invalid RX width do not fault the driver. Critical SPI,
GPIO, verification, and TX-operation errors do. A TX error can be returned
after the packet was already transmitted if RX restoration then fails; avoid
blindly retrying application data in that case.

## Hardware configuration

The following are general integration requirements, not board-specific wiring
instructions:

- **Supply:** The nRF24L01+ IC is specified for 1.9–3.6 V. Use a clean supply
  within the voltage limits of the actual module; do not assume a module has a
  regulator or 5 V tolerance. PA/LNA modules can have larger current peaks
  than the bare IC. Provide suitable local decoupling and verify supply
  integrity during radio activity.
- **SPI:** Connect SCK, MOSI, and MISO to the MCU SPI bus. Use Mode 0,
  MSB-first, and stay at or below 10 MHz at the radio. Start at a lower rate
  for initial bring-up if appropriate for the board.
- **CSN:** The supplied `SpiDevice` owns CSN and must assert it for each SPI
  transaction.
- **CE:** Connect CE to a controllable GPIO output. The driver controls CE;
  the pin is not interchangeable with CSN.
- **IRQ:** Not required. The driver polls synchronously and does not configure
  or consume IRQ.
- **RF peers:** Configure the same channel, five-byte address, data rate, and
  compatible CRC on both ends. This driver uses one-byte CRC and pipe 0.
  Receiving dynamic payloads requires compatible DPL support and enabled
  auto-ack configuration at pipe 0. Its own transmissions are NO_ACK; use
  NO_ACK-compatible peer operation for the documented test workflow.
- **Startup:** Allow board/module supply and the transceiver's power-on reset
  to complete before initialization. The driver's 2 ms radio power-up delay is
  not a delay for board power sequencing.

Verify the module pinout and electrical ratings before wiring. Existing project
notes report SPI register read/write experiments with a module on an ESP32-C3;
they do not establish this driver's RF communication, module identity, or
compatibility with a Pico 2 W installation. No pin assignment is prescribed
here.

## Tests and verification

Run from the workspace root:

```bash
cargo fmt --all -- --check
cargo test -p nrf24-driver
cargo clippy -p nrf24-driver --all-targets -- -D warnings
cargo doc -p nrf24-driver --no-deps
RUSTDOCFLAGS="-D warnings" cargo doc -p nrf24-driver --no-deps
```

The driver has no MCU-specific dependencies. Its generic `no_std` code can be
checked for targets used by consumers, for example:

```bash
cargo check -p nrf24-driver --target thumbv8m.main-none-eabihf
cargo check -p nrf24-driver --target riscv32imc-unknown-none-elf
```

For this revision, both target-specific crate checks completed successfully.
The host validation also completed successfully: 61 unit tests and 1 doctest
passed, and formatting, strict Clippy, and documentation builds (including
rustdoc warnings denied) passed. These crate-only checks do not build or certify
the ESP32-C3 or Pico 2 W firmware projects. ESP32-C3 firmware needs the
consuming project's Xtensa/RISC-V toolchain configuration as applicable; a
successful library build is not a hardware test.

## Known Limitations and Future Work

### Relevant to the first physical test

- The exact module silicon and DPL/NO_ACK compatibility are not independently
  confirmed. Initialization checks FEATURE/DYNPD readback and has no
  `ACTIVATE` fallback; a module that does not expose those features as
  configured will fail initialization.
- RF transmission and reception, receiver sensitivity, range, and timing have
  not been established by the host mocks. The prior reported register
  experiments establish only basic SPI register access.
- Transmission is NO_ACK. A reported TX completion means the radio completed
  its transmit operation, not that another node received the payload.
- FIFO flushing to recover an invalid RX width discards every currently queued
  RX payload, and that recovery is not coordinated with an ACK transmission.
- The current [station firmware](../station-rp2350/src/main.rs) imports the
  removed `TxConfig` API, and the [train firmware](../train-esp32c3/src/bin/main.rs)
  calls the legacy PTX-only `initialize_tx` path. The station must be updated to
  `RadioConfig` and RX polling, and the train must use `initialize` before
  `transmit`. These consumer changes are outside this crate's scope.

For the specific ESP32-C3 TX → Pico 2 W RX test, the consumer integration is an
essential blocker. Once that is addressed, these radio limitations are
constraints to account for during controlled bring-up; no feature expansion is
required if the module, pinout, supply, and wiring are verified.

### Optional future work

- Physical interoperability tests across the specific modules and MCU boards.
- Recovery/reinitialization API after `RadioState::Faulted`.
- Interrupt-driven or async receive integration.
- Multi-pipe RX and configurable payload/CRC/address-width strategies.
- Configurable acknowledged TX and retransmission policy.
- More diagnostics and measured throughput, latency, and timing.

The legacy `initialize_tx` method remains public for diagnostics, but it leaves
the radio in PTX standby; the current `transmit` and `receive` data APIs require
RX mode. Use `initialize` for normal operation.
