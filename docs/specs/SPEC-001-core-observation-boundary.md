# SPEC-001: Core Observation Boundary

## 1. Status

**Proposed**

SPEC-001 establishes a small, owned observation contract between `metro-core` and its consumers. It preserves the existing one-second simulation step and makes train state observable without exposing internal model objects as the consumer contract.

This document specifies future implementation. Its creation does not authorize implementation. The educational plan below requires a review checkpoint after every step.

MUST and MUST NOT denote requirements; SHOULD denotes a recommendation; MAY denotes an optional choice. Requirements apply to the completed feature unless explicitly identified as a follow-up or optional presentation work.

## 2. Motivation

`metro-core` and `metro-bevy` currently both contain train behavior. The Bevy toy prototype proved that a train composed of body and base/wheels sprites can move, reverse, stop, and bounce. Its frame-driven operational behavior MUST NOT become a second simulation engine.

Consumers need a stable answer to “What is the simulation state now?” without depending on the core's internal state representation. Visualization, CLI debugging, deterministic tests, and future batch analysis should share that observation boundary while choosing different presentation and sampling strategies.

## 3. Design Principles

```text
metro-core owns simulation truth.
snapshots expose simulation truth.
consumers project simulation truth.
```

```text
metro-core -- step() --> simulation truth -- snapshot() --> SimulationSnapshot
                                                               |
                                                        Bevy / CLI / Analysis
```

- Core MUST determine arrival, departure, dwell completion, direction reversal, traversal timing, and railway rules.
- Observation MUST NOT change simulation execution, elapsed time, or present or future randomness.
- Presentation MUST NOT feed coordinates, frame deltas, orientation, or animation state back into operational behavior.
- The boundary MUST use domain IDs and integer simulation durations, not rendering types.
- The contract SHOULD contain only facts needed by the current slice. It MUST NOT become a model dump, event log, or analytics framework.

## 4. Current State

This section describes the inspected implementation, not additional requirements or planned features.

### Simulation

[`simulation.rs`](../../metro-core/src/simulation.rs) defines `Simulation` with public `elapsed_seconds: u64` and `network: Network`, and private `trains: Vec<Train>` and `dwell_policy: DwellPolicy`. `trains()` returns an immutable slice of internal trains. The public time and network fields can be changed by callers.

`step()` increments simulation time by one second, then processes each train once in vector order. Departure produces `Moving` with travel elapsed time zero. Arrival produces dwelling with dwell elapsed time zero. Departure does not also consume a second of travel; arrival does not also consume a second of dwell.

After dwelling, a train takes the next track in its current direction. If none exists, core tries the reverse direction and changes direction when departing. Missing tracks can cause panics. There is no RNG or seed API.

### Train and station

[`train.rs`](../../metro-core/src/train.rs) exposes `TrainId`, `Direction`, `Train`, `TrainState`, and `AtStationState`. Train fields are public: ID, capacity, state, and direction. Capacity currently has no behavioral effect.

`TrainState` is either `AtStation { station, state }` or `Moving { from, to, elapsed_seconds }`. The only station substate is `Dwelling { elapsed_seconds, dwell_seconds }`. `Train::new` starts dwelling for three seconds. There is no physical position, distance, velocity, or acceleration.

[`station.rs`](../../metro-core/src/station.rs) defines `StationId(pub usize)` and stations with an ID and name. `TrainId` also wraps `usize`.

### Network and dwell

[`network.rs`](../../metro-core/src/network.rs) stores stations privately in insertion order and directed tracks privately in a `HashMap` keyed by `(StationId, StationId)`. A track exposes `from`, `to`, and `travel_seconds`. Public APIs support construction, station count, exact track lookup, and next-track lookup. There is no public station enumeration/name lookup API.

`next_track()` follows adjacent station indices: increasing for `Forward`, decreasing for `Backward`. It is not general graph routing. Adding a track with the same endpoint pair replaces the previous entry. Zero-duration tracks are currently accepted.

[`dwell.rs`](../../metro-core/src/dwell.rs) always returns three seconds. Arrival uses this policy; initial dwell is separately hardcoded in `Train::new`.

