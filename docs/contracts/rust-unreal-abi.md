# Rust to Unreal interface

Status: pending agreement. Author: Julio. Consumer reviewer: Axel.

Inventory every client dependency on shared logic before defining the ABI. Include formations, individual soldier slots, placement/entrenchment, prices/availability/population, cover, regions and terrain queries. Decide canonical codec/client knowledge ownership rather than implementing two drifting state models.

To agree: query/data shapes, opaque handles, buffer lifetime/freeing, error codes, panic containment, threading, version/rules-data compatibility, target artifacts/build commands, linkage and recipient-visible-state restrictions.

Rust core remains engine/network/filesystem-free. No fabricated Rust library or invented ABI is accepted as bridge verification. Windows/Mac linkage and actual query results need real evidence.
