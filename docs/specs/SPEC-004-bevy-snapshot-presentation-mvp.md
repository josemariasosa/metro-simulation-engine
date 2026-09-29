# SPEC-004: Bevy Snapshot Presentation MVP

## 1. Status

**Proposed — specification and implementation planning only.**

This milestone connects the implemented [SPEC-001 observation
boundary](SPEC-001-core-observation-boundary.md) and [SPEC-002 manual
control](SPEC-002-manual-train-control-boundary.md) to `metro-bevy`. It does not
authorize production changes as part of writing this document.

MUST and MUST NOT identify requirements. The scope and Definition of Done below
govern this milestone. Earlier specifications retain historical descriptions of
unimplemented APIs; section 4 records the actual repository inspected for this spec.
SPEC-004 selects a manual A–B–C–D demo in place of SPEC-001's illustrative automatic
A–B integration fixture. Core contracts and existing regression tests remain intact.

## 2. Motivation

The current Bevy prototype animates a train without consulting `metro-core`.
The smallest useful next milestone is a playable line where a player presses
Space to depart, sees traversal progress, and stops at the next station. Core
must decide every railway transition. Visual polish is secondary to proving this
complete input → command → simulation → snapshot → screen path.

`metro-core` is authoritative. Bevy MUST expose the currently committed snapshot
as faithfully and transparently as possible, favoring observability over visual
polish. Presentation MUST NOT hide, smooth over, repair, delay, reinterpret, or
cosmetically compensate for observed behavior. Visible jumps from the discrete
one-second model are intentional; smoothing/interpolation is deferred.

## 3. Architecture

```text
Space press → TrainCommand::Accelerate { train_id }
                         |
                         v
                 Simulation::apply_command()
                         |
wall-clock accumulator → Simulation::step() (one simulation-second per call)
                         |
                         v
                 Simulation::snapshot()
                         |
                         v
              latest SimulationSnapshot
                         |
              station layout + TrainId association
                         |
                         v
                 Bevy transforms / text
```

Commands commit synchronously without advancing time; a new snapshot can therefore
be required even when no step occurs. One concrete runtime driver boundary MUST
own mutable access to `CoreSimulation`,
execute due `step()` calls, apply commands, and publish the snapshot in section 9
order. The arrows above show data flow, not tick/command execution order.
Presentation systems consume the cached snapshot and presentation metadata, never
the simulation resource.

## 4. Current State

### Implemented core APIs

The crate exposes modules, not root-level type re-exports. Use these real paths:

| API | Existing behavior |
| --- | --- |
| `metro_core::simulation::Simulation::new(network, trains, dwell_policy)` | Owns a `Network`, `Vec<Train>`, and `DwellPolicy`. |
| `Simulation::step(&mut self)` | Advances exactly one integer second; accepts no delta argument. |
| `Simulation::snapshot(&self)` | Returns an owned `SimulationSnapshot` without mutation. |
| `Simulation::apply_command(&mut self, command)` | Returns `Result<(), CommandError>`; commits immediately without stepping. |
| `metro_core::train::Train::new_manual(id, capacity, station, direction)` | Starts Manual, dwelling for three seconds, velocity 0. |
| `Train::new(...)` | Starts Automatic; not used for this demo. |
| `metro_core::network::Network::{new, add_station, connect_bidirectional}` | Supports construction of the ordered linear network. |
| `metro_core::dwell::DwellPolicy::new()` | Current arrival dwell is three seconds. |
| `metro_core::command::TrainCommand::Accelerate { train_id }` | Sole gameplay command. |

[`snapshot.rs`](../../metro-core/src/snapshot.rs) already exposes:

```rust
// Existing fields, not proposed additions:
SimulationSnapshot { elapsed_seconds: u64, trains: Vec<TrainSnapshot> }
TrainSnapshot {
    id: TrainId,
    direction: Direction,
    state: TrainSnapshotState,
    velocity: u8,
}
TrainSnapshotState::Dwelling { station: StationId, remaining_seconds: u64 }
TrainSnapshotState::Ready { station: StationId }
TrainSnapshotState::Moving {
    from: StationId,
    to: StationId,
    elapsed_seconds: u64,
    travel_seconds: u64,
}
```

