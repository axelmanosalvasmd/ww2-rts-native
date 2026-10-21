Three Crossroads native client: Linux x86_64 Development package
================================================================

This is a packaged milestone of the native Unreal client. It is not a public release: it is a Development
build, and it has no game server inside.

What it needs
-------------
- Linux x86_64 with a Vulkan driver for the GPU, for a window or offscreen rendering. The system Vulkan loader
  (libvulkan.so.1) is used when present, else the one in Linux/Engine/Binaries/ThirdParty/Vulkan. With -nullrhi
  (after --) it draws nothing and needs neither.
- A separate running game server. The pinned JavaScript reference backend (reference/ww2-rts at the commit in
  BUILD-INFO.txt) supports the reference modes. In a ww2-rts-native checkout:

      unreal/Tools/tc-local.sh serve     # an isolated copy of the pinned server on a free 127.0.0.1 port
      unreal/Tools/tc-local.sh status    # prints its ws:// URL

  The Rust executable also supports the approved trusted private R1 profile. Use its committed launch command
  in rust/crates/server/README.md. That profile allows exactly two human commanders on teams 0/1, Conquest/Default,
  Standard army, Sandbox off and temporary logistics off. AI, spectators and other modes remain refused there.

Start
-----
      ./tc-client.sh --server ws://127.0.0.1:PORT/ws --room native1 --seat host -- -windowed -ResX=1600 -ResY=900
      ./tc-client.sh --server ws://127.0.0.1:PORT/ws --room native1 --seat guest     # a second player
      ./tc-client.sh --server ws://127.0.0.1:PORT/ws --room native1 --spectate       # reference server only
      ./tc-client.sh --server ws://127.0.0.1:PORT/ws --check                         # check only, start nothing

tc-client.sh checks the machine (architecture, shared libraries, Vulkan loader, display) and that the server answers,
and stops with a plain message when something is missing. Settings, seat tokens (owner-only), logs and screenshots go
to ~/.local/share/three-crossroads-native (--user-dir or TC_USER_DIR to change it), never into this folder.

The seat token is the seat's key: it never goes on the command line. -TCToken= is refused.

What is inside
--------------
- Linux/                       Unreal's staged game: ThreeCrossroads.sh, Engine/, ThreeCrossroads/ (the executable and
                               the cooked content in Content/Paks)
- tc-client.sh                 the launcher above
- BUILD-INFO.txt               engine, toolchain, source digest, reference commit, the linked Rust core (ww2-ffi STAMP)
- SHA256SUMS                   every file's hash
- Manifest_*Files_Linux.txt    Unreal's staging lists: every file in the paks (UFS) and beside them (NonUFS)

The Rust core (ww2-ffi, C ABI 2) supplies formation spots and the default recipient-unit decoder. C++ still reads
row tails, HUD, cells and fog and supplies interim rules queries (see DESIGN.md). The exact FFI revision and file
hashes are in BUILD-INFO.txt. Windows and Mac packages are not built yet.