### Bevy prototype

[`metro-bevy/src/main.rs`](../../metro-bevy/src/main.rs) owns a separate `Train` component with floating-point direction and remaining dwell. `move_train` uses frame delta, a speed of 100 pixels/second, bounds at -300 and 300, and a one-second dwell. It detects arrival from coordinates, reverses immediately on arrival, and flips the visual root. It begins moving immediately, unlike core's initial dwell.

`bounce_train` animates the body independently, including while stopped. The body and base/wheels are child sprites. `metro-bevy` already declares a dependency on `metro-core`, but main does not use it.

### Existing integration scenario

[`tiny_metro_scenario.rs`](../../metro-core/tests/tiny_metro_scenario.rs) builds five stations A–E and two opposing trains. Bidirectional segment times are 180, 120, 240, and 180 seconds. Tests cover initial dwell, departure, arrival at opposite endpoints at second 732, reverse departure at second 735, and return progress at second 736.

This fixture MUST remain unchanged. The separate A–B demo MUST NOT reinterpret B as a terminal of that five-station network.

## 5. Responsibility Boundary

| Concern | Authority / responsibility |
| --- | --- |
| Elapsed simulation time | Core; one second per `step()` |
| Train state and transitions | Core; snapshot copies current facts |
| Station identity | Core domain IDs; consumers associate labels/layout separately |
| Active track endpoints | Core; directed `from` and `to` |
| Direction | Core; consumer observes `Forward` or `Backward` |
| Dwell duration/completion | Core; snapshot reports remaining seconds |
| Traversal timing | Core; snapshot reports elapsed and total seconds |
| Temporal progress ratio | Consumer may derive it from observed timing; it cannot trigger transitions |
| World/pixel coordinates | Consumer layout only |
| Visual interpolation | Consumer presentation only |
| Sprite orientation | Bevy derives it from observed direction and layout |
| Body bounce | Bevy visual animation only |
| Wall-clock pacing | Application driver decides when to request whole core steps |

The Bevy application MAY own a `Simulation` in a resource and invoke `step()`. Rendering systems SHOULD receive snapshots rather than internal trains. Only the driver should advance the engine; it MUST NOT decide railway transitions.

## 6. Observation Contract

### Conceptual public API

The following is illustrative Rust. Minor module placement or Rust-level adjustments MAY be made during reviewed implementation without changing these semantics. A small `metro_core::snapshot` module is the proposed location for DTOs.

```rust
use crate::station::StationId;
use crate::train::{Direction, TrainId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimulationSnapshot {
    pub elapsed_seconds: u64,
    pub trains: Vec<TrainSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrainSnapshot {
    pub id: TrainId,
    pub direction: Direction,
    pub state: TrainSnapshotState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrainSnapshotState {
    Dwelling {
        station: StationId,
        remaining_seconds: u64,
    },
    Moving {
        from: StationId,
        to: StationId,
        elapsed_seconds: u64,
        travel_seconds: u64,
    },
}

impl Simulation {
    pub fn step(&mut self); // Existing API and semantics, unchanged.
    pub fn snapshot(&self) -> SimulationSnapshot;
}
```

### Ownership and observation

`snapshot()` MUST return owned data with no references into `Simulation`. It MUST NOT expose internal `Train`, `TrainState`, `Track`, or `Network` objects as the observation contract. Existing simple ID and direction types MAY be reused.

The method MUST have no side effects: it MUST NOT advance time, mutate state, execute transitions, call behavior to predict a departure, or consume randomness. It MUST copy the committed state at time zero or after a completed step.

Repeated observations of an unchanged simulation MUST compare equal. A retained snapshot MUST remain unchanged after future steps or subsequent observations. “Read-only observation” describes the boundary: callers can edit their owned copy, but that MUST have no effect on the simulation or any other snapshot.

### Ordering and identity

The train vector MUST preserve the simulation's existing train insertion order. It MUST NOT depend on hash iteration order. Consumers MUST use `TrainId` to associate a train with a visual entity or record; vector position is not identity.

