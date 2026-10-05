# SPEC-006: Operational Sandbox

## Purpose

Build `metro-bevy` into an **interactive operational sandbox** for `metro-core`.

This is not yet the final game layer. Its purpose is to visualize, inspect, stress-test, and debug the simulation through the public `metro-core` API.

## Initial scenario

Use a larger network than previous visualization specs, with approximately:

- 10–15 stations
- 6–7 automatic trains
- 1 manual train
- enough topology to produce congestion, contention, terminal reversals, and queues

`metro-core` remains the sole source of simulation truth. Bevy consumes snapshots and submits public commands/constraint operations.

## Operator / “super-player”

Add simple controls that allow an operator to inject operational conditions while the simulation runs, for example:

- close a directed track
- block departures from a station
- make a station unavailable
- schedule a restriction to begin in N seconds
- remove/cancel a registered restriction when allowed

This operator is a debugging and simulation tool, not a game character.

## Visualization goals

The sandbox should make core behavior easy to understand visually:

- automatic and manual train movement
- train direction and state
- occupied/reserved resources where useful
- blocked tracks/stations
- scheduled vs active constraints
- remaining constraint duration
- queues forming behind blocked resources
- reopening and subsequent recovery according to normal core rules

## Architectural goal

Use the sandbox to **dogfood the public `metro-core` API**.

`metro-bevy` must not depend on private domain internals such as `ResourceView` or `RestrictionView`. If useful visualization or control requires breaking encapsulation, treat that as feedback about the public core boundary rather than bypassing it.

## Non-goals

This spec does not introduce:

- zombies, aliens, score, missions, or narrative events
- a dedicated `metro-game` layer
- rerouting or skip-station behavior
- new railway policies solely for presentation
- simulation rules implemented inside Bevy

The result should be a practical visual debugger and experimentation environment for increasingly complex `metro-core` scenarios.