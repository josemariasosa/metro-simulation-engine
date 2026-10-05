# SPEC-006: Operational Sandbox

## 1. Status and Purpose

**First complete draft — specification only.** Inspected against the repository
source and tests on 2026-10-05. This document proposes the next `metro-bevy`
milestone; it does not implement production code or claim that the proposed UI
exists.

The purpose is to turn the current visualization into a practical operational
sandbox for exercising, observing, debugging, and intentionally stressing the
simulation already implemented in `metro-core`. It is a:

```text
visualizer + interactive debugger + scenario laboratory
```

It is not a gameplay layer. The sandbox must make the existing system feel
closed and inspectable: network, trains, state, direction, movement, dwell,
resource contention, manual and automatic control, constraints, and simulation
time should be understandable without reading logs or source.

## 2. Repository Findings

Current source and tests are authoritative. SPEC-005's description of the
implemented constraint boundary matches the source inspected for this draft.
Older specs should be treated as historical wherever they describe earlier
module paths, constructor shapes, status, or ownership.

| Area | Current behavior and evidence |
| --- | --- |
| Public boundary | [`metro-core/src/lib.rs`](../../metro-core/src/lib.rs) keeps `domain` private and exports selected values. `Simulation`, commands, and owned snapshots are public modules. Bevy can create `Network`, `Train`, and `Simulation`, step it, issue commands, register/remove constraints, and consume snapshots. |
| Network and route selection | [`domain/network.rs`](../../metro-core/src/domain/network.rs) stores insertion-ordered stations and directed tracks. `next_track` only examines the adjacent station index in the requested direction. `add_track` can be directed and can give each direction a different duration, but non-adjacent edges are not selectable by current train movement. Terminal selection falls back to the opposite direction only when there is no track in the committed direction. |
| Network observation | `Network` has public `station_count()` and `track(from, to)`, but no public station-name getter or station/track iterator. Station names and an enumerable topology are not in snapshots. The current Bevy scenario keeps labels and station IDs in its own fixed four-station resource. |
| Train identity and ordering | [`domain/train.rs`](../../metro-core/src/domain/train.rs) defines stable `TrainId`; `Simulation::add_train` assigns monotonically increasing IDs. `Simulation::trains()` exposes train entities in storage order; snapshots preserve this order. Automatic proposal resolution sorts by numeric ID, independently of storage order. |
| Train control and state | Train control is `Automatic` or `Manual`. Automatic trains are eligible at dwell completion and retry while Ready. Manual trains only depart when `Accelerate` is submitted. Core state is AtStation/Dwelling, AtStation/Ready, or Moving; there is no Blocked state. `TrainSnapshot` contains state, direction, velocity, and ID, but omits control mode. The public `Train` API also has no control-mode getter. |
| Dwell and movement | [`simulation.rs`](../../metro-core/src/simulation.rs) advances one integer simulation second per `step()`. Snapshots expose dwell remaining seconds and movement elapsed/travel seconds. An admitted departure starts movement at elapsed zero. Arrival begins fresh dwell and resets velocity to zero. |
| Manual acceleration | `TrainCommand::Accelerate` is a one-shot departure command, not continuous throttle. It may depart before dwell completes. A successful departure sets velocity to 1; movement is otherwise controlled by simulation steps. Accelerating a moving manual train returns success without changing state. There is no brake, speed command, or player-controlled route choice. |
| Direction and terminals | Snapshot direction is committed direction. At a terminal, candidate selection may choose the reverse direction, but direction changes only when departure admission succeeds. A rejected reversal keeps its old direction and source slot. |
| Physical claims | [`domain/resource.rs`](../../metro-core/src/domain/resource.rs) derives occupied directional station slots, occupied directed tracks, and reserved destination slots from each train. A stopped train claims `(station, direction)`. A moving train claims `(from, to)` and reserves `(to, direction)`. Each directed track and each direction-specific station slot has capacity one; opposite station slots and reverse directed edges are independent. |
| Admission and automatic contention | Manual and automatic admission use the same physical and operational predicate. An automatic step freezes starting resources, resolves eligible proposals in ascending `TrainId`, accounts for accepted destination slots/tracks, then commits fixed decisions. Starting-world claims released during commit cannot be reused in that step. A rejected automatic train is/remains Ready and retries on a later step. |
| Manual contention | Manual commands are synchronous. Each sees the latest committed state; serial call order decides contention. A rejected command does not buffer intent or mutate the world. |
| Constraints | [`domain/constraint.rs`](../../metro-core/src/domain/constraint.rs) provides `TrackUnavailable`, `StationDeparturesBlocked`, and `StationUnavailable`, with `Planned`/`Injected` origin metadata. Public `Simulation` APIs support absolute/relative future scheduling and explicit removal. Intervals are half-open; activation is derived from simulation time; finite expiry is pruned at the end of a step. |
| Constraint effects | Constraints restrict new departure admission only. A directed track closure does not close its reverse edge or trigger rerouting. A departure block applies at the source; station unavailable blocks new admissions into and out of the station. Existing occupants, reservations, and admitted traversals remain valid. Automatic and manual admission use the same rules. |
| Snapshot observation | [`snapshot.rs`](../../metro-core/src/snapshot.rs) exposes owned, comparable time/train/constraint observations. Constraint records include ID, kind/target, interval, and origin. All registered unexpired records, scheduled and active, are visible in ID order. Snapshots are not history and do not contain resource claims, control mode, labels/topology, or command-result history. |
| Bevy adapter | [`metro-bevy/src/scenario.rs`](../../metro-bevy/src/scenario.rs) builds a four-station bidirectional A-B-C-D line and one manual train. [`timing.rs`](../../metro-bevy/src/timing.rs) accumulates `Time<Real>` into whole seconds, runs all due core steps, handles Space as `Accelerate`, then publishes a snapshot. [`presentation.rs`](../../metro-bevy/src/presentation.rs) maps snapshot state to fixed station positions and linearly interpolates movement from snapshot elapsed/travel values. A separate Bevy-time bounce currently decorates the train body; it is not simulation movement. |
| Existing Bevy tests | Scenario, timing, and presentation have local unit tests. They already cover accumulator partitioning, command ordering after catch-up ticks, snapshot publication, moving-command no-op, state labels, and projection error handling. |