IDs MUST remain stable within a simulation run. This specification does not promise globally unique IDs or identity across differently constructed runs. Station IDs refer to the network used to construct the simulation. A directed track is identified by `(from, to)`; reverse travel uses the reverse pair. A new `TrackId` is not needed for the current model.

### State semantics

- `SimulationSnapshot.elapsed_seconds` MUST equal the observed core elapsed time.
- `TrainSnapshot.direction` MUST equal core's current direction, including during dwell.
- `Dwelling.station` MUST identify the occupied station.
- For valid dwelling state, `remaining_seconds` MUST equal `dwell_seconds - elapsed_seconds`: the number of future one-second steps until departure. It is not wall-clock time. With current behavior this counts down 3, 2, 1, then becomes `Moving`; there is no intervening zero-remaining dwelling snapshot.
- `Moving.from` and `Moving.to` MUST identify the active directed connection. Neither endpoint represents a station currently occupied by the moving train.
- Moving elapsed seconds MUST come from the train; total travel seconds MUST come from its active core track. Consumers MUST NOT need internal track lookup to calculate temporal progress.
- The snapshot MUST NOT predict a next station while dwelling. Core chooses the outgoing track at departure.

Capacity, station names, network catalogs, policy objects, and prospective future states are omitted. Demo setup can retain station IDs and presentation labels when constructing the network. Future metadata access can be considered separately when a consumer needs it.

## 7. Position and Progress Semantics

For a valid moving observation:

```text
progress = elapsed_seconds / travel_seconds
```

Division here denotes a ratio, not truncating Rust integer division. A renderer MAY convert the integers to floating point locally. The contract itself preserves integer timing.

Progress means **fraction of configured traversal TIME completed**. It MUST NOT be described as physical distance traveled. Core currently has no distance or motion model.

For positive travel duration and valid state:

- Departure is moving with elapsed time zero.
- While moving, `0 <= elapsed_seconds < travel_seconds`.
- Completion changes the state to dwelling at the destination. A moving snapshot at progress one is not emitted by normal stepping.

Bevy owns `StationId -> visual coordinates`. It MAY place a dwelling train at its station and a moving train at `lerp(layout[from], layout[to], progress)`. This spatial mapping of the current snapshot is part of direct projection; it does not require smoothing between successive snapshots.

Changing layout distances MUST NOT change travel duration, arrival, or departure. Pixels, sprite dimensions, Bevy transforms, animation, and frame interpolation MUST NOT enter core types or `step()`.

## 8. Determinism Contract

Given identical valid initial state and configuration, after the same number of simulation steps, snapshots MUST be equal regardless of observation frequency or renderer frame frequency. This assumes callers do not externally mutate simulation fields between steps.

For two identically initialized runs:

```text
Run 1: snapshot A -> step -> snapshot B
Run 2: snapshot A -> snapshot A -> snapshot A -> step -> snapshot B
```

Both runs MUST produce equal B values and equal subsequent results. Omitting all intermediate observations MUST also preserve the final result.

`step()` MUST retain its existing one-second semantics. Frame delta MUST NOT be passed into core stepping or used to alter train timers. Calling one core step per render frame is not a valid real-time pacing strategy.

A Bevy driver MAY accumulate wall-clock time and request zero, one, or multiple whole steps between rendered frames. This only schedules calls; it does not define the duration or behavior of a core step. Pending work MUST NOT be silently dropped or replaced by variable-duration steps. If catch-up is limited, pending work MUST remain pending. Results are compared at equal simulation step counts, not at a presumed identical wall-clock instant on different machines.

The first integration MUST directly project the latest snapshot. One-second visual increments are expected. Rendering more often without stepping MUST leave logical train position unchanged.

There is currently no RNG/seed API, and none is introduced here. If future execution uses randomness, observations MUST NOT draw from or otherwise affect it. The current step uses vector order and exact-key map lookups; it does not depend on track hash iteration order.

## 9. Consumer Model

### Bevy

A small application driver owns the simulation, advances whole steps, and publishes observations. Presentation associates `TrainId` with a visual entity and maps observed state to transforms. Frame-time body animation is allowed because it cannot change core state.

