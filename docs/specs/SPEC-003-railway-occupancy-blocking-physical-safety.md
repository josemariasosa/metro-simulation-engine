# SPEC-003: Railway Occupancy / Blocking / Physical Safety

## 1. Status

**Selected MVP proposal — ready for implementation review; not implemented.**

This document selects the physical-safety contract following the implemented
[SPEC-001 observation boundary](SPEC-001-core-observation-boundary.md),
[SPEC-002 manual control](SPEC-002-manual-train-control-boundary.md), and
[SPEC-004 Bevy presentation](SPEC-004-bevy-snapshot-presentation-mvp.md).
Approval of this written proposal is separate from authorization to begin
Checkpoint 1. No SPEC-003 production code or tests are implemented by this rewrite.

MUST and MUST NOT express requirements; SHOULD expresses a recommendation.
The directional-capacity proposal in section 5 and its reversal consequences in
section 11 are explicit review points, not deferred implementation choices.

## 2. Motivation

Independent train timers cannot prevent incompatible resource ownership. A train
must not depart toward a directional station slot that another train occupies or
has reserved. Checking only current occupancy is insufficient: a destination
must remain protected throughout traversal.

SPEC-003 introduces deterministic discrete occupancy. Admission happens before
departure; admitted trains complete their timed segment without stopping midway.
The model guarantees safety for valid initial state, not progress.

## 3. Architectural Boundary

```text
Automatic operational policy ──┐
                              ├→ candidate departure → physical admission → commit
Manual Accelerate intent ─────┘
```

Physical rules MUST apply equally to both control modes, including early manual
acceleration. `Simulation` coordinates candidate selection, admission, and atomic
transitions. Domain helpers return domain values; they MUST NOT depend on command
or snapshot DTOs. Prefer private concrete helpers in the existing crate over new
traits, generic resolvers, or an event/ECS architecture.

Snapshots observe committed truth. Adapters submit commands and display snapshots;
they do not authorize movement. No Bevy changes are required by this specification.

Out of scope: external incidents, game events, randomness, dwell modifiers,
continuous position, acceleration physics, braking, signaling, subsegment blocks,
platforms, arbitrary capacities, overtaking, shared single-track resources,
branching topology changes, pathfinding, rerouting, dispatch optimization,
fairness, deadlock detection/resolution, multiplayer, and presentation changes.

## 4. Baseline Repository Reality

This section describes the implementation before SPEC-003:

- [`train.rs`](../../metro-core/src/train.rs) defines `TrainControl::{Automatic,
  Manual}`, `TrainState::{AtStation, Moving}`, and
  `AtStationState::{Dwelling, Ready}`. Direction is a separate train field.
  Velocity is binary: station states use 0 and movement uses 1; it does not drive
  traversal timing. Constructors start with three-second dwell.
- [`simulation.rs`](../../metro-core/src/simulation.rs) increments time once per
  `step()`, then mutates trains sequentially in vector order. Its helpers currently
  inspect only the individual train and immutable network/policy, so there is no
  existing cross-train observation or occupancy arbitration.
- Automatic trains depart on dwell completion. Manual trains finish dwell into
  Ready and wait. Ready currently performs no retry, including for fabricated
  automatic Ready states; SPEC-003 introduces supported automatic Ready behavior.
- `apply_command(TrainCommand::Accelerate { train_id })` is synchronous between
  steps. It allows early manual departure, selects current direction or reverse
  fallback, and commits movement immediately. Already-moving manual acceleration
  returns `Ok(())` without mutation. No input is buffered. `RequestDeparture` is
  historical wording, not an API.
- Manual and automatic paths currently duplicate departure selection/commit logic.
  Existing command errors are `UnknownTrain`, `NotManual`, and `NoOutgoingTrack`.
- [`network.rs`](../../metro-core/src/network.rs) keys directed tracks by
  `(StationId, StationId)`. `next_track()` follows adjacent station insertion
  indices, increasing for Forward and decreasing for Backward. It is not general
  graph routing. Reverse edges are separate entries; physical sharing is absent.
- Departure starts traversal elapsed at 0 and consumes no travel second. Arrival
  occurs after the configured number of subsequent one-second steps and starts
  fresh policy dwell at elapsed 0, consuming no dwell second.
