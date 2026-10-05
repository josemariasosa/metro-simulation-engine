# SPEC-005: External Operational Constraints

## 1. Status and purpose

**Draft for review — specification only; implementation is not authorized.**

Inspected on 2026-10-05 at commit `9738b62`, following the domain/application
boundary cleanup. MUST and MUST NOT express requirements; SHOULD expresses a
recommendation. This document selects an MVP and future implementation checkpoints.
It does not introduce production code, placeholder APIs, or new tests.

`metro-core` must support operational conditions imposed by external systems.
It understands restrictions on railway operation, never their narrative causes.
A game, scenario runner, maintenance system, or future policy engine translates
its own concepts into these restrictions and retains its own cause metadata.

```text
external system: incident / policy / scenario
        ↓ consumer-owned translation
core boundary: add or remove an operational constraint
        ↓ Simulation-owned active constraint records
World N: physical ResourceView + immutable operational restriction view
        ↓ shared domain departure admission
automatic arbitration / synchronous manual command
        ↓ committed World N+1 / owned snapshot
```

## 2. Repository findings and dependency direction

The source is authoritative where older specifications describe historical APIs.
In particular, SPEC-003 and SPEC-004 still contain pre-implementation status,
old module paths, constructor shapes, and ownership descriptions. This proposal
does not rewrite those documents or infer today's architecture from those passages.

| Area | Actual responsibility today | Evidence |
| --- | --- | --- |
| Domain trains | `TrainId`, `Direction`, control mode, capacity, private train fields, `AtStation`/`Moving` state, and narrow state transitions | [train.rs](../../metro-core/src/domain/train.rs) |
| Domain topology | Station IDs, station insertion order, directed tracks and travel duration; adjacent-index route lookup | [network.rs](../../metro-core/src/domain/network.rs), [station.rs](../../metro-core/src/domain/station.rs), [track.rs](../../metro-core/src/domain/track.rs) |
| Domain departures | Pure candidate selection and admission checks; selection reverses only when the current-direction track does not exist | [departure.rs](../../metro-core/src/domain/departure.rs) |
| Domain physical resources | Generic `ResourceView<Owner>` derived from trains paired with opaque owner tokens; owners are neither interpreted nor ordered | [resource.rs](../../metro-core/src/domain/resource.rs) |
| Application simulation | `TrainEntity`, monotonic identity allocation, registration validation, command handling, eligibility, proposal ordering, contention, time advancement, movement/dwell orchestration, snapshot mapping | [simulation.rs](../../metro-core/src/simulation.rs) |
| Boundary DTOs | `TrainCommand::Accelerate`, `CommandError`, owned comparable snapshot values | [command.rs](../../metro-core/src/command.rs), [snapshot.rs](../../metro-core/src/snapshot.rs) |
| Consumer adapter | Whole-second pacing, tick-before-input sequencing, command submission, snapshot publication and presentation | [timing.rs](../../metro-bevy/src/timing.rs), [presentation.rs](../../metro-bevy/src/presentation.rs) |

[`lib.rs`](../../metro-core/src/lib.rs) keeps `domain` private and re-exports
selected domain types at the crate root. Public `command`, `simulation`, and
`snapshot` modules consume domain values. Domain modules do not depend on
application commands, snapshot DTOs, Bevy, or `Simulation`. `Train` does not own
departure selection, resource arbitration, or simulation identity allocation.
`TrainEntity` associates a domain train with its simulation identity.

`ResourceView::derive` asserts incompatible physical claims rather than replacing
owners. A stopped train occupies `(station, committed direction)`. A moving train
occupies `(from,to)` and reserves `(to, committed direction)`. Each station has
two independent capacity-one directional slots. Reverse directed tracks are
independent capacity-one resources. `Network` stores topology, not ownership.

