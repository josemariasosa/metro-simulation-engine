# SPEC-002: Manual Train Control Boundary

## 1. Status

**Proposed — specification only; implementation is not authorized by this document.**

This specifies an independently useful MVP following the implemented observation
boundary in [SPEC-001](SPEC-001-core-observation-boundary.md).
Section 6 defines acceptance; future work does not add MVP gates.

MUST and MUST NOT denote requirements; SHOULD denotes a recommendation; MAY
denotes an optional choice. Implementation proceeds one educational checkpoint
at a time, with review between checkpoints.

## 2. Motivation

The player operates the train through actions; `metro-core` decides and commits
legal world-state transitions. The previous `RequestDeparture` abstraction sounded
like administrative permission and encoded a high-level transition: “please
transition this train from station state to Moving.” Replace it with the concrete
player action `Accelerate`: “Apply the player's intent for this train to be moving.”
This expresses intent rather than a request for a particular state transition.
If a manual train is already Moving, that intent is already satisfied: return
`Ok(())` as an exact no-op. Adapters need not suppress repeated input to avoid an
error. This decision fits the binary-velocity MVP; future multi-level throttle or
physical acceleration semantics may revisit it.

For this MVP, acceleration is deliberately binary: velocity 0 means stopped and
velocity 1 means moving. Existing timed track traversal remains authoritative;
velocity is not a physical speed or a multiplier for traversal progress. This
provides a gameplay control boundary without introducing realistic physics.
Manual operation permits accelerating before normal dwell finishes or waiting
longer than expected.

```text
player action
    ↓
command boundary
    ↓
metro-core application logic
    ↓
domain rules / routing
    ↓
committed simulation state
    ↓
snapshot
    ↓
adapter / renderer
```

## 3. Architecture and Rule Categories

Keep the existing crate layout and use these conceptual layers:

| Layer | Location | Responsibility |
| --- | --- | --- |
| Domain | `train.rs`, `network.rs`, `station.rs`, `dwell.rs` | Railway state and rules |
| Application | `simulation.rs` | Steps, command validation, transitions, snapshot mapping |
| Boundaries | New `command.rs`, existing `snapshot.rs` | Owned input and observation DTOs |
| Adapters | Bevy / CLI / keyboard input | Translate input; call application API; present results |

Command and snapshot DTOs may reference domain IDs and existing value types.
`Simulation` depends on domain types and boundary contracts and maps between them.
Production domain types MUST NOT depend on command or snapshot DTOs.
`TrainControl` and the velocity property belong in the domain, not in `command.rs`.

No new crates, traits, generic command bus, or dependency injection are required.

### Operational rules

These describe expected operation: dwell duration, automatic departure timing,
and route progression. Automatic trains MUST follow their operational rules.
Manual trains use an explicitly permissive control policy: in this MVP, dwell
duration is advisory and departure timing is player-controlled.
Manual control does not grant arbitrary route or direction selection.

### Physical / safety constraints

These describe what the simulated world permits, independent of control mode:
existing directed tracks, continuous traversal, and eventually resource
occupancy, blocking, collisions, and signaling.

```text
Automatic: operational rules ──────────────┐
                                          ├→ physical constraints → committed movement
Manual: player intent + control rules ─────┘
```

Both categories remain core-owned. Manual mode can relax selected operational
rules, never universal physical constraints. SPEC-002 enforces existing topology
and traversal rules only; occupancy and safety systems are not yet implemented.
SPEC-003 will define additional constraints for BOTH automatic and manual paths.

## 4. Current State

The inspected implementation has the following behavior:

- `Train::new()` starts `AtStation::Dwelling` with three seconds of dwell.
- `AtStationState` currently has only `Dwelling`; `TrainState` also has `Moving`.
- `Simulation::step()` increments time once, then processes each train once.
- Dwell completion immediately creates `Moving { elapsed_seconds: 0, ... }`.
- Arrival starts dwelling at elapsed zero using `DwellPolicy`, currently three seconds.
- Departure consumes no traversal second; arrival consumes no dwell second.
- `Network::next_track()` selects adjacent station insertion indices: increasing
  for Forward, decreasing for Backward. It is not general graph routing.