Existing core coverage relevant to this sandbox includes
[`automatic_blocking.rs`](../../metro-core/tests/automatic_blocking.rs),
[`manual_control.rs`](../../metro-core/tests/manual_control.rs),
[`operational_constraints.rs`](../../metro-core/tests/operational_constraints.rs),
[`snapshot_observation.rs`](../../metro-core/tests/snapshot_observation.rs),
[`automatic_blocking_tests.rs`](../../metro-core/src/test_utils/automatic_blocking_tests.rs),
and [`physical_safety_tests.rs`](../../metro-core/src/test_utils/physical_safety_tests.rs).
These tests already establish conga/frozen-world behavior, ID arbitration under
reversed test storage order, serial manual contention, reservation safety,
terminal reversal, constraint lifecycle, and observation independence. SPEC-006
must expose those results, not reimplement their rules in Bevy.

## 3. Architectural Boundary

Preserve the existing dependency direction:

```text
metro-core
	authoritative state, deterministic rules, commands, snapshots
						 ↓
metro-bevy operational sandbox
	scenarios, operator input, time pacing, snapshot projection, rendering
```

`metro-core` remains unaware of Bevy, sprites, panels, input devices, colors,
debug overlays, layout, or scenario-editor concepts. Bevy must not inspect
private `ResourceView`, `ConstraintView`, or other domain internals. It may call
existing public APIs and derive presentation-only values from owned snapshots.

The scenario definition should be the single Bevy-side source used to construct
the core `Network` and its display metadata (station labels, order, and visual
positions). It is immutable after construction. Do not build a second mutable
topology cache that can drift from the simulation. Dynamic topology editing is
out of scope.

Do not add core behavior just to simplify rendering. Where an observation gap is
real, identify its narrow contract and reason. Section 6 distinguishes direct
snapshot data, values derivable in the adapter, and information requiring a
possible future snapshot addition.

## 4. Operational Sandbox Philosophy

The first screen is the running sandbox, not a landing page. Correctness and
legibility take priority over polish. Simple geometry, stable labels, directional
lanes, progress indicators, and explicit occupancy overlays are sufficient.
Existing Aseprite train artwork may be used but is optional.

The sandbox must not imply that core guarantees liveness. A Ready train can remain
Ready indefinitely. Queues, starvation, blocked reversals, and deadlock-like
conditions are valid observations unless core behavior says otherwise. The
sandbox records no new railway policy and does not repair, reroute, or resolve
congestion.

## 5. Feature Inventory and Visual Mapping