Resource availability is evaluated in `ResourceView::track_available` and
`station_slot_available`, composed by `can_admit_departure`. Both manual and
automatic paths call that helper. Automatic proposals use one frozen starting
view, then application-owned accepted-track and accepted-destination-slot sets;
proposals resolve by ascending numeric `TrainId.0`. The stored train vector is
not sorted. Released starting claims cannot be reused during that step.
Manual commands derive the latest committed resources and commit synchronously;
serial call order determines contention, even at equal simulation timestamps.

Movement completion does not call departure admission. Accepted departures start
at traversal elapsed zero; later steps complete the timed track and begin fresh
dwell. Blocked automatic departures become/remain Ready and retry on later steps.
Blocked manual commands preserve state exactly and do not buffer intent.
Already-moving manual Accelerate remains an exact successful no-op.

Snapshots currently contain only `elapsed_seconds` and vector-ordered trains,
including direction, velocity, state and movement timing. Snapshot generation is
pure and owned. There is no external state, event queue, constraint registry,
resource catalog, blocked train state, or incident framework in the current core.

The relevant regression evidence includes [automatic_blocking.rs](../../metro-core/tests/automatic_blocking.rs),
[automatic_blocking_tests.rs](../../metro-core/src/test_utils/automatic_blocking_tests.rs),
[physical_safety_tests.rs](../../metro-core/src/test_utils/physical_safety_tests.rs),
[manual_control.rs](../../metro-core/tests/manual_control.rs), and
[snapshot_observation.rs](../../metro-core/tests/snapshot_observation.rs).
These prove frozen conga ownership, numeric-ID terminal contention under reversed
storage order, no reverse fallback on blocking, independent lanes, exclusive
reservations through arrival, serial manual ordering and observation independence.
Inspection baseline: `cargo test -p metro-core --offline` passes 114 tests
(74 unit and 40 integration tests). The existing unused station-field warning remains.

## 3. Event versus persistent constraint

An external event says something happened. An operational constraint says a rule
remains in force. The MVP MUST model the latter as persistent simulation state.
There is no public `ExternalEvent` envelope, event bus, narrative payload, callback,
or event-sourced world in this slice.

Adding/removing a constraint is a synchronous boundary operation, analogous to a
command. The operation is momentary; its accepted record persists until explicitly
removed or expired. Core does not infer repair, incident resolution, or a duration
from a cause. The consumer translates causes into constraints and retains the
returned IDs to manage their lifecycle.

For example, an external incident affecting B–C may create two directed-track
constraints, B→C and C→B. An incident that prevents departures from C creates
one station-departure constraint. Names, artwork, severity, source actor, narrative
and causation remain in the consumer. Core MUST NOT expose game-specific variants
such as `AlienInvasion`, `ZombieAttack`, or `BossFight`.

## 4. Selected operational vocabulary

The proposed public domain value is `OperationalConstraint`:

```rust
// Proposed contract only; not existing source.
pub enum OperationalConstraint {
    TrackUnavailable { from: StationId, to: StationId },
    StationDeparturesBlocked { station: StationId },
    StationUnavailable { station: StationId },
}
```

| Constraint | Additional departure rejection rule |
| --- | --- |
| `TrackUnavailable { from, to }` | Candidate uses exactly that directed edge. Reverse edge is unaffected. |
| `StationDeparturesBlocked { station }` | Candidate source is that station, for either selected direction. Arrivals and inbound admissions are permitted. |
| `StationUnavailable { station }` | Candidate source or destination is that station, covering both directional slots for new admissions. |

`StationUnavailable` is included because prohibiting admission to a station cannot
be expressed by a source-departure restriction. It is an admission closure, not
physical deletion, evacuation, loss of existing capacity, or prevention of a
previously reserved arrival. A consumer wanting only inbound closure has no
dedicated station variant in this MVP; incident-specific combinations must use
the defined vocabulary rather than silently reinterpret it.

