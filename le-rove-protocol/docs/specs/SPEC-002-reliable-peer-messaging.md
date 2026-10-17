# SPEC-002 — Reliable Peer Messaging & State Exchange

**Status: Draft — Pending Design Review**

Inspected on 2026-10-16 against working-tree source at base commit `85cc24a`.
This document proposes a protocol and later implementation checkpoints. It
authorizes no implementation. MUST/MUST NOT describe the proposed contract,
subject to design review; SHOULD describes a recommendation. All wire assignments
below are provisional until review. SPEC numbering here belongs to
`le-rove-protocol`, independently of the simulation specifications.

## 1. Summary and motivation

Brain maintains a model of a physical railway; it does not dictate physical
reality. Automatic trains execute Brain commands. Manual trains can depart or
stop independently, including contrary to Brain's expectations. Future station
and track nodes can independently observe the same occurrence. Messaging must
carry these reports without requiring a preceding authorization or deciding
which observer is correct.

Propose an explicit **version 2 frame**, a stateless codec, and a small separate
bounded peer engine. Each direction of each configured peer relationship uses
stop-and-wait delivery: one message in flight, immutable retransmissions, explicit
receipt ACKs, and a monotonically increasing message ID. A shared pair session
separates communication lifetimes. Command correlation remains distinct.

Add unsolicited `Departed`, minimal `StatusRequest`/`StatusReport`, and source-local
observation revisions. The largest proposed frame is 31 bytes. No heap, runtime,
driver, synchronized clock, traffic policy, or durable event log is required.

The deliberate simplifications are limited throughput and externally established
sessions. Automatic session discovery/recovery is not hidden inside this design.
A deployment unable to establish a fresh session after either endpoint restarts
cannot claim the restart protection described here.

## 2. Findings from the current implementation

Source is authoritative. Several existing specs retain historical baseline text
and old paths. In particular, protocol SPEC-001 is headed “Implemented” but its
body still describes a proposed future codec; the codec and tests actually exist.
The simulation's SPEC-003 and SPEC-005 status/baseline passages likewise do not
fully describe today's implementation.

### 2.1 Simulation architecture and manual behavior

| Area | Verified behavior and evidence |
| --- | --- |
| Ownership | [`Simulation`](../../../metro-core/src/simulation/mod.rs) owns `Vec<TrainEntity>`, identity allocation, constraints, stepping, command handling, and snapshot mapping. [`Train`](../../../metro-core/src/domain/train.rs) owns private state/control/velocity and narrow transitions. Core uses `std`, `Vec`, `HashMap`, and `HashSet`; it is not the embedded messaging library. |
| States | `AtStation { Dwelling, Ready }` and `Moving`; velocity is binary 0/1, not measured speed. Both constructors accept initial dwell; [`DwellPolicy`](../../../metro-core/src/domain/dwell.rs) supplies three seconds after arrival. |
| Automatic trains | Eligible on dwell completion or while Ready. Proposals use frozen starting resources, resolve by ascending numeric train ID, then commit. Blocking leaves Ready for later retries. |
| Manual trains | `apply_command(Accelerate)` can depart from either station substate, including unfinished dwell. It abandons that dwell and starts movement at elapsed zero without advancing time. Without input, dwell completes into Ready indefinitely. Already-moving acceleration is an exact successful no-op. |
| Manual restrictions | [`TrainCommand`](../../../metro-core/src/command.rs) has only `Accelerate { train_id }`. Errors are `UnknownTrain`, `NotManual`, `NoOutgoingTrack`, and `Blocked`. Rejections preserve state and do not buffer intent. Both control modes pass the shared admission predicate. |
| Routing and movement | [`Network`](../../../metro-core/src/domain/network.rs) chooses adjacent insertion indices. [`select_departure_candidate`](../../../metro-core/src/domain/departure.rs) tries current direction, then reverse only if no outgoing track exists. Blocking does not trigger reverse fallback. Subsequent one-second steps advance traversal; arrival stops and starts fresh dwell. There is no manual brake or mid-segment stopped state. |
| Resources | [`ResourceView`](../../../metro-core/src/domain/resource.rs) derives exclusive directed-track occupancy and directional station occupancy/reservations. A moving train reserves its destination slot. Reverse edges are independent resources. Conflicting registration is rejected; deriving invalid ownership can panic. |
| Operational constraints | [`constraint.rs`](../../../metro-core/src/domain/constraint.rs) and Simulation implement scheduled half-open intervals for track unavailable, station departures blocked, and station unavailable. They restrict new admission for both control modes, not already admitted traversal. |
| Observation | [`snapshot.rs`](../../../metro-core/src/snapshot.rs) provides owned time/train/constraint DTOs. Train observations include ID, direction, velocity, dwell/ready/movement and traversal timing, but no control mode, event identity, observation revision, or command history. Snapshot generation is pure. |
| IDs and integration | Simulation train/station IDs use `usize` and start at zero. Protocol IDs are nonzero fixed-width values. No direct cast or implicit identity mapping is valid. No physical-event reconciliation API or protocol-to-core adapter is implemented. |