- Snapshots are owned, pure DTOs, preserving train vector order. They expose
  direction, velocity, Dwelling/Ready/Moving, and traversal timing, but no control
  mode or blocking reason. `TrainId` supports equality/hashing, not ordering traits;
  arbitration can compare its numeric `.0` value without changing that type.
- Bevy owns a core simulation, runs whole steps, applies Space-triggered Accelerate,
  then publishes a snapshot. Presentation projects both directions onto one visible
  line. SPEC-004 establishes no physical lane policy. Directional station occupancy
  is a new SPEC-003 contract, not a behavior inferred from graphics.

## 5. Physical Resource Model

### Directional Station Capacity — Selected MVP Proposal

A logical station remains one `StationId`. Its physical station resources are
conceptually `StationSlot(station, direction)`, with one Forward slot and one
Backward slot. These are not duplicate station objects such as `B_forward` and
`B_backward`. `StationSlot` is explanatory notation, not a required public type.

Each directional slot MUST permit either one occupant, one reservation, or neither.
It MUST NOT be simultaneously occupied and reserved. A logical station may thus
hold up to two stopped trains, provided their committed directions differ.

Global station capacity one would unnecessarily couple the two independent
movement lanes selected here. Capacity one per direction extends the independent
directed-track model naturally to station occupancy. This assumes the current
linear topology and exactly two directions. It does not model shared platforms,
multiple platforms per direction, overtaking, or configurable station capacity.

A stopped train occupies `(station, train.direction)`. A moving train reserves
`(to, train.direction)`, using its committed travel direction. Thus simultaneous
Forward A→B and Backward C→B movements reserve different slots at B and MUST NOT
conflict merely because their destination `StationId` is equal.

### Directed tracks and availability

`Track(from, to)` identifies one physical directed-track resource, with capacity
one. `Track(A,B)` and `Track(B,A)` are independent and may be occupied together.
No shared single-track identity is introduced.

- **Occupied:** a train is physically at the slot or traversing the track.
- **Reserved:** a moving train exclusively claims its destination slot for arrival;
  that slot is not yet physically occupied.
- **Available:** a track has no occupant, or a slot has neither occupant nor
  reservation. During batch resolution, it must also have no accepted claim.

## 6. Resource Derivation

Committed train state remains the source of ownership:

| Committed train state | Occupied resource | Reserved resource |
| --- | --- | --- |
| AtStation at S, direction D | Slot `(S,D)` | None |
| Moving X→Y, direction D | Track `(X,Y)` | Slot `(Y,D)` |

`Simulation` SHOULD construct a private temporary resource view from all trains,
keeping occupants and reservations conceptually distinct. A persistent duplicate
ownership/reservation registry is unnecessary and MUST NOT be introduced for this
slice. Network remains topology and timing, not mutable train ownership.

The view MUST detect incompatible owners rather than overwrite one with another.
`Simulation::new()` MUST fail fast with an assertion/panic for duplicate TrainIds
or incompatible initial physical ownership, retaining its existing signature.
A structured fallible constructor and exhaustive configuration validation are
separate work. Crate-local validation tests may construct consistent moving trains;
no public train constructor is needed solely for those fixtures.

## 7. Departure Admission and Atomicity

Core selects the current-direction track, falling back to the reverse direction
only when no track exists in the current direction. A physically blocked existing
track MUST NOT cause reverse fallback or rerouting.

The candidate contains the source, destination, and selected travel direction.
Before mutation, admission MUST check both:

1. The outgoing directed track is available.
2. The destination slot in the selected direction is available.

On acceptance, the transition MUST atomically release the original source slot,
occupy the track, reserve the destination slot, commit selected direction, set
velocity to 1, and enter `Moving` at elapsed 0. Derived ownership changes with
that one committed train transition; separate persistent claim writes are not
required. Other trains MUST NOT observe a partially committed departure.

Rejection MUST NOT commit any movement, reversal, velocity change, or claim.
For a manual command it preserves all state exactly. During a step, normal dwell
completion and global time advancement still occur: a blocked automatic train
becomes Ready. This timing progression is not a partial departure.