`TrainId` and `Direction` are in `metro_core::train`; `StationId` is in
`metro_core::station`. IDs support equality and hashing. Snapshot train vector
order is preserved but MUST NOT be treated as entity identity.

There is **no missing traversal observation**: endpoints and both durations are
already available. No snapshot or other core API adjustment is required. Snapshots
omit control mode, station labels, and topology catalogs; this demo knows its
configured player train and retains layout/labels at construction time.

[`simulation.rs`](../../metro-core/src/simulation.rs) implements Accelerate from
both Dwelling and Ready, including early departure that abandons unfinished dwell.
Accelerate on a moving Manual train is an idempotent `Ok(())`. Core selects a track
in the current direction or tries the reverse direction, committing reversal only
with departure. Arrival sets velocity to 0 and starts fresh dwell. Without input,
a Manual train completes dwell into Ready and stays there. Velocity is binary
0/1, not a screen speed. Routing currently follows adjacent station indices.

`Simulation::trains()`, public `network`, and public `elapsed_seconds` still exist.
Their existence is not permission for the runtime renderer to use them. No
visibility refactor or general core hardening is needed for this milestone.

### Prototype and tests

[`metro-bevy/src/main.rs`](../../metro-bevy/src/main.rs) currently contains a local
`Train { direction: f32, dwell_remaining: f32 }`, `move_train`, `SPEED`,
`DWELL_TIME`, LEFT/RIGHT arrival checks, transform-based reversal, and movement
using `Time::delta_secs()`. `bounce_train` animates a `TrainBody` child. Startup
loads `trains/train_base.png` and `trains/train_body.png` and spawns `Camera2d`.
There are no Bevy adapter tests. Its manifest already depends on `metro-core`
and declares Bevy `0.19.1`; no dependency upgrade is part of this work.

Core unit tests and [`manual_control.rs`](../../metro-core/tests/manual_control.rs),
[`snapshot_observation.rs`](../../metro-core/tests/snapshot_observation.rs), and
[`tiny_metro_scenario.rs`](../../metro-core/tests/tiny_metro_scenario.rs) cover
command semantics, ordering, observation purity, retained snapshots, traversal,
and the existing automatic five-station scenario. Inspection baseline:
`cargo test -p metro-core` passes all 65 tests.

## 5. Responsibility Boundary

| Core owns | Bevy owns |
| --- | --- |
| Train state and Automatic/Manual mode | Entity association by `TrainId` |
| Dwelling, Ready, Moving and dwell behavior | Labels and visual state feedback |
| Direction, route progression and track selection | Layout, camera and visual orientation |
| Traversal elapsed time and station arrival | Projection of observed temporal progress |
| Velocity and Accelerate validity/semantics | Space binding and command diagnostics |
| All railway transitions | Whole-step pacing, snapshot projection and primitive visuals |

Runtime presentation MUST NOT depend on `TrainState`, `Train` internals, `Network`
lookups, or independently ticking domain timers. Trusted startup may use normal
constructors. Coordinates and visual segment lengths MUST NOT enter core or affect
travel duration. Snapshot values are observations, never editable railway controls.

Presentation MUST NOT choose next stations, derive routing from `StationLayout`,
use visual adjacency as operational topology, turn `Direction` into routing logic,
or mutate or repair domain state. `StationLayout` is presentation metadata only.

## 6. MVP Scope

Use **exactly one Manual train** and no Automatic train. Existing core tests already
protect Automatic behavior; a second visible train adds no necessary interaction.

The concrete fixture is:

- Add stations in order A, B, C, D, retaining each returned `StationId`.
- Connect A↔B, B↔C, C↔D using `connect_bidirectional(..., 3)`.
- Construct `Train::new_manual(TrainId(0), 100, a, Direction::Forward)`.
- Construct `Simulation::new(network, vec![train], DwellPolicy::new())`.
- Retain `TrainId(0)` as the player selection; do not infer it from vector index.
- Assign presentation coordinates A=(-300, 0), B=(-100, 0), C=(100, 0), D=(300, 0).