### CLI and debugging

A caller MAY step explicitly and print IDs, direction, state, time, and remaining dwell or traversal time. It does not need Bevy, rendering coordinates, or access to internal trains. Building a CLI executable is not required by this spec.

### Batch and Monte Carlo analysis

A runner MAY observe only the final state, periodically sample states, or inspect individual runs. It MUST NOT be required to allocate or retain a snapshot after every step. Current identical runs have no stochastic variation.

Snapshots answer “What is true now?” They cannot alone report historical arrival counts or accumulated operational metrics. Possible future `SimulationEvent` (“What just happened?”) and `SimulationMetrics` (“What happened over the run?”) are distinct, out-of-scope concepts. Future analysis MAY combine final or sampled snapshots with events or aggregates. This spec neither implements nor commits to those APIs.

No consumer can mutate simulation truth through snapshots.

## 10. First Vertical Slice

The first demo MUST build a separate A <-> B network using existing construction APIs, with one train initially at A facing `Forward`. Use six seconds in each direction for the example and the current three-second core dwell. Capacity can retain the existing conventional value of 100; it has no effect here.

```text
Station A ---------------- Station B
                 train
```

Core MUST supply initial dwell, departure, travel, arrival, subsequent dwell, reverse departure, and return movement. Bevy supplies station coordinates, markers, train sprite composition, and observed-state projection. No shared scenario framework is required; small fixture setup may exist in the core test and Bevy demo independently, with no duplicated behavior rules.

| Core elapsed seconds | Snapshot state | Direction |
| --- | --- | --- |
| 0 | Dwelling at A, remaining 3 | Forward |
| 1 | Dwelling at A, remaining 2 | Forward |
| 2 | Dwelling at A, remaining 1 | Forward |
| 3 | Moving A -> B, elapsed 0 / total 6 | Forward |
| 4 | Moving A -> B, elapsed 1 / total 6 | Forward |
| 8 | Moving A -> B, elapsed 5 / total 6 | Forward |
| 9 | Dwelling at B, remaining 3 | Forward |
| 11 | Dwelling at B, remaining 1 | Forward |
| 12 | Moving B -> A, elapsed 0 / total 6 | Backward |
| 13 | Moving B -> A, elapsed 1 / total 6 | Backward |
| 18 | Dwelling at A, remaining 3 | Backward |
| 21 | Moving A -> B, elapsed 0 / total 6 | Forward |

At departure the state changes before visual position changes: progress zero is still at the departure coordinate. The first visible outbound position increment is at second 4.

The existing five-station/two-train integration scenario MUST remain unchanged and pass alongside the new fixture.

## 11. Migration of metro-bevy

Once direct projection is connected, the following prototype logic MUST be deleted:

- Local operational direction and `dwell_remaining` fields.
- Dwell countdown based on frame delta.
- `SPEED` and `DWELL_TIME` constants.
- Coordinate-bound arrival checks and operational clamping.
- Locally decided direction reversal.
- The current `move_train` movement simulation.

The following remain presentation responsibilities:

- Station coordinates (including -300 and 300 if desired).
- Body and base/wheels sprite composition.
- Sprite orientation derived from snapshot direction and the chosen layout.
- Body bounce, optionally active only while observed moving.
- Optional later interpolation between observations.

For this horizontal demo, Forward maps to facing right and Backward to facing left. The visual MUST NOT reverse early during terminal dwell. A visual entity's core ID association replaces its operational state component. Core timers MUST NOT be copied into independently ticking Bevy components.

## 12. Non-Goals

SPEC-001 does not introduce:

- Passengers or passenger behavior.
- Schedules or timetables.
- Signaling, collisions, or block occupancy.
- Breakdowns.
- UI controls or dashboards.
- Persistence, serialization formats, networking, or replay.
- Generalized plugin systems or an ECS redesign of core.
- Analytics, event, or metrics frameworks.
- RNG/seed APIs or stochastic behavior.
- Physical distance, velocity, acceleration, or motion models.
- General graph routing or new track identity abstractions.
- A redesign of `Simulation::step()`.
- Broad input-validation or encapsulation refactors.