No per-direction station restriction, speed limit, capacity adjustment, dwell
modifier, train-targeted restriction, route mutation, or emergency stop is included.
Closed tracks remain in the network and remain candidates. Blocking MUST NOT cause
reverse fallback, rerouting, or `NoOutgoingTrack`. Terminal reversal checks the
selected outgoing track and station endpoints, while preserving current source-slot
semantics. A blocked reversal does not commit a direction change.

## 5. Keep restrictions separate from physical claims

Existing resource primitives can identify the relevant directed edges and slots,
but they cannot truthfully represent an external closure as ownership. A closure
has no train owner, may overlap an occupied/reserved resource, may have multiple
independent causes, and may prohibit source departures without claiming any slot.

`ResourceView<Owner>` MUST remain derived exclusively from physical train state.
Do not insert fake trains, sentinel owners, external owners, or synthetic station
reservations. Do not persist a second ownership registry or move external state
into `Train`, `Station`, or `Track`.

Introduce a small crate-private domain `RestrictionView` with membership queries
for closed directed tracks, blocked station departures, and unavailable stations.
It contains operational values only, with no record IDs, expiry scheduling, source
metadata, clock access, or knowledge of `Simulation`. Simulation derives this
temporary view from its active records. Concrete sets are sufficient; a generic
policy plugin trait, resource-manager rewrite, or callback engine is unwarranted.

Extend the shared domain admission helper to accept both views:

```text
admit(candidate) =
    physical track available
    AND physical destination directional slot available
    AND outgoing track not externally unavailable
    AND source departures not externally blocked
    AND source station not externally unavailable
    AND destination station not externally unavailable
```

External restrictions only subtract admission permission. Removing them never
overrides occupancy, reservations, or accepted batch claims. Multiple restrictions
compose by logical OR for blocking; there is no permissive override or priority.
Resource availability queries continue to describe physical claims. Operational
permission is composed in departure admission, rather than changing the meaning
of `ResourceView::track_available`.

## 6. Public lifecycle contract

Use a separate boundary from train commands. Proposed signatures are:

```rust
// Proposed APIs on Simulation; no implementation in this draft.
fn add_constraint(
    &mut self,
    constraint: OperationalConstraint,
    expires_at: Option<u64>,
) -> Result<ConstraintId, ConstraintError>;

fn remove_constraint(&mut self, id: ConstraintId)
    -> Result<(), ConstraintError>;
```

`ConstraintId(pub u64)` is an opaque simulation-local handle with equality/hashing,
Copy/Clone/Debug, not an incident ID. Simulation allocates monotonically from zero
and MUST NOT reuse IDs after removal or expiry. Like train IDs, handles are scoped
to a simulation; cross-simulation use is unsupported and is not detectable merely
from the numeric value. Identity allocation and record storage belong to application
orchestration. `OperationalConstraint` belongs to domain; exported handles/errors
and the application record belong to the public constraint boundary module.

An active record stores its ID, operational constraint, `activated_at` simulation
second, and optional absolute `expires_at`. `Simulation::new` starts with no records
and retains its current signature. Callers can add initial constraints before
the first step. There is no mutable registry accessor or replace/update API.
Changing a restriction means removing a handle and adding another in caller order.
There is no atomic multi-record transaction in the MVP.

Validation MUST complete before changing state or consuming an ID:

1. Check station references; an ID is valid when its index is below station count.
2. For a track constraint, require the exact directed `Network::track(from,to)`.
3. Require `expires_at > elapsed_seconds` when an expiry is supplied.
4. Check identity-counter capacity before allocating.

Proposed errors: `UnknownStation`, `UnknownTrack`, `InvalidExpiry`,
`IdExhausted`, and `UnknownConstraint`. Unknown/removed/expired handles return
`UnknownConstraint` on removal, without mutation. Repeated additions of an equal
constraint MUST create independent records with distinct IDs. Retried addition is
therefore not idempotent; reliable input deduplication belongs to the consumer.
Errors MUST preserve records, allocator, trains, resources and time exactly.