The current simulator is intentionally more constrained than independent physical
participants. Receiving a report MUST NOT require first successfully applying
`Accelerate` to the simulation. An observed departure may contradict occupancy,
constraints, dwell expectations, or the current model. How Brain adapts its model
is separate work; this draft does not relax core's invariants or invent an adapter.

The 120-second dwell / departure at second 30 scenario is representable by the
current initial-dwell constructor and manual acceleration when admission permits.
It is not today's default arrival dwell. A physical manual train may also depart
when simulated admission would deny it; the protocol must still carry the report.

Relevant evidence includes [`manual_control.rs`](../../../metro-core/tests/manual_control.rs)
(early departure, waiting, no-op acceleration, blocked intent, round trip),
[`automatic_blocking.rs`](../../../metro-core/tests/automatic_blocking.rs),
[`operational_constraints.rs`](../../../metro-core/tests/operational_constraints.rs),
and [`snapshot_observation.rs`](../../../metro-core/tests/snapshot_observation.rs).
Existing uncommitted test edits were present during inspection; they are not
changes made by this draft. No test-run or embedded-build success is claimed here.

Background specifications inspected: simulation
[observation boundary](../../../docs/specs/SPEC-001-core-observation-boundary.md),
[manual control](../../../docs/specs/SPEC-002-manual-train-control-boundary.md),
[occupancy](../../../docs/specs/SPEC-003-railway-occupancy-blocking-physical-safety.md),
[operational constraints](../../../docs/specs/SPEC-005-external-operational-constraints.md),
and [sandbox](../../../docs/specs/SPEC-006-operational-sandbox.md).

### 2.2 Protocol and codec

[`SPEC-001`](SPEC-001-wire-protocol.md), [`identity.rs`](../../src/identity.rs),
[`frame.rs`](../../src/frame.rs), and [`message.rs`](../../src/message.rs) establish:

- Nonzero `NodeId`, `TrainId`, and `StationId`: u16; nonzero `CommandId`: u32.
  Node and train identities are independent. Source and destination must differ.
- V1's 10-byte header contains version, kind, source, destination, and optional
  command correlation. Bodies identify the train. Six message kinds occupy 13–18
  bytes: Depart, EmergencyStop, Accepted, Rejected, Departed, and Stopped.
- `Departed` requires a nonzero command ID and matching departure authorization.
  An autonomous manual departure currently cannot be represented legally.
  `Stopped` already supports uncorrelated Obstacle/LocalStop/Unspecified reports.
  There is no throttle, arrival, status exchange, receipt ACK, session, or event ID.
- Depart is a single movement authorization, not the simulator's local Accelerate
  action. Accepted is application acceptance, never proof of motion or receipt ACK.
  `LocationMismatch` already exists for rejecting Depart, but location checking
  belongs to the controller, not the codec.

[`lib.rs`](../../src/lib.rs) uses `#![no_std]`, exports typed frames/messages,
identity constructors, encode/decode and fixed errors; the crate has no
dependencies. [`encode.rs`](../../src/codec/encode.rs) validates before writing
caller-owned slices; [`decode.rs`](../../src/codec/decode.rs) checks maximum size,
header, version, kind, exact length, identities, and cross-field rules in a fixed
order. The codec is stateless and repeated decoding has no execution semantics.

[`codec/tests`](../../src/codec/tests/mod.rs) includes all eleven SPEC-001 vectors,
buffer preservation, identity boundaries, malformed inputs, validation precedence,
round trips, and bounded arbitrary-input checks. [`public_api.rs`](../../tests/public_api.rs)
exercises exported APIs. [`brain_train_flow.rs`](../../tests/brain_train_flow.rs)
models encoded exchanges and correlation, not delivery, execution, or loss recovery.

## 3. Goals and non-goals

Goals: bounded reliable delivery attempts between peers; explicit receipt ACKs;
message/source identity; duplicate suppression; explicit delayed-message and
restart limits; independent observations; requested and voluntary state reports;
deterministic external-time execution; allocation-free embedded use; simulated
loss/reordering acceptance tests.

Non-goals: radio drivers or hardware ACK configuration; sockets/Wi-Fi/MQTT;
motor control; collision avoidance; dispatch, scheduling or resource allocation;
merging contradictory observations; comprehensive station/track schemas;
topology transfer; persistent event logs; exactly-once physical execution; full
restart recovery; adapting `metro-core` to physical reports. Neither manual
movement nor reporting requires Brain permission.

## 4. Terminology and reliability guarantees

| Term | Meaning |
| --- | --- |
| Pair session | Nonzero u32 `SessionId`, shared by exactly two configured nodes for one communication lifetime; replaced after either endpoint loses protocol state. |
| Message identity | `(source NodeId, destination NodeId, SessionId, MessageId)`. `MessageId` is nonzero u32, strictly increasing per direction within the pair session. |
| Command identity | `(issuing Brain NodeId, peer NodeId, SessionId, CommandId)`. Separate from the identities of command, result, and observed event messages. |
| Receipt ACK | Receiver has admitted this exact message to its volatile application boundary, retaining duplicate protection. Not application acceptance, durable storage, or execution. |
| DeliveryUnknown | Attempts exhausted or session retired without matching receipt. Message may have been delivered and executed. |
| Observation revision | Nonzero u32 assigned by the reporting source per train within the pair session, ordering its observations independently of send/retry order. |