## 8. Arrival Lifecycle

Every admitted traversal MUST complete without further admission checks or
mid-track stopping. Its destination reservation remains exclusive throughout.

For Forward A→B:

| Boundary | `(A,Forward)` | Track `(A,B)` | `(B,Forward)` |
| --- | --- | --- | --- |
| Before departure | Occupied T1 | Available | Available |
| Accepted departure | Available | Occupied T1 | Reserved T1 |
| Arrival | As determined by other trains | Available | Occupied T1 |

Arrival MUST atomically release track occupancy, convert the destination reservation
into occupancy in the same direction, set velocity to 0, and start fresh dwell at
elapsed 0. No committed intermediate state may expose a missing or duplicate claim.
Direction stays unchanged on arrival. Reversal, if needed, occurs on departure.

A two-second traversal is observed at elapsed 0 immediately after departure,
elapsed 1 after one step, and dwelling at the destination after the second step.
No moving snapshot at elapsed equal to travel duration is emitted.

## 9. Simulation-Step Determinism

All automatic departure decisions in `World N → World N+1` MUST use the same
committed starting resource state. A resource released during the transition
MUST NOT become available to another proposal derived from World N.

The smallest required structure is:

1. Derive immutable starting ownership and collect automatic candidates from trains
   whose dwell completes on this step or which are already Ready.
2. Resolve candidates in ascending numeric `TrainId.0`, checking starting ownership
   plus claims of accepted candidates. Never remove starting owners during resolution.
3. Update each train once from its entry state: advance existing movement or dwell,
   commit an accepted departure, or retain a rejected eligible train as Ready.
4. Advance global time by exactly one second for the completed step.

The internal placement of the clock increment may remain as today. Departure does
not also advance traversal; arrival does not also dwell or depart. Only departures
need arbitration. Arrival and dwell need no generalized proposal/event engine.
The final mutation loop may remain sequential because decisions are already fixed.

Existing ownership MUST NOT be displaced by TrainId priority. Competing eligible
proposals for the same available resource resolve by lowest numeric TrainId.
Sort proposal indices or equivalent working data, not the stored train vector.
Hash iteration order MUST NOT affect decisions.

### Canonical conga

Use A–B–C–D, bidirectional two-second tracks, and three automatic Forward trains
T1/T2/T3 initially dwelling at A/B/C. D is empty.

```text
Forward slots at World N:
A       B       C       D
T1      T2      T3      empty

T1 wants (B,Forward): occupied → blocked
T2 wants (C,Forward): occupied → blocked
T3 wants (D,Forward): available → accepted
```

| Global time | Expected committed world |
| --- | --- |
| 3 | T1 Ready A; T2 Ready B; T3 Moving C→D, elapsed 0 |
| 4 | T1 Ready A; T2 Moving B→C, elapsed 0; T3 Moving C→D, elapsed 1 |
| 5 | T1 Moving A→B, elapsed 0; T2 Moving B→C, elapsed 1; T3 Dwelling D, elapsed 0 |

The departing train is moving, not already occupying its destination. This is a
primary architectural acceptance scenario: it proves frozen occupancy, later-step
reuse, and the absence of cascading resolution. Repeat with reversed insertion
order; compare by TrainId while separately preserving snapshot vector order.

## 10. Automatic vs Manual Semantics

Automatic eligibility follows dwell completion; physical admission is additional.
An accepted departure has unchanged uncontended timing. A rejected automatic train
becomes/remains `AtStation::Ready`, stopped in its original directional slot, and
MUST retry once on each later step. Do not restart or saturate dwell.

Manual `Accelerate` remains synchronous between complete steps. Preserve validation
order: find ID, require Manual, return exact `Ok(())` no-op if already Moving,
otherwise select the track, check physical admission, then commit. Add
`CommandError::Blocked` for an unavailable required resource. Keep existing
`UnknownTrain`, `NotManual`, and `NoOutgoingTrack` behavior.

Blocked acceleration MUST leave all trains, directions, velocities, timers, claims,
and global time unchanged. It MUST NOT remember intent; a later departure requires
a fresh Accelerate. Manual dwell still completes into Ready without departure.
Early acceleration remains allowed when physically admissible.

