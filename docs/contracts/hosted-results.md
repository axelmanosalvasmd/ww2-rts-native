# Hosted Horde results and record ownership

Baseline: prototype `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`. Owner decisions I/K/N open the existing save, hosted Horde and JSON v0 shapes. Hermes' exact-head runtime acknowledgement remains required.

Natural Horde completion sends the existing common result fields, including the internal Horde name/team/story row, territory timeline and `winVp:null`. Each defender receives `you:{team:0,outcome:"defeat"}`. `result.horde` has exactly `{wave,kills,time,names,record,best}` (`server.js:80-86,752-753`). `kills` sums defender story kills, `time` rounds final simulation tick times 0.05 seconds, and `names` contains the room defenders. The hold's 60 simulation steps contribute to this final time. Sandbox results omit `horde` entirely.

The key is `map|defender count|army`, with Standard as the default. Higher wave wins; at equal wave only strictly longer time wins. `best` is the previous run or null, while `record` reports whether this run beats it. The file stores only `{wave,kills,time,names}` per key. The lobby's `hordeBest` reflects current settings, including sandbox lobbies (`server.js:66-86,344`). A failed write retains the in-memory best and reports the error as in the prototype.

The front constructs one record owner and gives game workers request handles. Workers submit lookups and completed runs; only the owner thread accesses the file and compares against the current best. This implements the prototype's forwarding-process ownership (`server.js:803-804`) inside the native thread-based shard architecture. Synchronous replies make the new lobby and result observe the completed comparison/write. It adds no client message or field.

A winner starts 120 host callbacks. Odd callbacks retain the game and send no frame. Even callbacks step once and deliver a frame. Callback 120 publishes the last frame, then the result lobby. Human orders remain refused and brains idle throughout (`server.js:737-754`). Host end continues to send only the existing ended result.

The Horde mode row uses the prototype's exact omission rules (`shared/sim.js:5895-5897,6017-6019`): initial profile/boss omitted, nextProfile present only during a break, later absent boss null, nonnegative ceiling timeLeft. World admission remains closed. The composed Tutorial delivery and result path is described in [tutorial-room-start.md](tutorial-room-start.md).

On-map aviation events match `shared/sim.js:2250-2251`: `aa` is `{f,t,fo,to,x,z,k:"aa"}`; `planedown` is `{k:"planedown",t,to,x,z,dir,kill:true}`. They have no `local` or `pub` key. Ordinary event admission uses reveal, point visibility, an allied owner or a visible named actor; unseen `f`/`t` are omitted while `fo`/`to` remain (`:6080-6087`). Retained events may refer to retired actors. Producer-buffer and support-plane capability guards remain in place.

ROOM-AI #417 supplies the human-seat lifecycle and private planner persistence. The scripted internal Horde seat retains its existing persistent director and delivery memory; actual roster AI uses the human commander and private planner owner. Requested human observations are delivered before the director or human scheduler acts. The host's lobby-only level message changes only AI seats; departed human armies hand over at Normal (`server.js:640-643`). Both kinds of commander remain idle during the complete ending hold.