Three-second segments expose visible one-third and two-thirds positions while
keeping the round trip short. The standard three-second dwell is observed, not
reimplemented or configured by Bevy. Capacity is an existing constructor argument
and has no gameplay role in this slice.

## 7. In Scope

- One owned simulation, initial snapshot, fixed-step driver, and Space command.
- Four labeled station markers and a simple line connecting them as decoration.
- One clearly visible train root positioned from snapshots, using an
  orange rectangle, square, diamond, or similarly obvious primitive generated
  directly in Bevy, independent of assets.
- Small on-screen text: `Space: Accelerate`, plus snapshot-derived Dwelling with
  remaining seconds, Ready, or Moving with endpoint labels and elapsed/total time.
  For example: `Dwelling at A — 2s`, `Ready at A`, or `Moving A → B — 1/3`.
  Velocity and direction text are not required. This is debugging/observation
  output, not a polished HUD. The hint remains available during Dwelling; dwell
  is not a command lockout.
- Minimal error logging and pure adapter tests.

The required acceptance-path train visual MUST be the orange asset-independent
primitive. It MUST NOT require emoji rendering, custom fonts, sprite files, or
existing train artwork. That artwork MAY remain in the repository and later be
layered on cosmetically, but SPEC-004 MUST NOT depend on it or replace the required
primitive with it. Optional child-only cosmetics, including existing body bounce,
MUST NOT obscure or compensate for snapshot behavior or displace the logical root;
its position MUST be driven exclusively by snapshot projection. If an optional
asymmetric visual is used, facing follows observed `direction` (Forward right,
Backward left for this layout), including unchanged facing during endpoint dwell.

A–B–C–D MUST use one visible horizontal presentation line. Reverse journeys use
the exact observed `from` and `to` over that same line. Presentation MUST NOT add a second
parallel line or assign physical lanes from direction (such as Forward = lower,
Backward = upper). Core exposes endpoints and direction, but this milestone does
not define a physical two-track model. Distinct physical tracks may be visualized
later if future domain work models them.

## 8. Out of Scope

No second train, train selection UI, new commands, brake/throttle controls,
runtime control-mode changes, realistic units, distance-based speeds, passengers,
map editor, generic adapter/plugin framework, command bus, replay, persistence,
networking, asset pipeline work, or core ECS conversion. No smoothing or predictive
motion in this MVP. No physical lane modeling, collision avoidance, generic
adapter infrastructure, new core APIs, or safety/occupancy behavior from SPEC-003.

## 9. Timing Model

```text
simulation seconds: 0 -------- 1 -------- 2 -------- 3
render frames:      ||||||||||||||||||||||||||||||||||
```

Use a concrete accumulator of `std::time::Duration`, initially zero, fed once per
render update by Bevy's real elapsed frame duration. Use the real-time clock rather
than a default virtual delta that may clamp long frames; verify the exact accessor
against the installed Bevy version during implementation. Do not manually measure
time in each rendering system or run a second ticking schedule.

For this four-station, one-train demo, consume all accumulated whole seconds:

```text
pending += real_frame_delta
while pending >= one_second:
    simulation.step()
    pending -= one_second
```

Retain the fractional remainder. Do not drop/clamp accumulated time, round a short
frame up to a step, multiply domain velocity by delta, or call one step per frame.
No catch-up cap is introduced for this small fixture. Long suspension may cause
many steps and skip intermediate visual observations; that is a known limitation.
Bevy MUST NOT fix this catch-up behavior for presentation purposes by delaying
publication, replaying skipped observations, slowing core, or dropping due steps.
If a cap is considered later, pending whole seconds must remain queued.

The concrete driver MUST execute steps 1–3 in order within one runtime mutation
boundary; downstream presentation systems MUST run after publication:

1. Accumulate delta and execute every due core step.
2. Process this frame's Space press through `apply_command()` once.
3. Replace the snapshot cache with `simulation.snapshot()`.
4. Project train transforms and update status text from that cache.

