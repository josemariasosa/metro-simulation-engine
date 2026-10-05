# SPEC-005: External Operational Constraints

## 1. Status and purpose

**Revised proposal — ready for checkpoint implementation review; specification only.**

Inspected on 2026-10-05 at commit `5381e1a`, following the domain/application
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
core boundary: schedule or remove an operational constraint
        ↓ Simulation-owned scheduled/active constraint records
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
The prior inspection reported 114 passing core tests (74 unit and 40 integration
tests). This revision inspects source; that count is historical evidence, not a
new test run or a fixed acceptance-test count.

## 3. Event versus persistent constraint

An external event says something happened. An operational constraint says a rule
remains in force. The MVP MUST model the latter as persistent simulation state.
There is no public `ExternalEvent` envelope, event bus, narrative payload, callback,
or event-sourced world in this slice.

Adding/removing a constraint is a synchronous boundary operation, analogous to a
command. Registration schedules future permission changes; it never closes a resource in
the already-committed present. Its accepted record persists until explicitly
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

Introduce a small crate-private domain `ConstraintView` with membership queries
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

One operational model has two scheduling entry points on `Simulation`:

```rust
// Proposed contract only, not existing implementation. All times are u64 seconds.
fn create_constraint_at(
    &mut self,
    constraint: OperationalConstraint,
    start_at: u64,
    end_at: Option<u64>,
    origin: ConstraintOrigin,
) -> Result<ConstraintId, ConstraintError>;

fn create_constraint_in(
    &mut self,
    constraint: OperationalConstraint,
    starts_in: u64,
    duration: Option<u64>,
    origin: ConstraintOrigin,
) -> Result<ConstraintId, ConstraintError>;

fn remove_constraint(&mut self, id: ConstraintId) -> Result<(), ConstraintError>;

pub enum ConstraintOrigin { Planned, Injected }
```

`ConstraintId(pub u64)` is an opaque simulation-local handle supporting
Copy/Clone/Debug/equality/hashing. Allocate monotonically from zero, never reuse
after removal or expiry. Cross-simulation handles are unsupported; equal numbers
cannot establish provenance. Use a checked next-counter increment, matching the
train allocator convention: counter exhaustion fails before allocation or mutation.

The private canonical application record contains exactly `id`, `constraint`,
`start_at`, `end_at`, and `origin`. There are no `PlannedConstraint` or
`InjectedConstraint` domain types. Do not store `activated_at`: activation is
completely determined by `start_at` and the current World time. A registration
timestamp, mutable active flag, activation callback, or activation queue is not
needed. `Simulation::new` retains its signature and starts with an empty registry.
There is no mutable registry accessor, update API, or bulk transaction.

### Canonical interval and conversion

Absolute registration supplies `[start_at, end_at)` directly; `None` means
`[start_at, infinity)`. Require **`start_at > elapsed_seconds` always**, including
scenario setup. A finite end MUST satisfy `end_at > start_at`.

Relative registration reads the current simulation clock T once during its serial
call. Require `starts_in > 0` and, when supplied, `duration > 0`. Compute with
checked arithmetic:

```text
start_at = T + starts_in
end_at   = Some(start_at + duration), or None
```

Duration runs from activation, not registration. Overflow is an explicit error;
never wrap, saturate, round, clamp, or translate zero into one second. Immediately
pass converted values to the same canonical validation/allocation/storage routine
used by absolute registration. There is one registry and one lifecycle.
For example, at T=10, `_at(c, 11, Some(15), o)` and
`_in(c, 1, Some(4), o)` produce the same timing. In equal initial simulations they
produce equal records and IDs; within one simulation they create independent IDs.

At initial time 0, the earliest permitted start is 1. There is deliberately no
setup exception for time 0 and no immediate-activation API. A planned closure
cannot affect automatic decisions in the step 0→1 or manual admission at 0.
Scenario authors must schedule their operational horizon accordingly; core MUST
NOT silently shift a requested interval or suppress those initial admissions.

### Origin is explicit metadata

