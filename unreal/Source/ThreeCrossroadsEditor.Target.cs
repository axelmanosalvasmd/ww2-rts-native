using UnrealBuildTool;

public class ThreeCrossroadsEditorTarget : TargetRules
{
	public ThreeCrossroadsEditorTarget(TargetInfo Target) : base(Target)
	{
		Type = TargetType.Editor;
		DefaultBuildSettings = BuildSettingsVersion.Latest;
		IncludeOrderVersion = EngineIncludeOrderVersion.Latest;
		ExtraModuleNames.Add("ThreeCrossroads");
	}
}
