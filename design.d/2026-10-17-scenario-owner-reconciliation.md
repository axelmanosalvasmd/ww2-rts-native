# One canonical authored mission owner

Resolve the overlapping timed mission implementations with one persisted `AuthoredScenario` owner. Retain main's `AuthoredMission` evaluator for canonical reconstruction of the admitted time-trigger subset, including its multiplication-based clock boundary, objective completion protection and exact announcement validation. Do not execute both implementations. Preserve main's startup restrictions and public-only full-map spectator projection.

Authored room saves use the complete shared v2 schema required by the save contract. Add a required nullable `last_tick` to preserve main's stale-clock refusal even when no trigger crosses its threshold. Existing non-authored saves remain v1. Refuse a present legacy `authored_mission` owner rather than ignoring it. No version 3 is introduced. The complete schema reserves condition, capture and arrival owners for the scenario engine continuation without admitting their unsupported actions here.

This is an internal state and validation choice. Recipient fields retain the prototype shapes at `shared/scenarios.js:159-175`; World admission stays closed.