Smooth presentation is optional later work, not a requirement for the first core-driven demo.

## 13. Invariants and Risks

The first implementation MUST support valid states produced by the current engine from the following valid setup assumptions. It MUST NOT silently repair invalid state or invent observations to conceal it. Comprehensive constructor validation and an error API are follow-up hardening, not prerequisites for defining the DTOs or implementing observations for valid runs.

| Invariant / risk | Requirement now | Follow-up hardening, not required for this slice |
| --- | --- | --- |
| Positive travel duration | Fixtures and supported moving states MUST use `travel_seconds > 0`; otherwise temporal progress is undefined. | Reject zero-duration tracks at construction. |
| Unique train identity | A run MUST use unique `TrainId` values so observations can be associated reliably. | Validate uniqueness in construction. |
| Valid state and references | Station IDs and moving endpoints MUST resolve; moving elapsed time MUST be below total time; dwelling MUST have positive total time and elapsed below total. Snapshot code MUST NOT substitute a default duration or fabricated track. | Broader state validation and structured errors; focused assertions may document assumptions without redesigning constructors. |
| Public mutable simulation fields | Consumers MUST NOT edit time or topology to drive behavior. Snapshot purity is guaranteed for observation; determinism assumes no external mutation. | Making fields private MAY be reviewed separately. It is not necessary to copy owned observations and MUST NOT block PR1. |
| Active-track duration lookup | Snapshot MUST copy the duration from the actual active track. Supported runs assume topology/timing are not externally changed during execution. | Define runtime topology editing only if later needed. |
| Initial dwell inconsistency | Preserve existing three-second initialization and policy behavior. Snapshot MUST report stored dwell state, not call the policy again. | Unify initialization with policy in a separate change. |
| Direction semantics | Preserve index-based Forward/Backward and reversal at departure. Missing next track triggers reversal, even at an interior gap; no track either way can panic. | Explicit terminals or routing rules are separate design work. |
| Temporal progress | MUST be described as traversal-time fraction, never physical distance. | A physical motion model would require its own specification. |
| Finite integer counters | Supported runs must stay within representable `u64` time. | Overflow policy is separate hardening. |

IDs are stable within a run, not a persistence format. Owned snapshot allocation is acceptable for the first boundary; performance work SHOULD follow measured need. Snapshot sampling is optional, so a batch run need not pay this cost every tick.

## 14. Acceptance Criteria

The core contract is accepted when:

1. DTOs contain only owned domain values, IDs, and integer times; no Bevy or rendering types appear in the core API.
2. `snapshot()` observes through `&self`, does not change exposed core time/train state, and repeated calls without steps compare equal.
3. A retained snapshot stays equal to its saved expected value after later steps. Editing a caller-owned snapshot does not change a fresh observation.
4. Dwelling observations count remaining steps correctly, including departure on the third step of the current default dwell and a separately configured dwell duration.
5. Moving observations include both endpoints and elapsed/total travel seconds; tests cover departure at zero, an intermediate fraction, and arrival as dwelling. A one-second track also arrives after exactly one moving step.
6. Snapshot direction matches core, remaining unchanged during terminal dwell and reversing on departure.
7. Train ordering preserves construction order, including when IDs are not numerically sorted.
8. Two independently built identical runs produce equal results at matching step counts when one is observed repeatedly and the other sparsely or only at the end.
9. Existing core tests, including the unchanged five-station integration scenario, pass.

The first Bevy vertical slice is accepted when:

10. The separate one-train, six-second A–B fixture matches the timeline in section 10.
11. The driver produces the same per-step snapshot sequence under different synthetic frame/update partitions that request the same total number of steps, including an interval requiring multiple steps. Test collection may capture each tick even if rendering publishes only the latest one.
12. Bevy displays the latest snapshot directly; additional render frames without a core step do not advance the logical root position.
13. No Bevy code decides arrival, departure, dwell completion, traversal duration, or reversal. The listed toy simulation logic is removed.
14. Orientation follows observed direction. Body animation does not change the root's logical position or any core state.