Retain `ConstraintOrigin` in the MVP for observation. The caller explicitly labels
`Planned` when the record belongs to its prearranged scenario plan, and `Injected`
when it is an intervention introduced during execution. These are caller assertions,
not phases inferred or enforced by core. Either API accepts either origin: absolute
runtime injection and relative scenario setup are both valid. Core does not infer
origin from the clock, API choice, first step, or train activity.

Origin MUST NOT affect admission, priority, scheduling validation, expiry,
arbitration, or lifecycle paths. It is not source identity or narrative causation.
Replaying identical records with only origin changed preserves operational results
while intentionally changing observed metadata.

### Validation and errors

Use a separate `ConstraintError`, not `CommandError`. Define explicit variants:

| Error | Meaning |
| --- | --- |
| `InvalidStart` | Absolute start is at or before T, or relative delay is zero. |
| `InvalidEnd` | Finite absolute end is at or before start, or duration is zero. |
| `TimeOverflow` | A relative start or end cannot be represented as u64. |
| `UnknownStation` | A referenced station index is outside the network. |
| `UnknownTrack` | Both endpoints exist but the exact directed edge does not. |
| `IdExhausted` | The checked identity counter cannot advance. |
| `UnknownConstraint` | Removal names an unknown, removed, or expired handle. |

For deterministic error precedence, relative normalization checks zero delay,
zero duration, start overflow, then end overflow; absolute calls need no
normalization. The common path checks start, finite end, station references
(source before destination for tracks), exact directed track, then ID capacity.
Only after all checks succeed may it allocate and insert. With otherwise valid
inputs, both APIs produce identical canonical validation results.

Every failure registers nothing and consumes no ID. Trains, physical claims,
records, time, allocator state and resulting snapshots MUST remain exactly equal
to their pre-call values. Do not perform opportunistic cleanup on failed calls.
Validate private allocator state as well as visible snapshots in tests.

Equal constraints create distinct records and independent lifecycles. They are
not deduplicated; consumer retry deduplication remains outside core. Removing a
scheduled record cancels it; removing an active record lifts that cause for later
serial admissions. Removing one cause never clears another. Removal is immediate,
including at the same timestamp, but never revisits committed decisions. The
strict future-only rule governs registration, not explicit cancellation/removal.
This preserves the existing synchronous command boundary, without adding a second
scheduled-removal API. Changing a record requires removal and new future registration.

Successful registration/removal does not advance time, mutate a train or physical
claim, retry a departure, or start a train. Registration changes observable future
records immediately, but leaves current operational permission unchanged.

## 7. Time, activation, expiry and ordering

Lifecycle uses integer simulation seconds, never wall time or frame deltas:

```text
t < start_at                             scheduled, inactive
start_at <= t && (end_at is None || t < end_at)   active
end_at is Some(E) && t >= E               expired
```

Derive activity from the World timestamp; do not persist a second lifecycle state.
At the end of each one-second step, after fixed decisions and movement/dwell
updates, advance time to T+1 and remove records whose finite end is at or before
T+1 before returning World N+1. Scheduled records remain stored. Activation needs
no mutation: records whose start equals T+1 participate in the next view, never
in the step just completed. No cleanup or activation occurs inside observation.
Pausing steps does not age restrictions.

For a record registered at current_time=10 with start_at=11 and end_at=15:

| Admission point | Effect of this record |
| --- | --- |
| Manual at 10; automatic step 10→11 | None; record is scheduled. Any accepted traversal remains valid. |
| Snapshot at 11 | Record is active, even though decisions producing 11 used time 10. |
| Manual at 11, 12, 13, 14 | Blocks matching new admission. |
| Automatic steps 11→12, 12→13, 13→14, 14→15 | Blocks matching proposals against each starting World. |
| Snapshot at 15 | Record has expired and is absent. A train blocked during 14→15 remains stopped. |
| Manual at 15; automatic step 15→16 | This record no longer blocks; physical claims and other restrictions still apply. |

Thus an otherwise free automatic Ready train first departs in the reopening step
15→16, observed Moving with traversal elapsed zero at 16. Manual admission can
succeed at 15 with a fresh command. For `[11,12)`, exactly step 11→12 is restricted.
`None` has the same start semantics and no automatic expiry.