- Departure tries the current direction, then the reverse. Direction changes only
  on reverse departure. Missing tracks in both directions currently cause a panic.
- `Simulation::snapshot()` maps state in `simulation.rs`; domain objects do not
  construct DTOs. Dwelling mapping asserts elapsed time is below dwell duration.
- Simulation trains are private, but `trains()` exposes an immutable slice;
  simulation time and network are public. There is no command, control mode, or velocity property.
- Bevy still contains independent prototype movement; migration is separate work.

The five-station integration scenario reaches opposite endpoints at second 732,
departs in reverse at 735, and advances at 736. These timings MUST remain intact.
Its exhaustive dwelling assertion needs a failing `Ready` arm when that variant
is introduced; scenario data and timing assertions remain unchanged. Existing tests that directly
construct Moving states must also initialize velocity consistently once it exists.

## 5. Responsibility Boundary

| Concern | Owner |
| --- | --- |
| Key mapping and configured player train ID | Adapter / trusted setup |
| Command lookup and control-mode validation | Core application |
| Normal dwell and automatic departure policy | Core |
| Legality of acceleration during dwell or Ready | Core manual-control policy |
| Outgoing track, direction, arrival, traversal time | Core |
| Future physical/safety checks for either mode | Core |
| Whole-step pacing and call ordering | Application driver |
| Layout, sprites, input feedback | Adapter |

The runtime player/input/rendering path MUST use commands and snapshots rather
than accessing `Train`, `TrainState`, `Network`, or their timers and direction.
Trusted scenario construction may use existing domain constructors before play.
Legacy public APIs remain; full visibility enforcement is separate hardening.

## 6. MVP Control Contract

### IN SCOPE

- One player-controlled train, with existing automatic behavior and timing preserved.
- Domain `TrainControl::{Automatic, Manual}`, fixed at construction.
- A velocity property on every Train: only 0 (stopped) or 1 (moving).
- Exactly one player command: `Accelerate { train_id: TrainId }`, synchronous and atomic.
- Acceleration from Dwelling or Ready, including immediately at time zero.
- Acceleration during unfinished dwell abandons that dwell interval.
- Accelerate while a manual train is already Moving is an idempotent `Ok(())`
  with no mutation; repeated input does not increase velocity beyond 1.
- Without input, a manual train finishes normal dwell and waits in Ready.
- Core-selected outgoing track, direction, endpoint reversal, traversal timing,
  and arrival station under existing routing rules.
- Automatic stopping on arrival: velocity returns to 0 and fresh normal dwell starts.
- Owned, pure snapshots of committed station/movement state and velocity.
- Focused command, compatibility, and determinism tests.

The implementation need not enforce a singleton manual train or track player ownership.

### OUT OF SCOPE

- `Brake`, mid-track stopping, multiple velocity levels, real acceleration/deceleration.
- m/s, m/s², mass, momentum, braking distance, throttle curves, speed limits.
- Passing through stations without stopping, doors, continuous coordinates or physical distance.
- Player-selected source, destination, direction, track, or route.
- Occupancy, collisions, blocking, signaling, reservations (SPEC-003).
- Bevy migration/input bindings, W/A/S/D or keyboard mapping, hardware/Pico integration.
- Multiplayer, multiple-player-train workflows, networking, AI dispatch.
- Queues, buffering, scheduled commands, serialization, event sourcing, undo,
  replay frameworks, generic command buses, dependency injection, new crate architecture.
- Runtime control-mode changes and exhaustive invalid-setup validation.

### Definition of Done

Focused tests MUST demonstrate:

1. Default construction remains Automatic at velocity 0; existing automatic timing
   is unchanged, with velocity 1 while Moving and 0 at stations, and no observable Ready.
2. Accelerate succeeds at time zero, during unfinished dwell, and from Ready.
   It sets velocity 0 → 1 and enters Moving at traversal elapsed zero.
3. Without input, manual dwell completes into Ready at velocity 0; further steps
   change only global time for that train, preserving station, direction, and state.