Successful lifecycle operations do not advance time, change any train, retry a
departure, or revoke a physical claim. Successful removal affects subsequent
admission; it does not start a train. There is deliberately no global `unblock`
operation: removing one cause cannot clear another cause's restriction.

## 7. Time, expiry and ordering

The lifecycle uses integer simulation seconds, never wall time, frame deltas, or
randomness. `None` means indefinite until removal. A record added at T with expiry E
is effective over `[T,E)`. Setting E=T or a past expiry is rejected. Future scheduled
activation is deferred; additions activate immediately between completed steps.

At entry to a step at T, all records effective at T participate in one immutable
restriction view. After resolving departures and updating trains, advance to T+1
and remove records with expiry at or before T+1 before returning. Thus a record
expiring at 3 blocks admission during the step from 2 to 3, is absent in the
committed snapshot at 3, and permits a manual command at 3 or an automatic proposal
in the next step from 3 to 4. The automatic train does not depart retroactively
in the expiring step. Expiry does not restart dwell or enqueue work.

This end-of-step cleanup ensures snapshots and commands see only active records
without making observation mutate state. No expiry processing occurs inside
snapshot generation. Pausing steps does not age a constraint. Time remains within
the repository's existing representable-counter assumptions; arbitrary writes to
the legacy public clock or topology are unsupported runtime behavior.

Calls are serial transactions: add-before-Accelerate can reject that command;
Accelerate-before-add commits a traversal that the later addition cannot revoke.
Remove-before-step affects that step; remove-after-step affects subsequent calls.
Equal timestamps are insufficient to reconstruct ordering. Deterministic replay
requires the same initial setup and ordered sequence of steps, train commands and
constraint operations. No replay storage or persistence format is introduced.

## 8. Integration with the existing simulation pipeline

The automatic pipeline MUST remain:

1. Derive physical starting resources and operational restrictions from committed
   World N. Collect eligible candidates with the existing domain selector.
2. Resolve proposals by numeric train ID using the shared admission helper and
   the existing application-owned accepted-claim sets. Both views remain frozen.
3. Commit fixed decisions and advance other trains with existing narrow transitions.
4. Advance one simulation second and expire constraint records at that boundary.

Do not migrate proposal identity, ordering, clocks, or record lifecycles into domain
primitives. Only departure admission gains operational input. Existing movement,
dwell and topology semantics remain authoritative.

Manual validation order remains unknown train → control mode → moving no-op →
candidate selection → combined admission → commit. A rejected operational admission
returns existing `CommandError::Blocked`. It preserves the entire pre-command
world, including constraints. No new train blocked state or reason taxonomy is
required. Already-moving manual Accelerate returns `Ok(())` even after closure.

An automatic train denied by a constraint completes dwell into Ready, stays in
its committed directional source slot at velocity zero, and retries once on each
later step. A manual train requires a fresh command after removal/expiry. Early
manual departure is still permitted when combined admission succeeds.

## 9. Constraints imposed on occupied resources

Adding a closure over an occupied track, reserved station, or occupied station
MUST succeed when target/time/ID validation succeeds. Restrictions and claims can
coexist without violating the physical capacity invariants.

An already-moving train MUST continue its timed traversal and arrive into its own
reserved destination, including when the track or destination becomes unavailable
after departure. Its reservation remains exclusive. This preserves SPEC-003's
admitted-traversal completion contract; mid-track stopping or revoking arrival
would require additional states and safety rules outside this milestone.

A train already at an unavailable station remains there; dwell continues normally
and subsequent departures are blocked. No train is removed, teleported, damaged,
or evacuated. A departure-only restriction allows new arrivals, which may then
wait indefinitely. Station closure or physical congestion may prevent all progress;
safety, rather than guaranteed liveness, is the MVP contract.

