# Metro Sim 🚇

A small transportation simulation engine written in Rust.

The goal of this project is to explore **simulation, systems modeling, state machines, deterministic systems, and visualization** by building a simplified metro network from first principles.

This is deliberately **not a web application** and not intended to become one.

The first milestone is intentionally small: five stations, two trains, simulated passengers, and a visualization built with Bevy.

Eventually, the same simulation engine should be capable of modeling simplified versions of real transportation networks — from Mexico City to Madrid and beyond.

---

## Why?

Trains are great.

But more importantly, transportation networks are a fascinating systems problem.

A metro network combines:

* finite resources
* queues
* capacity constraints
* state machines
* scheduling
* stochastic demand
* network topology
* congestion
* cascading delays
* coordination
* time-dependent behavior

A seemingly simple event — such as a train arriving three minutes late — can affect passenger accumulation, train occupancy, transfers, headways, and eventually other parts of the network.

The purpose of Metro Sim is to experiment with those behaviors while learning how to model them in Rust.

---

# P0 — Tiny Metro

The first version consists of one fictional metro line.

```text
                    Train 02 ←
                         ▰

A ●────────●────────●────────●────────● E
           B        C        D

              ▰
          Train 01 →
```

Five stations:

```text
A ─── B ─── C ─── D ─── E
```

with fixed travel times:

```text
A → B    3 min
B → C    2 min
C → D    4 min
D → E    3 min
```

Two trains operate in opposite directions.

When a train reaches the end of the line, it waits and begins its return journey.

---

## Passengers

Stations generate passengers throughout the simulation.

For P0, demand is deliberately simple:

```text
Station A     3 passengers/min
Station B     5 passengers/min
Station C     8 passengers/min
Station D     4 passengers/min
Station E     2 passengers/min
```

Passengers have a destination and wait at the station until a train traveling in the appropriate direction arrives.

When a train arrives:

```text
Train arrives
      │
      ▼
Passengers exit
      │
      ▼
Waiting passengers board
      │
      ▼
Capacity reached?
      │
      ▼
Train dwells
      │
      ▼
Train departs
```

Trains have finite capacity.

Passengers who cannot board remain at the station.

---

# Simulation Core

The simulation engine must remain independent from its visualization.

```text
                 metro-core
                     │
              Simulation State
                     │
           ┌─────────┴─────────┐
           │                   │
           ▼                   ▼
       metro-bevy          future clients
       visualization
```

`metro-core` should know nothing about:

* Bevy
* graphics
* windows
* databases
* networking
* operating system UI

It models only the transportation system.

Conceptually:

```rust
let mut simulation = Simulation::new(config);

for _ in 0..10_000 {
    simulation.step();
}

println!("{:?}", simulation.metrics());
```

The same simulation should produce the same result when initialized with the same configuration and random seed.

---

# Train State Machine

A train is modeled as an explicit state machine.

For P0:

```text
             ┌──────────────┐
             │  At Station  │
             └──────┬───────┘
                    │
                    ▼
             ┌──────────────┐
             │   Dwelling   │
             └──────┬───────┘
                    │
                    ▼
             ┌──────────────┐
             │    Moving    │
             └──────┬───────┘
                    │
                    ▼
               next station
```

The simulation engine determines the state.

The renderer merely observes it.

---

# Visualization

P0 uses **Bevy** for a minimal 2D visualization.

The objective is not impressive graphics.

The objective is to make the state of the system visible.

For example:

```text
METRO SIM                              08:34:21

                    T02 ←
                     ▰

A ●────────●────────●────────●────────● E
   23       51       14       72       8

              ▰
            T01 →

T01     73 / 100 passengers
T02     91 / 100 passengers

Average waiting time:  04:13

Simulation speed:       10x

              [ Pause ]
```

Trains interpolate their visual position between stations based on simulation state.

The simulation clock must remain independent from Bevy's rendering frame rate.

---

# Metrics

P0 will collect a small set of operational metrics.

At minimum:

```text
Passengers generated
Passengers transported
Passengers waiting

Average waiting time

Train occupancy
Maximum train occupancy
```

Future versions can introduce metrics such as:

```text
P50 / P95 waiting time
headway adherence
station congestion
passengers left behind
train utilization
on-time performance
transfer waiting time
service reliability
```

These metrics will eventually contribute to a broader **Level of Service** model.

---

# Determinism

Simulation runs should be reproducible.

Given:

```text
network configuration
+
demand configuration
+
random seed
```

the simulation should produce the same sequence of events and final metrics.

For example:

```text
seed = 42

run #1 → average wait 04:31
run #2 → average wait 04:31
```

This allows scenarios and operational strategies to be compared reliably.

---

# Project Structure

Initial workspace:

```text
metro-sim/
│
├── Cargo.toml
│
├── README.md
│
├── metro-core/
│   └── src/
│       ├── lib.rs
│       ├── simulation.rs
│       ├── network.rs
│       ├── station.rs
│       ├── train.rs
│       └── passenger.rs
│
└── metro-bevy/
    └── src/
        ├── main.rs
        ├── renderer.rs
        └── ui.rs
```