Within an uninterrupted correctly configured session, conforming endpoints
provide **at-most-once application admission per message identity**, bounded
attempts, and positive receipt evidence when a matching ACK arrives. Delivery is
conditional on a usable link, available receiver capacity, and timely polling
within the retry budget. Permanent loss cannot guarantee delivery.

There is no exactly-once action guarantee, global ordering, bounded physical
response, synchronized time, or proof that observations are truthful. Command
validation remains necessary even for a newly admitted message. Autonomous or
adversarial physical behavior is reportable; malicious identity spoofing and
forged ACKs require separate transport integrity/authentication measures.

## 5. Proposed protocol architecture

```text
controller / observer / Brain consumer
       ↕ bounded admission, delivery notices, decoded observations
peer engine: configured sessions, TX slot, RX duplicate record, retry clock
       ↕ typed v2 frames and caller-owned byte buffers
stateless version-specific codec
       ↕ complete frames, length ≤ 32
transport adapter (radio, simulated link, or other frame transport)
```

The peer engine knows configured identities and sessions, not railway resources.
It must be usable by Brain, Train, Station, and Track roles. Only existing command
contracts restrict Brain→Train direction; ACKs and status exchanges are symmetric.
No source role is inferred from numeric IDs. Endpoint provisioning owns role and
node-to-train mappings.

### 5.1 Session installation and restart boundary

For the smallest first implementation, session establishment is an explicit local
API supplied by a trusted deployment coordinator/test harness, not an on-wire
handshake. Both sides must install the same fresh pair token before exchanging
data. The coordinator need not be Brain and does not authorize physical movement.

Session tokens MUST NOT be reused for that node pair while any old frame can
exist. Prefer a durable monotonically allocated u32 token; exhaustion requires
reprovisioning with an explicit old-frame exclusion procedure, not wraparound.
Random u32 boot values alone do not give this guarantee. A simulator can use a
deterministic allocator. Provisioning/persistent storage mechanics are outside the
library, but satisfying this contract is mandatory for the stated guarantees.

After reboot, an endpoint starts Unbound and cannot admit messages or emit ACKs.
Restoring only the old token without deduplication state is forbidden. A fresh
installation discards old pending delivery state with DeliveryUnknown notices
where that volatile state still exists. The other endpoint may continue retrying
until it learns of retirement; old frames are rejected by the restarted node.
Installation may be asynchronous: mismatched sessions make no progress, but do
not execute old commands. Unknown-session traffic MUST NOT install or roll back a
session automatically. Losing RX history or evicting a peer also retires its session.

This is an intentionally limited deployment contract. Unattended radio rejoining
needs a later approved handshake or provisioning mechanism. Until then, restart
means messaging remains unavailable until fresh installation; local manual
operation remains independent. Outstanding physical actions are not canceled.

## 6. Wire format and message contracts

All integers are unsigned, fixed width, and little-endian. No native struct layout,
padding, fragmentation, CRC, authentication tag, or transport framing is added.
The transport supplies one intact application-frame boundary and integrity
appropriate for its environment. Exact message length is mandatory.

### 6.1 Version 2 data header

| Offset | Bytes | Field |
| --- | --- | --- |
| 0 | 1 | Version = `0x02` |
| 1 | 1 | Kind |
| 2 | 2 | Nonzero source node |
| 4 | 2 | Nonzero destination node, different from source |
| 6 | 4 | Nonzero pair session |
| 10 | 4 | Nonzero message ID |
| 14 | 4 | Kind-specific correlation; zero only as defined below |
| 18 | variable | Body |

Correlation is a tagged meaning determined by kind, not an interchangeable ID:
commands/results/correlated events carry `CommandId`; StatusReport carries the
request's `MessageId`; StatusRequest carries zero. Typed APIs SHOULD expose these
as distinct fields/types even though the wire slot is shared.

### 6.2 Kinds and lengths

| Kind | Body, in wire order | Correlation | Total bytes |
| --- | --- | --- | --- |
| `0x01` Depart | train:u16, from:u16, to:u16 | Nonzero command | 24 |
| `0x02` EmergencyStop | train:u16, reason:u8 | Nonzero command | 21 |
| `0x10` Accepted | train:u16, command_kind:u8 | Nonzero command | 21 |
| `0x11` Rejected | train:u16, command_kind:u8, reason:u8 | Nonzero command | 22 |
| `0x20` Departed | train:u16, from:u16, to:u16, observation_revision:u32 | Command or zero | 28 |
| `0x21` Stopped | train:u16, reason:u8, location_kind:u8, location_a:u16, location_b:u16, observation_revision:u32 | SPEC-001 stop rules | 30 |
| `0x30` StatusRequest | scope:u8, subject:u16 | Zero | 21 |
| `0x31` StatusReport, Node | scope=0:u8 | Request message ID or zero | 19 |
| `0x31` StatusReport, Train | scope=1:u8, train:u16, observation_revision:u32, motion:u8, location_kind:u8, location_a:u16, location_b:u16 | Request message ID or zero | 31 |
| `0x31` StatusReport, Unavailable | scope=2:u8, subject:u16, reason:u8 | Nonzero request message ID | 22 |
| `0x7F` ReceiptAck | Special layout below | No command correlation | 14 |