| Feature | Core truth and public observation | Proposed sandbox representation / interaction | Mode and API sufficiency | Duplication risk |
| --- | --- | --- | --- | --- |
| Station IDs, labels, and order | IDs/order are created by `Network`; names and all-station enumeration are private/unavailable after construction. | Render labels and order from the same immutable scenario definition used to build `Network`. Select station by its core `StationId`. | Passive; sufficient for built-in scenarios. No runtime topology editor. | Low if one immutable scenario descriptor creates both; high if labels/order are maintained separately. |
| Directed tracks and travel duration | `Network::track(from,to)` is public, but there is no edge iterator. Current candidate traversal is adjacent-index only. | Render each declared directed edge with arrows and duration. A bidirectional segment is two arrows; show a missing direction as missing. | Passive; scenario descriptor is sufficient for built-ins. | Do not infer unobserved topology only from current train positions. |
| Terminal status | Derived from the scenario's ordered stations and absence of a selectable edge in current direction. No terminal flag exists. | Mark ends of the supported ordered line. At a terminal, show committed direction separately from a candidate reverse arrow. | Passive/derived; no core addition. | Avoid storing a second mutable terminal state. |
| Train identity | `TrainSnapshot.id` is authoritative and stable. | Always show a compact numeric ID on each train; select by ID. | Passive; sufficient. | Never use Bevy entity ID, vector index, color, or selection order as identity. |
| Automatic/manual mode | `TrainControl` is core state but no public accessor or snapshot field exposes it. | Distinct compact `AUTO`/`MANUAL` marker, repeated in inspector. | Passive. **Public snapshots are insufficient.** Add `control: TrainControl` to `TrainSnapshot` (or an equivalently narrow owned observation) before claiming correct mode display. | Do not keep a second authoritative Bevy map of control modes. |
| Dwelling, Ready, Moving | Snapshot variants represent these exact states. | Distinct state label and symbol; Ready must look stationary and different from movement. Never show a fabricated core `Blocked` state. | Passive; sufficient. | No shadow train state. |
| Committed direction | `TrainSnapshot.direction`; a reversal commits only on successful departure. | Arrow/head orientation and directional lane/slot; show committed arrow even when candidate is blocked. | Passive; sufficient. | Presentation must not pre-commit a visual reversal. |
| Velocity | Snapshot exposes current `u8`; observed implemented values are 0 while stopped and 1 while moving. | Show numeric velocity in inspector and optionally a small status mark. | Passive; sufficient. | Do not animate an independently simulated speed. |
| Manual acceleration | Public `TrainCommand::Accelerate`; one-shot, may leave dwell early, moving is successful no-op, blocked returns error. | Select a manual train and activate one explicit Accelerate control (button/keyboard). Show the immediate command result locally; do not buffer failures. | Active; sufficient. No brake/throttle. | Input mapping must not locally change train state. |
| Dwell progression | Snapshot gives `remaining_seconds`; total dwell/elapsed is not directly present. | Show remaining seconds or a progress bar using scenario/core-observed dwell duration if available. A reliable total duration is not in the snapshot; use remaining time only unless the adapter has the exact dwell policy used to build the scenario. | Passive; largely sufficient. | Avoid guessed total/progress values. |
| Movement and edge progress | Moving snapshot gives `from`, `to`, `elapsed_seconds`, `travel_seconds`. | Place on that directed edge; compute fraction `elapsed/travel` and interpolate between static endpoint positions. Keep position derived from latest snapshot, not a Bevy physics clock. | Passive; sufficient for configured layout. | Do not persist a second movement timer. |
| Station occupancy | Derive for Dwelling/Ready as `(station, committed direction) -> TrainId`. | Two visually distinct directional lanes/slots at each station; show occupant ID and occupied marker. Opposite directions may coexist. | Passive derived observation; public train snapshots are sufficient under current core invariant. | Keep derivation pure and keyed by ID; never assign claims by draw order. |
| Directed track occupancy | Derive for Moving as `(from,to) -> TrainId`. | Highlight that directed edge and show occupant ID. Reverse direction is a separate resource. | Passive derived observation; sufficient. | Do not treat both directions as one shared track. |
| Destination reservation | Derive for Moving as `(to, committed direction) -> TrainId`; this is separate from current track occupancy. | Show a reservation marker on the destination directional slot, distinct from a stopped occupant. At arrival it converts to occupancy according to the next snapshot. | Passive derived observation; sufficient. | Never infer reservation only from destination appearance or mutate core claims. |
| Automatic contention | Core arbitration result is reflected in snapshots; reason is not a core state. Numeric ID chooses among eligible proposals against frozen resources. | Provide built-in contested layouts; show all involved trains, claims, and resulting Ready/Moving states. A small diagnostic may list observable competing claims. | Passive observation; scenario setup sufficient. | Do not emulate arbitration in Bevy. Reversed storage-vs-ID arbitration is proven by core tests, not a runtime storage-order feature. |
| Manual contention | Synchronous commands see the latest state; first successful serial admission owns the destination/track. | Allow commands to be sent to competing manual trains one at a time and show success/error. | Active; sufficient. | Keep command submission order explicit. |
| Frozen World-N / conga | Core step freezes starting claims; released starting resources cannot be reused until a later step. | Dedicated A-B-C-D scenario; support pause and exactly-one-step. Show claims before and after each step. | Passive + time control; sufficient. | Do not update presentation occupancy incrementally during a step. |
| Constraint records/lifecycle | Snapshot gives ID, variant/target, start/end, origin; time is snapshot time. | List records and derive Scheduled/Active with half-open interval semantics. Show ID, kind, target, origin, start/end, time until start or remaining active duration. | Passive; sufficient. | Do not maintain another active flag or infer expiry from Bevy wall clock. |
| Constraint effects | Admission semantics are core-owned. Existing traversal/claims are unaffected. | Directed closure marker; station-departure marker on source; station-unavailable marker. Differentiate scheduled (outlined/dashed) and active (solid). | Passive; enough to show configured scenario topology and constraint snapshots. | Do not depict closure as removal, destruction, physical occupancy, or rerouting. |
| Constraint interaction | `create_constraint_at`, `create_constraint_in`, `remove_constraint` are public. Starts must be in the future; IDs are returned. | Minimal target/kind/time form and a registered-record list with Remove action. Preserve returned IDs in the local display model only as UI handles; refresh authoritative records from snapshot. | Active; sufficient. | UI bookkeeping must not replace core registry or lifecycle. |
| Time | Snapshot `elapsed_seconds`; `step()` advances exactly one core second. | Persistent clock, running/paused state, Pause/Resume, Single Step. | Active Bevy pacing; sufficient. | Wall time never directly changes core time except through explicit calls to `step()`. |
| Ready-train explanations | Snapshots expose Ready state, resources derivable from all trains, and active restrictions. They do not expose which rule(s) rejected an individual proposal or command history. | Show “Ready” plus observable matching blockers (occupied track/slot, active applicable constraint, or candidate edge unavailable). If multiple apply, show multiple; if topology/policy makes cause uncertain, say “Ready; no departure observed” rather than asserting one reason. | Presentation-only derived diagnostics. Exact historical denial is not available. | Do not add a Bevy `Blocked` train state or imply exact arbitration internals. |

## 6. Observation Gaps and Ownership

### 6.1 Information already in snapshots

Snapshots already provide simulation time; stable train IDs; direction; velocity;
Dwelling/Ready/Moving state; dwelling remaining time; movement edge, elapsed
time, and traversal duration; and the complete registered, unexpired constraint
records with IDs, target/kind, interval, and origin.

### 6.2 Safely derivable in Bevy

Given the existing physical invariant and a complete train snapshot, resource
claims can be reconstructed exactly:

```text
Dwelling or Ready: occupied station slot = (station, direction)
Moving: occupied track = (from, to)
Moving: reserved destination slot = (to, direction)
```

Train IDs are the claim owner labels. Implement this as a pure adapter function
and test uniqueness/invariant assumptions. Do not expose `ResourceView` or invent
an alternate core ownership registry.

Scheduled versus active constraint state is also derivable for snapshot time `T`:

```text
scheduled: T < start_at
active:    start_at <= T && (end_at is None || T < end_at)
expired:   absent from the current snapshot
```

For a finite active constraint, remaining duration is `end_at - T`. For a
scheduled constraint, time until activation is `start_at - T`. These are display
values only.

Movement position is derivable from snapshot progress and configured endpoint
positions. Candidate destination and likely current blockers may be derived from
the scenario's ordered topology, snapshot resource claims, and currently active
constraints. Such explanations are advisory diagnostics, not authoritative
admission results or historical denial reasons.

### 6.3 Narrow justified observation addition

Control mode is not derivable: snapshots omit `TrainControl`, while the public
`Train` API has no control getter. A scenario-side ID-to-mode table would duplicate
authoritative state and fail for any future way of adding/changing trains.
Therefore add the mode to the owned train snapshot before the sandbox claims to
visually distinguish all trains:

```rust
pub control: TrainControl,
```

This is an observation-only contract. It adds no simulation behavior, and it uses
the already-public `TrainControl` enum. Update snapshot constructors/tests and
consumers. The adapter renders the value; it never owns or changes it.

### 6.4 Information that remains private or scenario-owned

`ResourceView` owners/claim maps, `ConstraintView`, proposal ordering internals,
accepted-claim sets, and transient admission decisions remain private to core.
They are not needed to reconstruct physical claims or display the result.

Network station names and enumerable topology are not currently observable from
the public API. The built-in scenarios do not require a new network snapshot:
their immutable scenario descriptor supplies names/order/declared edges to both
network construction and rendering. Supporting arbitrary externally constructed
networks later would require reconsidering a deliberately narrow topology
observation contract; it is not a blocker for this fixed scenario laboratory.

Exact denial history, per-train blocked reason, manual command history, and
automatic proposal diagnostics are not current snapshot concepts. Keep these
private/derived unless repeated sandbox use establishes a concrete need for a
small future observation contract. Never smuggle mutable core internals into
Bevy.

## 7. Runtime and Operator Controls

The smallest useful runtime surface is:

- Scenario chooser for the built-in deterministic scenarios.
- Pause, Resume, and Single Step.
- Select a train/station/track and view a compact inspector.
- One Accelerate action for a selected Manual train.
- Constraint form to choose one of the three supported kinds, target, absolute
	or relative future start, optional end/duration, and `Planned` or `Injected`
	origin; list current snapshot records and remove by ID.
- A visible simulation clock and a local result line for the most recent operator
	action (for example API error or returned constraint ID). The result line is
	UI feedback, not persisted core state.

Use a restrained control panel and simple keyboard shortcuts only where they
remain discoverable. Controls must not require source edits to perform ordinary
experiments. This is not a scenario editor: no arbitrary station/train placement,
topology mutation, or save/load.

Do not add a speed multiplier in the first implementation. Pause, resume, and
single-step provide the determinism and inspection needed for this milestone;
real-time pacing remains at the existing one simulation second per real second.
A Bevy-only multiplier may be considered later if catch-up pacing proves
operationally limiting. It must scale the adapter's accumulated real-time delta,
preserve whole-step/fractional-remainder semantics, and never alter core rules.

## 8. Built-In Scenarios

Provide approximately six deterministic fixtures. Scenario order, train creation
order, IDs, dwell values, directed track durations, and scheduled constraint
records must be explicit and repeatable. Validate that no fixture creates
duplicate initial physical claims.

| Scenario | Suggested fixture | Purpose and expected observation |
| --- | --- | --- |
| Basic automatic loop | Ordered line of roughly 8–10 stations; several automatic trains distributed across stations and both directions; varied directed travel durations. | Ordinary movement, dwell-to-Ready-to-Moving, direction lanes, terminal reversal, and normal contention. |
| Mixed automatic/manual | Roughly 8–10 stations, 6–7 automatic trains, one manual train; enough space to issue manual commands without every outcome being predetermined by immediate overlap. | Manual commands in an active automatic world, early manual departure, moving-command no-op, mixed resource competition. |
| Frozen conga | A-B-C-D, three Forward automatic trains initially at A/B/C, no train at D; bidirectional adjacent links, 3-second travel and 3-second initial dwell. | At t=3 only C→D departs. Then B→C departs at t=4 and A→B at t=5. Step individually to see no within-step cascade or same-step reuse of a released claim. |
| Constraint laboratory | Small 4–6 station line with free test targets and one or more trains. Include scheduled planned records in initial setup; allow runtime injected records. | Compare directed-track closure, departure-only station block, station-unavailable, activation, expiry, overlap, cancellation/removal, and reopening. |
| Occupied-resource closure | One train begins a sufficiently long traversal; schedule a matching closure to start after it has departed. Place a second ready train so a later new admission can be observed. | Closure activates mid-traversal; moving train keeps its track and destination reservation and arrives normally; later matching admission is blocked; expiry/reopening permits only a later retry. |
| Congestion laboratory | Deliberately dense but physically valid placement on a short ordered line, with competing directions and optional station/track constraints. | Queues, indefinite blocking, starvation, blocked reversal, and legal but surprising outcomes. No automatic repair or liveness promise. |

For the frozen conga, show the committed snapshot after every step, including
starting World claims and accepted outcomes. A display may highlight the one
currently available forward destination/resource, but must not present a
Bevy-computed arbitration as core state.

Reversed storage order versus numeric-ID arbitration is covered by the core test
hook and must remain a core regression. The current public scenario API assigns
IDs in insertion order and has no storage-order mutation operation; do not add a
production escape hatch solely to reproduce that internal permutation in the UI.
The sandbox can still make ordinary two-train automatic contention visible.

## 9. Inspection and Debug UX

### Selected train

Show `TrainId`, Automatic/Manual, committed direction, exact snapshot state,
velocity, current station or directed edge, movement `elapsed/travel` or dwell
remaining, and derived physical claims. For Moving, list occupied edge and
reserved destination slot. For Dwelling/Ready, list the occupied directional
station slot. The Accelerate action is enabled only for Manual trains; command
errors remain visible when attempted.

### Selected station

Show ID and scenario label, both directional slots and occupant/reservation IDs,
trains at or reserving the station, station-related active/scheduled constraints,
and the distinction between departures-blocked and station-unavailable.

### Selected directed track

Show `from → to`, travel duration from scenario definition, occupant ID or free,
and matching active/scheduled directed-track constraints. The reverse edge is a
separate selection and resource.

### Global status

Show snapshot simulation time, running/paused state, train count, and constraint
records. Constraint classification comes from snapshot timestamp and interval;
never cache a second lifecycle state. A compact event/result line may show the
last operator action without implying that core stores an event log.

### Readiness explanations