4. Arrival sets velocity 1 → 0 and starts fresh normal dwell at elapsed zero.
   Abandoned dwell does not carry over; repeated manual operation works.
5. Core chooses tracks and reverses direction when movement begins as required.
6. All three rejection reasons (`UnknownTrain`, `NotManual`, `NoOutgoingTrack`)
   are covered, including no outgoing track from both station states; rejection
   changes no state. Duplicate Accelerate while Moving returns `Ok(())` as an
   idempotent exact no-op, proven by before/after state comparison, including
   simulation time, traversal elapsed time, direction, velocity, and unrelated trains.
7. Accepted commands may change only the addressed train and advance neither simulation
   time nor traversal time. Departure starts traversal at elapsed zero; the next step
   consumes the first traversal second. Commands while Moving preserve existing elapsed time.
8. Snapshots distinguish Dwelling, Ready, and Moving, expose consistent velocity,
   and remain owned and pure, including across commands at unchanged timestamps.
9. Equal ordered steps/commands produce equal results despite different observation
   frequency; command-before-step and step-before-command timing are tested explicitly.
10. The A↔B round trip passes, as do all existing core tests with only necessary
    exhaustive-match and velocity-consistent fixture accommodations.

Run `cargo test -p metro-core`. No exhaustive command matrix, Bevy work, safety
implementation, or full hardening is an additional acceptance gate.

## 7. Proposed Command API

Add `metro_core::command` with these concrete boundary values:

```rust
use crate::train::TrainId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrainCommand {
    Accelerate { train_id: TrainId },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandError {
    UnknownTrain,
    NotManual,
    NoOutgoingTrack,
}
```

Application entry point:

```rust
pub fn apply_command(&mut self, command: TrainCommand) -> Result<(), CommandError>;
```

`Accelerate` means “Apply the player's intent for this train to be moving.” A stopped
manual train at a station begins traversal toward the core-selected next station;
an already Moving manual train satisfies that intent without any mutation.
It carries only `train_id`, never source or destination station, direction,
selected track, coordinates, target velocity, or traversal time.
`Ok(())` means the intent is satisfied before return: either departure to Moving
at velocity 1 was committed or the train was already Moving and unchanged. Nothing
is queued. Observe committed state through `snapshot()`.
There is no `StillDwelling` error or separate success payload. Expose only
`apply_command()` as the command application entry point.

The three errors describe acceleration validation: `UnknownTrain` for an unknown
ID, `NotManual` for an automatic train (including one already Moving),
and `NoOutgoingTrack` when neither routing direction works. These names remain
appropriate; no departure-permission terminology is needed.

Validation order is:

1. Locate the train by ID equality, not vector index; a linear search is sufficient.
   If absent, return `UnknownTrain`.
2. Require Manual; otherwise return `NotManual`.
3. If already Moving, return `Ok(())` immediately as an exact no-op, without
   station validation or track selection. Preserve simulation time, traversal
   elapsed time, direction, velocity, train state, and all unrelated trains.
4. Otherwise require either stopped station state (Dwelling or Ready) with
   velocity 0, as guaranteed by the supported-state invariant in section 8.
5. Select the current-direction track or reverse fallback. If neither exists,
   return `NoOutgoingTrack`.
6. Commit the selected direction and `Moving { elapsed_seconds: 0, ... }`
   together with velocity 1, then return `Ok(())`.

All validation precedes mutation. Rejection leaves train state, velocity, direction,
timers, global simulation time, and unrelated trains unchanged. A failed reverse
lookup MUST NOT commit a direction change. Future safety checks fit before commit
without changing the command shape or synchronous result model; do not add placeholder errors now.
A private helper returning domain destination/direction values may share track
selection with automatic departure. Domain helpers do not return DTO errors.

## 8. Manual vs Automatic State-Machine Semantics

Add domain `TrainControl::{Automatic, Manual}` in `train.rs`, stored per train
with crate visibility sufficient for `Simulation`. No runtime setter is needed.
Keep `Train::new(id, capacity, station, direction)` automatic; add
`Train::new_manual(id, capacity, station, direction)` with the same initial dwell.