All SPEC-001 train/station identity, endpoint inequality, enum, command-kind,
rejection-combination and location-padding rules carry forward unless explicitly
changed here. Nonzero from/to remain different. Unknown kinds/tags are errors.

Depart, EmergencyStop, Accepted and Rejected preserve their meanings. Result
correlation must match session, reversed endpoints, command kind and train.
Accepted still precedes generation of an associated observed event; transmission
order does not establish physical ordering or successful execution.

### 6.3 ReceiptAck

Offsets 0–9 have version `02`, kind `7F`, reversed nodes and the same session.
Offsets 10–13 contain the acknowledged data MessageId. The ACK has no independent
message ID, correlation field or body. It is never acknowledged or put in the
reliable TX slot. Duplicate data causes another ACK. Thus ACK loss costs a data
retry but cannot create an ACK loop. A hardware/link ACK is not this receipt ACK.

The sender matches all four identity components before clearing its pending slot.
An ACK for an old session, wrong peer, unrelated ID, or no pending message has no
effect. A receipt ACK MUST NOT be inferred from Accepted or StatusReport.

### 6.4 Autonomous Departed

Nonzero correlation retains SPEC-001's causal meaning: an accepted Depart caused
this departure, with matching train and station pair in that session. Zero means
an observed departure without that command cause. It does not mean unauthorized,
invalid, or less important. Do not fabricate a command ID for a manual action.

The minimum semantic change would simply permit zero for Departed; it needs no
new event kind or departure-reason taxonomy. V2 also adds independent message
identity and revision for delivery and ordering. If destination is unknown, the
existing endpoint-pair Departed schema cannot represent it truthfully: send Train
StatusReport with Moving and Unknown location. Do not invent a station ID.

### 6.5 Provisional byte examples

Session `0x01020304`; Train node 2 reports Train 1 leaving Station 2 toward 3,
message 7, no command, observation revision 9:

```text
02 20 02 00 01 00 04 03 02 01 07 00 00 00 00 00 00 00
01 00 02 00 03 00 09 00 00 00
```

Brain node 1 acknowledges that message (14 bytes):

```text
02 7F 01 00 02 00 04 03 02 01 07 00 00 00
```

These are review examples, not replacements for the v1 vectors. Implementation
must add exact vectors for every v2 kind and scope after layout approval.

## 7. Sender/receiver state machines

### 7.1 Sender: independent in each direction

| State / input | Action and next state |
| --- | --- |
| Unbound / enqueue | Return SessionUnavailable; no transmission. |
| Idle / valid enqueue with capacity | Allocate next MessageId, encode immutable bytes, attempt transmission; Pending. |
| Pending / matching ACK | Emit Delivered(identity), release slot; Idle. Application result may still be absent. |
| Pending / retry deadline | If attempts remain, emit identical bytes and advance deadline. |
| Pending / final deadline | Emit DeliveryUnknown(identity), release slot; Idle. Never recycle its ID. |
| Pending / another enqueue | Return QueueFull, or use caller-configured bounded waiting storage. Never overwrite the pending message. |
| Any / session retirement | Outstanding deliveries become unknown; clear state and become Unbound. |

Only one data message per direction is on the wire awaiting receipt. Reverse
traffic proceeds independently, including results while a command awaits its ACK.
Allocate IDs when moving into the TX slot, not when observations first occur.
Any optional waiting queue is bounded and supplied by the consumer; the baseline
engine needs no internal waiting queue. No retransmission may change content.

### 7.2 Receiver: one retained frame plus high-water mark

For each bound peer retain the largest admitted MessageId H (initially zero), its
exact encoded bytes/length, and a command-ID high-water mark (initially zero).

1. Decode and validate session, destination and configured source permissions.
2. For ID = H: compare full bytes. Identical means duplicate: emit ACK, no second
   application admission. Different means IdentityConflict: no ACK, no dispatch.
3. For ID < H: discard as Superseded, with no ACK or dispatch. It might be an old
   admitted duplicate or an earlier abandoned message; this small cache cannot
   distinguish them and MUST NOT falsely acknowledge an unseen message.
4. For ID > H: require application admission capacity. For a command require a
   strictly greater CommandId than the last admitted command in this pair session;
   otherwise report CommandIdentityReuse and do not dispatch or ACK.
5. Atomically reserve the application item and retain its frame/H before emitting
   ACK or making the item available for execution. On capacity failure do neither.

Gaps are allowed because a sender can abandon a message after its budget expires.
With stop-and-wait, accepting a higher ID means any lower transmission was already
acknowledged or abandoned by a conforming sender. Late lower IDs must never be
executed. A sliding window and reorder buffer are unnecessary for this profile.

H is retained for the entire session, not expired by a timer. Keeping only recent
IDs with a time-to-live would risk redispatching a delayed command after eviction.
Changed content below H is not diagnosable with this cache but is still suppressed.
An invalid/forged high ID must not advance H before all admission checks pass.

### 7.3 Application boundary and command execution

