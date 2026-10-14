Three Crossroads native client: Linux x86_64 Development package
================================================================

This is the first packaged milestone of the native Unreal client. It is not a public release: it is a Development
build, and it has no game server inside.

What it needs
-------------
- Linux x86_64 with a Vulkan driver for the GPU, for a window or offscreen rendering. The system Vulkan loader
  (libvulkan.so.1) is used when present, else the one in Linux/Engine/Binaries/ThirdParty/Vulkan. With -nullrhi
  (after --) it draws nothing and needs neither.
- A running game server. The Rust ww2-server does not exist yet, so today this is the TEMPORARY reference backend:
  the pinned JavaScript game server (reference/ww2-rts at the commit named in BUILD-INFO.txt), on Node.js. It is not
  part of this package and must not be mistaken for the final server. In a ww2-rts-native checkout:

      unreal/Tools/tc-local.sh serve     # an isolated copy of the pinned server on a free 127.0.0.1 port
      unreal/Tools/tc-local.sh status    # prints its ws:// URL

Start
-----
      ./tc-client.sh --server ws://127.0.0.1:PORT/ws --room native1 --seat host -- -windowed -ResX=1600 -ResY=900
      ./tc-client.sh --server ws://127.0.0.1:PORT/ws --room native1 --seat guest     # a second player
      ./tc-client.sh --server ws://127.0.0.1:PORT/ws --room native1 --spectate       # a spectator
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

The Rust core (ww2-ffi, C ABI v1) is linked into the executable; formation spots come from it. Everything else the
client computes is still its interim C++ port (see DESIGN.md in the repository). Windows and Mac packages are not
built yet.
