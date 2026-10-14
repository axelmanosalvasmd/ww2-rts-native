# Imports the pinned reference game's sound and voice files, unchanged, as Unreal sound waves (parity gap 9: CLI-115 to
# CLI-119). Run by unreal/Tools/import-reference-audio.sh inside a throwaway editor project (UnrealEditor-Cmd
# -run=pythonscript); it never opens the game project itself.
#
# Input (environment, set by the shell driver):
#   TC_AUDIO_REF    the reference checkout (reference/ww2-rts at the pin)
#   TC_AUDIO_PIN    the reference commit (full sha) the driver checked
#   TC_AUDIO_BLOBS  a file of "<git blob sha> <path>" lines for client/audio (git ls-files -s), so the manifest names the
#                   exact committed objects
#   TC_AUDIO_OUT    the manifest to write
#
# What it does, in order:
#   1. reads client/audio/index.json, the reference's own list of effects (sfx) and voice lines (voice), and refuses to
#      go on when a listed file is missing or a file in sfx/ or voice/ is not listed;
#   2. imports every listed MP3 with Unreal's SoundFactory (libsndfile decodes it; the cook encodes it per platform) to
#      /Game/Audio/SFX/<id> and /Game/Audio/Voice/<fac>/<file stem>;
#   3. marks the three loops of index.json (loop: true: tank_engine, fire_loop, ambient) as looping waves, the only
#      setting changed from the factory default (client/audio.js plays them with src.loop = true);
#   4. saves the packages and writes the manifest: per file its reference path, git blob, sha256 and size, the asset it
#      became, that asset's sha256, the imported duration and channel count beside index.json's dur.
import hashlib
import json
import os
import sys

import unreal

REF = os.environ["TC_AUDIO_REF"]
PIN = os.environ["TC_AUDIO_PIN"]
BLOBS = os.environ["TC_AUDIO_BLOBS"]
OUT = os.environ["TC_AUDIO_OUT"]
AUDIO = os.path.join(REF, "client", "audio")


def fail(msg):
    unreal.log_error("import-reference-audio: " + msg)
    sys.exit(1)


def prop(obj, name):
    try:
        return obj.get_editor_property(name)
    except Exception:
        return None


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


with open(os.path.join(AUDIO, "index.json"), "rb") as f:
    index_bytes = f.read()
index = json.loads(index_bytes)

blobs = {}
with open(BLOBS) as f:
    for line in f:
        sha, path = line.strip().split(" ", 1)
        blobs[path] = sha

# (reference path relative to client/audio, destination folder, asset name, looping, index dur)
entries = []
for sid, s in index["sfx"].items():
    entries.append(("sfx/%s.mp3" % sid, "/Game/Audio/SFX", sid, bool(s.get("loop")), s.get("dur")))
for fac in sorted(index["voice"]):
    for speaker in sorted(index["voice"][fac]):
        for kind in sorted(index["voice"][fac][speaker]):
            for takes in index["voice"][fac][speaker][kind]:
                for name in takes:
                    entries.append(("voice/%s/%s" % (fac, name), "/Game/Audio/Voice/%s" % fac, name[:-4], False, None))

listed = {e[0] for e in entries}
on_disk = set()
for sub in ("sfx", "voice"):
    for root, _, files in os.walk(os.path.join(AUDIO, sub)):
        for name in files:
            on_disk.add(os.path.relpath(os.path.join(root, name), AUDIO).replace(os.sep, "/"))
missing = sorted(listed - on_disk)
extra = sorted(on_disk - listed)
if missing:
    fail("listed in index.json but missing: %s" % ", ".join(missing))
if extra:
    fail("present but not listed in index.json: %s" % ", ".join(extra))

tasks = []
for rel, dest, name, loop, dur in entries:
    t = unreal.AssetImportTask()
    t.filename = os.path.join(AUDIO, rel)
    t.destination_path = dest
    t.destination_name = name
    t.replace_existing = True
    t.automated = True
    t.save = False
    t.factory = unreal.SoundFactory()
    tasks.append(t)
unreal.AssetToolsHelpers.get_asset_tools().import_asset_tasks(tasks)

rows = []
packages = []
for (rel, dest, name, loop, dur), t in zip(entries, tasks):
    path = "%s/%s" % (dest, name)
    wave = unreal.load_asset(path)
    if not isinstance(wave, unreal.SoundWave):
        fail("%s did not import as a sound wave (%s)" % (rel, path))
    if loop:
        wave.set_editor_property("looping", True)
    packages.append(wave.get_outermost())
    rows.append([rel, path, wave, loop, dur])

if not unreal.EditorLoadingAndSavingUtils.save_packages(packages, False):
    fail("saving the imported packages failed")

content = unreal.Paths.convert_relative_path_to_full(unreal.Paths.project_content_dir())
files = []
for rel, path, wave, loop, dur in rows:
    src = os.path.join(AUDIO, rel)
    ref_path = "client/audio/" + rel
    asset_file = os.path.join(content, path[len("/Game/"):] + ".uasset")
    if not os.path.isfile(asset_file):
        fail("no package file for %s at %s" % (path, asset_file))
    files.append({
        "source": ref_path,
        "blob": blobs.get(ref_path),
        "sha256": sha256(src),
        "bytes": os.path.getsize(src),
        "asset": path,
        "assetFile": "Content/" + path[len("/Game/"):] + ".uasset",
        "assetSha256": sha256(asset_file),
        "looping": bool(wave.get_editor_property("looping")),
        "duration": round(float(prop(wave, "duration") or 0.0), 4),
        "channels": prop(wave, "num_channels"),
        "indexDur": dur,
    })
    if files[-1]["blob"] is None:
        fail("%s is not a committed file of the reference" % ref_path)

manifest = {
    "tool": "unreal/Tools/import-reference-audio.sh",
    "reference": "reference/ww2-rts",
    "pin": PIN,
    "index": {"source": "client/audio/index.json", "blob": blobs.get("client/audio/index.json"), "sha256": hashlib.sha256(index_bytes).hexdigest()},
    "engine": unreal.SystemLibrary.get_engine_version(),
    "factory": "SoundFactory (defaults; looping set on index.json loop entries)",
    "counts": {"sfx": len(index["sfx"]), "voice": len(entries) - len(index["sfx"]), "total": len(entries)},
    "files": files,
}
with open(OUT, "w") as f:
    json.dump(manifest, f, indent=1, sort_keys=False)
    f.write("\n")
unreal.log("import-reference-audio: %d files imported, manifest %s" % (len(files), OUT))