Both paths MUST share candidate selection, relevant admission checks, and departure
commitment. Manual calls do not join the automatic batch. Each call sees preceding
committed commands and steps, even when timestamps are equal. Command order, not
TrainId priority, determines which of two serial competing calls acquires resources.
A command after a step may use resources released by that completed step.

## 11. Direction Reversal

Direction identifies the committed slot while stopped and travel direction while
moving. A train dwelling at a terminal retains its arrival direction until an
accepted reverse departure.

For a train at `(C,Forward)` selecting C→B / Backward:

- Check Track `(C,B)` and destination slot `(B,Backward)`.
- On acceptance, release `(C,Forward)` and commit backward movement atomically.
- Do not acquire or require availability of `(C,Backward)` at the source.
- On rejection, retain Forward and occupancy of `(C,Forward)`.

This is a discrete turnaround abstraction. It models no intermediate slot transfer,
crossover geometry, platform transfer, or shared turnaround resource. Opposite-slot
occupants at the source do not by themselves prevent departure. Current reverse
fallback at interior missing connections follows the same rule.

This consequence is intentional and must be reviewed with directional capacity.
No separate direction-change action or stopped reversal phase is introduced.

## 12. Observation Semantics

Retain the current SPEC-001/SPEC-002 snapshot schema and train ordering:

| Observation | Meaning |
| --- | --- |
| Dwelling | Normal dwell remains; its countdown is not a guarantee of departure. |
| Ready | Dwell completed; train remains stopped. Applies to either control mode. |
| Moving | Traversal and its directional destination reservation are committed. |

Do not introduce `TrainState::Blocked`. Blocking is failed admission, not physical
location. Manual callers receive Blocked through the command result. Current DTOs
support safety assertions and stationary presentation but do not distinguish
manual waiting from automatic waiting or report a detailed blocking reason.
Richer feedback and control-mode observation are deferred.

Snapshots MUST remain owned, pure observations. They MUST NOT acquire resources,
retry, resolve contention, repair state, or predict departure. Existing direction
and state fields suffice to derive ownership for tests without a resource catalog.
Repeated or omitted observations MUST NOT change present or future outcomes.

## 13. Safety Invariants

At construction and after every completed step or command:

- TrainIds MUST be unique.
- Each directional station slot MUST have at most one occupant or reservation owner,
  and MUST NOT be both occupied and reserved.
- Each directed track MUST have at most one occupant.
- Every AtStation train MUST occupy exactly its committed `(station,direction)` slot.
- Every Moving train MUST occupy exactly its `(from,to)` track and reserve exactly
  its `(to,direction)` slot. It occupies neither endpoint station slot.
- Arrival MUST convert the arriving train's own reservation to occupancy.
- Station-state velocity MUST be 0; moving-state velocity MUST be 1.
- Moving direction MUST agree with its selected adjacent-index track direction.
- Valid moving elapsed time MUST be below positive configured travel duration.

Supported setup retains existing assumptions: valid station/track references,
positive durations, consistent timers and velocities, representable counters, and
no external runtime mutation of topology or time through legacy public fields.
Only duplicate IDs and incompatible physical ownership gain mandatory constructor
rejection in this slice. General malformed-configuration handling is not required.

Tests MUST check these invariants across acceptance traces. Invariant failure is a
model/setup defect, not a normal collision state. Snapshot mapping must not conceal it.

## 14. Deadlocks and Progress

Safety does not guarantee progress. A train may remain blocked indefinitely;
lowest-ID arbitration may starve another train. Both are acceptable for this MVP.
Do not bypass occupancy to preserve a historical timing expectation. Do not add
fairness, deadlock detection/resolution, rerouting, or dispatch policy.

## 15. Acceptance Scenarios

Unless stated otherwise, fixtures use adjacent bidirectional two-second tracks,
three-second initial dwell, and unique IDs. F/B below mean Forward/Backward.
Use public constructors, `Simulation::new`, `step`, `apply_command(Accelerate)`, and
snapshots for integration traces; match trains by ID. Assert section 13 at each
committed boundary. Crate-local tests may initialize consistent moving states for
validation cases unavailable through public setup APIs.