Calls are serial transactions between complete steps; no operation interleaves
with resolution/commit. Registering a future constraint before versus after an
Accelerate at the same T does not change current permission. By contrast,
remove-active-before-Accelerate can permit it, while Accelerate-before-remove
returns Blocked. A registration made before a step resolves relative time from
that starting timestamp; the same call after the step resolves from the new time.
Replay requires the same initial state and ordered sequence of registrations
(including arguments/origin), removals, commands and steps, not just timestamps.
No replay storage or event-sourcing framework is introduced.

Time remains within the existing representable step-counter assumptions; this
milestone does not redesign `step()` overflow behavior. Scheduling arithmetic is
nevertheless checked, including near u64::MAX. Arbitrary writes to the legacy
public clock or topology are unsupported runtime behavior, not scheduling APIs.

## 8. Integration with the existing simulation pipeline

For World N committed at timestamp T, the automatic pipeline MUST remain:

1. Derive physical `ResourceView` from World N and one immutable `ConstraintView`
   from records active at T. Collect eligible candidates with the existing selector.
2. Resolve every proposal in ascending numeric train ID against those frozen views
   and the application-owned accepted-track/destination-slot sets.
3. Commit fixed decisions; advance other moving/dwelling trains with existing
   narrow transitions. Do not reevaluate admission during commit.
4. Advance time to T+1 and prune expired records. Return committed World N+1;
   activity at its timestamp governs subsequent calls and the next step.

No mid-step activation or expiration can revise a resolved or committed decision.
Reopening cannot enable same-step cascading resource reuse: a source slot occupied
in the starting World stays unavailable to other proposals throughout that step,
including when its train departs. Accepted claims still prevent batch contention.

Only departure admission gains operational input. Identity, eligibility, proposal
ordering, clocks, record lifecycle and orchestration remain in application.
`ConstraintView` receives active operational values, never application records.

Manual validation order remains unknown train → control mode → moving no-op →
candidate selection → combined admission → commit. For each command, derive views
from the latest committed serial state at T using the same activity predicate and
shared domain admission helper. Operational rejection returns existing
`CommandError::Blocked`, preserving the entire pre-command world. Moving manual
Accelerate remains `Ok(())` without mutation, even under active closure.

Blocked automatic trains complete dwell into Ready, remain in their committed
source slot at velocity zero, and retry each later step. Manual trains require a
fresh command after blocking clears; no intent is buffered. Early manual departure
remains permitted when combined admission succeeds. There is no new blocked state.

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

