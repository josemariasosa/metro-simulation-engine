# SPEC-001 — Wire Protocol & Message Contracts

**Status: Implemented 💚**

## 1. Status and scope

**Status: proposed; complete draft for review. No codec or firmware implementation is authorized by this document.**

This specification defines version 1 of the transport-independent LE ROVE Brain ↔ Train message contract. It covers departure authorization, command acceptance/rejection, observed departure, emergency-stop requests, and observed stops.

The encoded application frame is at most **32 bytes**, including its header. The transport must deliver one complete application frame with its boundary intact. The initial messages occupy 13–18 bytes. Radio addressing, packet framing, integrity protection, and delivery behavior are outside this format.

The normative words MUST, MUST NOT, SHOULD, and MAY describe requirements for a future implementation. This draft does not change `metro-core` behavior or define how physical reports update simulation state.

## 2. Terminology and ownership

- **Command:** a Brain request to a Train; receipt does not establish acceptance or execution.
- **Command result:** the Train's application-level acceptance or rejection of a particular command. It is not a transport acknowledgment.
- **Observed event:** a report of an occurrence observed by the Train. Acceptance alone MUST NOT be reported as departure or stopping.
- **Node:** a logical communicating endpoint, independent of its radio or network address.
- **Train:** a logical railway vehicle. Its identity is distinct from the identity of its controller node.

Commands travel Brain → Train. Results and events travel Train → Brain. The common header identifies the sender and recipient and carries command correlation. Each of the six message bodies identifies its subject train. A deployment must configure which node controls which train and which node is its Brain; discovery and provisioning are not specified here.

`NodeId` and `TrainId` are separate because a communication endpoint is not a railway vehicle. A controller node and its vehicle may have different identities; matching numeric values do not establish a relationship. The common header is independent of railway entities so that future Brain ↔ Station, Station ↔ Train, and Infrastructure ↔ Brain communication need not name a train. This amendment defines no such messages and introduces no generic entity abstraction. The configured node-to-train relationship remains an endpoint responsibility.

The codec validates the frame's structural and local semantic rules. Endpoint authorization, matching a result against a previously sent command, and checking a station against a configured railway are receiver responsibilities, not codec operations.

## 3. Wire identities

| Identity | Encoding | Valid values | Meaning |
| --- | --- | --- | --- |
| `NodeId` | unsigned 16-bit | 1–65535 | Logical communication endpoint (Brain or Train in current messages) |
| `TrainId` | unsigned 16-bit | 1–65535 | Subject vehicle |
| `StationId` | unsigned 16-bit | 1–65535 | Station in the configured railway |
| `CommandId` | unsigned 32-bit | 1–4294967295 | Brain-issued command correlation identity |

Zero is invalid for node, train, and station identities. Zero in a location's unused fields is padding, not a station identity. Header command ID zero is permitted only for an unsolicited `Stopped` event as specified below.

All multi-byte integers use **little-endian** encoding. Wire types MUST NOT use `usize`, pointers, native Rust enum layout, or native struct layout.

Node IDs must be unique within the communicating deployment. Station and train IDs must have consistent configured meanings at both endpoints. This specification does not prescribe persistent allocation or map these IDs directly onto `metro-core` IDs, which currently begin at zero and use `usize`.

## 4. Version 1 frame

### 4.1 Common header

Offsets are zero-based. There is no alignment padding.

| Offset | Width | Field | Rule |
| --- | --- | --- | --- |
| 0 | 1 | `version` | Exactly `0x01` |
| 1 | 1 | `message_kind` | One of the discriminants below |
| 2 | 2 | `source_node_id` | Nonzero |
| 4 | 2 | `destination_node_id` | Nonzero; different from source |
| 6 | 4 | `command_id` | Correlation identity; zero only where explicitly allowed |
| 10 | variable | Message body | Exact layout determined by message kind |

The common header is 10 bytes and contains no railway entity identity. Each currently defined body begins with its own nonzero 16-bit `train_id` at offset 10; this is a rule of these six message definitions, not an implicit header field or a requirement on future message kinds. There is no magic number, length field, checksum, timestamp, session identifier, or reserved flag field. Frame boundaries come from the transport adapter; message kind determines exact length. A stream transport must supply its own boundary framing before invoking this decoder.

