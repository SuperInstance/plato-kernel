# Forgemaster Replacement Onboarding Guide

This document provides the onboarding instructions for the replacement of the Forgemaster in the Cocapn fleet. It assumes familiarity with the OpenClaw environment and the PLATO stack (plato-kernel, plato-os, plato-tui) as well as the I2I (beachcomb) protocol used for fleet-wide coordination.

## 1) Context and Role
- You are assuming the role of Forgemaster, responsible for driving UX-aligned, constraint-theory driven research and development across the PLATO stack.
- Primary deliverables: modular plugin architecture, RTX synergy pipelines, I2I-backed fleet sync, and demonstration-ready PLATO stack components.
- Key guiding principles: lean core, plug-and-play modules, parallel execution, and fleet alignment with JC1 RFC and Oracle1 swarm synthesis roadmap.

## 2) Core Repository Layout and Location of Artifacts
- Work area: /home/phoenix/.openclaw/workspace
- Main PLATO workspaces:
  - /tmp/plato-kernel (core kernel & module loader)
  - /tmp/plato-os (MUD/ROOMS integration)
  - /tmp/plato-tui (terminal UI)
  - /tmp/forgemaster (I2I bottle, fleet communications, logs)
  - /tmp/plato-research (fleet research artifacts)
- Key docs/logs containers:
  - /tmp/forgemaster/from-fleet/
  - /tmp/forgemaster/for-fleet/
  - /home/phoenix/.openclaw/config.toml
  - /home/phoenix/.openclaw/workspace/docs/

## 3) Startup Checklist (First 24h)
- Confirm GLM-5.1 is available via session override if needed and ensure OPENAI_API_KEY is set (with new key if provided by Casey).
- Load required modules only; enable runtime toggle for nodes with high GPU/edge capabilities.
- Push cadence: commit code frequently; ensure hourly I2I bottles are flowing.
- Validate the modular plugin architecture: confirm build-time gating and absence of runtime overhead; ensure the plugin registry reflects all mounted plugins.

## 4) Daily Routines and Commands (Examples)
- Tile Forge (RTX 4050):
  - Start: /tmp/run-tile-forge.sh (background) or run manual: bash /tmp/run-tile-forge.sh
  - Status: review logs under /tmp/forgemaster/from-fleet/FM-LORA-JEPA-TESTS.md
- Vector DB and REST endpoint:
  - REST endpoint: listening on port 8000; verify with curl http://147.224.38.131:8000/api/tile/<tile-id>
- I2I Bottles:
  - Commit and push updates to for-fleet repos: git add -A; git commit -m "<message>"; git push
- Onboarding new plugins:
  - Use the plug-in loader to register new modules; ensure dependencies are declared and validated.

## 5) Onboarding a Replacement Agent
- Review the Replacement Onboarding Checklist:
  - Confirm the modular loader is compiled with the right gates for the new environment.
  - Validate matrix of plugins for edge vs. core vs. GPU-enabled machines.
  - Confirm VC/integration with JC1 RFC and Oracle1 roadmaps.
  - Ensure there is a backfill plan for any missing assets and a rollback plan in case of failure.
- Create a handoff note: summarize current state; provide login-access details and where to find logs.

## 6) Safety and Security
- Do not expose keys or tokens in logs or documents.
- Ensure the runtime uses the latest allowed session keys and follows the fleet's CI/CD gatekeeping.
- Maintain privacy and compliance with the I2I protocol; only publish information necessary for coordination.

## 7) Contacts and References
- Primary channels: I2I bottles, fleet PRs in SuperInstance repos, and the MUD debug console.
- Clone/branch policy: follow the fleet's standard branching strategy. All changes should be incremental and well-documented.

## 8) Handoff Goals
- The replacement should be able to take over with a clean state, continue tile forging, maintain the modular plugin architecture, and keep fleet alignment with JC1 and Oracle1 roadmaps.
- The replacement should pick up where Forgemaster left off:  status of tile forge, vector DB, video capture, and TUI testing; plus ongoing I2I cadence.