Publish an initial snapshot at startup, before the first presentation update.
The driver MUST refresh the snapshot every Update, including command-only updates
and updates without mutation. This avoids stale command results at unchanged
`elapsed_seconds`; timestamp equality MUST NOT be used to skip publication. Multiple catch-up ticks can publish just
their final state. Tests may sample after each step for comparisons.

Tick-before-input means a press in an update that also arrives at a station can
immediately depart again under SPEC-002. All accumulated steps precede that press;
it is not retroactively applied to the catch-up interval. Input does not reset the
accumulator. Departure has elapsed zero; the next global tick consumes its first
second, possibly less than one wall-clock second after the press.

Given equal accumulated duration and no commands, frame partitions produce the
same number of steps and remainder. With input, determinism is defined by equal
ordered command/step sequences, not identical wall-clock keypress times across
different frame rates. No replay or timestamped input system is required.

Render the latest committed position only. Extra frames without a new committed
state leave the root unchanged. Moving MUST render exactly the position derived
from committed `elapsed_seconds / travel_seconds`: 0/3, 1/3, and 2/3 are discrete
positions, subject to the accepted catch-up limitation. Presentation MUST NOT use
per-frame
interpolation, prediction, velocity integration, tweening, or wall-clock motion
between snapshots. Spatial `lerp` within a snapshot is not temporal interpolation
between snapshots. Future smoothing/interpolation remains explicitly deferred.

## 10. Snapshot-to-Presentation Mapping

A small pure adapter function takes `&TrainSnapshotState` and the station layout,
and returns a position or a local presentation error. It does not take Simulation,
Network, frame delta, velocity, or a mutable snapshot.

| Observed state | Logical root position |
| --- | --- |
| `Dwelling { station, .. }` | Exactly `layout[station]` |
| `Ready { station }` | Exactly `layout[station]` |
| `Moving { from, to, elapsed_seconds, travel_seconds }` | `lerp(layout[from], layout[to], progress)` |

For a valid Moving observation:

```text
progress = (elapsed_seconds as f64) / (travel_seconds as f64)
position = from_position + (to_position - from_position) * progress
```

Convert before division; convert the ratio to the presentation scalar type if
needed by Bevy. The configured small integers avoid significant conversion loss.
Core durations stay `u64`. Require `travel_seconds > 0` and
`elapsed_seconds < travel_seconds` before projection. Zero duration, elapsed at or
beyond total duration, or unknown layout IDs yield an error, not NaN, a fabricated
duration, a guessed station, or a new core transition. A tiny private error enum
or equivalent is sufficient; no new core error API is needed.

Progress zero is the departure coordinate. At 1/3 and 2/3 the root is between
stations. Core commits arrival as Dwelling, placing the root exactly at the
destination; normal Moving snapshots never report progress one. Do not infer
arrival from a floating-point threshold.

Use `from` and `to` exactly as supplied, including reverse journeys. Do not sort
endpoints, increment station IDs, inspect `direction` to choose a target, or reverse
the ratio. A B→A snapshot at 1/3 projects from B toward A regardless of facing.
Varying screen distance changes only the visual spacing of the three increments.
Minimal state text is sufficient; velocity and direction need not be displayed.
Velocity MUST NOT be integrated into a position.

## 11. Input / Command Flow

Use the press edge (`ButtonInput<KeyCode>` / `just_pressed(KeyCode::Space)` in the
Bevy input system), not continuous held-key polling. One press submits:

```rust
simulation.apply_command(TrainCommand::Accelerate {
    train_id: configured_player_train_id,
})
```

The driver handles the result and publishes the resulting snapshot that frame.
No Bevy pre-validation of dwell, Ready, velocity, direction, or route is allowed.
There is no queued intent, retry, deferred departure, or automatic submission when
dwell completes. Holding Space does not cause later station departures; another
press is needed. Repeated presses while Moving are forwarded, and core accepts
them idempotently. Early presses during Dwelling are also forwarded unchanged and
may cause early departure. Bevy MUST NOT suppress a command because it believes departure should
be disallowed. A tick and press in the same Update retain tick-before-command
semantics, including arrival followed immediately by departure.