Add a stored minimal velocity property to `Train` (a crate-visible `u8` is
sufficient); supported values are exactly 0 and 1. Both constructors initialize
velocity to 0. No public velocity setter or physical units are needed.

Maintain this invariant at every committed boundary for BOTH control modes:

| Domain state | Velocity |
| --- | ---: |
| AtStation::Dwelling | 0 |
| AtStation::Ready | 0 |
| Moving, including elapsed traversal zero | 1 |

Velocity records stopped versus traversing status; it does not drive integration,
change track duration, or accumulate across steps. The existing state machine and
track timer remain authoritative. Every transition updates state and velocity
together. A Moving train at elapsed zero is already logically moving even though
its first traversal second has not yet been consumed.

Retain the existing states and add `AtStationState::Ready`:

```text
Manual:
Dwelling, velocity=0 ── Accelerate ──→ Moving, velocity=1
    │
    └── normal dwell completes without command ──→ Ready, velocity=0
                                                      │
                                                      └── Accelerate ──→ Moving, velocity=1
Moving, velocity=1 ── traversal completes ──→ Dwelling at next station, velocity=0
Moving, velocity=1 ── Accelerate / Ok(()) ──→ exact same Moving state and timers

Automatic:
Dwelling, velocity=0 ── dwell completes ──→ Moving, velocity=1
Moving, velocity=1 ── traversal completes ──→ Dwelling at next station, velocity=0
```

Ready means normal dwell has completed and the train is waiting at its station.
It is neither required for acceleration nor proof of a valid outgoing route.
Further steps leave the Ready train unchanged, including velocity and direction,
while global time advances. Automatic trains have no externally observable Ready
phase during normal execution and no extra step before movement begins.

Keeping Ready preserves unfinished versus completed dwell without hidden flags.
Saturating Dwelling at zero would contradict its existing snapshot invariant.

Accelerating during dwell abandons that interval; unused dwell does not carry
over. Arrival automatically stops the train and starts fresh dwell at elapsed
zero through the existing policy. Arrival consumes no dwell second. A manual
train may Accelerate again immediately after arrival. No Brake command is required.

Route progression remains core-selected in both modes. For A → B → C → D, setup
inserts stations in that order and connects adjacent pairs. Screen layout does
not define route order. Existing reverse fallback also applies at interior gaps.

## 9. Command Timing Semantics

Calls are serial: commands run between complete one-second steps, never inside one.
They advance neither global time nor traversal time. Accelerate from a stopped
station state sets velocity to 1 and starts Moving at elapsed zero; the next step
consumes the first traversal second. Accelerate while already Moving returns
`Ok(())` without resetting elapsed time or changing any state.

| Situation | Result for a manual train (station departure requires a valid outgoing track) |
| --- | --- |
| Accelerate at initial time zero | Moving immediately, velocity 1 |
| Accelerate before dwell-completion step | Moving, velocity 1; next step advances traversal |
| Dwell completes with no preceding command | Ready, velocity 0 |
| Accelerate after dwell-completion step | Moving at that same time, velocity 1 |
| No command while Ready | Remains stopped at station, velocity 0 |
| Accelerate while Moving, including duplicate command | `Ok(())`; exact no-op, no state or time change |
| Accelerate immediately after arrival | Moving from fresh Dwelling, velocity 1 |

No command is remembered or retried automatically. For a one-second track, the
first step after departure arrives and starts dwell at zero, as today.
The driver must establish explicit ordering; “same tick” alone is ambiguous.
Adapter frame scheduling is outside this MVP.

## 10. Snapshot Implications

Add `TrainSnapshotState::Ready { station: StationId }` and a `velocity: u8`
field to `TrainSnapshot`, copied from the committed Train by the application
mapping in `simulation.rs`. Preserve existing fields, train ordering, and the
owned DTO architecture. Domain types MUST NOT construct snapshot DTOs.
Snapshot velocity MUST agree with section 8: Dwelling/Ready = 0, Moving = 1.
Observation must never repair or mutate inconsistent domain state.