| Scenario | Initial world and action | Expected world / invariant |
| --- | --- | --- |
| Occupied same-direction slot | A–B–C: automatic T1 at A/F; manual T2 at B/F. Step to t=3 and again. Repeat with manual T1, requesting at t=0 and after dwell. | Automatic T1 stays Ready A/F; each manual call returns Blocked with exact before/after equality. `(B,F)` occupancy excludes entry in both modes. |
| Opposite-direction coexistence | A–B–C: automatic T1 at A/F, T2 at C/B. Step to t=3, then t=5. | Both depart at t=3 reserving different B slots; both dwell at B at t=5, in F and B respectively. Logical station equality is not contention. |
| Independent reverse tracks | A–B: automatic T1 at A/F, T2 at B/B. Step to t=3, then t=5. | Both move on opposite tracks at t=3 and arrive at opposite stations at t=5. Destination slots differ from starting occupied slots. |
| Terminal contention | A–B: automatic ID 2 at B/F, ID 9 at B/B. Step to t=3. | Both select B→A/B; ID 2 wins, reverses and moves at elapsed 0; ID 9 remains Ready B/B. Lowest ID wins genuine shared-resource contention. |
| Reversed storage order | Repeat terminal contention and the conga with reversed vector order. | Equal ID-keyed outcomes; each snapshot retains its own insertion order. |
| Reservation and arrival | A–B: manual T1 at B/F, T2 at B/B. Accelerate T1 at t=0; request T2 before stepping, after one step, and after two. | T1 moves with elapsed 0, then 1, then dwells A/B. T2 is Blocked throughout: first track/destination are claimed, then destination is occupied. At arrival the track is free and reservation becomes occupancy atomically. |
| Conga | Three automatic F trains at A/B/C; D empty. | Exact t=3/4/5 trace in section 9; no same-step resource reuse. |
| Automatic retry | A–B–C: automatic T1 at A/F, manual T2 at B/F. Step to t=3; accelerate T2 B→C; step once. | T1 is Ready at t=3 and departs A→B at t=4, elapsed 0; no renewed dwell. |
| Manual no buffering | A–B–C: manual T1 at A/F, T2 at B/F. Reject T1 at t=0; accelerate T2 B→C; step three times. | T1 remains Ready A/F despite availability. Fresh Accelerate succeeds at t=3 without advancing global time. |
| Blocked reversal | A–B–C: T1 at C/F, manual T2 at B/B. Test manual T1 at t=0 and automatic T1 at t=3. Then accelerate T2 B→A and request/step T1 again. | Rejection retains C/F, direction F and velocity 0; later acceptance commits C→B/B. Manual rejection is exact; automatic rejection permits dwell completion into Ready. |
| No source-slot transfer | A–B: manual T1 at B/F, T2 at B/B. Accelerate T1. | T1 departs B→A/B even while T2 occupies `(B,B)`; only its original `(B,F)` slot is released. |
| Serial command ordering | A–B: manual ID 9 at B/F, ID 2 at B/B. Call 9 then 2 at t=0; repeat in fresh simulation with calls reversed. | First caller succeeds, second is Blocked; reversing call order reverses winner. Separate transactions are not a lowest-ID batch. |
| Observation independence | Replay conga and terminal contention with frequent versus sparse snapshots; retain an old snapshot. | Corresponding outcomes equal; retained snapshot unchanged; observation does not affect resolution. |
| Initial ownership | Construct duplicate IDs, same-slot occupants, duplicate track occupants, competing destination reservations, and occupied/reserved same slots. Also construct opposite-slot occupants. | Invalid ownership fails construction; opposite-slot coexistence succeeds. Use valid direction/timing/velocity in fabricated moving fixtures. |

Terminal contention is a valid public-API fixture in the existing linear topology:
one train continues backward while the opposite-slot train reverses onto the same
track and destination slot. No branching topology or artificial proposal is needed.
Distinct opposite-direction arrivals at B MUST NOT be used as a contention test.