Keep explanation text explicitly presentation-derived. A Ready train may show
one or more currently visible conditions such as destination slot occupied,
directed track occupied, or matching active constraint. If several apply, list
all visible blockers rather than choosing a false single cause. Show the
candidate edge only when it can be determined from the configured scenario
topology and the current committed direction. If it cannot, show the observed
Ready state without a speculative cause.

There is no core `Blocked` state. Do not change the train label from Ready simply
because its next departure is currently denied. Manual rejection is an
operation-result message; automatic rejection is observable as Ready and may not
have a uniquely inferable historical cause after the fact.

## 10. Time, Steps, and Input Sequencing

The current adapter accumulates real time in whole seconds, runs all due core
steps, applies the Space command after catch-up ticks, and publishes one snapshot
per Bevy update. Preserve the key property that only explicit calls to
`Simulation::step()` advance core time. Rendering frequency and snapshot reads do
not advance it.

For the sandbox, define one clear adapter mutation boundary per Bevy update:

1. Read controls and determine whether the simulation is paused, resumed, or
	 single-stepped. A pause request prevents wall-paced steps from that update
	 onward; a single-step while paused calls `step()` exactly once.
2. While running, consume whole seconds from the Bevy-side accumulator and call
	 `step()` once per whole second, preserving the fractional remainder. Do not
	 catch up while paused; resuming starts with no hidden paused-time backlog.
3. Apply discrete manual-command and constraint actions serially between complete
	 core steps. For same-update automatic catch-up and input, preserve the current
	 tick-before-input convention: all due steps occur before the manual command.
	 Keep discrete actions in stable input/event order. Do not interleave an
	 operation with a core step.
4. Publish a fresh owned snapshot after the final mutation, even if no step was
	 due, so manual commands and constraint registration/removal appear immediately.
5. Update rendering only from that snapshot plus immutable scenario metadata.

The operator workflow is therefore reproducible:

```text
launch scenario → pause → inspect → single-step → register/remove constraint
→ issue manual command → step again → inspect the resulting committed world
```

Steps advance exactly one core second each, and commands/constraint operations do
not themselves advance time. Snapshot frequency and render frame rate must not
affect results. Deterministic replay means same initial scenario, same ordered
operator actions, and same ordered core steps; no replay framework is required.

## 11. Constraint Visualization and Interaction

Support exactly the currently implemented kinds:

- `TrackUnavailable { from, to }`: marker on that directed edge only.
- `StationDeparturesBlocked { station }`: departure-block marker at that source;
	inbound admission remains possible.
- `StationUnavailable { station }`: station-unavailable marker; new admissions
	into and out of that station are blocked.

For every record, show ID, kind, target, origin, start, and optional end. Derive
Scheduled/Active from snapshot time using the half-open interval rules. Show
remaining active duration or time until scheduled start where finite/applicable.
Expired or explicitly removed records disappear from current snapshot listing;
do not imply a historical record remains available.

Provide both absolute and relative scheduling forms using public
`Simulation::create_constraint_at` and `create_constraint_in`, plus removal via
`remove_constraint`. Starts are strictly future-only and relative delay/duration
must meet core validation. Surface returned ID or core error. Do not silently
adjust an invalid time. Keep origin as explicit metadata and do not assign it
operational effect.

Constraints affect admission, not topology or existing claims. A closure may
overlay an occupied edge/slot. It does not destroy infrastructure, stop a train,
revoke a reservation, trigger route fallback, or guarantee eventual reopening.
Removal/expiry only clears that cause; physical occupancy and overlapping
constraints still apply. An automatic Ready train retries on a subsequent step.
A manual train requires a fresh Accelerate command; failed intent is not buffered.

The occupied-resource scenario must allow this reproduction without code edits:

```text
admit a train → while it is moving, schedule a future closure on its edge
→ observe activation and persistent reservation → observe normal arrival
→ observe another admission blocked → observe expiry/removal → step to retry
```

## 12. Physical Resource Visualization

Implement a pure, testable snapshot-to-claims projection. Use a map keyed by
resource identity and store `TrainId` as owner. For each snapshot train:

- Dwelling or Ready claims its station slot `(station, direction)`.
- Moving claims directed track `(from, to)` and reserves destination slot
	`(to, direction)`.

Render occupied station slots, occupied directed tracks, and destination
reservations as distinct states. Include owner IDs. Render opposite station slots
and reverse tracks independently. At a transition, update all markers from the
new complete snapshot, not from incremental Bevy events.

The core currently guarantees valid exclusive claims at committed boundaries.
The projection should still detect duplicate derived claims as an adapter
diagnostic/test failure; it must not silently overwrite an owner or feed a
repaired claim map back to core. Resource inspection is observation, not a
mutable resource editor.

## 13. Derived Diagnostics Policy

Derived explanations exist to help an operator inspect observable conditions,
not to create a second simulation authority. Inputs may include snapshot time,
train states and IDs, derived resource claims, immutable scenario topology, and
constraint records classified by their intervals.

Permitted examples:

- “Ready; destination Forward slot occupied by Train 4.”
- “Ready; active A→B closure.”
- “Ready; destination slot occupied and station unavailable.”
- “Ready; no departure observed.”

Do not claim an exact rejected-candidate result unless the relevant candidate is
unambiguous from public scenario metadata. Do not infer that the core evaluated
each possible blocker or store `BlockedReason` in Bevy. If exact per-decision
diagnostics become necessary, record the concrete user question and propose a
narrow observation contract separately; do not expose `ResourceView` or add a
core blocked state preemptively.

## 14. Testing Strategy

Keep railway behavior tests in `metro-core`. Bevy tests cover projection,
operator sequencing, and presentation correctness without reimplementing core
physics.

Core work is limited to the control-mode snapshot addition and corresponding
snapshot fixtures/tests. Preserve existing coverage for manual commands,
automatic arbitration, physical safety, constraints, and snapshots.

Sandbox tests should cover:

- Every snapshot train state, direction, velocity, and control mode maps to the
	right label/marker.