---

# P0 Definition of Done

P0 is complete when the simulator can demonstrate:

* five stations
* one bidirectional line
* two trains
* train capacity
* passengers with destinations
* passenger boarding
* passenger disembarking
* passengers waiting when a train is full
* deterministic simulation
* simulation clock
* `1x`, `10x`, and `100x` simulation speeds
* play / pause
* animated train positions in Bevy
* visible station passenger counts
* visible train occupancy
* average passenger waiting time

The simulation core should also have automated tests covering its fundamental invariants:

```text
train_moves_between_stations
train_stops_at_station

passengers_board_train
passengers_leave_at_destination
passengers_wait_if_train_is_full

train_never_exceeds_capacity

same_seed_produces_same_result
```

Once these conditions are satisfied:

**P0 is done.**

Do not add another line.

---

# Future Work

P0 intentionally models a fictional network.

Once the simulation engine works reliably, future milestones can introduce increasingly realistic systems.

## Real Networks

Represent simplified versions of real metro networks using external network definitions:

```text
networks/
├── tiny-metro/
├── mexico-city/
└── madrid/
```

The simulation engine should eventually be capable of loading different networks without requiring changes to the engine itself.

Possible networks include:

### Mexico City

Model selected lines of the Sistema de Transporte Colectivo Metro.

Potential early experiment:

```text
Line 1
Line 2
Line 3
```

This introduces interchange stations and interactions between independent services.

### Madrid

Model selected lines of Metro de Madrid.

Besides being objectively necessary for emotional reasons, Madrid provides an interesting network for experimenting with:

* dense interchange stations
* radial and orbital connectivity
* different line lengths
* transfer behavior
* service coordination

The renderer should eventually be able to switch between networks:

```text
$ metro-sim --network tiny

$ metro-sim --network cdmx

$ metro-sim --network madrid
```

Network definitions should contain infrastructure and operational configuration rather than application logic.

---

## Multiple Lines

Introduce:

```text
Line A ───────●───────────
              │
              │ transfer
              │
Line B ───────●───────────
```

Passengers can then plan journeys involving transfers.

This will require graph-based routing and introduces network effects that do not exist in P0.

---

## Demand Modeling

Replace constant passenger generation with time-dependent stochastic demand.

For example:

```text
06:00 ───────── low
07:00 ───────── rising
08:00 ───────── ██████████ rush hour
09:00 ───────── ███████
10:00 ───────── normal
...
18:00 ───────── ██████████ rush hour
```

Possible models include:

* Poisson arrivals
* origin-destination matrices
* time-dependent demand
* station-specific demand profiles
* event-driven demand spikes

---

## Incidents

Introduce operational disturbances:

```text
train delay
train failure
station closure
track closure
reduced speed
excessive dwell time
demand surge
```

The interesting question becomes not merely whether the trains move, but:

> How does a local disturbance propagate through the system?

---

## Discrete-Event Simulation

P0 may use a fixed simulation timestep for simplicity.

A future version can transition to a discrete-event model:

```text
08:31:02 PassengerArrival
08:31:17 PassengerArrival
08:31:43 TrainArrival
08:32:13 TrainDeparture
08:34:51 TrainArrival
```

The simulation clock can then jump directly between meaningful events.

---

## Operational Experiments

Eventually the simulator should allow experiments such as:

> What happens if headway changes from 5 minutes to 4 minutes?

> What happens if demand increases by 30%?

> What happens if a train is removed from service during rush hour?

> How does a five-minute delay propagate through interchange stations?

> Can dispatch decisions recover regular headways?

Different strategies can then be evaluated using simulation metrics.

---

## Embedded Control Panel

A future embedded client could run on hardware such as an RP2040 or ESP32.

During development it could be simulated using Wokwi.

```text
              ┌─────────────────┐
buttons ─────▶│                 │
encoder ─────▶│ Embedded Rust   │
              │                 │
              └────────┬────────┘
                       │
                       ▼
                  metro-core
```

A physical control panel could introduce delays, change simulation speed, dispatch trains, or display operational metrics.

This would provide an opportunity to explore:

* embedded Rust
* `no_std`
* GPIO
* hardware timers
* serial communication
* constrained memory
* deterministic state machines

without coupling embedded concerns to the simulation engine.

---

# Non-Goals

At least initially, Metro Sim is **not**:

* a transportation planning tool
* an accurate model of Metro CDMX or Metro de Madrid
* a replacement for professional transit simulation software
* a game
* a web service
* a CRUD application
* a database project

The emphasis is on understanding and implementing the underlying system.

---

# Long-Term Direction

Metro Sim is an exploration of the question:

> **How can complex system behavior emerge from relatively simple rules?**

Trains move.

Passengers arrive.

Capacity is finite.

Time passes.

Individually, those rules are trivial.

Together they produce queues, congestion, delays, utilization patterns, bottlenecks, cascading effects, and operational trade-offs.

The long-term goal is to build a small but rigorous environment for experimenting with those behaviors while exploring Rust beyond traditional backend development.

And, naturally:

**God bless trains. 🚇**