## 12. State Ownership

These are proposed concrete Bevy wrappers/components, not existing core APIs:

| Resource/component | Contents and access |
| --- | --- |
| `CoreSimulation` resource | One `metro_core::simulation::Simulation`; only startup and the single concrete runtime driver boundary access it. |
| `LatestSnapshot` resource | One owned `SimulationSnapshot`; driver replaces, presentation reads. |
| `SimulationClock` resource | Pending wall-clock `Duration`; used only to schedule whole steps. |
| `StationLayout` resource | Four entries holding retained `StationId`, label, and `Vec2` position; presentation metadata only; decorative adjacent pairs may be retained here, never operational topology. |
| `PlayerTrain` resource | Configured `TrainId`, used for command addressing. |
| `TrainEntity` component | `train_id: TrainId` on the visual root. |
| Existing `TrainBody` component | Optional cosmetic child marker only. |

The `TrainEntity` association and an ID-based query/lookup are sufficient for one
train; no separate entity registry is required. Startup spawns the single root
using the configured ID. Rendering finds its snapshot by matching ID, never by
`trains[0]`. Dynamic spawning/despawning is outside scope. Station names and
decorative connectivity come from startup metadata, not runtime Network reads.

Keep implementation inside `metro-bevy`; small private `scenario`, `presentation`,
or `timing` modules may be extracted for clarity/testing. Do not add a crate or
generic traits for this adapter. Bevy wrappers keep framework types out of core.
Downstream presentation systems MUST consume `LatestSnapshot`; they MUST NOT inspect or mutate `CoreSimulation`.
This boundary does not require a command bus, plugin framework, or trait abstraction.

### Replacement of the prototype

When the root starts using snapshots, unregister and delete `move_train` and its
local Train component together. Remove local direction/dwell fields, SPEED,
DWELL_TIME, per-frame displacement, edge arrival checks, operational coordinate
clamping, and locally decided reversal. Layout coordinates replace LEFT/RIGHT
only as drawing metadata. Never run old and new root-position writers together.
Preserve the camera. Existing artwork may remain in the repository; any retained
optional child cosmetics MUST satisfy section 7 and leave the required orange
primitive and committed position clearly observable.

## 13. Error Handling

Handle every command `Result` without `unwrap()` or a crash. Log a concise diagnostic
including the addressed ID and existing `CommandError::{UnknownTrain, NotManual,
NoOutgoingTrack}`. Logging is sufficient; a diagnostic UI is optional. Do not
invent rejection variants or change core state to recover. Core rejects atomically;
the ordinary snapshot refresh still runs.

For invalid projection data or a missing snapshot for the visual ID, log the
problem once per changed diagnostic, hide the affected train until valid data
returns, and keep the application running. Use only the smallest local/private
state needed to log once per changed diagnostic; do not add a general diagnostics
framework. Do not present a guessed position or repair the domain. Valid fixture setup should never reach this path. Core panics
from unsupported fabricated state are not converted into a new recovery framework.

## 14. Testing Strategy

Use normal Rust tests for pure helpers within `metro-bevy`; no GPU/window is needed.
Tests should cover behavior at the boundary rather than duplicate core transitions:

- Dwelling and Ready map to exact station positions, regardless of dwell remaining.
- Moving at elapsed 0, 1, and 2 of 3 maps to 0, 1/3, and 2/3 with float tolerance.
- Reversed endpoints map correctly with unequal coordinates; facing is irrelevant
  to position. Different layout lengths do not alter snapshot durations.
- Zero duration, elapsed ≥ duration, and missing station coordinates report errors.
- ID matching works when the player is not first in a synthetic snapshot vector.
- An accumulator consumes no step at 400 ms, one after another 600 ms, and two
  after another 2500 ms with 500 ms retained. Equivalent frame partitions give
  equal counts/remainders; zero-delta frames do not advance time.