- Movement projection uses snapshot elapsed/travel values and stable station
	positions; it does not advance between snapshots through an independent clock.
- Physical claim projection derives station occupants, directed-track occupants,
	and destination reservations with correct owner IDs and opposite-direction
	independence; conflicting derived claims are diagnosed.
- Constraint scheduled/active classification and countdown use snapshot time and
	`[start,end)` semantics, including exact start/end boundaries and indefinite
	constraints.
- Pause prevents wall-paced steps; resume advances only unpaused accumulated
	real time; Single Step advances exactly one core step; rendering updates do not
	create extra steps.
- Manual input submits exactly one `Accelerate`; moving no-op and blocked result
	are presented without buffering or local state mutation.
- Constraint controls call the matching public API with exact target/timing/origin;
	removal calls the returned ID and displays errors without shadow lifecycle.
- Same-update automatic ticks precede manual input as specified; discrete
	operator action ordering is stable.
- Scenario descriptors construct deterministic networks, train IDs, initial
	claims, and initial snapshots.
- Selection, rendering order, and snapshot observation do not mutate core.
- Given the same fixture, action sequence, and step sequence, adapter-driven core
	snapshots match regardless of Bevy render/update frequency.

Do not duplicate all core admission/physics matrices as Bevy tests. Use small
Bevy `App` tests and pure projection tests, consistent with the current
`metro-bevy` test style. Verification gates for implementation are
`cargo test -p metro-core`, `cargo test -p metro-bevy`, `cargo test --workspace`,
and `cargo check --workspace` after snapshot API migration. This specification
draft has not run tests because it changes no code.

## 15. Acceptance Matrix

“Core contract” identifies behavior already guaranteed by core tests; “visual”
and “interactive” identify what the sandbox must make observable or reproducible.
Bevy acceptance must not be mistaken for a second proof of core arbitration.

| Behavior | Visual acceptance | Interactive reproduction | Core contract already guaranteed |
| --- | --- | --- | --- |
| Normal automatic movement | Train ID, direction, directed edge, progress, arrival, and fresh dwell are legible. | Run Basic automatic loop. | Stepped movement and snapshot timing. |
| Dwell → Ready → Moving | Three distinct observed states; Ready is stationary. | Pause/step through dwell completion and departure. | Automatic eligibility at dwell completion; Ready on rejection. |
| Terminal reversal | Arrival preserves old direction; reverse direction appears only on admitted departure. | Step at terminal; optionally block reversal then remove/expire blocker. | Candidate reversal and direction commit only on accepted departure. |
| Manual departure | Manual marker and one-shot Accelerate outcome visible. | Select Manual train and invoke Accelerate, including during dwell. | Public command may depart early and does not advance time. |
| Manual moving no-op | Moving/progress unchanged after command; local result is clear. | Accelerate while selected Manual train is moving. | Successful exact no-op. |
| Mixed auto/manual network | Control mode distinguishes each train; claims and directions remain legible together. | Run Mixed scenario and issue manual commands during automatic operation. | Both paths use shared admission; manual call is synchronous. |
| Directional station capacity | Two independent directional lanes/slots and owners shown. | Use scenario with opposite-direction station occupancy. | Opposite station slots may coexist. |
| Occupied track | Exact directed occupied edge and owner shown; reverse edge distinct. | Inspect or construct contention in scenario. | Directed track capacity one. |
| Destination reservation | Moving train's destination directional slot visibly reserved by its ID before arrival. | Step traversal and observe reservation-to-occupancy transition. | Exclusive reservation persists through traversal. |
| Automatic contention | Competing Ready trains and resulting admitted/remaining trains are visible. | Run contention fixture and step. | Numeric-ID arbitration; accepted batch claims prevent collision. |
| Manual serial contention | First command's resulting claim is visible before second result. | Submit competing commands in chosen order. | Call order decides; rejected command does not mutate. |
| Frozen conga | At each boundary show train positions, states, and physical claims. | Pause and single-step Frozen conga. | Starting-world claims freeze the step; no same-step cascading release. |
| Scheduled constraint visibility | Dashed/outlined target, start/end, ID, origin, and time-until-start shown. | Load planned fixture or schedule future constraint. | Snapshot includes scheduled unexpired records. |
| Constraint activation | Same marker changes classification exactly at `start_at`, using snapshot time. | Step through start boundary. | Half-open interval and step-boundary lifecycle. |
| Constraint expiry | Active marker ends at `end_at`; expired record disappears. | Step through end boundary. | Expiry is processed at step end; no retroactive retry in expiry step. |
| Explicit removal | Record vanishes immediately; other overlapping records remain visible. | Remove scheduled and active records by ID. | Removal is synchronous and independent per record. |
| Directed track closure | Only target arrow is marked; reverse arrow remains distinct. | Inject/schedule `TrackUnavailable`. | Exact directed-edge admission restriction; no rerouting. |
| Station departure closure | Source departure marker; inbound edge remains eligible absent other blockers. | Apply `StationDeparturesBlocked`. | Source departures blocked, arrivals permitted. |
| Station unavailable | Station marker indicates admission restriction in/out; no destruction cue. | Apply `StationUnavailable`. | New admission to/from station blocked; existing train retained. |
| Overlapping constraints | Distinct IDs/records and all applicable targets are shown. | Add overlapping same or different constraint, remove/expire one at a time. | Denials compose; removing one cause does not clear another. |
| Occupied-resource closure | Closure overlay and physical occupant/reservation coexist visibly. | Activate closure after train admission. | Registration succeeds; occupied-resource closure does not revoke claims. |
| Admitted traversal completion | Train continues edge progress and arrives on schedule under active closure. | Use Occupied-resource closure scenario. | Admission is not rechecked mid-traversal; reservation is preserved. |
| Waiting train retry | Ready train remains Ready during closure and departs only after a later eligible attempt. | Expire/remove closure, then step; issue fresh command if Manual. | Automatic retries; manual intent is not buffered. |
| Pause | Clock and world remain unchanged while paused despite render updates. | Pause and allow multiple Bevy updates. | Core only advances through `step()`. |
| Single Step | Clock advances exactly one second and all state changes are one committed World-N+1. | Single-step while paused. | One `step()` advances one integer second. |
| Deterministic operator replay | Same action/step trace yields same visible snapshots and constraint IDs. | Repeat same scenario/action sequence. | Core trace deterministic under same ordered operations; observation is pure. |
| Control mode | Each train visibly displays the snapshot mode. | Select either mode; only Manual exposes Accelerate. | Requires proposed snapshot mode field; control behavior exists. |
| Reversed storage order / ID arbitration | Result can be inspected through IDs; UI need not permute core storage. | Core-only test fixture, not an operator control. | Existing core test covers both storage orders. |

