using UnrealBuildTool;

public class ThreeCrossroads : ModuleRules
{
	public ThreeCrossroads(ReadOnlyTargetRules Target) : base(Target)
	{
		PCHUsage = PCHUsageMode.UseExplicitOrSharedPCHs;
		PublicDependencyModuleNames.AddRange(new string[] { "Core", "CoreUObject", "Engine", "InputCore" });
		// WW2Core (Plugins/WW2Core): the Rust core's C interface, or its not-linked stand-in (see that module's Build.cs)
		PrivateDependencyModuleNames.AddRange(new string[] { "WebSockets", "Json", "ProceduralMeshComponent", "WW2Core", "RenderCore" });
		// RenderCore: the white texture the HUD's filled shapes (flags, minimap discs) are drawn with
		// MeshDescription, StaticMeshDescription: the unit renderer builds its owner and selection rings as static meshes
		// at run time (UStaticMesh::BuildFromMeshDescriptions), so they draw instanced like the unit models
		PrivateDependencyModuleNames.AddRange(new string[] { "MeshDescription", "StaticMeshDescription" });
		PublicIncludePaths.Add(ModuleDirectory);
	}
}