- One small driver test without graphical plugins proves tick-before-command
  ordering and same-frame cache publication after a command with no due tick.
  If a harness is useful, extract only the concrete driver operation, not a generic
  command architecture. Compare snapshots at equal step boundaries for different
  frame partitions, with commands placed at the same logical boundaries.

Core tests remain the authority for early acceleration, idempotence, reversal,
arrival, Automatic compatibility, and observation purity. Do not copy their full
matrix into Bevy. Rendering review should confirm systems cannot mutate core.

Implementation verification commands:

```text
cargo test -p metro-core
cargo test -p metro-bevy
cargo build -p metro-bevy
cargo run -p metro-bevy
```

The last command is a human visual check in a graphical environment, not a heavy
automated graphical test. Build/test commands are gates for implementation; this
document alone does not claim the future demo has been built or visually verified.

### Manual end-to-end scenario

1. Launch the demo with A/B/C/D visible and the Manual train at A, Dwelling (3).
2. Wait three core ticks; observe remaining 2, 1, then Ready at exactly A.
3. Wait at least five more ticks. Confirm Ready remains at A without departure.
4. Press Space. Observe Moving A→B at 0/3, still at A on that update.
5. Observe 1/3 then 2/3 positions on subsequent ticks; the third tick places it
   exactly at B with fresh Dwelling (3), stopped.
6. Wait for Ready at B, press again, and repeat through C to D.
7. Wait at D through dwell and Ready. It must not reverse or depart by itself.
8. Press Space at D. Observe core-selected D→C and leftward progress over the same
   presentation line. Optional asymmetric facing follows `Direction::Backward`;
   no direction text is required. Continue to A and verify the next press returns
   toward B.
9. In a separate pass, press during Dwelling and repeatedly during Moving. Confirm
   early departure works and moving presses do not reset progress or add speed.
10. Hold Space across arrival; confirm it does not trigger another departure.

The visual moves in one-second increments. This is the selected usable baseline,
not a failed smoothing requirement. Labels make departure-at-zero and waiting clear.

## 15. Definition of Done

1. `metro-bevy` owns one real `Simulation` built with the specified linear fixture.
2. Authoritative train state comes only from core; no Bevy direction, dwell,
   arrival, route selection, or velocity integration remains.
3. A/B/C/D markers and labels and one Manual train represented by an orange
   asset-independent Bevy primitive are visible; minimal snapshot state text and
   the Space hint are readable. No emoji, custom font, or train artwork is required.
4. Dwelling/Ready project exactly to their station; Moving projects only committed
   elapsed/total positions between its exact observed endpoints. Both traversal
   directions use the same single horizontal presentation line, without lane
   assignment.
5. Space submits `TrainCommand::Accelerate` for the configured ID, handles its
   result, and refreshes the snapshot in the same update.
6. Ready can wait indefinitely without input; another press begins the next
   core-owned traversal. Early and repeated acceleration semantics are preserved.
7. Arrival visibly stops the train; endpoint reversal comes from core on departure.
8. One-second stepping is independent of frame rate, retains accumulated time,
   and has explicit tick/input/publication/presentation ordering.
9. Rendering and observation do not mutate core; extra render frames do not move
   the logical root without a changed snapshot. Faithful visibility takes priority
   over smoothness; no smoothing or cosmetic compensation conceals discrete behavior.
10. Focused adapter tests pass; all existing core tests remain green.
11. `metro-bevy` builds and runs; the manual round-trip scenario is verified.
12. No SPEC-003 occupancy, collision, blocking, signaling, reservation, or safety
    behavior is introduced in either crate. No presentation-generated railway
    behavior, physical lane model, additional train, train selection, replay,
    generic adapter infrastructure, or new core API is introduced.

## 16. Known Limitations

- Movement is stepped at one-second intervals; no sub-tick motion or prediction.
- Long stalls/suspension cause catch-up work and may skip visible intermediate
  states. The tiny fixture does not justify a catch-up budgeting framework yet.
- Input is sampled per frame, ordered after due steps. Extremely brief keypresses
  between input samples and cross-machine wall-clock replay are not addressed.