## 16. Implementation Checkpoints

Each checkpoint is tests-first, separately reviewable, and may be implemented
without opening a broad scenario editor or changing core rules.

### 1. Deterministic scenario descriptors

- **Scope:** Replace the fixed four-station fixture representation with a small
	immutable descriptor that creates the core network/trains and corresponding
	display labels/order/positions. Add the initial built-in scenario definitions.
- **Tests first:** Descriptor repeatability, station IDs/order, exact directed
	edges/durations, train creation order/IDs, valid initial claims, initial
	snapshot equality.
- **Implementation:** Have scenario construction use one descriptor for both
	`Network` building and static layout metadata.
- **Non-goals:** Network editing, arbitrary user placement, control-mode UI.
- **Review gate:** No parallel mutable topology/layout copies; existing four-node
	presentation still maps snapshot IDs correctly.

### 2. Train control mode observation

- **Scope:** Add `TrainControl` to `TrainSnapshot` and populate it from core-owned
	train state. Migrate core and workspace snapshot literals.
- **Tests first:** Snapshot reports Automatic/Manual correctly and remains owned,
	comparable, repeatable, and unaffected by observation.
- **Implementation:** Add the narrow snapshot field; no state or admission change.
- **Non-goals:** Public mutable control setter, mode switching, scenario logic in
	core, extra diagnostic fields.
- **Review gate:** Core remains the only source of mode; workspace compiles and
	existing behavior tests remain unchanged apart from expected DTO shape.

### 3. Pause, resume, and single-step

- **Scope:** Bevy-side pacing state and explicit time controls; keep integer core
	step semantics and define same-update ordering.
- **Tests first:** Pause blocks wall-paced step calls; resume has no paused backlog;
	Single Step causes exactly one; command follows catch-up ticks; update/render
	frequency causes no additional steps.
- **Implementation:** Extend the current accumulator/driver boundary, publish
	after mutations, and show simulation time/running status.
- **Non-goals:** Speed multiplier, core clock changes, wall-clock core behavior.
- **Review gate:** Equivalent unpaused frame partitions yield equivalent steps;
	paused frame partitions leave snapshot unchanged.

### 4. Train projection and identity/state rendering

- **Scope:** Render every snapshot train with stable ID, control mode, state,
	direction, velocity, and snapshot-derived position/progress.
- **Tests first:** Pure mappings for each state/mode/direction; projection uses
	snapshot movement values; unknown station/layout errors remain diagnostic.
- **Implementation:** Spawn/update presentation entities by `TrainId`, not array
	position; replace single `PlayerTrain` assumptions with selection state.
- **Non-goals:** New train states, physics clock, polished art.
- **Review gate:** Rendering is read-only and all labels match snapshots.

### 5. Resource and contention overlays

- **Scope:** Purely derive and render directional station occupancy, directed
	track occupancy, and destination reservations with owner IDs.
- **Tests first:** Each claim mapping, opposite-direction independence, moving
	train dual claims, duplicate-claim diagnostics, ordering/selection independence.
- **Implementation:** Add a presentation projection and simple overlays to the
	network view.
- **Non-goals:** Exporting `ResourceView`, editing claims, internal arbitration
	visualization.
- **Review gate:** Claims exactly match the snapshot reconstruction rules and do
	not alter `Simulation`.

### 6. Constraint visualization

- **Scope:** Display all snapshot constraints, classification, target markers,
	interval/origin/ID, and countdown values.
- **Tests first:** Scheduled/active boundary, finite/indefinite end, target
	rendering for all three kinds, expiry disappearance, overlapping IDs.
- **Implementation:** Derive lifecycle only from snapshot time and records.
- **Non-goals:** Runtime constraint editing, event history, mutable lifecycle
	cache.
- **Review gate:** Rendering does not advance time or mutate core; reverse track
	and inbound/departure semantics are represented accurately.

### 7. Runtime constraint controls

- **Scope:** Minimal create-at/create-in/remove controls for valid targets and
	times; display returned IDs/errors and refresh from snapshots.
- **Tests first:** Exact API/argument invocation, invalid input reporting, removal
	behavior, same-update ordering, no time advance by constraint operations.
- **Implementation:** Translate operator choices to existing public APIs.
- **Non-goals:** Cause/narrative system, bulk edits, constraint variants beyond
	current core API.
- **Review gate:** No shadow registry/lifecycle; active effect is still owned by
	core and only applies through core admission.

### 8. Inspector, manual command, and derived diagnostics

- **Scope:** Selection of train/station/directed edge; compact inspection details;
	selected manual train Accelerate; advisory current-blocker explanations.
- **Tests first:** Selection is presentation-only, correct claim/constraint
	association, manual command exactly once, error/no-op feedback without local
	state changes, ambiguous blocker wording.
- **Implementation:** Add simple operator controls and diagnostic formatter from
	public observations and immutable scenario metadata.
- **Non-goals:** Manual route choice, braking, command buffering/history,
	authoritative blocked reason.
- **Review gate:** Core results remain authoritative; no speculative explanation
	is rendered as a fact.

### 9. Scenario suite and end-to-end acceptance