Consumers MUST translate only effects compatible with these semantics. A real-world
or narrative assertion that an occupied tunnel is physically destroyed requires
an emergency movement model beyond SPEC-005; this vocabulary represents the resulting
closure to new admissions, not destruction physics.

## 10. Observation contract

Extend `SimulationSnapshot` with `constraints: Vec<ConstraintSnapshot>`:

```rust
// Proposed owned DTO, alongside existing snapshot types.
pub struct ConstraintSnapshot {
    pub id: ConstraintId,
    pub constraint: OperationalConstraint,
    pub activated_at: u64,
    pub expires_at: Option<u64>,
}
```

These values and the operational enum MUST be owned, cloneable and comparable.
Snapshots list active records in ascending numeric constraint-ID order, regardless
of internal registry iteration. Duplicate effects appear as distinct records so
consumers can observe independent lifecycles. Empty simulations report an empty
constraint vector. No cause strings, mutable registry, synthetic claims, event
history, or predicted departure time are exposed.

Train snapshot fields and train vector ordering remain unchanged. Ready continues
to mean completed dwell, not a unique explanation of why a train waits. Observed
constraints show restrictions in force, not a definitive per-train denial reason.
Repeated/omitted snapshots cannot affect expiry, IDs, admissions or arbitration.
Retained snapshots survive lifecycle changes at the same elapsed timestamp.

Adding a public snapshot field requires updating struct literals in core tests and
any workspace consumers. This is an explicit source compatibility change in the
current 0.1 crate. Existing snapshot methods and train command signatures stay
unchanged. Export operational values, IDs and errors through a public `constraint`
module; the private domain model remains private. No serialization dependency is
needed. Bevy continues to display train snapshots and publish after mutations;
constraint rendering, incident UI and a new `metro-game` crate are deferred.

## 11. Acceptance and verification

All scenarios MUST assert physical invariants at committed boundaries and use
ID-keyed train comparisons where storage order differs. Unless specified, use
adjacent bidirectional two-second tracks and three-second initial dwell.

| Scenario | Required result |
| --- | --- |
| Empty restrictions | Existing 114 core tests preserve railway behavior; snapshot literal changes only add empty records. |
| Directed closure | A→B closure blocks automatic and early manual A→B admission; B→A remains admissible when physically free. |
| Departure-only station restriction | A→B remains admissible with departures blocked at B; B→C and terminal reverse departures from B are blocked. |
| Station unavailable | New inbound and outbound departures in either direction are blocked; unrelated stations/tracks remain admissible. |
| Existing-track selection | Blocked current-direction edge does not select a free reverse edge; blocked terminal reversal preserves original direction and slot. |
| Manual exact rejection | `Blocked` preserves all trains, timings, directions, velocities, records and derived physical claims; unknown/nonmanual/moving validation precedence is unchanged. |
| Automatic dwell and retry | Block at A before dwell completes: t=3 is Ready A, stopped. Remove at t=3: no immediate departure; next step commits movement at t=4, traversal elapsed zero. |
| Manual no buffering | Reject, remove closure, then step: train remains stopped; fresh Accelerate can depart without advancing time. |
| Independent causes | Two equal records block one target. Removing or expiring one leaves the other effective; removing the last exposes normal physical admission. |
| Restriction composition | Track closure, source block and destination closure compose without override; clearing a restriction never clears an unrelated one. |
| Occupied resources | Add closures after departure and over occupied stations successfully; exact physical owners/reservations remain unchanged. Moving train completes at the original time with fresh dwell. |
| Moving no-op | Accelerate on a moving manual train succeeds without progress/reset even under track and destination closure. |
| Physical ownership after removal | With closures cleared, existing occupants/reservations still block; lower-ID proposals cannot displace them. |
| Expiry boundary | Add at t=0, E=3: effective at t=0/1/2, automatic t=3 remains Ready, snapshot at 3 omits record, manual admission at 3 can succeed, automatic retry departs at 4. |
| Indefinite duration | Many steps and arbitrary snapshots do not remove a `None` record; only explicit removal does. |
| Validation atomicity | Invalid source/destination/station/edge, expiry at or before now, exhausted allocator and unknown removal leave full world and allocator unchanged. Include reverse-edge absence and expired handle removal. |
| Identity lifecycle | Equal additions get unique IDs; IDs are not reused after removal or expiry; failed additions consume no ID. |
| Frozen conga | A/B/C Forward trains, D empty, closure C→D: t=3 all Ready. Remove at 3: t=4 only C→D departs; t=5 B→C departs; t=6 A→B departs. No same-step cascading release. |
| Deterministic contention | Hold both terminal competitors with a closure, then remove it. Existing lowest-ID winner is unchanged under reversed train storage order; manual competitors still resolve by call order. |
| Ordered calls at equal time | Add then Accelerate rejects; Accelerate then add retains moving train. Replay identical operation order produces equal worlds and result sequences. |
| Snapshot ownership/order | Repeated reads equal; editing retained DTOs changes no simulation; record order numeric by ID; retained snapshots preserve removed/expired records and old train state. |
| Observation independence | Frequent versus sparse snapshots through add/remove/expiry, conga and contention produce identical outcomes and ID allocation. |