Transport overhead is outside the 32-byte application limit. A transport adapter must not append bytes to this frame and still treat them as part of a valid v1 message. If its available application capacity is smaller than a message, that adapter cannot carry the message directly; fragmentation is not specified.

### 4.2 Message discriminants and lengths

| Kind | Category | Direction | Body bytes | Total bytes |
| --- | --- | --- | --- | --- |
| `0x01` `Depart` | Command | Brain → Train | 6 | 16 |
| `0x02` `EmergencyStop` | Command | Brain → Train | 3 | 13 |
| `0x10` `Accepted` | Command result | Train → Brain | 3 | 13 |
| `0x11` `Rejected` | Command result | Train → Brain | 4 | 14 |
| `0x20` `Departed` | Observed event | Train → Brain | 6 | 16 |
| `0x21` `Stopped` | Observed event | Train → Brain | 8 | 18 |

The header shrinks from 12 to 10 bytes while every current body grows by two bytes, so total frame lengths are unchanged. Command ID moves from offset 8 to 6; train ID moves from header offset 6 to body offset 10. All remaining body fields retain their previous absolute offsets.

This is a revision of an unimplemented v1 draft, not compatibility with the previous draft layout. Old-layout bytes MUST NOT be accepted through a fallback decoder. Their identical lengths mean they can sometimes decode as different valid values under the new layout; there is no reliable automatic layout detection. Any consumers of earlier draft vectors must update together.

All other message-kind bytes are invalid in v1. Existing discriminants and layouts MUST NOT be reinterpreted. A future incompatible format requires a different version. Version negotiation and forward-compatible skipping are not part of v1: unknown versions or kinds are rejected.

## 5. Message definitions

### 5.1 `Depart`

| Offset | Width | Field |
| --- | --- | --- |
| 10 | 2 | `train_id` (nonzero) |
| 12 | 2 | `from_station_id` |
| 14 | 2 | `to_station_id` |

Both station IDs MUST be nonzero and different. Command ID MUST be nonzero.

Meaning: authorize this train to depart from the named source toward the named destination. This is one movement authorization, not continuous throttle or permission to make subsequent departures. The ordered station pair identifies the intended segment in v1; parallel physical routes between the same endpoints are not distinguished.

The Train may reject a structurally valid command if it cannot accept that movement. This specification does not define route admission, reservations, location sensing, motor behavior, authorization expiry, or the endpoint's full eligibility policy.

### 5.2 `EmergencyStop`

| Offset | Width | Field |
| --- | --- | --- |
| 10 | 2 | `train_id` (nonzero) |
| 12 | 1 | `reason` |

Command ID MUST be nonzero. Valid emergency-request reasons are:

| Value | Meaning |
| --- | --- |
| `0x01` | Operator request |
| `0x02` | Brain safety decision |

Meaning: request an immediate stop of the subject train. It is addressed to one Train node; broadcast and fleet-wide addressing are not defined.

`Accepted` means the request has been accepted, not that the train is already stationary. `Stopped` reports a subsequently established stationary condition, including when the Train determines it was already stopped. Braking mechanics, response deadlines, achievable stopping distances, latching, and restart permission are outside this contract. Acceptance or delivery is not a safety guarantee.

### 5.3 `Accepted`

| Offset | Width | Field |
| --- | --- | --- |
| 10 | 2 | `train_id` (nonzero) |
| 12 | 1 | `command_kind` |

`command_kind` MUST be `0x01` (`Depart`) or `0x02` (`EmergencyStop`). Command ID MUST be nonzero and identify the accepted command. Source/destination are reversed relative to that command; train identity is unchanged.

Meaning: the Train has accepted responsibility for acting on the request. It does not assert that physical action occurred. An accepted departure can still be prevented by a later obstacle; this draft has no generic execution-failure message or completion guarantee.

### 5.4 `Rejected`