- **Scope:** Complete six scenarios, add adapter-level traces for the acceptance
	matrix, and run workspace verification.
- **Tests first:** Conga step trace, occupied-resource closure, planned/injected
	activation/expiry/removal, mixed auto/manual, deterministic action replay,
	frequent versus sparse rendering/observation.
- **Implementation:** Fill scenario-specific metadata and presentation gaps
	exposed by tests; do not change railway policy.
- **Non-goals:** Liveness repairs, arbitrary scenario editor, gameplay systems.
- **Review gate:** Acceptance rows are visibly reproducible, core tests remain the
	proof of core semantics, and `cargo test --workspace` plus
	`cargo check --workspace` pass.

## 17. Definition of Done

SPEC-006 is implemented when:

- The built-in scenarios launch from a deterministic chooser and visibly expose
	topology, IDs, control modes, state, direction, movement, dwell, time, and
	constraint state.
- Pause, resume, single-step, manual Accelerate, future constraint scheduling,
	and constraint removal work through the public core API and preserve explicit
	action ordering.
- Physical station occupancy, directed-track occupancy, and destination
	reservations are visibly distinct and derived from snapshots.
- Scheduled and active constraints are classified solely from snapshot time and
	half-open intervals; all three current kinds have correct target markers.
- The conga, occupied-resource closure, and congestion scenarios can be inspected
	one committed step at a time without code edits.
- Derived diagnostics never masquerade as authoritative core state, and a Ready
	train remains Ready in the UI even when a blocker is visible.
- Rendering frequency, selection, and snapshot inspection do not affect core
	results; same ordered scenario/actions/steps replay deterministically.
- Core and workspace tests/checks pass, with core behavior tested in `metro-core`
	and adapter behavior tested in `metro-bevy`.

## 18. Deferred Work and Non-Goals

Explicitly defer:

- Gameplay rules or a `metro-game` crate.
- Zombies, aliens, enemies, scoring, missions, economy, progression, narrative
	incidents, player abilities, or passenger simulation.
- Timetables unless later modeled in core; schedule optimization.
- Dynamic pathfinding, rerouting, skip-station, bypass, emergency braking,
	destruction physics, or automatic congestion/deadlock repair.
- Network editing, arbitrary scenario editor, save/load, multiplayer/networking.
- Mobile controls, sound, particles, camera effects, production art/assets, and
	polished game feel.
- New core event log, command journal, exact denial-history API, or public
	`ResourceView`/`ConstraintView`.
- Simulation speed multiplier in the initial sandbox implementation.

The sandbox should reveal whether later work needs concepts such as liveness,
fairness, alternate routing, stronger topology observation, or a richer
admission explanation. These are not preemptively added here.

## 19. Bug-Discovery Mindset and Architectural Consistency Review

The point is to make unusual but valid states easy to create and inspect. Use
pause/step and constraint controls to look for:

- Missing domain concepts that operators repeatedly need to explain a world.
- Ambiguous operational semantics and surprising but valid outcomes.
- Hidden deadlocks, starvation, and indefinite blocking.
- Unexpected terminal reversal or directional-slot behavior.
- Confusing physical ownership/reservation visualization.
- Constraint combinations whose admission effects are unintuitive.
- Snapshot gaps that prevent a reliable explanation.

Record findings for a future spec (for example SPEC-007+); do not solve
hypothetical findings by expanding this milestone. A legal infinite wait is an
observation, not automatically a bug.

| Boundary | SPEC-006 consistency |
| --- | --- |
| Core/domain privacy | Core remains authoritative; no Bevy dependency or UI concepts enter core. Only train control mode is proposed as a justified observation field. |
| Network/topology | Scenario descriptor builds topology and display metadata once. Current adjacent-index route selection is honored; no dynamic graph/pathfinding is implied. |
| Physical resource ownership | Resource claims derive from snapshot train states; private `ResourceView` remains private and closures never become physical claims. |
| Train lifecycle | UI shows only Dwelling, Ready, and Moving from snapshots; there is no invented Blocked state, brake, or buffered command. |
| Arbitration | Core owns frozen World-N proposal decisions, ID ordering, and serial manual admission. Bevy displays committed outcomes only. |
| Constraints | Existing public scheduling/removal APIs, snapshot records, and interval semantics are consumed directly. No duplicated activation/expiry truth or physical destruction fiction. |
| Time and mutation | Bevy controls when `step()` is called. Inputs occur between core steps; frame/render frequency is not simulation authority. |
| Testing | Core rules remain covered in core tests; Bevy tests validate projection, sequencing, and interaction mapping. |

### Final design answers

- **Can the existing public core API support the sandbox without architectural
	leakage?** Yes for simulation, constraints, commands, snapshots, and exact
	resource reconstruction. Complete control-mode display requires the narrow
	snapshot addition below; no private internals need exposure.
- **What is genuinely missing from snapshots?** Train control mode. The
	recommended addition is `TrainSnapshot.control: TrainControl`. Network labels
	and enumerable topology are also not snapshots, but built-in scenarios have a
	single immutable descriptor that supplies both core construction and display
	metadata, so no new network snapshot is needed now.
- **What should remain derived in Bevy?** Resource claims, movement position,
	scheduled/active/remaining constraint presentation, selection, and cautious
	observable-blocker explanations.
- **What should remain private to core?** `ResourceView`, `ConstraintView`,
	automatic proposal/accepted-claim internals, transient denial decisions, and
	authoritative simulation state.
- **Are there unresolved decisions that block checkpoint implementation?** No
	architectural decision blocks it. The control-mode snapshot field is a
	concrete, narrow checkpoint requirement. Arbitrary external network
	introspection and exact denial history are deliberately deferred.
- **Does SPEC-006 remain a sandbox/debugger rather than becoming a game layer?**
	Yes. It adds observation and operator controls around existing rules only;
	gameplay, progression, narrative, pathfinding, and liveness policy remain out
	of scope.