The reservation fixture intentionally checks both track and destination ownership;
the observed moving state proves the destination claim independently of which
unavailable resource the minimal Blocked error reports. No public reason taxonomy
is required. Crate-local resource-view tests verify each ownership fact separately.

## 16. Compatibility / Migration

Directional capacity supersedes earlier global-capacity-one analysis. Opposing
trains can coexist at a logical station or pass on independent directed tracks.

### Existing five-station scenario

[`tiny_metro_scenario.rs`](../../metro-core/tests/tiny_metro_scenario.rs) uses
A–B–C–D–E with segment durations 180, 120, 240, 180. ID 0 starts A/F and ID 1 E/B.
Both depart at t=3 and arrive at B and D at t=183. The selected model predicts:

| Time | Train originating at A | Train originating at E |
| --- | --- | --- |
| 186 | Departs B→C; reserves `(C,F)` | Departs D→C; reserves `(C,B)` |
| 306 | Arrives C | Still moving toward C |
| 309 | Departs C→D | Still moving D→C |
| 426 | Still moving toward D | Arrives C |
| 429 | Still moving toward D | Departs C→B |
| 549 | Arrives D | Arrives B |
| 552 | Departs D→E | Departs B→A |
| 732 | Arrives E | Arrives A |
| 735 | Reverses and departs E→D | Reverses and departs A→B |
| 736 | Traversal elapsed 1 | Traversal elapsed 1 |

These ownerships are compatible. Retain the existing 732/735/736 assertions. The
previous predicted C/D deadlock relied on global station capacity one and does not
apply. The existing round trip needs no timing relaxation under this model.

### Other regression fixtures and contract amendments

- `snapshot_preserves_train_order_and_uses_each_active_track` in
  [`snapshot_observation.rs`](../../metro-core/tests/snapshot_observation.rs)
  remains valid: opposing endpoint occupants use different destination slots.
- In [`manual_control.rs`](../../metro-core/tests/manual_control.rs),
  `accelerate_while_manual_train_is_moving_is_idempotent` and
  `duplicate_acceleration_preserves_zero_and_advanced_traversal` depart toward
  opposite-direction occupants and remain valid. The reversal-selection fixture's
  opposite-direction co-location and destination occupancy also remain valid.
- `command_rejections_preserve_all_trains_and_timers` starts two Forward trains at
  A. Move its unrelated manual train to a separate safe station when constructor
  validation lands, preserving isolated-station and rejection assertions.
- SPEC-002's normal automatic no-Ready behavior is extended: blocked automatic
  trains now expose Ready and retry. Uncontended automatic timing stays unchanged.
- Ready means completed dwell, not exclusively waiting for player intent. Dwelling
  countdown means time until dwell completion, not guaranteed departure.
- Keep existing single-train timing and Bevy tests unchanged. The current Bevy
  scenario has one manual train and no physical conflict. No visual lane change is
  required or authorized.

Snapshots preserve ordering and remain pure. The added Blocked error extends the
command result contract; no command shape or snapshot field changes are needed.
Public mutable network/time fields remain an unsupported runtime escape hatch;
encapsulation cleanup is outside this slice.

## 17. Implementation Checkpoints

These are future tests-first checkpoints, each ending in review. Approval of this
specification does not authorize starting one. No placeholder public APIs or
unimplemented stubs are required.

### Checkpoint 1 — Derived resource view and validation

Tests first establish directional station-slot occupancy, moving-track occupancy,
destination-slot reservation, and opposite-direction slot coexistence. Reject
all incompatible initial ownership cases from section 15 and duplicate TrainIds;
prove that resource derivation detects conflicts instead of overwriting owners.

Implement only the private temporary resource view and the minimum constructor/setup
validation required by sections 6 and 13. Preserve `Simulation::new()`'s signature
and fail-fast behavior. Repair the invalid overlapping rejection fixture identified
in section 16; do not expand into general malformed-configuration validation.

**Review stop:** A committed state deterministically yields occupied station slots,
occupied directed tracks, and reserved destination slots. Normal train behavior is
unchanged, and all existing valid core tests remain green.

### Checkpoint 2 — Shared departure candidate and commit path

