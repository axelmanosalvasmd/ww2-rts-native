using UnrealBuildTool;

public class ThreeCrossroadsTarget : TargetRules
{
	public ThreeCrossroadsTarget(TargetInfo Target) : base(Target)
	{
		Type = TargetType.Game;
		DefaultBuildSettings = BuildSettingsVersion.Latest;
		IncludeOrderVersion = EngineIncludeOrderVersion.Latest;
		ExtraModuleNames.Add("ThreeCrossroads");
	}
}