ACK commits only volatile admission. A controller separately validates the actual
location/state and produces exactly one Accepted or Rejected decision for an
admitted command. Operational rejection is still receipt success. Commands MUST
NOT be re-created with a new message ID merely because a result is missing.

CommandIds remain a separate globally increasing Brain allocation across command
kinds and destinations during its allocation lifetime (a stricter rule than v1's
uniqueness alone). Per-peer high-water checking catches logical command reuse
under new message IDs without a growing command cache. Replacing one pair session
does not reset Brain's global allocator. Restarting that allocator requires
retiring all its command-bearing pair sessions and separating all old contexts;
the new sessions can then admit command IDs starting at one again.

Reserve bounded result capacity before command admission. One reserved result
slot per peer plus one command admission slot is sufficient when new commands
are backpressured until the result slot is released. Results use the reverse
reliable sender and its own MessageId; they are not regenerated on duplicate
command receipt. No unbounded result replay cache is required. Result exhaustion
means application outcome is unknown to Brain even if command receipt succeeded.

The reserved result slot must remain available even if voluntary telemetry fills
other buffers. ACK processing must never wait for a data TX slot or application
response. Consumers must drain admitted items independently of receiving their
own outbound ACKs; otherwise symmetric traffic can deadlock.

## 8. Retry, deduplication and timeout policies

Propose a configurable constant retry interval R > 0 and maximum transmissions
1 ≤ A ≤ 255, including the initial attempt. Deterministic host fixtures use R=100 ms,
A=4: attempts at 0, 100, 200, 300; final uncertainty at 400 if polled on time.
These are test settings, not validated radio timings or safety deadlines.

The engine takes externally supplied monotonic u64 ticks and checked arithmetic.
No sleeping, wall clock, async executor, random jitter or transport callback is
required. Equal input bytes, capacities, times and call ordering yield equal
outputs. A decreasing timestamp returns TimeWentBackwards without mutation;
overflow/configuration failure is explicit, never silent wraparound.

On late polling, emit at most one retry per pending slot per poll and schedule its
next deadline from supplied now; do not produce a catch-up burst. Thus attempts
are bounded but elapsed completion time depends on polling. At a final deadline
the pending message becomes unknown. If ACK and deadline share a timestamp, the
caller must supply an explicit order; recommended harness ordering is receive
frames first, then poll timers. Transport submission failure consumes an attempt
for a finite budget. A writable output buffer alone is not transmission success.

No MessageId, CommandId, observation revision or session token may wrap/reuse in
its defined lifetime. Exhaustion returns an explicit error; obtain a fresh session
before resetting pair-local counters. A new session does not authorize retrying an
uncertain physical operation. Message ID gaps do not prove packet loss or action.

The baseline has head-of-line delay: an EmergencyStop can wait behind another
outbound message. No emergency latency guarantee is made. An independent priority
channel would require additional IDs/deduplication state and is a review alternative,
not an implicit exception allowing parallel sends on this channel.

## 9. State request/report semantics

### 9.1 Request and response

StatusRequest scope 0 means the addressed node itself; subject MUST be zero.
Scope 1 asks the addressed node for one train's observed state; subject is a
nonzero TrainId. Other scopes are invalid in this version. This is symmetric:
Brain can request Train state and Train can request Brain's node status.

A Node StatusReport (scope 0) reports only that the node's status handler is
responsive in this session. It asserts no railway state or operational readiness.
This deliberately minimal Brain status avoids exporting a whole simulation.
Future richer node, station and track state requires separately specified contracts.

A Train StatusReport (scope 1) reports that source's observation of the named train.
Motion is `0=Unknown`, `1=Stopped`, `2=Moving`. Unknown motion avoids claiming a
stop during sensor initialization. Location uses SPEC-001's Unknown / AtStation /
OnSegment tags and zero-padding rules. Motion and location are independent: a
stopped train can be on a segment and movement can be observed without location.
No speed, ETA, dwell countdown, resource claim, mode or sensor confidence is added.

A valid request for an unavailable train gets scope 2, echoing its nonzero subject,
with reason `1=UnknownSubject` or `2=StateUnavailable`; other reasons are invalid.
Unknown location/motion in an available state is different from having no state
to report. Unavailable is a status response, not CommandResult::Rejected.

Response correlation is the request's MessageId in the same pair session with
reversed endpoints. Match requested scope/subject as well; an unsolicited report
uses zero. A response is independently reliable and independently ACKed. Reserve
bounded response capacity before request admission, as for commands. A duplicated
request is ACKed but does not sample/generate another response. A new request ID
asks for a new sample. Request delivery and response timeout are separate results.

Voluntary Train/Node reports may follow changes or a consumer-selected interval;
the engine schedules no mandatory heartbeat. Backpressure must not stop a physical
train or imply its state was unchanged. Consumers may coalesce unsent voluntary
reports to the latest state; they MUST NOT replace already transmitted bytes or
silently discard a command result. Full queues return explicit errors. Applications
can retain one latest-state slot to retry publishing after capacity returns.

### 9.2 Ordering and freshness

The observer serializes state observations for each train and assigns revisions
at observation/sample time, not retransmission time. Every newly sampled report
or event receives a strictly increasing revision, even if values are unchanged.
Departed, Stopped and Train StatusReport share this counter. A delayed emission
retains its original revision. Revisions may have gaps due to unsent/coalesced data.