Domain unit tests isolate each restriction predicate with physically available
resources and prove physical guards still reject when the restriction view is
empty. Application unit tests may inspect private allocator state or reverse
storage order using established test hooks. Public integration traces MUST use
supported constructors, lifecycle operations, steps, train commands and snapshots.
Avoid exposing internal resource views or mutable records for testing convenience.

Future implementation verification: `cargo test -p metro-core`, relevant workspace
consumer tests, and a workspace build/check to catch snapshot/API migration.
No graphical run is required for a core-only milestone with no presentation change.

## 12. Future implementation checkpoints

Each checkpoint is tests-first, compiles independently, and ends in review.
Specification approval alone does not authorize beginning implementation.

1. **Domain vocabulary and admission.** Add operational values, a private restriction
   view and isolated combined-admission tests. Wire both departure paths to an
   initially empty restriction view without changing behavior. Review dependency
   direction and existing regression results.
2. **Application lifecycle and observation.** Introduce private simulation records,
   IDs, validation, add/remove operations, snapshot records and literal migrations.
   Wire active records into both paths in the same checkpoint so no accepted
   restriction is observable yet operationally ignored. Verify independent causes,
   occupied-resource coexistence, serial ordering and owned snapshots.
3. **Expiry and complete traces.** Add end-of-step expiry, boundary tests, automatic
   retry/manual no-buffering traces, conga and contention under restrictions, and
   frequent/sparse observation replays. Complete workspace compatibility checks.

Do not use these checkpoints to move orchestration back into domain, introduce a
new generic simulation engine, or implement consumer game concepts.

## 13. Definition of done and deferred work

Implementation is complete when all selected constraint kinds have the defined
independent lifecycle, optional simulation-time expiry and owned observation;
manual and automatic admission share the same restriction rules; existing physical
claims and traversal completion remain correct; deterministic acceptance traces
pass; and workspace consumers compile after the snapshot addition.

Deferred: scheduled activation, event journals/replay storage, persistence/serde,
atomic bulk changes, external source identity/deduplication, permission systems,
blocking-reason DTOs, emergency stopping, dynamic topology/destruction, rerouting,
configurable station capacities, directional station closures, speed/dwell effects,
fairness/deadlock handling, presentation and game integrations.

The selected decisions for review are persistent constraints rather than a core
event framework; separate restriction and ownership views; three generic closure
types; independent handle-based records; immediate serial lifecycle operations;
optional half-open simulation-time expiry; completion of already-admitted movement;
and an explicit additive snapshot field. No architectural decision is left to an
implementation accident, and this draft does not authorize production changes.