- Fixed startup layout, adjacent-index routing, and one configured player train.
- No physical distance model: longer drawn segments still take three core steps.
- No occupancy/safety guarantee. If later setup adds trains, their visuals may
  overlap; presentation must not prevent or resolve that overlap operationally.
- Existing core assumptions remain: valid station/track references, positive
  durations, unique train IDs, and representable counters. No topology mutation
  during play or exhaustive invalid-setup hardening is included.

## 17. SPEC-003 Boundary

[SPEC-003](SPEC-003-railway-occupancy-blocking-physical-safety.md) remains the
separate, unresolved milestone for occupancy, collisions, blocking, signaling,
reservations, and physical/safety constraints. SPEC-004 neither requires it first
nor selects its model. Its scaffold still mentions `RequestDeparture`; that is
historical draft wording, not an API to implement or call. The implemented command
is `TrainCommand::Accelerate`.

Do not add Bevy colliders, occupied-station checks, dispatch permission, minimum
spacing, or visual-overlap avoidance that delays commands or movement. Future
safety rules must live in core and apply to both Manual and Automatic trains.

## 18. Educational Implementation Plan

Implement only one checkpoint at a time. Each starts with its smallest test or
observable behavior, makes the stated architectural change, compiles, runs focused
checks plus core regressions, and **stops for review**. Do not start later checkpoints
early, add stubs/`todo!()` branches, or merge a partial migration as the finished MVP.
Suggested files are private Bevy modules or `src/main.rs`; no core edits are expected.

### Checkpoint 1 — Own the scenario and initial snapshot

- **Start with:** A headless scenario test observes one configured ID dwelling at A
  and retains four returned station IDs/labels.
- **Change:** Add fixture construction, `CoreSimulation`, `LatestSnapshot`, and
  `PlayerTrain` resources. Capture the initial snapshot; do not tick or wire input.
- **Files:** `metro-bevy/src/main.rs`, optionally `src/scenario.rs`.
- **Observable result:** Existing prototype still runs; the dormant core resource
  is not yet claimed to drive it. No new gameplay behavior.
- **Stop/review:** Build and scenario test pass; confirm constructor/API usage.
- **Commit:** `feat(bevy): own a manual core scenario`

### Checkpoint 2 — Draw the station layout

- **Start with:** Observe four labeled markers fitting in the camera view.
- **Change:** Retain presentation coordinates with station IDs and draw the simple
  single horizontal line/markers/labels. No runtime Network reads or physical lanes.
- **Files:** `src/main.rs`, optionally `src/presentation.rs`.
- **Observable result:** A/B/C/D remain fixed; legacy train remains temporary.
- **Stop/review:** Build passes and layout is readable; no simulation pacing yet.
- **Commit:** `feat(bevy): draw the linear station layout`

### Checkpoint 3 — Define complete snapshot projection

- **Start with:** Pure tests for Dwelling/Ready coordinates, then Moving 0/3, 1/3,
  2/3, reverse endpoints, and invalid projection input.
- **Change:** Implement the complete state-to-position helper and ID matching,
  without registering a new root writer yet. Use synthetic DTOs for Moving.
- **Files:** `src/presentation.rs` or private helpers/tests in `src/main.rs`.
- **Observable result:** All existing snapshot variants project without placeholder
  branches. No temporary mutation of live core state to demonstrate movement.
- **Stop/review:** Projection tests and build pass; inspect boundary dependencies.
- **Commit:** `feat(bevy): project snapshot states to world positions`

### Checkpoint 4 — Replace the prototype simulation with observation

- **Start with:** Observe the initial core train fixed at A with Dwelling feedback;
  additional render frames must not move its root.
- **Change:** Use `TrainEntity` and the projection helper for the real visual root,
  add snapshot-derived status, and delete old Train/move_train behavior in this
  same checkpoint. Use the required orange asset-independent Bevy primitive.
- **Files:** `src/main.rs`, `src/presentation.rs` if extracted.
- **Observable result:** The train is stationary because core is not ticking yet;
  optional child cosmetics obey section 7. Only one system owns root translation.