Consumers retain the latest revision/state per `(source, pair session, train)`.
A greater revision may update that source's latest reported state; a lower or
equal revision must not replace it. Transport admission/ACK still succeeds for a
well-formed stale report. Historical events and pending request completion can be
recorded separately; a correlated old response may finish the request without
overwriting newer state. Equal revision with different contents is a source
consistency error, not a reason to apply the contents.

Departed supplies a moving/on-segment observation at its revision; Stopped supplies
stopped/location. This shared ordering prevents a delayed departure event from
overwriting a later stopped status. MessageId cannot substitute for revision:
old observations can wait before enqueueing, and message IDs also cover commands.

Revision ordering is not elapsed-time freshness. Record local receive time if
useful, but a late first delivery may already describe obsolete reality. An ACK
does not refresh the observation sample. No clock synchronization or maximum
observation age is claimed. StatusRequest invites a fresh sample but cannot prove
the physical sensor is current. Mark previous-session observations historical or
unknown until a new report is obtained; never compare revision numbers across
sessions. Never compare revisions from different reporting nodes or merge them.

## 10. Error handling and edge cases

| Condition | Required behavior |
| --- | --- |
| Malformed/oversized/trailing bytes | Codec error; no ACK or domain response. |
| Unknown version/kind | Explicit codec error; no fallback interpretation. |
| Wrong destination, unauthorized source, unknown/retired session | No dispatch or ACK; local diagnostic. An observational conflict with Brain is not authorization failure. |
| Full application/result/response capacity | No admission or ACK; sender retries within budget. Local QueueFull is visible. |
| Duplicate current message | Re-ACK exact bytes, no repeat execution/response generation. |
| Lower ID | Drop without ACK; no resurrection after cache replacement. |
| New message, stale physical command | Admit and ACK, then controller can return Rejected(LocationMismatch). Deduplication cannot establish location correctness. |
| Accepted arrives before command receipt ACK | Record application result separately; command delivery engine still awaits its matching ACK. |
| Event arrives without known command context | Preserve observation/source/session and flag missing correlation context; do not fabricate history or discard physical evidence. |
| Result or event delivery exhausted | Notify local owner; consumer may request state. Missing messages do not prove non-execution. |
| Late ACK after DeliveryUnknown | Does not resurrect a cleared slot or falsely complete a newer one. Baseline does not retain terminal delivery history. |
| Node restart mid-action | No automatic re-execution or cancellation. Fresh session, fresh observation, explicit application uncertainty. |

A same-session command delayed before its first admission may still be executed
after the sender times out, if no higher message has been admitted. This draft
does not define command expiry. Even LocationMismatch cannot catch a train that
left and later returned to the requested station. Session protection prevents
cross-session replay, not all stale operational intent. Applications must not
translate DeliveryUnknown into “safe to repeat.” Expiry/precondition revisions
are an explicit review alternative in section 14.

V2 decode precedence should be size limit, minimum common prefix (14 bytes),
version, kind, kind-specific exact length (StatusReport scope needs bounded
inspection), node/session/message fields, correlation, body fields, cross-field
rules. Missing scope/body yields InvalidLength without indexing past input. ACKs
use their special exact length. Final named error variants and multi-error vectors
must be approved before implementation; v1 error precedence remains untouched.

## 11. Embedded and memory constraints

The codec and peer engine MUST build with `#![no_std]` and no `alloc`, driver,
`metro-core`, Linux, or async-runtime dependency. Use caller-owned byte arrays,
fixed enums, bounded slots and const-generic or caller-provided peer storage.
Public API names/module placement are implementation review choices; an additional
crate is not required. Keep v1 codec operations stateless and available separately.

Minimum storage accounting per bound peer, excluding Rust alignment:

| Record | Approximate payload/metadata bytes |
| --- | ---: |
| TX frame 32, length 1, attempts 1, deadline 8, active flag 1 | 43 |
| RX retained frame 32, length 1, H 4 | 37 |
| Peer node 2, session 4, next TX ID 4, command high-water 4, bound flag 1 | 15 |
| ACK pending identity/flag (ID 4 + flag 1) | 5 |
| Application admission slot and reserved response slot, each 32+length+flag | 68 |
| Total illustrative baseline | 168 |

Budget **about 256 bytes per peer** as an initial target, not an ABI promise;
measure `size_of` on the chosen embedded target. External queues add roughly
32 bytes plus metadata per frame. Latest observation tracking costs at least a
train key, revision, and retained state per source/train; bound that separately.
Command contexts and sensor state are consumer memory, not free library storage.
One peer may suit a Train; Brain scales with its configured peer count P.

Use bounded scans, O(P) per full poll and O(1) processing after peer lookup. No
dynamic peer creation on incoming frames. One pending ACK per peer can be
coalesced for duplicate ID H; emission should precede data retries. Admission
must not overwrite an un-emitted ACK for a different ID; backpressure instead.
Malformed input must not panic. Encoding errors must preserve output buffers.
No reliance on Rust struct packing for wire size or RAM estimates is permitted.

## 12. Compatibility and migration from SPEC-001

