# SPEC-003: Railway Occupancy / Blocking / Physical Safety

## 1. Status

**Draft scaffold — scope and physical model are undecided.**

This document records the next design boundary after
[SPEC-002](SPEC-002-manual-train-control-boundary.md). It is not an implementation
plan or authorization to change code. SPEC-002 is a proposed contract, not an
assumption that manual control has already been implemented.

MUST denotes an architectural requirement. Candidate approaches and TBD items
are deliberately unresolved and do not define acceptance criteria yet.

## 2. Motivation

Departure timing alone cannot determine whether a train can move safely.

```text
Station A -------- Station B -------- Station C
Automatic train   Manual train
                  stopped here
```

When the automatic train completes dwell at A, core must account for the train
at B. The same constraints must apply if the train at A is manually controlled.

This specification will define which physical resources trains occupy, when
movement can acquire them, and what happens when they are unavailable.

## 3. Architecture and Responsibility

```text
Automatic operational rules ────────────┐
                                       ├→ core physical checks → committed movement
Manual intent and control rules ────────┘
```

- **Domain:** physical resource identity, occupancy, and movement constraints.
- **Application:** `Simulation` coordinates checks and commits state transitions.
- **Boundaries:** commands express intent; snapshots describe committed facts.
- **Adapters:** display occupancy/blocking feedback without authorizing movement.

Physical constraints MUST apply to both control modes. Manual operational
freedom, including early departure, MUST NOT bypass them.
Domain types MUST NOT depend on command or snapshot DTOs. Prefer concrete types
within the existing crate; introduce abstractions only for a demonstrated need.

## 4. Starting Point

The current engine models directed station-to-station tracks and integer travel
durations. It has no physical distance, braking, occupancy, or mid-track stop model.
The opposite directed tracks do not establish whether they share physical space.

SPEC-002 proposes synchronous `RequestDeparture { train_id }` handling, manual
early departure, and waiting after normal dwell. This spec should add constraints
before movement commits without adding destinations or tracks to that command.

Reinspect the repository when this draft is developed; implementation may have
changed since the preceding specifications were written.

## 5. Candidate MVP Boundary

**Working candidate, not a selected model:** prevent unsafe movement at departure
boundaries, keeping trains at stations when necessary. This fits the current
timed-traversal engine without introducing braking or physical position.

Before adopting this approach, define how a destination remains available until
arrival. Checking that it is empty only at departure is insufficient if another
train can claim it before the first train arrives.

An alternative allows segment entry and later stopping before an occupied
resource. That requires a richer movement model and may belong after the MVP.

### Candidate IN SCOPE

- A minimal, explicit physical-resource model.
- Shared departure constraints for automatic and manual trains.
- Atomic permission checks and movement commitment.
- Observable waiting or blocking information where needed.
- Deterministic contention behavior and focused integration tests.

### Candidate OUT OF SCOPE

- Physical braking curves, acceleration, or continuous collision geometry.
- Full signaling systems, dispatch optimization, or deadlock resolution.
- General route planning, networking, multiplayer, or Bevy integration.

The final scope must identify any minimal claim/reservation mechanism required
by the selected model rather than silently relying on one.

## 6. Physical Model — Decisions Required

| Question | Decision |
| --- | --- |
| What is occupied: stations, berths, segments, blocks, or a subset? | TBD |
| Does a station hold one train or have explicit capacity? | TBD |
| Do opposite directed tracks share one physical resource? | TBD |
| When are resources acquired and released? | TBD |
| How is destination availability protected during traversal? | TBD |
| Can a train stop mid-track, or only wait before departure? | TBD |
| How are invalid initial overlaps handled? | TBD |

Distinguish actual occupancy from permission or a claim on future occupancy.
Do not label an empty but reserved resource as physically occupied unless the
chosen model explicitly defines that terminology.

## 7. Command and Automatic Behavior

Preserve the intent shape proposed by SPEC-002:

```text
RequestDeparture
    → validate train and control state
    → select candidate movement in core
    → check physical constraints
    → commit movement or return rejection
```

Automatic departures MUST apply the same relevant physical checks.
Failed checks MUST NOT partially change direction, train state, or resource claims.

TBD: minimal rejection reason, automatic retry timing, and whether blocked manual
requests require a fresh request. Do not introduce queued intent implicitly.
Any change to SPEC-002's no-buffering contract must be explicit and justified.

## 8. State and Snapshot Semantics

TBD: whether blocking is a committed train state, a separate observed condition,
or only a command result for the first slice.

The final design must distinguish:

- Normal dwell still in progress.
- Dwell complete but waiting for player intent.
- Movement prevented by a physical constraint.
- Movement in progress.

Snapshot construction MUST remain pure. It must not acquire resources, retry
departure, or resolve contention. Avoid a general resource catalog unless the
first consumer needs it.

## 9. Timing and Determinism

Preserve one-second `Simulation::step()` unless the selected model demonstrates
that a change is necessary.

Specify before implementation:

- Whether a resource released during a step can be acquired in that same step.
- How simultaneous automatic departure attempts are ordered.
- How commands between steps interact with resource availability.
- Whether results depend on train insertion order, IDs, or another explicit rule.

The current sequential train loop must not accidentally become an undocumented
arbitration policy. Equal initial state and equal ordered inputs MUST produce
equal results, independent of snapshot frequency.

## 10. First Vertical Slice and Acceptance — TBD

Start with the A–B–C example: one train waits at B while another attempts to move
from A. Once the physical model is selected, specify an exact expected timeline.

Candidate focused checks:

1. A conflicting movement cannot commit, for either control mode.
2. A failed request leaves train and resource state unchanged.
3. Movement can proceed after the relevant resource becomes available.
4. A resource cannot be assigned incompatibly to two trains.
5. Repeated observations do not affect contention or movement.
6. Uncontended automatic and manual journeys preserve their expected timing.

This list is not a complete Definition of Done until resources, release timing,
and retry semantics are selected.

## 11. Compatibility and Risks

The existing five-station scenario has opposing trains on a bidirectional line.
Depending on physical resource sharing, safety constraints may necessarily change
its timings or expose a deadlock. Review it explicitly; do not promise unchanged
behavior for scenarios that the selected model makes physically conflicting.

Safety does not imply progress: a model may prevent conflicting occupancy while
leaving trains unable to proceed. Define whether that is acceptable for the MVP.
Do not silently add deadlock resolution or bypass constraints to preserve progress.

## 12. Educational Implementation Plan — Placeholder

After design decisions are resolved, expand these into separately reviewed,
tests-first checkpoints with files, expected behavior, stop conditions, and commits:

1. Define the smallest physical-resource model.
2. Represent and verify resource ownership/lifecycle.
3. Apply physical checks to manual departure atomically.
4. Apply equivalent checks and explicit retry semantics to automatic departure.
5. Expose only the blocking facts required by the chosen observation contract.
6. Prove contention, release, determinism, and compatibility scenarios.

Adjust this sequence to keep intermediate changes coherent. No checkpoint is
authorized until the final MVP contract is defined.

## 13. Decisions Before Implementation

Resolve resource identity/capacity, direction sharing, destination protection,
acquisition/release timing, arbitration, retry behavior, and blocking observations.
Then write the exact first-slice timeline and a small Definition of Done.

Keep the departure intent boundary established by SPEC-002. The open decision is
which core physical model evaluates that intent, not whether an adapter may bypass it.
