// Links the Rust core's C interface (ww2-ffi) from ThirdParty/ww2_ffi, where unreal/Tools/ww2-ffi.sh installs it.
//
// Linux x86_64 only for now. ABI 2 (native 03a49ac): the header must say WW2_ABI_VERSION 2, which WW2Core.cpp checks at
// compile time. Present and matching its STAMP (archive, header, formation fixture and the recorded protocol streams):
// WITH_WW2_FFI=1, the archive and the system libraries rustc listed for it are linked, and the commit and toolchain it
// came from are compiled in. Absent: WITH_WW2_FFI=0 with a warning, and the client keeps its interim C++ port
// (TC_REQUIRE_WW2_FFI=1 turns that into an error). A partial or changed install, or a STAMP that disagrees with this
// file, is always an error. Win64 and macOS arm64 are pending.
using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Security.Cryptography;
using Microsoft.Extensions.Logging;
using UnrealBuildTool;

public class WW2Core : ModuleRules
{
	const string LinuxTriple = "x86_64-unknown-linux-gnu";
	// `cargo rustc -p ww2-ffi --release --target x86_64-unknown-linux-gnu -- --print native-static-libs` with rustc 1.99.0
	// at native c244184 (ABI 1) and 03a49ac (ABI 2). ww2-ffi.sh stops when rustc lists anything else, and the STAMP must
	// repeat this list.
	static readonly string[] LinuxSystemLibraries = { "gcc_s", "util", "rt", "pthread", "m", "dl", "c" };
	// The recorded server-message streams and their browser-oracle reports that ww2-ffi.sh installs under
	// fixtures/protocol (native rust/crates/core/tests/fixtures, decompressed), replayed by the view self-check.
	static readonly string[] ProtocolStreams = { "protocol-conquest-p0", "protocol-conquest-p1", "protocol-world-p0", "protocol-world-p1" };

	public WW2Core(ReadOnlyTargetRules Target) : base(Target)
	{
		PCHUsage = PCHUsageMode.UseExplicitOrSharedPCHs;
		PublicDependencyModuleNames.AddRange(new string[] { "Core" });
		// Json: the view self-check reads the recorded streams with Unreal's own parser, independently of the library.
		PrivateDependencyModuleNames.AddRange(new string[] { "Projects", "Json" });

		string Root = Path.Combine(PluginDirectory, "ThirdParty", "ww2_ffi");
		string Missing;
		bool bLinked;
		if (Target.Platform == UnrealTargetPlatform.Linux && Target.Architecture == UnrealArch.X64)
		{
			bLinked = LinkLinux(Path.Combine(Root, "Linux", LinuxTriple), out Missing);
		}
		else
		{
			bLinked = false;
			Missing = String.Format("ww2-ffi for {0} {1} is pending: only Linux x86_64 is built and verified", Target.Platform, Target.Architecture);
		}
		if (!bLinked)
		{
			if (Environment.GetEnvironmentVariable("TC_REQUIRE_WW2_FFI") == "1")
			{
				throw new BuildException("WW2Core: " + Missing + ". TC_REQUIRE_WW2_FFI=1 requires the Rust core.");
			}
			Logger.LogWarning("WW2Core: building without the Rust core: {Missing}. The client uses its interim C++ formation port.", Missing);
			PrivateDefinitions.Add("WITH_WW2_FFI=0");
			PrivateDefinitions.Add("WW2_FFI_MISSING=\"" + Quote(Missing) + "\"");
		}
	}