- Dwelling: remaining seconds until normal dwell completion, not a manual-input lockout.
- Ready: normal dwell complete; stopped, waiting for Accelerate.
- Moving: existing endpoints and elapsed/total traversal timing.

Adapters may offer Accelerate input in BOTH station states for the configured
manual train. They must not disable it merely because remaining dwell is positive.
Repeated input while Moving is safe to submit: it succeeds without mutation.
Ready does not promise a usable track or future safety clearance.
No `can_accelerate`, predicted destination, or control-mode snapshot field is needed.

Snapshots report committed truth, including transitions caused by commands at
unchanged simulation time. Equal timestamps do not imply equal state. Existing
owned-copy, read-only, and observation-independence requirements remain applicable.

## 11. Determinism

Identical valid initial state plus the same ordered sequence of `step()` and
`apply_command(command)` MUST produce identical results and corresponding snapshots.
Commands at equal simulation times must also have equal relative ordering.
Additional or omitted snapshot calls MUST NOT affect that outcome.

No wall-clock time, input-device state, frame frequency, or hash iteration order
may decide command behavior. Tests can encode calls directly; no replay system
or timestamp field is required.

## 12. First Vertical Slice

Use a core-only A↔B fixture: two-second directed tracks both ways, three-second
normal dwell, one manual train at A facing Forward with velocity 0. Assert through
commands and snapshots after trusted setup.

| Time | Operation / committed observation | Velocity |
| ---: | --- | ---: |
| 0 | Initial Dwelling at A, 3 remaining, Forward | 0 |
| 1 | Step; Dwelling at A, 2 remaining | 0 |
| 1 | Accelerate succeeds early; Moving A → B, elapsed 0 | 1 |
| 2 | Step; Moving A → B, elapsed 1 | 1 |
| 2 | Duplicate Accelerate returns `Ok(())`; exact same state, direction, and elapsed 1 | 1 |
| 3 | Step; arrives B, fresh Dwelling, 3 remaining, still Forward | 0 |
| 6 | Three steps without input; Ready at B, still Forward | 0 |
| 7 | Another step; still Ready at B, unchanged | 0 |
| 7 | Accelerate succeeds; core reverses to Backward, Moving B → A, elapsed 0 | 1 |
| 8 | Step; Moving B → A, elapsed 1 | 1 |
| 9 | Step; arrives A, fresh Dwelling, 3 remaining, still Backward | 0 |
| 9 | Accelerate succeeds immediately; core reverses to Forward, Moving A → B, elapsed 0 | 1 |

This proves early acceleration, deliberate waiting, a complete round trip,
automatic stopping, fresh dwell, and idempotent repeated input while Moving
without graphics.

## 13. Future Physical Constraints / SPEC-003 Boundary

Consider A — B — C, with a manual train stopped at B and an automatic train at A.
The automatic train's completed dwell cannot override future physical constraints.
Possible models include refusing departure as Blocked, or permitting segment
entry and later stopping before an occupied resource. This spec selects neither.

**SPEC-003: Railway Occupancy / Blocking / Physical Safety** will define resources,
occupancy, blocking, and where movement may stop. These rules must govern both
control modes, including automatic departures that do not use player commands.

Manual validation can gain physical checks between candidate selection and commit.
Rejections such as Blocked may be added then; automatic stepping must use the same
applicable constraints. No safety trait, placeholder variant, or resource registry
is required now. Source matches may need extension; frozen API/serialization
compatibility is not promised. The intent boundary itself remains unchanged.

## 14. Non-Goals and Risks

- MVP acceptance is not a collision-free guarantee; safety rules remain unimplemented.
- Supported setup uses unique train IDs, valid station/track references, positive
  durations, and representable counters. Exhaustive constructor validation is deferred.
- Runtime topology/time mutation through legacy public fields is unsupported.
- No-outgoing-track Accelerate commands reject atomically from either station state;
  existing automatic invalid-topology panic behavior need not change in this slice.
- Normal execution creates Ready only for Manual. Fabricated automatic-ready states
  are outside supported setup; snapshots must not conceal invalid domain state.
