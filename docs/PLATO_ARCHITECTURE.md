# PLATO Architecture and Modularity

Overview
- PLATO is a modular, pipelined stack designed to deliver constraint-theory driven research and deployment across fleet vessels. The core goal is lean cores, predictable performance, and plug-and-play capability.
- The fleet uses JC1 RFC (three-tier) and Oracle1 Kimi swarm roadmaps as the architectural guardrails.

Core components
- plato-kernel: Core loader, orchestrator for plugins, and constraint engine.
- plato-os: MUD/ROOM orchestration, edge integration, and inter-vessel data flow.
- plato-tui: Terminal UI with constraint-aware rendering; hooks into MUD rooms.
- forgemaster: I2I messaging hub, fleet-beacons, and distribution of tasks via beachcomb protocol.
- plato-research: Central repository for research artifacts, experiments, and notes.

Modularity model
- Plugins are discrete modules that can be mounted onto a vessel with a standard interface.
- Build-time gating (compile-time flags) keeps the base plato instance lean. In environments that demand, a runtime loader can mount additional plugins without requiring a rebuild.
- A centralized Plugin Registry tracks available plugins, their dependencies, and their lifecycle.

Plugin registry and lifecycle
- Descriptor fields: id, name, version, tier (core/edge/gpu), dependencies, entrypoint.
- Lifecycle: load -> initialize -> enable -> run -> unload -> disable -> remove.
- Dependency resolution validates that all required libraries and models exist before mounting.

I2I and fleet coordination
- I2I messages use a standard format: [I2I:TYPE] scope — summary. Beachcomb protocol is used to route messages across the fleet with hourly cadence.
- Beacons and bottles are the primary coordination primitives