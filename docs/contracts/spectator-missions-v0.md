# Authored mission privacy in spectator delivery

Baseline: prototype `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`.
Under owner decisions J and P, spectators see the current full map outside World
Conquest; World admission stays closed. This extends the ordinary spectator
projection documented in [spectators-v0.md](spectators-v0.md).

The admitted authored subset contains match-scoped, once-only timed triggers,
Say actions, objective activation and objective completion. Groups, areas,
reinforcements, damage actions, other conditions, repeats and player/team trigger
scopes remain refused. Private recipient policies are supported within the
admitted match scope. Localized message objects require an English entry;
string messages retain their original string representation. Definitions outside
this subset fail startup before the room changes.

The authoritative owner uses the prototype clock, `tick * (1/20)`
(`shared/sim.js:27,4884`).
Triggers use authored order, share the `scenario:N` execution counter, and stop
with the existing winner guard. An objective completion cannot be reversed by a
later activation. These rules match `shared/scenarios.js:83-86,99-103,132-169`.

Recipient `scenario` is exactly `{version:1,objectives:[...]}`. Each objective
contains exactly `id`, `scope`, `text`, `state`, `recipients`, `side`, `updated`
and `execution`. Scope is `match`; state is `active` or `complete`; updated is
simulation seconds. Objectives retain their first activation order when updated.
The definition's recipient policy and side override the activating action and
trigger (`shared/scenarios.js:161-164,171-174`).

A Say shot contains exactly `k:"say"`, `text`, `localized`,
`scenarioRecipients`, `scenarioSide`, `pub`, `execution` and `atTick`.
`pub` is true exactly for public policy; `atTick` is the producing match tick.
Text is the authored string or its English localized value
(`shared/scenarios.js:159-160`). Messages preserve action order.

Seated recipients receive public rows, their own side rows, and team rows whose
side belongs to their team (`shared/scenarios.js:87,171-175`). Spectators receive
only public objective and Say rows. A private seat-zero or seat-zero-team row
must stay hidden from spectators even though their start uses seat zero. These
filters match `server.js:261-263`. Mission messages have no ground position and
use their recipient policy rather than a fog test. Ordinary non-mission shots
keep their existing visibility and identity rules. Other spectator privacy,
unit flags, cooldowns, full-map delivery and starts remain as documented in
`spectators-v0.md` (`server.js:201-204,247-265,455-466`).

The private checkpoint uses the complete `authored_scenario` v2 owner described in
[save-format.md](save-format.md). It retains the definition, required execution
clock, contextual trigger records, ordered objectives, condition facts, target
owners and deferred arrivals. Main's bounded timed evaluator reconstructs the
canonical state and refuses inconsistent records. The old `authored_mission`
field remains decode-only and a present value is refused. Ordinary non-authored
saves remain v1; authored saves require v2, and v3 is unsupported. Pending authored
Say events use the private tick-event checkpoint codec. The room's raw-map binding
checks the mission definition before attachment or load.

`intent_net77_authored_objectives_and_private_messages_filter_spectators_through_ticks_and_restore`
checks exact objective and Say objects for an owner, an ally, an enemy and a
spectator on eight ordinary ticks. It saves after activation and restores before
completion, then verifies continued delivery and no runtime fault. It also checks
seat-zero starts and the literal spectator private-key omissions. The admission
check refuses unsupported actions and a different private definition on the same
public map. No complete authored scenario executor, World, AI or operative role
admission is claimed.