Optional smoothing is accepted separately and MUST preserve all core determinism tests. No acceptance criterion requires new events, metrics, analytics, or generalized hardening.

## Educational Implementation Plan

This plan is not an instruction to begin implementation. After spec review, work proceeds one numbered step at a time. At every checkpoint: inspect the change, run the relevant tests, discuss the result, commit only that checkpoint, and STOP. Do not automatically continue into the next step.

The first four checkpoints deliberately separate DTO definition from observation mechanics. To avoid shipping a public `snapshot()` that panics on a normal moving train, early dwelling conversion can be exercised through a private helper in `simulation.rs`; the complete public method arrives in Step 4. No `todo!()` branch or fabricated moving observation is an acceptable checkpoint.

### Step 1 — Define owned snapshot DTOs

**Goal:** Define the vocabulary consumers will receive without observing or changing a simulation yet.

**Concept:** Owned data, enums, domain IDs, public modules, and derived equality/debug traits.

**Files likely touched:** New `metro-core/src/snapshot.rs`; `metro-core/src/lib.rs`.

**Change:** Add only the three proposed DTOs and expose their module. Reuse existing ID and direction types. Do not add `Simulation::snapshot()` yet.

**Tests first:** Add one small test constructing dwelling and moving DTO values, retaining an owned clone, and comparing equality. This is a compile-time/API learning check, not a test of simulation behavior.

**Expected observation:** DTOs can be constructed and compared without a simulation, Bevy, or new dependencies. `cargo test -p metro-core` remains green.

**Stop condition:** Review every field and its meaning; verify only the DTO module and module export changed; stop before adding observation logic.

**Suggested commit:** `feat(core): define owned snapshot DTOs`

### Step 2 — Observe an initial dwelling train

**Goal:** Translate an actual initial train state into its owned dwelling observation.

**Concept:** Borrowing internal state while returning owned domain data; separating state from behavior.

**Files likely touched:** `metro-core/src/simulation.rs` only.

**Change:** Add a narrowly scoped private dwelling-conversion helper that will be used by the full snapshot method. Pass stored station/dwell values; do not hardcode three seconds or re-run the policy. Keep this helper private and exercise it in the module's tests.

**Tests first:** Construct a train with the existing constructor and verify the helper reports its initial station and three remaining seconds without changing the train.

**Expected observation:** Initial core state becomes a `Dwelling` DTO; no simulation step occurs and no public partial observation API exists.

**Stop condition:** Review where each copied value comes from, run core tests, and stop before testing advancing dwell or exposing `snapshot()`.

**Suggested commit:** `feat(core): map initial dwell to observation data`

### Step 3 — Specify remaining dwell semantics

**Goal:** Verify that remaining dwell means future core steps until departure.

**Concept:** State-machine boundaries, integer countdowns, and off-by-one reasoning.

**Files likely touched:** `metro-core/src/simulation.rs` tests and the private helper only if necessary.

**Change:** Refine the existing helper only if tests reveal a problem; otherwise make this a tests-only checkpoint. Do not alter stepping or dwell policy.

**Tests first:** Observe stored dwelling values before stepping and after one and two steps: remaining 3, 2, 1. On the third step verify internal state becomes moving. Add a five-second stored dwell case to prove conversion does not assume the default policy.

**Expected observation:** Countdown reflects stored state; departure timing remains the engine's existing behavior.

**Stop condition:** Explain why there is no zero-remaining dwelling snapshot in a normal run, run core tests, and stop before moving conversion.

**Suggested commit:** `test(core): specify remaining dwell observation semantics`

### Step 4 — Complete snapshot observation and travel progress

**Goal:** Expose a complete `Simulation::snapshot()` that supports every current train state.

**Concept:** Exhaustive enum matching, read-only lookups, owned collection construction, and temporal progress.

**Files likely touched:** `metro-core/src/simulation.rs`; new `metro-core/tests/snapshot_observation.rs`.

**Change:** Add the full public method, using dwelling conversion and active-track lookup for moving observations. Copy time, ID, and direction; preserve train vector order. Do not change `step()` or field visibility.