- **Stop/review:** Build/tests pass; confirm all legacy railway logic is gone.
- **Commit:** `refactor(bevy): replace toy movement with snapshot presentation`

### Checkpoint 5 — Pace fixed core steps

- **Start with:** Accumulator partition/remainder tests, then observe Dwelling
  reach Ready after three core ticks and remain at A.
- **Change:** Add `SimulationClock`, whole-step driver, cache refresh, and ordered
  presentation after publication. Feed real delta without discarding long frames.
- **Files:** `src/main.rs`, optionally `src/timing.rs`.
- **Observable result:** Core time advances at fixed one-second steps; rendering
  alone does not progress the train. No keyboard action yet.
- **Stop/review:** Focused timing tests, core tests, and build pass; review catch-up.
- **Commit:** `feat(bevy): drive fixed one-second core steps`

### Checkpoint 6 — Send Accelerate from Space

- **Start with:** Driver test for tick-before-command and immediate publication
  without a tick; then observe a Space-triggered A→B traversal.
- **Change:** Add edge-triggered input after ticking, `apply_command()` result
  handling, and the input hint within the existing concrete runtime driver
  boundary. Complete the specified Update ordering.
- **Files:** `src/main.rs` and the concrete driver helper if extracted.
- **Observable result:** Space works in Dwelling and Ready; moving presses reach
  core unchanged. Arrival and reversal need no adapter additions.
- **Stop/review:** Focused tests/build pass; inspect for pre-validation or retries.
- **Commit:** `feat(bevy): send manual accelerate commands from Space`

### Checkpoint 7 — Verify the complete vertical slice

- **Start with:** Run section 14's complete manual scenario without train images.
- **Change:** Verification only; fix only demonstrated adapter defects. Do not add another train,
  smoothing, physics, or new architecture. Record verification in the PR.
- **Files:** Only relevant Bevy fixes; optional short run instructions in README.
- **Observable result:** A→B→C→D→C→B→A with waiting, fresh arrival dwell, and
  core-controlled endpoint reversal; all regressions remain green.
- **Stop/review:** All Definition of Done items checked, core/adapter tests and
  Bevy build pass, and graphical run verified. Review final source-of-truth boundary.
- **Commit:** `test(bevy): verify the snapshot-driven manual vertical slice`
  (Use a docs-only verification commit if no code/test correction is needed.)

Projection is completed before runtime migration so no reachable snapshot needs an
unfinished rendering branch. Legacy behavior is deleted at that migration, rather
than left active beside the new driver until the final checkpoint.

## 19. Commit / PR Strategy

Deliver this specification as a documentation-only change. A later implementation
PR contains the seven reviewed checkpoints, stays draft until the vertical slice
is complete, and keeps each checkpoint compiling. Do not collapse the educational
sequence into a single rewrite or proceed beyond a checkpoint's review stop.

The implementation PR should explain that train behavior now comes from snapshots,
give the run command and Space interaction, state the intentional one-second visual
increments, and record tests/build/manual round-trip results. No core feature or
SPEC-003 change belongs in that PR. A demonstrated core defect requires separate
scope review rather than a Bevy workaround.

## 20. Decisions Before Implementation

Selected: one Manual train; A↔B↔C↔D; three-second tracks; existing core dwell;
Space press-edge Accelerate; real-time Duration accumulator consuming all whole
seconds; tick-before-input; cache refresh every update including command-only
updates; committed positions only; fixed presentation layout; ID-based association;
orange asset-independent Bevy primitive with minimal state text; one horizontal
presentation line for both directions; one concrete runtime mutation boundary;
log command errors; pure adapter tests plus one manual round trip; no core API extension and no SPEC-003 behavior.

No unresolved product or domain decision blocks implementation. Confirm exact
Bevy time/text/primitive API spellings against the installed dependency while
implementing each checkpoint; this is routine implementation work, not permission
to alter timing or ownership. Optional child cosmetics do not replace the required
primitive, conceal observed behavior, or expand acceptance. Smoothing,
Automatic-train display, and safety models remain deferred.
Spec review does not begin implementation.