- Stored velocity duplicates the stopped/moving distinction in TrainState. Keep
  them consistent on every transition; this requires no physics model. Existing
  tests that fabricate Moving state must also set velocity to 1, retaining their
  timing assertions. Inconsistent fabricated state is outside supported setup;
  no new command error or automatic normalization is required.
- Initial dwell remains hardcoded and arrival dwell policy-based; cleanup is separate.
- Adding Ready requires exhaustive-match updates, including a failing arm in the
  five-station test helper, not weakened assertions.

## 15. Educational Implementation Plan

Each checkpoint starts with tests, implements only its stated change, runs focused
verification, and stops for review. No placeholder public methods or `todo!()`
branches. Run existing core tests throughout; checkpoint 7 is not permission to
defer regressions. All paths below are relative to `metro-core/`.

### Checkpoint 1 — Represent control mode without changing behavior

- **Goal:** Make automatic configuration explicit.
- **Tests first:** Assert `Train::new()` selects Automatic; retain constructor tests.
- **Likely files:** `src/train.rs`.
- **Smallest production change:** Add `TrainControl` and stored mode defaulting to
  Automatic. Do not expose manual construction or alter stepping yet.
- **Expected behavior:** Existing scenarios behave identically.
- **Stop condition:** Constructor and existing core tests pass; no command behavior.
- **Commit:** `feat(core): represent train control mode`

### Checkpoint 2 — Represent velocity without changing timing or behavior

- **Goal:** Make stopped versus traversing explicit for existing automatic trains.
- **Tests first:** Constructor velocity 0; automatic movement begins at velocity 1
  and elapsed zero, remains 1 during traversal, and arrives at velocity 0 with
  fresh dwell. Check copied snapshot velocity and unchanged timing.
- **Likely files:** `src/train.rs`, `src/simulation.rs`, `src/snapshot.rs`, and
  existing tests with directly constructed Moving states or snapshot literals.
- **Smallest production change:** Add stored velocity and snapshot mapping; update
  existing movement/arrival transitions together to preserve the invariant.
- **Expected behavior:** Existing automatic behavior remains unchanged; no manual
  command API yet. Adjust fixture velocity only, retaining behavioral assertions.
- **Stop condition:** All reachable states have consistent velocity; core tests pass.
- **Commit:** `feat(core): represent binary train velocity`

### Checkpoint 3 — Represent manual station waiting

- **Goal:** Preserve normal dwell progress without forcing automatic departure.
- **Tests first:** Manual initialization, dwell countdown, Ready on step three,
  unchanged station/direction and velocity 0 on later steps, and owned Ready snapshots.
- **Likely files:** `src/train.rs`, `src/simulation.rs`, `src/snapshot.rs`, new
  `tests/manual_control.rs`, exhaustive helper in `tests/tiny_metro_scenario.rs`.
- **Smallest production change:** Add manual constructor, domain/snapshot Ready,
  and manual dwell-completion branch. Snapshot mapping lands with reachable state.
- **Expected behavior:** Manual trains wait; automatic trains still depart on time.
  This checkpoint provides no input API; Accelerate remains checkpoint 5.
- **Stop condition:** All reachable states can be observed; existing tests pass.
- **Commit:** `feat(core): represent manual station waiting`

### Checkpoint 4 — Define Accelerate command and error DTOs

- **Goal:** Establish the concrete command contract.
- **Tests first:** Small construction/equality test for command and error values.
- **Likely files:** New `src/command.rs`, `src/lib.rs`.
- **Smallest production change:** Export the single command and three errors.
- **Expected behavior:** No simulation behavior changes and no domain DTO dependency.
- **Stop condition:** Contract tests pass; no stub `apply_command()` is exposed.
- **Commit:** `feat(core): define accelerate command contract`

### Checkpoint 5 — Apply manual acceleration

- **Goal:** Apply Accelerate atomically from either stopped manual station state
  and accept it idempotently while already Moving.