**Blocked means blocked.** Planned and injected closures may produce queues that
remain indefinitely. No alternate path search, rerouting, skip-station behavior,
bypassing a closed station, automatic service-plan modification, fairness/deadlock
resolution, or dynamic topology mutation belongs to this milestone. Passing through
a station while forbidding passenger boarding/alighting is a distinct future
service-policy concept; it MUST NOT be represented by reinterpreting
`StationUnavailable`.

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
    pub start_at: u64,
    pub end_at: Option<u64>,
    pub origin: ConstraintOrigin,
}
```

These values and the operational enum MUST be owned, cloneable and comparable.
Snapshots list **all registered, unexpired records, both scheduled and active**,
in ascending numeric constraint-ID order, regardless of internal registry iteration.
Consumers derive Scheduled versus Active from the snapshot's `elapsed_seconds` and
the interval using section 7; no duplicate status field is necessary. Expired and
explicitly removed records are absent. This exposes future maintenance plans and
queued interventions deterministically without requiring polling to trigger lifecycle
changes. It is a current registry view, not a history: consumers retaining a prior
record can infer expiry eligibility and detect disappearance before its end, but
cannot reconstruct unobserved removals; a single snapshot does not explain every
absent handle. Duplicate effects appear as distinct records so
consumers can observe independent lifecycles. Simulations with no registered constraints report an empty
constraint vector; a simulation without trains may still contain constraints. No cause strings, mutable registry, synthetic claims, event
history, or predicted departure time are exposed.

Train snapshot fields and train vector ordering remain unchanged. Ready continues
to mean completed dwell, not a unique explanation of why a train waits. Observed
constraints show planned intervals and current restrictions, not a definitive
per-train denial reason.
Repeated/omitted snapshots cannot affect expiry, IDs, admissions or arbitration.
Retained snapshots survive lifecycle changes at the same elapsed timestamp.

Adding a public snapshot field requires updating struct literals in core tests and
any workspace consumers. This is an explicit source compatibility change in the
current 0.1 crate. Existing snapshot methods and train command signatures stay
unchanged. Export operational values, IDs, origin and errors through a public `constraint`
module; the private domain model remains private. No serialization dependency is
needed. Bevy continues to display train snapshots and publish after mutations;
constraint rendering, incident UI and a new `metro-game` crate are deferred.

## 11. Acceptance and verification

All traces MUST preserve SPEC-003 physical invariants at every committed boundary:
capacity one per directed track and directional station slot, no occupied/reserved
slot overlap, exclusive destination reservation through arrival, opposite lanes
independent, atomic departure and committed direction, no claim theft, no same-step
reuse of released starting claims, and admitted traversal completion followed by
fresh dwell. Use ID-keyed comparisons where storage order differs. Unless stated,
use adjacent bidirectional two-second tracks and three-second initial dwell.

| Scenario | Required result |
| --- | --- |
| Empty restrictions | All existing core regressions retain behavior; snapshot literals gain empty constraint vectors only. |
| Absolute scheduling | At T=0 register `[1,5)`; canonical interval is exact, inactive at 0, active at 1–4, absent at 5. No train changes during registration. |
| Relative scheduling | At T=10 register delay 1, duration 4; canonical interval is `[11,15)`. `None` remains indefinite from the computed start. |
| API equivalence | Equal simulations, origin and ordered calls using absolute versus equivalent relative inputs yield identical canonical records, IDs, snapshots and operational traces. |
| Strict future validation | Separately reject start equal to T, start less than T (T>0 fixture), and zero delay with `InvalidStart`; include scenario setup at T=0. No implicit adjustment. |
| End validation | Reject end equal to or before start and zero duration with `InvalidEnd`; accept duration 1 and indefinite end. |
| Arithmetic limits | Checked relative start and end overflow each return `TimeOverflow`; no wrap/saturation. Test normalization/error precedence and canonical validation precedence. |
| Validation atomicity | Every temporal error preserves full state, snapshots and private next-ID counter; the next valid call gets the ID it would have received without failure. Repeat for invalid targets, ID exhaustion and unknown removal. |
| Target validation | Invalid source/destination/station and absent exact directed edge reject; existence of the reverse edge alone is insufficient. |
| Exact World boundaries | Assert every row of the T=10, `[11,15)` table, including scheduled observation at 10 and active observation at 11; no retroactive rejection of admission in 10→11. Include `[11,12)`. |
| Planned queue | Before stepping, schedule C→D `[1,5)` with Forward trains at A/B/C and D empty. At 3 all are Ready; at 4 and 5 all remain stopped. Congestion is permitted, not repaired. |
| Injected equivalent queue | Step the same initial state to 1, inject delay 1/duration 3 (`[2,5)`). No affected proposal occurs before 2 under default dwell, so train/claim traces equal the planned fixture through reopening. Metadata/timing observations intentionally differ. Also compare identical intervals with only origin changed. |
| Origin independence | Exercise both APIs with both origins. Equal intervals/physical state produce equal admission, contention, expiry and train traces, irrespective of origin. |
| Directed closure | While active, A→B blocks automatic and early manual admission; physically free B→A is unaffected. |
| Departure-only station | Blocking departures at B permits new A→B admission but blocks B→C and terminal reverse departures from B. |
| Station unavailable | New inbound/outbound admission in both directions is blocked; unrelated routes remain usable. No bypass or passenger-only reinterpretation. |
| Existing-track selection | Blocking a current-direction edge does not select a free reverse edge or return `NoOutgoingTrack`; blocked terminal reversal preserves original direction and slot. |
| Manual exact rejection | `Blocked` preserves trains, timings, directions, velocity, records, time and claims. Unknown/nonmanual/moving validation precedence remains unchanged. |
| Automatic retry | Closure `[1,3)` blocks the dwell-completion proposal in 2→3. At 3 the train is Ready and record absent; 3→4 retries and commits Moving at elapsed zero if physically free. No departure during expiry cleanup. |
| Manual no buffering | Command while closure is active rejects. After expiry or explicit removal, steps do not retry it; fresh Accelerate can depart at the same timestamp without advancing time. |
| Independent causes | Equal records have distinct IDs; removing one or expiring it leaves the other's restriction effective. Cover overlapping unequal intervals, scheduled cancellation, and one scheduled record becoming active as another expires. |
| Composition | Track, source and destination restrictions combine by OR for denial; removal never overrides other restrictions or physical claims. |
| Occupied resources/admitted movement | After a supported departure, register a future closure over its occupied track and reserved destination. Registration succeeds without claim changes. Activation during traversal does not stop, redirect, revoke or reject arrival; traversal finishes on its original schedule with fresh dwell. |
| Occupied station | Register/activate over an occupied station; train remains, dwell progresses, later departures are blocked. |
| Moving no-op | Manual Accelerate while Moving is an exact successful no-op before and after activation. |
| Physical safety on reopening | Existing occupants and reservations still block when the last restriction clears; a lower-ID proposal cannot displace them. |
| Indefinite interval | Inactive before start, active thereafter across many steps and snapshots until explicit removal. |
| Identity lifecycle | Distinct successful registrations get monotonically increasing IDs; cancellation/removal/expiry never permit reuse. Unknown, removed and expired handles return `UnknownConstraint` without mutation. |
| Frozen conga | Planned queue above: `[1,5)` expires at 5, no retroactive departure in 4→5; at 6 only C→D departs, at 7 B→C, at 8 A→B. Repeat reopening by removal. No same-step cascading release. |
| Deterministic contention | Hold terminal competitors with scheduled closure, then expire/remove it. Lowest numeric train ID wins automatic contention under reversed storage order; manual competitors resolve by serial call order. |
| Equal-time ordering | Future registration never blocks a current command. Removing an active cause before versus after a command affects that command only. Moving a relative call across a step shifts its interval by one second. |
| Snapshot lifecycle/order | Immediately expose scheduled records; derive active status at start; omit at end or cancellation. Numeric ID order survives registry reordering. Duplicate records remain distinct. |
| Snapshot ownership | Repeated reads equal; editing retained DTOs cannot mutate simulation; old snapshots retain scheduled/active records and train state after later activation/removal/expiry, even for mutations at equal timestamps. |
| Observation independence | Frequent versus sparse/no intermediate reads across registration, activation, cancellation, expiry, conga and contention produce equal results and subsequent IDs. |
| Replay equivalence | Same initial state and ordered constraint operations, commands and steps produce equal result sequences, IDs, committed snapshots and train/claim traces; observation frequency is irrelevant. |

Domain unit tests isolate predicates with physically available resources and prove
physical guards still reject with empty restrictions. Application unit tests may
inspect private records/allocator or reverse storage using established hooks.
Public integration traces use supported constructors, scheduling/removal APIs,
steps, commands and snapshots; do not expose mutable internals for tests.

Future implementation gates: `cargo test -p metro-core`, relevant consumer tests,
`cargo test --workspace`, and `cargo check --workspace` after public API/snapshot
migration. No graphical run is required. This specification-only revision does
not introduce tests or claim a fresh implementation verification run.

## 12. Implementation checkpoints

Each checkpoint is tests-first, independently compiling, separately reviewable and
committable. Tests introduced at that checkpoint pass before proceeding, alongside
existing core regressions. These are implementation instructions for subsequent
authorized work; this revision implements none of them.

To avoid publicly accepting restrictions that are operationally ignored, keep the
application scheduling methods crate-private during checkpoints 3–4. Checkpoint 5
publishes both APIs when behavior is wired. Do not ship intermediate checkpoints
as the completed feature. Snapshot visibility follows in checkpoint 6; this staged
construction is deliberate, not an alternative final observation contract.

### 1. Operational domain vocabulary and ConstraintView

- **Scope:** Three `OperationalConstraint` variants and private concrete membership
  view in domain. No application IDs or time.
- **Tests first:** Each directed-track/source-station/station-endpoint predicate;
  reverse-edge independence; overlapping values compose by OR; empty view permits.
- **Implementation:** Add owned comparable values and view construction/queries from
  operational values only; expose only the intended value type through the boundary.
- **Non-goals:** Admission signature changes, registry, scheduling, snapshots.
- **Review gate:** Domain has no Simulation/application imports; ResourceView remains
  untouched and physical; existing railway tests pass.

### 2. Shared combined departure admission with empty integration views

- **Scope:** Extend the existing domain admission helper to combine both views;
  supply empty restrictions from both current call sites.
- **Tests first:** Each operational rejection with physically free resources,
  occupied/reserved rejection with empty restrictions, combined guards and terminal
  candidate behavior; run existing manual/automatic regression tests.
- **Implementation:** Change helper signature and manual/automatic plumbing only.
  Selection, accepted-claim sets and commit ordering remain as they are.
- **Non-goals:** Live restrictions, identity/storage, scheduling APIs, snapshots.
- **Review gate:** Empty restrictions preserve existing behavior; both flows call
  the same helper; frozen physical views and numeric-ID arbitration remain intact.

### 3. Application records and absolute scheduling lifecycle

- **Scope:** Private canonical registry, ConstraintId/origin/errors, crate-private
  absolute registration/removal, activity selection, and boundary expiry cleanup.
- **Tests first:** Future/equal/past start, finite end ordering, target validation,
  allocator exhaustion/atomicity, duplicate IDs/causes, explicit origin preservation,
  scheduled cancellation, activity at start/end, pruning and expired-handle removal.
- **Implementation:** One validate-then-allocate path; application-only records and
  active-value derivation; end-of-step pruning after time advancement. No mutable
  activation flag. Keep registration unavailable to external callers at this stage.
- **Non-goals:** Relative conversion, live admission wiring, snapshot migration.
- **Review gate:** Registry lifecycle works independently; no failed call consumes
  an ID or changes world state; stepping without records remains identical. Domain
  receives neither records nor clock. Expiry foundations are complete here, not
  secretly postponed to checkpoint 7.

### 4. Relative scheduling through canonical conversion

- **Scope:** Crate-private relative entry point as a checked conversion adapter.
- **Tests first:** Equivalent absolute/relative records, zero delay/duration, both
  overflow sites, indefinite duration, duration measured from start, origin parity,
  error precedence and no-ID-consumption across conversion errors.
- **Implementation:** Capture T once, normalize, delegate to checkpoint 3's common
  path. No second allocator, registry, validation policy or lifecycle.
- **Non-goals:** New domain types, admission integration, public release, snapshots.
- **Review gate:** Equal canonical inputs share validation/storage results; no silent
  input correction; existing absolute lifecycle tests pass unchanged.

### 5. Active scheduled restrictions in both simulation flows

- **Scope:** Wire active-value views into step and command admission; publish the
  two scheduling APIs and removal when they enforce their complete behavior.
- **Tests first:** Activation/expiry admission table, planned/injected queues,
  origin invariance, manual exact rejection/no buffering, automatic retry, occupied
  track/station coexistence, admitted arrival, moving no-op and duplicate causes.
- **Implementation:** One frozen active view per step and latest committed view per
  serial manual command. Preserve shared domain rules and accepted-claim handling.
- **Non-goals:** Snapshot DTO changes, blocked-reason APIs, recovery/rerouting, UI.
- **Review gate:** Public APIs cannot accept an operationally ignored active record;
  future registration changes no present admission; SPEC-003 safety and frozen
  World-N ordering hold; cleanup never retries departures.

### 6. Snapshot observation and workspace API migration

- **Scope:** Owned constraint DTOs for all scheduled/active records, ordered by ID;
  migrate snapshot literals and consumer compilation.
- **Tests first:** Scheduled observation immediately after creation; start/end and
  cancellation visibility; retained/modified DTO independence; duplicate ordering;
  unchanged train order; pure repeated snapshots at equal timestamps.
- **Implementation:** Map private records to the specified public fields; export
  intended IDs/errors/origin/value types; update workspace literals and adapters
  only where required by the added field.
- **Non-goals:** Rendering constraints, lifecycle changes in observers, serialized
  history, consumer/game features, new train snapshot fields.
- **Review gate:** Snapshot reads mutate nothing; no time/status duplication;
  scheduled and active records are distinguishable by interval and snapshot time;
  core tests, relevant consumer tests and workspace check pass.

### 7. Full boundary, safety and deterministic trace verification

- **Scope:** Cross-feature integration verification using completed public APIs;
  close remaining acceptance-matrix gaps, not implement deferred lifecycle basics.
- **Tests first:** Full reopening conga, reversed-storage contention, duplicate
  handoff at equal end/start, serial command ordering, replay equality and
  frequent/sparse observation across every lifecycle boundary. Assert physical
  invariants throughout, not just final positions.
- **Implementation:** Add integration traces and only fixes exposed by them; verify
  every matrix row and complete workspace tests/check. No new feature subsystem.
- **Non-goals:** Recovery, event journals, optimizers, new timing semantics or
  consumer integration beyond compatibility.
- **Review gate:** All matrix cases covered, workspace verification passes, domain
  dependency direction intact, both flows obey identical operational rules, and
  origin/observation/storage order cannot change arbitration or safety.

## 13. Definition of done, deferred work and consistency review

Implementation is complete when the three generic constraint kinds have one
application-owned scheduled lifecycle, checked absolute/relative registration,
strict future-only starts, optional half-open end, independent IDs and explicit
metadata-only origin; manual/automatic admission uses frozen applicable views;
owned snapshots expose scheduled and active records deterministically; admitted
traversals and physical safety remain intact; and the acceptance matrix and
workspace compatibility checks pass.

Deferred: event journals/replay storage, persistence/serde, atomic bulk changes,
external source identity/deduplication, permission systems, blocking-reason DTOs,
emergency braking, stranded trains/evacuation, destruction/dynamic topology,
rerouting/alternate paths/skip-station/bypass/service-plan modification,
pass-through passenger service policies, configurable capacities, directional
station closures, speed/dwell effects, fairness/deadlock handling, presentation
and game integrations. Scheduled activation is part of this MVP, not deferred.

Architectural consistency review against commit `5381e1a`:

| Repository boundary | Revised contract |
| --- | --- |
| Private domain and pure departure helper | Operational values and predicates only; no Simulation, IDs, origins, clocks or records in ConstraintView. |
| Generic ResourceView owners | Physical claims only; closures neither occupy nor mutate resources/topology. |
| Shared manual/automatic admission | Same combined predicate; retain serial manual order versus numeric-ID batch arbitration. |
| Simulation-owned orchestration | Identity, checked conversion, storage, interval filtering, pruning, proposal ordering and time remain application concerns. |
| Existing commit-before-clock step | World-N restrictions stay frozen through commit; T+1 activation/expiry affects subsequent decisions only. |
| Existing admitted movement | Arrival bypasses new admission as before; closures never revoke its reservation. |
| Pure owned snapshots | Add ordered scheduled/active records without read-triggered cleanup or a second active-state clock. |
| Existing candidate selection | Closure never removes an edge or triggers fallback, rerouting or station bypass. |
| Incremental integration | Domain vocabulary and shared admission separate; private lifecycle precedes public behavior, then observation and complete traces. |

No unresolved architectural decision blocks checkpoint implementation. Known limits
are explicit choices: initial World 0 cannot be restricted by new registration;
explicit removal affects later serial calls immediately; snapshots are not history;
legacy direct clock/topology mutation is unsupported; overall step-counter overflow
is not redesigned. A requirement for initially active closures would require a
separate future contract discussion, not an exception to this specification's
strict start rule. Production implementation remains outside this revision.