**Tests first:** Check empty simulation, initial dwell, departure at elapsed zero, intermediate movement, and arrival; include a one-second track. Use a small local two-station setup without importing or modifying the existing five-station fixture. Check two deliberately unsorted train IDs preserve insertion order.

**Expected observation:** Public snapshots alone expose logical state and sufficient timing for progress; no caller track lookup is needed.

**Stop condition:** All current state variants work with no placeholder branches; core tests pass; review the public API and stop before Bevy work.

**Suggested commit:** `feat(core): expose complete simulation snapshots`

### Step 5 — Prove observation independence

**Goal:** Demonstrate that taking or retaining observations cannot change execution.

**Concept:** Purity, ownership, observational determinism, and comparisons between independently initialized runs.

**Files likely touched:** `metro-core/tests/snapshot_observation.rs`.

**Change:** Add focused behavioral tests; change production code only if they expose a contract defect.

**Tests first:** Compare repeated snapshots with no steps; retain one across several steps; edit a local copy and compare a fresh observation. Run identical simulations with frequent versus sparse/no intermediate observations and compare results at matching step counts through arrival and reversal. Check exposed core time/state before and after observation as well.

**Expected observation:** Observation count and ownership of old values have no effect on present or future core results.

**Stop condition:** Explain why `&self` alone is not the complete purity proof, review the absence of hidden mutation, run core tests, and stop. PR1 can end here.

**Suggested commit:** `test(core): verify snapshot purity and determinism`

### Step 6 — Establish the A–B core demonstration timeline

**Goal:** Prove the exact one-train shuttle behavior before connecting a renderer.

**Concept:** Scenario tests as an executable behavioral specification.

**Files likely touched:** New `metro-core/tests/two_station_observation.rs`.

**Change:** Build a separate fixture with existing APIs: A, B, a bidirectional six-second connection, and one train. Do not add a production scenario framework or change the five-station fixture.

**Tests first:** Assert the section 10 timeline using public snapshots, including initial dwell, both directions, and direction retained during terminal dwell.

**Expected observation:** The round trip and second outbound departure are proven headlessly at seconds 18 and 21 respectively.

**Stop condition:** Read the timeline against test output/assertions, run core tests, confirm the original integration file is untouched, and stop before Bevy integration.

**Suggested commit:** `test(core): specify the two-station snapshot demo`

### Step 7 — Host a real simulation in Bevy

**Goal:** Instantiate and pace the real A–B simulation inside the Bevy application.

**Concept:** Resource ownership, application pacing versus simulation time, and ordered systems.

**Files likely touched:** `metro-bevy/src/main.rs` only; the core dependency already exists.

**Change:** Construct the same simple setup locally, retain station IDs, own the simulation in a Bevy resource, and publish an initial snapshot plus snapshots after whole steps. Add a small pacing path that can handle multiple due steps and retain residual time. Disable registration of the toy movement system at this checkpoint; leave its deletion to Step 8. Rendering is temporarily static and is not yet evidence of integration.

**Tests first:** Test the driver without opening a window using synthetic elapsed durations. Compare snapshots after each step for different partitions with equal total due steps, including a long interval requiring catch-up. Check that an interval below one tick does not step.

**Expected observation:** Core time and state advance according to the tested timeline inside the application; the sprite remains static because projection is not connected. Core behavior never uses frame delta directly.

**Stop condition:** Run `cargo test -p metro-core` and `cargo test -p metro-bevy`; inspect the driver and confirm toy movement is disabled. Stop before transform projection.

**Suggested commit:** `feat(bevy): host and pace the core shuttle simulation`

### Step 8 — Project snapshots and delete toy simulation

**Goal:** Make the train's screen position come exclusively from observed core state.

**Concept:** Projection, identity association, and removal of duplicate authority.

**Files likely touched:** `metro-bevy/src/main.rs` only.