### 12.1 Feasible strategies

| Strategy | Compatibility | Wire cost | Complexity / RAM | Extensibility |
| --- | --- | --- | --- | --- |
| Extend v1 with new kinds carrying reliability metadata | Keep original six kinds byte-for-byte; allocate new reliable counterparts and ACK/status kinds. Old decoders reject the new kinds. Autonomous Departed needs a new counterpart, not reinterpretation of old `0x20`. Requires an explicit amendment to v1's closed kind list. | Existing 10-byte header plus session/message metadata adds 8 bytes; revisions add 4 to events. Same maximum 31 bytes is achievable with layouts equivalent to this proposal. | Codec kind list roughly doubles; endpoints must distinguish reliable and legacy modes. Engine RAM essentially the same. | Feasible, but one version would encompass incompatible delivery/causality contracts and encourage accidental legacy fallback. |
| **New v2 frame, selected proposal** | V1 decoder continues rejecting version 2. New codec has explicit version boundary; v1 bytes/semantics remain frozen. | 18-byte data header, 14-byte ACK, maximum 31 bytes. Eight bytes more header than v1. | Separate typed v2 codec and peer engine; no nested frame buffer required. Constant per-peer RAM above. | Clear place for optional causal correlation and status. Only one byte remains in the largest report; richer schemas require new layouts/version decisions. |
| Envelope around unchanged v1 bytes | Inner codec can remain untouched, but unmodified inner Departed still cannot express autonomous causality; requires an inner extension too. | A minimal outer marker + session:u32 + message:u32 adds 9 bytes; largest v1 frame becomes 27, or 31 with revision. Inner nodes can be reused. Duplicating outer routing/correlation or adding two boot epochs rapidly consumes 32 bytes. | Nested dispatch, separate ACK format, and careful buffer borrowing; avoid storing both inner and outer copies. Same reliability state plus possible scratch storage. | Feasible with tightly defined inner extensions, but little advantage over direct v2 for this small protocol. |

Adding fields to existing v1 headers/bodies under the same kinds would invalidate
exact lengths, field offsets, vectors and validation precedence. Permitting
zero-command Departed under v1 would invalidate SPEC-001 §§3–7 and tests that
reject missing command IDs. Treating Accepted as ACK would invalidate its meaning.
None of those changes is made or authorized here.

### 12.2 Required contract changes and migration

V2 explicitly supersedes v1's mandatory Departed correlation, six-kind closed
message set, 10-byte header, command-only correlation slot, absence of sessions,
and absence of message/observation identity. Stopped's causal rules, existing
command/result meanings, station/location rules and 32-byte limit are preserved.
Command allocation becomes increasing rather than merely unique.

No existing SPEC-001 vector or invariant changes **for v1**. After approval,
SPEC-001 may gain a cross-reference clarifying its version-limited guarantees;
its current text is not edited by this task. V2 needs new vectors and versioned
API types; extending existing public Rust enums/Frame fields in place would also
break downstream exhaustive matches and struct literals. Prefer explicit v2 types
and entry points instead of silently changing `encode`/`decode`.

Configure protocol version per peer. There is no automatic negotiation/downgrade.
A v2 reliable peer must never accept a v1 command as equivalent reliable input,
since that bypasses session and duplicate checks. A dual-version application may
run a separately identified legacy path, with v1's documented lack of guarantees.
No lossless gateway can invent missing event IDs, sessions, or autonomous command
causes; reliable migration requires both endpoints of that relationship to upgrade.

## 13. Mandatory test scenarios and acceptance criteria

Future tests use a deterministic host link carrying only encoded bytes, explicit
virtual time, finite queues and scripted drop/duplicate/delay/reorder decisions.
Use a small fake controller with an execution counter and independently mutable
observed location; do not require changes to `metro-core` to model physical autonomy.

| Scenario | Required acceptance evidence |
| --- | --- |
| 1. Reliable Brain command | Drop first command receipt ACK and first Accepted transmission. Retransmitted command has identical identity/bytes; execution/admission count stays one; duplicate is re-ACKed; Accepted's independent retry eventually arrives. Also test losing only Accepted: Brain does not resend a new command after receipt success. |
| 2. Autonomous manual departure | Initial reported Station 2, expected dwell 120 s. At 30 s the fake operator moves Train 1; no Brain command exists. Departed has zero causal correlation, nonzero message ID/revision, is admitted and ACKed. A contradictory model expectation does not reject the observation. |
| 3. Lost departure event | Separately prove (a) event retry succeeds, (b) all event attempts fail then a new Brain request obtains current Train status, (c) a later voluntary report obtains delivery. Assert source/state is available to Brain's consumer; do not implement traffic adaptation. Permanent loss yields uncertainty rather than a fabricated state. |
| 4. Stale command | Physical controller is at Station 3 when Depart(2→3) arrives. Receipt succeeds; controller emits reliable Rejected(LocationMismatch). Duplicate command never executes or generates a contradictory decision. |
| 5. Independent observers | Two configured sources can use equal numeric message IDs without collision. Preserve source and session with every delivered observation. Assert no cross-source suppression/merging. Use generic engine test payloads or independent Train status observations; do not allocate station/checkpoint wire schemas merely for this test. |
| 6. Restart and delayed traffic | Restart either sender or receiver, including receiver after admission before ACK. Unbound node dispatches nothing. Install fresh pair session; old data/results/ACKs cannot affect new state even when numeric message IDs repeat. Old action outcome remains unknown; no automatic command reissue. Reinstalling the old session without history must be rejected by lifecycle policy. |