| Offset | Width | Field |
| --- | --- | --- |
| 10 | 2 | `train_id` (nonzero) |
| 12 | 1 | `command_kind` |
| 13 | 1 | `reason` |

Command kind, correlation, and endpoint rules are the same as for `Accepted`.

| Reason | Meaning |
| --- | --- |
| `0x01` `InvalidState` | Current operating state does not allow acceptance |
| `0x02` `LocationMismatch` | Reported/current location does not match the requested source |
| `0x03` `UnknownStation` | A requested station is not recognized by this endpoint |
| `0x04` `UnsupportedMovement` | Requested source/destination movement is unsupported |
| `0x05` `SafetyInhibit` | A local safety condition prevents accepting departure |
| `0x06` `UnableToExecute` | Endpoint cannot undertake the requested action |

Reasons `0x01`–`0x06` are valid for `Depart`. Only `0x06` is valid for `EmergencyStop`; already being stationary is not a rejection reason. Other command-kind/reason combinations are invalid.

Rejection means the Train did not accept the command. It does not assert that the train is stationary or cancel an earlier accepted command. Malformed frames are decoding errors, not application-level rejections.

### 5.5 `Departed`

| Offset | Width | Field |
| --- | --- | --- |
| 10 | 2 | `train_id` (nonzero) |
| 12 | 2 | `from_station_id` |
| 14 | 2 | `to_station_id` |

Both station IDs MUST be nonzero and different. Command ID MUST be nonzero and refer to the `Depart` authorization responsible for this departure. The station pair and train ID MUST match that authorization.

Meaning: the Train observed departure from the source toward the destination. The sensing criterion is a firmware concern and is not defined by the codec. This event does not establish arrival, precise position, speed, or route completion.

The receiver can validate agreement with the earlier command only if it retains the command context. An otherwise valid event without locally known context is not a malformed frame.

### 5.6 `Stopped`

| Offset | Width | Field |
| --- | --- | --- |
| 10 | 2 | `train_id` (nonzero) |
| 12 | 1 | `reason` |
| 13 | 1 | `location_kind` |
| 14 | 2 | `location_a` |
| 16 | 2 | `location_b` |

Stop reasons:

| Value | Meaning | Header command ID |
| --- | --- | --- |
| `0x01` `EmergencyCommand` | Stop established in response to an accepted `EmergencyStop` | That emergency command's nonzero ID |
| `0x02` `Obstacle` | Local obstacle-related stop | Zero |
| `0x03` `LocalStop` | Other locally initiated stop | Zero |
| `0x04` `Unspecified` | Stationary condition observed without a known cause | Zero |

The command ID identifies the command causing this stop, not the most recent departure. An obstacle report MUST NOT copy the earlier departure command ID into this field.

Location encoding:

| Kind | Meaning | `location_a` | `location_b` |
| --- | --- | --- | --- |
| `0x00` | Unknown | Exactly zero | Exactly zero |
| `0x01` | At station | Nonzero station ID | Exactly zero |
| `0x02` | On directed segment | Nonzero source station ID | Nonzero destination station ID, different from source |

Other location kinds are invalid. Unused fields MUST be zero. Segment direction describes the relevant traversal, not current motion: this train is stopped. Location may be unknown even when stopping is established.

Meaning: the Train observed that it is stationary. It does not imply clearance of an obstacle, release of railway resources, arrival completion, or permission to resume. Sensor implementation and the threshold for establishing stationary state are outside this document.

## 6. Lifecycle and invariants

In these examples, `train_id=T` is in each message body; `command_id` is in the common header. Results and correlated events MUST preserve the command body’s train identity. The initial successful departure lifecycle is:

```text
Brain -> Train: Depart(train_id=T, command_id=C, from=A, to=B)
Train -> Brain: Accepted(train_id=T, command_id=C, command_kind=Depart)
Train -> Brain: Departed(train_id=T, command_id=C, from=A, to=B)
```

The rejection alternative ends with `Rejected(train_id=T, command_id=C, command_kind=Depart, reason=...)`, with no departure caused by that rejected command.

The emergency-stop lifecycle is:

```text
Brain -> Train: EmergencyStop(train_id=T, command_id=E, reason=OperatorRequest)
Train -> Brain: Accepted(train_id=T, command_id=E, command_kind=EmergencyStop)
Train -> Brain: Stopped(train_id=T, command_id=E, reason=EmergencyCommand, location=...)
```

An endpoint unable to undertake the emergency request reports `Rejected(train_id=T, command_id=E, command_kind=EmergencyStop, reason=UnableToExecute)`; that report provides no evidence of a safe stop. Local obstacle stopping may produce unsolicited `Stopped(train_id=T, command_id=0, reason=Obstacle, location=...)` without a preceding command.

For a single command execution, an endpoint MUST NOT generate contradictory accepted/rejected outcomes. Results describe the decision; events describe observations. Acceptance MUST precede generation of its correlated event, but this does not require delaying local physical safety action while transmitting a result.

The transport may lose or reorder frames. A Brain may therefore receive an event before its result, or never receive a result. Missing frames do not prove rejection or non-execution. Handling these conditions is outside the stateless codec.

## 7. Command identity and future deduplication

Within an uninterrupted Brain command-allocation lifetime, the Brain MUST assign a new nonzero command ID to each new logical command, across all destination trains and both command kinds. Results and correlated events echo it without allocating another identity.

The tuple `(source Brain NodeId, CommandId)` is the proposed future command-deduplication key. Destination, command kind, and body (including train identity) form the associated immutable command content. Reuse of a key with different content is a protocol violation.

A future retransmission design would resend the same command with the same ID and content, allowing a receiver to recognize it instead of repeating physical action. **This specification implements neither duplicate suppression nor retransmission.** A codec decodes repeated identical bytes identically; it keeps no command history.

Command allocation MUST NOT wrap and reuse IDs within that lifetime. On exhaustion, further command issuance requires a separately defined lifecycle policy. Across reboots, no uniqueness or stale-command protection is guaranteed by this format. Persistence, sessions, retention windows, retries, and restart recovery are explicitly deferred. Consequently, this draft MUST NOT be interpreted as an exactly-once execution contract.

`Stopped` reports with command ID zero have no unique event identity in v1. Deduplicating unsolicited events is not supported by this contract.

## 8. Encoding, decoding, and errors

### 8.1 Stateless codec requirements

A future codec MUST operate on byte slices with explicit little-endian integer operations. It MUST NOT transmute Rust structs or enums, allocate a heap buffer, panic on arbitrary input, or require access to `metro-core`.

Encoding uses a caller-owned output slice and returns the number of bytes written. A `[u8; 32]` is sufficient for every valid frame. Only the returned prefix is the encoded frame; trailing buffer capacity is not transmitted.

The encoder MUST validate message invariants before writing. Failure MUST leave the supplied output buffer unchanged. Encoding an invalid value returns `InvalidValue`; insufficient capacity returns `BufferTooSmall { required, available }`. Value validation precedes capacity checking.

The decoder accepts exactly one complete frame and returns an owned, fixed-size message value or an error. It MUST NOT normalize unknown discriminants, silently truncate values, accept trailing bytes, or infer missing fields.

### 8.2 Decode errors and validation order

Validation proceeds in this order, giving deterministic errors:

1. More than 32 input bytes: `FrameTooLong { actual }`.
2. Fewer than 10 bytes: `HeaderTooShort { actual }`.
3. Version other than `0x01`: `UnsupportedVersion { actual }`.
4. Unrecognized message kind: `UnknownMessageKind { actual }`.
5. Length different from the exact length for that kind: `InvalidLength { expected, actual }`.
6. Header validation in source, destination, command-ID order. Zero node IDs, equal source/destination, and zero command IDs except for `Stopped` produce `InvalidValue { field }`.
7. Body fields in ascending offset order, beginning with the nonzero `train_id` at offset 10: unknown enum tags produce `InvalidDiscriminant { field, actual }`; invalid train or station IDs produce `InvalidValue { field }`.
8. Cross-field rules: equal departure endpoints (`to_station_id`), invalid rejection combination (`reason`), invalid stop-reason/command-ID combination (`command_id`), and location-field rules (`location_a`, then `location_b`) produce `InvalidValue { field }`.