Protect existing behavior with tests for current-direction selection, reverse
fallback only when the current-direction track does not exist, early manual
acceleration, automatic departure timing, manual Ready behavior, already-moving
Accelerate idempotence, no outgoing track, direction/velocity transitions, and
traversal elapsed starting at zero. Preserve existing command validation order and
`UnknownTrain`, `NotManual`, and `NoOutgoingTrack` behavior.

Consolidate the duplicated manual/automatic mechanics into the smallest private
representation/helpers for selecting a departure candidate and committing an
accepted departure. Keep domain helpers independent of command and snapshot DTOs.
Do not introduce physical rejection yet except where structurally unavoidable.

**Review stop:** Manual and automatic paths share candidate-selection and
departure-commit mechanics with valid existing behavior preserved. This is a
behavior-preserving refactor protected by tests.

### Checkpoint 3 — Single-departure physical admission

Tests first prove admission of one candidate against one committed resource view:

- An occupied outgoing directed track, occupied destination directional slot, or
  reserved destination directional slot blocks departure; the opposite-direction
  destination slot does not conflict.
- Acceptance releases the original source slot, occupies the directed track,
  reserves the selected destination slot, commits direction and velocity, and
  enters Moving at elapsed zero in one atomic transition.
- Rejection commits no direction change, velocity change, movement, or claim.
  A blocked terminal reversal retains its original committed direction, and a
  physically blocked existing track never triggers reverse fallback.

Implement the shared physical-admission primitive and connect manual Accelerate
end-to-end. Unavailable required resources return `CommandError::Blocked`
synchronously with exact before/after equality for all trains, timers, claims,
and global time. Cover early and post-dwell rejection, successful later reversal,
and the section 15 manual no-buffering trace: availability alone never executes
rejected intent; a fresh Accelerate is required.

Prepare the small admission helper for automatic use, but do not require the full
multi-candidate frozen-world resolver in this checkpoint.

**Review stop:** Candidate plus committed resource view reliably yields accepted
or blocked. Accepted transitions produce exactly the section 7 ownership changes,
and manual physical blocking is synchronous, atomic, and unbuffered. This proves
physical admission independently of multi-train arbitration.

### Checkpoint 4 — Automatic frozen-world batch resolution

Tests first establish automatic Ready retry: blocked dwell completion enters Ready
in the original directional slot, and each subsequent step retries once without
restarting dwell. Cover the automatic occupied-slot and blocked-reversal traces,
including later acceptance, using the shared admission and commit mechanics.

Prove that every automatic candidate uses the same starting ownership for
World N → World N+1. Resources released during this transition cannot be reused by
another proposal from World N. Resolve available-resource contention by ascending
numeric `TrainId.0`, checking starting owners plus accepted claims; existing owners
are never displaced. Train-vector and hash iteration order must not arbitrate.
Use the valid linear-topology terminal contention fixture from section 15 and
repeat with reversed train insertion order, comparing outcomes by ID.

Make the canonical conga the central architectural test: A–B–C–D, bidirectional
two-second tracks, and automatic Forward T1/T2/T3 initially dwelling at A/B/C with
D empty. Assert the exact committed trace:

| Global time | Expected committed world |
| --- | --- |
| 3 | T1 Ready A; T2 Ready B; T3 Moving C→D, elapsed 0 |
| 4 | T1 Ready A; T2 Moving B→C, elapsed 0; T3 Moving C→D, elapsed 1 |
| 5 | T1 Moving A→B, elapsed 0; T2 Moving B→C, elapsed 1; T3 Dwelling D, elapsed 0 |

Repeat with reversed storage order while preserving each snapshot's vector order.
Implement section 9's derive/collect, resolve, then commit sequence. Update each
train once from its entry state and advance global time exactly one second; a
departure consumes no travel second and an arrival consumes no dwell second or
second departure. Do not introduce a generalized arrival/dwell proposal engine.

**Review stop:** Automatic departures resolve against frozen World N before
committing World N+1. Automatic Ready retry, lowest-ID contention, insertion-order
independence, and the conga pass with no same-step cascading resource reuse.
Both control modes now enforce the shared physical-admission rule.

### Checkpoint 5 — Complete scenario and invariant proof

