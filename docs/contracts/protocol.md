# Network protocol

Status: pending agreement. Author: Julio. Consumer reviewer: Axel.

Document the existing reference WebSocket protocol first; do not invent a replacement handshake. Record exact source commit and message schemas with sanitized fixtures.

Required coverage: join/roles/session ownership, start/lobby settings, player/terrain/entity state, snapshots/deltas and baseline recovery, commands/acknowledgments/rejections, sequences/lifecycle, visibility/privacy, disconnect/reconnect, version/data compatibility and later migration constraints.

Unresolved fields and behavior remain explicitly pending until inspected and agreed. Publishing this document does not claim compatibility.