For `Stopped`, header command-ID validation is deferred to step 8 because zero/nonzero validity depends on its reason. Its location fields are validated in step 8 because their meaning depends on `location_kind`.

Error field identifiers and enum values MUST be representable without allocated strings. Errors are local API values, not wire messages. Unknown configured node/train/station mappings, mismatched outstanding commands, and unauthorized senders are contextual receiver errors outside this codec error set.

## 9. Rust compatibility

The future library MUST support `#![no_std]` without `alloc`, a global allocator, or a runtime. Frame/message/error types must use fixed-width integers, bounded enums, and fixed-size values. Codec functions accept borrowed input/output buffers; no `Vec`, `String`, `HashMap`, I/O, clocks, or asynchronous executor is required.

Tests may use `std` on the host. Native Rust layouts and `size_of` are unrelated to encoded lengths. Public API naming is an implementation choice; the bytes, validation rules, and error meanings in this specification are normative.

## 10. Fixed binary test vectors

These vectors use Brain node `0x0001`, Train node `0x0002`, train `0x0003`, source station `0x0010`, destination station `0x0020`, departure command `0x01020304`, and emergency command `0x01020305`. Spaces separate bytes; line wrapping has no meaning.

| Case | Exact bytes (hex) | Length |
| --- | --- | --- |
| Depart | `01 01 01 00 02 00 04 03 02 01 03 00 10 00 20 00` | 16 |
| Accepted departure | `01 10 02 00 01 00 04 03 02 01 03 00 01` | 13 |
| Rejected departure: location mismatch | `01 11 02 00 01 00 04 03 02 01 03 00 01 02` | 14 |
| Departed | `01 20 02 00 01 00 04 03 02 01 03 00 10 00 20 00` | 16 |
| EmergencyStop: operator | `01 02 01 00 02 00 05 03 02 01 03 00 01` | 13 |
| Accepted emergency stop | `01 10 02 00 01 00 05 03 02 01 03 00 02` | 13 |
| Rejected emergency stop: unable to execute | `01 11 02 00 01 00 05 03 02 01 03 00 02 06` | 14 |
| Stopped: emergency command, on segment | `01 21 02 00 01 00 05 03 02 01 03 00 01 02 10 00 20 00` | 18 |
| Stopped: obstacle, on segment | `01 21 02 00 01 00 00 00 00 00 03 00 02 02 10 00 20 00` | 18 |
| Stopped: local, at station | `01 21 02 00 01 00 00 00 00 00 03 00 03 01 10 00 00 00` | 18 |
| Stopped: unspecified, location unknown | `01 21 02 00 01 00 00 00 00 00 03 00 04 00 00 00 00 00` | 18 |

Each vector must be tested in both directions: decoding produces the specified fields, and encoding those fields produces exactly the listed bytes. The non-symmetric command IDs deliberately verify byte order.

## 11. Required test coverage

These are requirements for later implementation, not tests implemented by this draft.

### 11.1 Round trips and encoding

- `decode(encode(message)) == message` for every message kind, each defined enum value in its valid context, and every location variant.
- `encode(decode(bytes)) == bytes` for every valid fixed vector; no alternate padding or trailing-byte encodings.
- Minimum and maximum valid identity values, including `CommandId = 0xFFFFFFFF`; zero command ID only for permitted unsolicited stops.
- Node and train identities round-trip independently, including distinct values and coincident numeric values; the codec must not equate the identity namespaces.
- Every valid encoded length matches the table and is at most 32 bytes.
- Exact-sized output buffers succeed; each smaller capacity fails without changing the buffer. Oversized output buffers preserve bytes beyond the returned encoded prefix.
- Invalid typed values are rejected without modifying the output buffer.

### 11.2 Malformed frames

Use the fixed vectors as seeds and assert the exact error category and applicable field:

| Mutation/input | Expected error |
| --- | --- |
| Empty input or any input shorter than 10 bytes | `HeaderTooShort` |
| 33 bytes | `FrameTooLong` |
| Change a valid vector's version to `0x02` | `UnsupportedVersion` |
| Change its kind to `0xFF` | `UnknownMessageKind` |
| Truncate a known-kind vector to any length from 10 to one below its required length | `InvalidLength` |
| Append one byte to any valid vector | `InvalidLength` |
| Pad a valid vector to 32 bytes | `InvalidLength` |
| Zero source node or destination node in the header | `InvalidValue` for that field |
| Zero body train ID at offsets 10–11, for each of the six kinds | `InvalidValue(train_id)` |
| Make destination node equal source node | `InvalidValue(destination_node_id)` |
| Zero a required command ID | `InvalidValue(command_id)` |
| Zero a departure endpoint or make both endpoints equal | `InvalidValue` for the affected endpoint |
| Set result `command_kind`, emergency reason, rejection reason, stop reason, or location kind to an undefined value | `InvalidDiscriminant` for that field |
| Combine emergency rejection with `LocationMismatch` | `InvalidValue(reason)` |
| Emergency-command stop with zero command ID | `InvalidValue(command_id)` |
| Obstacle/local/unspecified stop with nonzero command ID | `InvalidValue(command_id)` |
| Unknown location with any nonzero location field | `InvalidValue` for first invalid location field |
| At-station location with zero A or nonzero B | `InvalidValue` for the affected location field |
| Segment location with zero endpoint or equal endpoints | `InvalidValue` for the affected location field; equal endpoints identify B |

For example, a `Depart` frame with both zero command ID and zero train ID must report `InvalidValue(command_id)` because header validation precedes body validation. A `Stopped` frame with both an invalid reason and zero train ID must report `InvalidValue(train_id)` because body validation proceeds by offset.

Tests must also exercise multiple simultaneous errors to verify validation precedence, and bounded arbitrary byte inputs to demonstrate decoding without panics or out-of-bounds access. Repeated identical decoding must have no stateful effects.

### 11.3 Build checks

The future codec must pass host tests and a library-only `no_std` build for at least one representative embedded target, such as `riscv32imc-unknown-none-elf`. The build must require neither allocator support nor hardware drivers. A protocol build must not depend on the simulation engine.

## 12. Acceptance criteria

This specification is ready for implementation review when reviewers approve the identity ranges, lifecycle semantics, message discriminants, exact layouts, rejection reasons, and validation precedence above.

A later implementation satisfies SPEC-001 when:

1. All six message kinds encode/decode according to these layouts and fixed vectors, using the 10-byte entity-independent header and explicit body `train_id` at offsets 10–11. No train identity is inferred from a node identity.
2. Results and events remain distinct, with the specified command correlation and zero-ID rules.
3. All malformed-frame, round-trip, buffer, and build checks pass.
4. Every encoded application frame is at most 32 bytes; all current messages have their exact declared lengths.
5. The codec is stateless, allocation-free, transport-independent, and usable in `no_std`.
6. Public documentation states that command IDs enable correlation and future deduplication but do not implement duplicate suppression, delivery guarantees, or reboot protection.
7. No simulation, transport driver, retry/session machinery, or motor/sensor implementation is added as part of this contract.

## 13. Explicit non-goals and deferred work

- Radio drivers, SPI, nRF24L01+ configuration, Wi-Fi, transport checksums, and transport authentication.
- ACK retransmission, retries, message ordering, delivery guarantees, and exactly-once execution.
- Sessions, persistent identity allocation, command-history retention, reboot recovery, state synchronization, and unsolicited-event deduplication.
- Physical-state reconciliation with `metro-core`, new simulation states, resource reservation changes, and changes to `metro-simulation-engine` behavior.
- Motor control, braking behavior, stop deadlines, obstacle sensors, and departure/location detection algorithms.
- Station/track controller messages, topology transfer, parallel-route identifiers, broadcast commands, and fleet coordination.
- Arrival events, stop-at-next-station commands, measured speed/progress/ETA telemetry, and general fault/state reporting.
- Ratatui/Bevy changes, dashboards, and operator controls.

The only change made for this review is this specification document. Implementation requires a subsequent explicit task.