Extend coverage across the complete contract and every remaining section 15 case:
exact reservation-to-occupancy arrival lifecycle, opposite-direction coexistence,
independent reverse-track use, no source-slot transfer during reversal, serial
manual command ordering, observation independence, arrival atomicity, and all
required construction/setup invariants. Prove that admitted traversal completes
without further admission or mid-track stopping, retaining its reservation until
arrival. A command sees preceding committed commands/steps and may use resources
released by a completed step; serial command winners follow call order, not ID.

Check every section 13 invariant at construction and each committed step/command
boundary across acceptance traces:

- Unique TrainIds and at most one owner per directional station slot or directed
  track; no occupied/reserved slot conflict.
- Moving owns exactly its track and destination reservation, neither endpoint
  slot; AtStation owns exactly its committed directional station slot.
- Arrival converts the train's own reservation into same-direction occupancy
  atomically, releases its track, and starts fresh dwell at elapsed zero.
- Correct velocity for physical state, moving direction consistent with its
  selected adjacent-index track, and elapsed below positive travel duration.

Verify owned, pure snapshots with unchanged schema/order, retained old snapshots,
and frequent versus sparse observations. Ready remains the stopped post-dwell
state; add no Blocked physical state or observation-driven side effects. Audit
sections 3–14 for architectural constraints and exclusions as well as behavioral
requirements, including derived ownership without a persistent registry, no new
public fixture APIs, and safety without fairness or guaranteed progress. Fix only
demonstrated contract defects; add no abstractions solely for test structure.

**Review stop:** Every section 15 acceptance scenario and all safety invariants are
demonstrated. The selected SPEC-003 contract is semantically complete, with no new
architectural mechanism introduced by this checkpoint.

### Checkpoint 6 — Compatibility and full regression verification

Preserve the five-station scenario's directional-capacity timing from section 16,
including its existing 732/735/736 assertions, uncontended automatic timing,
existing manual-control semantics, and snapshot ordering/purity. Verify that Bevy
tests remain unchanged and no Bevy behavior or presentation logic was modified.
Confirm no SPEC-001/SPEC-002 behavior changed beyond SPEC-003's explicit amendments.

Run:

```text
cargo test -p metro-core
cargo test --workspace
```

Fix only demonstrated regressions against the selected contracts.

**Review stop:** All SPEC-003 acceptance tests and compatible historical core/Bevy
regressions pass. No unresolved SPEC-003 implementation work remains; explicitly
deferred and out-of-scope features remain outside this slice.

### Coverage across review stops

| Existing contract / acceptance scenarios | Checkpoints proving completion |
| --- | --- |
| Resource model, derivation, initial ownership and minimum constructor rejection (sections 5–6, 13; Initial ownership) | 1, 5 |
| Shared departure mechanics, selection, timing, command validation (sections 3, 7, 10–11) | 2, 3, 4, 6 |
| Single-candidate admission, manual atomicity/no buffering, blocked reversal (sections 7, 10–11; Occupied same-direction slot, Manual no buffering, Blocked reversal) | 3 for manual admission; 4 for automatic behavior; 5 for complete traces |
| Frozen-world arbitration, Ready retry, determinism (section 9; Terminal contention, Reversed storage order, Conga, Automatic retry) | 4 |
| Arrival lifecycle and directional independence (sections 5, 8, 11; Opposite-direction coexistence, Independent reverse tracks, Reservation and arrival, No source-slot transfer) | 5 |
| Serial command ordering and observation contract (sections 10, 12; Serial command ordering, Observation independence) | 5, 6 |
| All boundary invariants, architectural constraints and scope exclusions (sections 3–14) | 5 |
| Migration, compatibility and full regression verification (section 16) | 6 |

This sequence assigns every normative requirement and section 15 acceptance
scenario to at least one checkpoint. Completion of Checkpoint 6 leaves no uncovered
SPEC-003 requirement; it does not expand the selected contract or its scope.

Review sections 5 (directional capacity), 11 (turnaround abstraction), 9 (frozen
starting world), 15 (terminal contention), and 6/13 (initial ownership and invariants)
most carefully before authorizing Checkpoint 1. With these selected decisions,
no unresolved architectural question prevents implementation review.
