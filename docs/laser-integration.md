# Laser integration

> **Status:** architecture scaffold; no hardware protocol or machine-control implementation yet.

## Goal

Add a native laser-production workflow to VectorCraft while keeping the vector editor independent
from machine-specific controller details. The first target is the owner's Reilogic Series 9 machine,
which is reported to have two controller boards.

## Initial architecture

- `vectorcraft-laser` owns machine-neutral job settings, board slots, connection snapshots,
  safety-interlock state and the backend boundary.
- The desktop UI will own the Laser workspace/panel and call the backend through this boundary.
- A future Reilogic backend will own device discovery, protocol framing, board-specific commands,
  status polling and emergency stop.
- Geometry/toolpath generation will be a separate stage from device communication. It must make
  units, origin, scale, kerf/offset, scan direction and raster/vector mode explicit.

## Safety requirements

- Unknown or missing safety inputs are unsafe.
- A job is rejected unless the selected board is ready and every required interlock is confirmed.
- The concrete backend must independently re-check live interlocks immediately before laser output.
- Stop/emergency-stop must not depend on the UI event loop remaining responsive.
- Initial development uses a fake backend only. No machine output is enabled by this scaffold.

## Hardware facts still to establish

Before implementing a Reilogic transport, inspect the machine and both boards and record:
- exact board model/revision and manufacturer markings;
- host connection (USB, serial/RS-232, Ethernet or another interface);
- controller firmware/version and documented command protocol;
- which board controls motion, laser power and safety inputs;
- available limit, lid, cooling, exhaust and emergency-stop signals.

Do not infer a protocol from the machine's series name. Do not send experimental commands to a
connected laser.

## Current scope

The first commit adds only the hardware-neutral Rust crate and this architecture note. It does not
claim to connect to Reilogic hardware, generate production toolpaths, or control laser emission.