Additional acceptance gates:

- Every v1 vector/error test remains unchanged; v2 vectors cover every kind/scope,
  non-symmetric IDs, exact lengths, zero rules and the largest 31-byte report.
- Arbitrary input is panic-free; truncation/trailing data, invalid scope, invalid
  correlation, bad location padding and enum combinations fail deterministically.
- ACK loss, duplicate ACKs, wrong-peer/session ACKs, changed-content duplicates,
  lower IDs after H advances, skipped IDs and counter exhaustion are covered.
- Prove an old unseen command arriving after a higher admitted message is dropped;
  prove/document that a delayed first admission before that point remains possible.
- Reverse-direction progress and ACKs work while both data slots are occupied;
  result reservation and backpressure do not admit work with nowhere to respond.
- Verify retry counts, final uncertainty, equal-time call ordering, late polls,
  transport send failure, decreasing time and overflow without real sleeps.
- Deliver a newer status followed by an older event/report with a higher message
  ID but lower revision; latest observed state remains newer. Include a stale
  correlated response, same-session unchanged-state sample, and revision reset
  after session change. Cross-source revision numbers are never compared.
- Queue saturation returns explicit errors; unsent report coalescing preserves
  newest observation and never mutates pending bytes or repeats physical actions.
- Host tests and a library-only build for a representative embedded target (for
  example the SPEC-001 `riscv32imc-unknown-none-elf` target) require no allocator or
  drivers. Record measured RAM for chosen peer/queue capacities.

## 14. Open questions and design alternatives

The five highest-priority human review decisions are:

1. **Is externally installed pair-session recovery acceptable for the first
   physical deployment?** It is the smallest explicit restart boundary, but needs
   a trustworthy non-reusing allocator and coordinated installation. An automatic
   handshake would improve unattended recovery but needs replay-safe challenges,
   lifecycle persistence/entropy assumptions, more states and test cases. Do not
   claim deployment readiness until the provisioning owner and procedure exist.
2. **Is stop-and-wait's throughput and head-of-line delay acceptable?** One TX slot
   and one retained RX frame minimize RAM. A small sliding window or independent
   priority channel allows urgent traffic to bypass telemetry, at the cost of
   reorder rules, duplicate windows and additional memory. Physical stop deadlines
   cannot be promised by the baseline.
3. **Should commands carry an operational freshness precondition now?** Current
   location validation plus session fencing does not prevent delayed same-session
   intent or leave-and-return cases. A relative lease needs a well-defined receiver
   time origin; an expected observation revision needs knowledge of the receiver's
   latest sample and adds rejection semantics. Keep this explicit limitation or
   approve a narrow extra contract; deduplication is not a substitute.
4. **Are the proposed state vocabulary and observation revisions sufficient?**
   Node response currently proves handler responsiveness only; Train report has
   Unknown/Stopped/Moving and coarse location, with no sample age. Alternatives
   include deferring Node scope, adding sample age, or separate observation counters
   per entity lifetime. Each consumes wire space or adds clock/lifecycle policy.
5. **Approve explicit v2 isolation or prefer v1 extension/envelope?** V2 preserves
   existing bytes and causal meanings in their original version but requires both
   endpoints and consumers to upgrade. Confirm the 18-byte data header, 14-byte ACK,
   correlation-slot typing and 31-byte maximum before freezing vectors/API design.

Further implementation review items: embedded RAM ceiling and maximum peer count;
real transport retry interval/attempt budget; maximum admitted application work;
status response wait policy; whether terminal delivery diagnostics need a bounded
history; and whether strong increasing CommandId allocation is preferable to a
larger explicit command-deduplication cache. None requires a railway scheduler.

## 15. Suggested implementation checkpoints

These are reviewable future work, not authorization to begin implementation.

1. **Contract review:** resolve the five decisions above, approve session provision
   assumptions, lengths/tags, explicit guarantee limits, and reference RAM profile.
2. **Versioned stateless codec:** add v2 types, exact vectors, malformed-input/error
   precedence tests; keep v1 public behavior and vectors unchanged.
3. **Bounded peer lifecycle:** bind/retire APIs, message allocation, one TX slot,
   RX high-water/full-byte comparison, receipt ACK matching and atomic admission.
4. **Deterministic retry engine:** externally supplied time, capacity errors,
   finite attempts, delivery uncertainty and counter/time exhaustion tests.
5. **Controller/status boundary:** reserved result/response capacity, independent
   command outcomes, autonomous departure and observation revision handling using
   fake controllers. No motor or `metro-core` adaptation work.
6. **Loss/restart integration:** all mandatory scenarios and edge matrices over
   encoded frames, including same-session stale-intent limitations.
7. **Embedded verification and documentation:** representative no_std build,
   measured memory, migration examples and explicit provisioning prerequisites.
   Radio transport and model reconciliation require separately authorized work.