**Change:** Associate the visual root with `TrainId`, add A/B station markers and layout coordinates, and project the latest snapshot after the driver publishes it. Map dwell to a station and moving time progress to the line. Delete the disabled `move_train`, operational direction/dwell fields, `SPEED`, `DWELL_TIME`, bound checks, and local reversal decisions. Do not add smoothing.

**Tests first:** Check projection at A, halfway A -> B, B, and halfway B -> A. Verify repeated projection of the same snapshot cannot advance position. Keep driver determinism tests passing.

**Expected observation:** At second 4 the train visibly advances because core reports one of six travel seconds completed. It reaches B at second 9, remains there through dwell, and advances back after reverse departure. Motion is deliberately stepped.

**Stop condition:** Run tests and the app; identify no railway transition logic in Bevy. Stop at the milestone: **“The train on screen is moving because metro-core says it is moving.”** Sprite facing is completed separately in Step 9.

**Suggested commit:** `feat(bevy): project core snapshots and remove toy movement`

### Step 9 — Derive visual orientation and motion cues

**Goal:** Make facing and body animation agree with observed state without creating operational state.

**Concept:** Presentation derived from state and separation of root position from child animation.

**Files likely touched:** `metro-bevy/src/main.rs` only.

**Change:** Map observed direction to the horizontal sprite orientation. Keep the body/base hierarchy; gate body bounce on observed moving state and return the body to its neutral local offset while dwelling. Animation phase may use render time.

**Tests first:** Check facing stays Forward at B during dwell and changes on the reverse-departure snapshot. Check dwelling selects a neutral body offset and animation leaves the projected root position unchanged. Do not test the sine implementation itself.

**Expected observation:** The train flips at second 12, not second 9; its body settles while dwelling. Core snapshots are unaffected by animation frames.

**Stop condition:** Run tests and visually inspect a round trip. Confirm presentation reads observations without changing the simulation, then stop. PR2 can end here.

**Suggested commit:** `feat(bevy): derive train presentation from snapshots`

### Step 10 — Optional later snapshot smoothing

**Goal:** Smooth presentation without changing the proven simulation boundary.

**Concept:** Adjacent samples, presentation delay, and interpolation without extrapolated operational decisions.

**Files likely touched:** `metro-bevy/src/main.rs`, or one small Bevy presentation module if readability requires it.

**Change:** Only after a separate decision to pursue smoothing, retain adjacent snapshots and interpolate observed positions on a presentation timeline one tick behind. Align discrete state and orientation with that timeline. Fall back to direct projection at startup or when samples are missing/nonadjacent. Never write interpolation results to core.

**Tests first:** Cover movement interpolation, arrival, dwell, reversal, and missing-sample fallback. Compare core results with smoothing enabled and disabled at equal step counts.

**Expected observation:** Motion appears smoother with an explicit presentation delay; authoritative simulation times and transitions remain identical.

**Stop condition:** Review the delay/tradeoff and transition behavior, run relevant tests, and stop without adding replay, event buffering infrastructure, or generalized animation systems. Skipping this step still completes the first vertical slice.

**Suggested commit:** `feat(bevy): add optional snapshot interpolation`

## PR / Commit Strategy

| PR | Educational commits | Scope |
| --- | --- | --- |
| PR1 — Core observation contract | Steps 1–5 | DTOs, complete observation, and deterministic semantics; no Bevy changes or broad hardening |
| PR2 — Core-driven Bevy vertical slice | Steps 6–9 | Separate A–B fixture, pacing, direct projection, deletion of toy behavior, and derived presentation |
| PR3 — Optional presentation smoothing | Step 10 | Presentation-only smoothing after the direct boundary is accepted |

PR grouping MUST NOT collapse the educational checkpoints. Each step is reviewed and committed before beginning the next. No implementation begins as part of writing this specification.

## Decisions Before Implementation

No unresolved technical decision blocks Step 1. This proposal selects the DTO fields, existing ID/direction types, insertion-order output, a `snapshot` module, and a six-second demo track. Spec review precedes implementation.

Comprehensive validation, private simulation fields, initialization/policy unification, future events/metrics, and optional smoothing are separate decisions. They MUST NOT be used to expand or delay the DTO checkpoint.