	bool LinkLinux(string Dir, out string Missing)
	{
		string Lib = Path.Combine(Dir, "lib", "libww2_ffi.a");
		string Header = Path.Combine(Dir, "include", "ww2_core.h");
		string Fixture = Path.Combine(Dir, "fixtures", "formation.txt");
		string Stamp = Path.Combine(Dir, "STAMP");
		// Protocol fixture files, keyed by their STAMP field: the name with '-' and '.' as '_', then _sha256 (ww2-ffi.sh).
		Dictionary<string, string> Protocol = new Dictionary<string, string>();
		foreach (string Stream in ProtocolStreams)
		{
			foreach (string Name in new string[] { Stream + ".jsonl", Stream + ".expected.jsonl" })
			{
				Protocol[Name.Replace('-', '_').Replace('.', '_') + "_sha256"] = Path.Combine(Dir, "fixtures", "protocol", Name);
			}
		}
		string[] Files = new string[] { Lib, Header, Fixture, Stamp }.Concat(Protocol.Values).ToArray();
		// All of them, even while absent: UnrealBuildTool re-reads these rules (and so re-checks the hashes below) whenever
		// one of them appears, changes or goes away. With only the STAMP listed, an edited header compiled unchecked.
		ExternalDependencies.AddRange(Files);
		string[] Absent = Files.Where(F => !File.Exists(F)).ToArray();
		if (Absent.Length == Files.Length)
		{
			Missing = "no ww2-ffi install in " + Dir + " (run unreal/Tools/ww2-ffi.sh build)";
			return false;
		}
		if (Absent.Length > 0)
		{
			throw new BuildException("WW2Core: partial ww2-ffi install in " + Dir + ", missing " + String.Join(", ", Absent) + ". Run unreal/Tools/ww2-ffi.sh build (or remove).");
		}

		Dictionary<string, string> S = new Dictionary<string, string>();
		foreach (string Line in File.ReadAllLines(Stamp))
		{
			int Eq = Line.IndexOf('=');
			if (Eq > 0) S[Line.Substring(0, Eq)] = Line.Substring(Eq + 1);
		}
		string Field(string Key)
		{
			string Value;
			if (!S.TryGetValue(Key, out Value) || Value.Length == 0) throw new BuildException("WW2Core: " + Stamp + " has no " + Key + ". Run unreal/Tools/ww2-ffi.sh build.");
			return Value;
		}
		string Libs = String.Join(" ", LinuxSystemLibraries.Select(L => "-l" + L));
		if (Field("target") != LinuxTriple) throw new BuildException("WW2Core: " + Stamp + " is for " + Field("target") + ", not " + LinuxTriple);
		if (Field("native_static_libs") != Libs) throw new BuildException("WW2Core: the install needs '" + Field("native_static_libs") + "' but WW2Core.Build.cs links '" + Libs + "'. Update both together.");
		Dictionary<string, string> Checks = new Dictionary<string, string>(Protocol) { { "lib_sha256", Lib }, { "header_sha256", Header }, { "fixture_sha256", Fixture } };
		foreach (KeyValuePair<string, string> Check in Checks)
		{
			if (Sha256(Check.Value) != Field(Check.Key)) throw new BuildException("WW2Core: " + Check.Value + " changed since ww2-ffi.sh installed it. Run unreal/Tools/ww2-ffi.sh build.");
		}
		string Rev = Field("native_rev");
		if (Rev.Length != 40 || !Rev.All(Uri.IsHexDigit)) throw new BuildException("WW2Core: bad native_rev in " + Stamp);

		PrivateIncludePaths.Add(Path.Combine(Dir, "include"));
		PublicAdditionalLibraries.Add(Lib);
		PublicSystemLibraries.AddRange(LinuxSystemLibraries);
		PrivateDefinitions.Add("WITH_WW2_FFI=1");
		PrivateDefinitions.Add("WW2_FFI_NATIVE_REV=\"" + Rev + "\"");
		PrivateDefinitions.Add("WW2_FFI_TARGET=\"" + LinuxTriple + "\"");
		PrivateDefinitions.Add("WW2_FFI_RUSTC=\"" + Quote(Field("rustc")) + "\"");
		PrivateDefinitions.Add("WW2_FFI_LIB_SHA256=\"" + Field("lib_sha256") + "\"");
		Missing = "";
		return true;
	}

	static string Sha256(string Path)
	{
		using (SHA256 H = SHA256.Create())
		using (FileStream F = File.OpenRead(Path))
		{
			return Convert.ToHexString(H.ComputeHash(F)).ToLowerInvariant();
		}
	}

	static string Quote(string Text) { return Text.Replace("\\", "/").Replace("\"", "'"); }
}