- **Tests first:** Accelerate at time zero and during dwell, acceleration from
  Ready, core-selected reversal, and the three rejection cases: `UnknownTrain`,
  `NotManual`, and `NoOutgoingTrack`. Include an automatic Moving train to prove
  control-mode validation precedes the idempotent return. Cover no route from
  both Dwelling and Ready. Test duplicate Accelerate on a manual Moving train
  both at elapsed zero and after traversal has advanced; require `Ok(())` and
  an exact no-op. For each rejection and idempotent success, compare before/after
  snapshots and domain state, proving unchanged train state, traversal elapsed
  time, other domain timers, velocity, direction, simulation time, and unrelated trains.
- **Likely files:** `src/simulation.rs`, `tests/manual_control.rs`.
- **Smallest production change:** Complete `apply_command()` with validation and
  immediate idempotent return before station/route validation, and departure
  commit in section 7's order; extract a private domain-valued track-selection
  helper only if useful.
- **Expected behavior:** Successful departures set velocity 0 → 1 and enter Moving
  at zero elapsed. Already Moving returns `Ok(())` without any mutation or speed
  increase; Dwelling is not an error, and failed reverse lookup cannot change direction.
- **Stop condition:** All command branches work; existing automatic tests pass.
- **Commit:** `feat(core): apply manual acceleration`

### Checkpoint 6 — Prove arrival resets velocity

- **Goal:** Prove automatic stopping and repeated manual actions compose correctly.
- **Tests first:** After early Accelerate, traversal arrival sets velocity 1 → 0
  and starts a full dwell at elapsed zero; abandoned dwell never carries over.
  Accelerate again immediately after arrival. Include a one-second track to prove
  the next step arrives and consumes no dwell second.
- **Likely files:** `tests/manual_control.rs`.
- **Smallest production change:** None expected: checkpoint 2 already maintains
  velocity on shared arrival. Fix only demonstrated contract defects.
- **Expected behavior:** Every arrival stops the train; the next Accelerate starts
  a new traversal without hidden pending input.
- **Stop condition:** Repeated manual traversal and fresh dwell tests pass.
- **Commit:** `test(core): verify manual arrival and velocity reset`

### Checkpoint 7 — Vertical slice, determinism, and compatibility

- **Goal:** Close the MVP with focused regression evidence.
- **Tests first:** Encode the A↔B trace in section 12. Compare identical command/step
  traces with sparse versus frequent observations, results at matching boundaries, and retained snapshots across a
  command. Exercise command-before-step versus step-before-command timing explicitly.
  Reuse the existing automatic suite and five-station timing assertions.
- **Likely files:** `tests/manual_control.rs`; optionally `tests/snapshot_observation.rs`.
- **Smallest production change:** None expected; no new infrastructure.
- **Expected behavior:** Ordered inputs determine results; observation has no effect.
- **Stop condition:** Section 6 is satisfied and `cargo test -p metro-core` passes.
- **Commit:** `test(core): verify command timing and automatic compatibility`

## 16. PR / Commit Strategy

Use one core-only MVP PR with the seven educational commits, remaining draft while
checkpoints are reviewed. Each checkpoint must compile and pass its relevant tests.
Do not collapse checkpoints or start the next without the agreed review step.
The PR should describe Accelerate's idempotent intent semantics, binary velocity, early movement, waiting,
automatic stopping on arrival, unchanged automatic timing,
and focused verification. Bevy integration and SPEC-003 are separate work.

## 17. Decisions Before Implementation

Selected: exactly one player action, `Accelerate { train_id }`; binary velocity;
synchronous `Result<(), CommandError>`; fixed per-train control mode; automatic default; explicit manual constructor; early
manual acceleration; idempotent `Ok(())` without mutation for Accelerate while Moving;
automatic stopping on arrival; Ready retained as a dwell-completion
fact; existing core routing; unchanged one-second steps; core-only round-trip proof.

No unresolved technical decision blocks this MVP. The velocity/state duplication
is resolved by the invariant in section 8, without expanding physics scope.
Future Brake, multiple speeds, mid-track stopping, braking distance, and speed
limits are not acceptance requirements. Physical resource definitions,
departure blocking versus later stopping, and future safety observations remain
deliberately undecided for SPEC-003. Keyboard mappings and Bevy scheduling belong
to a later adapter milestone. Spec review does not begin implementation.
