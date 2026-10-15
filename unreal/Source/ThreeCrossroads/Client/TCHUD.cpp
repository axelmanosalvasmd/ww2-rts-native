#include "Client/TCHUD.h"
#include "Client/TCStats.h"
#include "Client/TCGraphics.h"
#include "Client/TCPersistence.h"
#include "Client/TCTutorial.h"
#include "Client/TCGameMode.h"
#include "Client/TCOrders.h"
#include "Client/TCRecruitment.h"
#include "Client/TCArmyCommands.h"
#include "Client/TCConstruction.h"
#include "Client/TCLogistics.h"
#include "Client/TCSupportOrders.h"
#include "Client/TCAbilities.h"
#include "Client/TCFieldworks.h"
#include "Client/TCFormationControls.h"
#include "Client/TCSupportPowers.h"
#include "Client/TCCoverPreview.h"
#include "Client/TCMarkers.h"
#include "Client/TCUnitRenderer.h"
#include "Client/TCLobbySettings.h"
#include "Client/TCPersistence.h"
#include "Client/TCLocale.h"
#include "Client/TCAlerts.h"
#include "Client/TCBossBar.h"
#include "Client/TCEndReport.h"
#include "ThreeCrossroads.h"
#include "World/TCTerrain.h"
#include "World/TCWorldObjects.h"
#include "Net/TCConnection.h"
#include "Engine/Canvas.h"
#include "Engine/Engine.h"
#include "Engine/Font.h"
#include "Engine/Texture2D.h"
#include "CanvasItem.h"
#include "ImageUtils.h"
#include "Misc/Base64.h"
#include "UObject/StrongObjectPtr.h"

namespace
{
#include "Client/TCHUD.Icons.inl"
	UTexture2D* HudIcons()
	{
		static TStrongObjectPtr<UTexture2D> Atlas;
		static bool Attempted=false;
		if (!Attempted)
		{
			Attempted=true; TArray<uint8> Bytes;
			if (FBase64::Decode(HudIconPng,Bytes)) Atlas.Reset(FImageUtils::ImportBufferAsTexture2D(Bytes));
			if (Atlas.IsValid()) { Atlas->Filter=TF_Bilinear; Atlas->SRGB=true; Atlas->UpdateResource(); }
			else UE_LOG(LogTC,Warning,TEXT("hud: generated reference icon atlas could not be loaded"));
		}
		return Atlas.Get();
	}
}

void ATCHUD::DrawBottomLayout(const FTCHudFit::FBottomLayout& Layout, UFont* Font)
{
	FTCType::Draw(Layout.Paint,Canvas,Font,UiNow);
	for (const FBox2D& P : Layout.Paint.Panels) Panels.Add(ToScreen(P));
	UTexture2D* Atlas=HudIcons();
	if (Atlas && Atlas->GetResource())
		for (const FTCHudFit::FIcon& I : Layout.Icons)
		{
			int32 At=INDEX_NONE;
			for (int32 K=0; K<UE_ARRAY_COUNT(HudIconKeys); K++) if (I.Key==HudIconKeys[K]) { At=K; break; }
			if (At==INDEX_NONE) continue;
			const FVector2D Uv0(double(At%HudIconColumns)*HudIconSide/HudIconWidth,double(At/HudIconColumns)*HudIconSide/HudIconHeight);
			const FVector2D Uv1=Uv0+FVector2D(double(HudIconSide)/HudIconWidth,double(HudIconSide)/HudIconHeight);
			FCanvasTileItem Tile(I.Box.Min,Atlas->GetResource(),I.Box.GetSize(),Uv0,Uv1,I.Colour);
			Tile.BlendMode=SE_BLEND_Translucent; Canvas->DrawItem(Tile);
		}
	const FTCHudFit::FHit* Hover=nullptr;
	float MX=0,MY=0;
	const bool Mouse=GetOwningPlayerController()->GetMousePosition(MX,MY);
	for (const FTCHudFit::FHit& H : Layout.Hits)
	{
		Buttons.Add({ToScreen(H.Box),H.Control.Command,H.Control.Label,H.Control.bEnabled});
		if (Mouse && H.Box.IsInsideOrOn(FVector2D(MX,MY)/UiNow)) Hover=&H;
	}
	if (Hover && !Hover->Control.Hover.IsEmpty())
	{
		const FTCMeasure Measure=FTCType::IsLoaded()?FTCType::Measure():FTCPaint::CanvasMeasure(Font);
		const FString Shown=TC_T(Hover->Control.Hover);
		const float W=FMath::Min(360.f,Measure(Shown,13,false,0)+16);
		const TArray<FString> Lines=FTCPaint::Wrap(Shown,W-16,13,false,Measure);
		const float H=Lines.Num()*17.55f+12;
		const float X=FMath::Clamp((float)Hover->Box.Min.X,8.f,Canvas->ClipX-W-8.f);
		const float Y=FMath::Max(8.f,(float)Hover->Box.Min.Y-H-6.f);
		FTCPaint Tip; Tip.Rect(X,Y,W,H,TCCss::PanelSolid); Tip.Frame(X,Y,W,H,TCCss::LineSoft);
		for (int32 K=0; K<Lines.Num(); K++) Tip.Text(Lines[K],X+8,Y+6+K*17.55f,13,TCCss::Text);
		FTCType::Draw(Tip,Canvas,Font,UiNow);
	}
}

namespace TCHudBottom
{
	using namespace FTCHudFit;
	float Border(float Scale) { return FMath::Max(1.f, FMath::FloorToFloat(Scale)) / Scale; }
	bool Overlap(const FBox2D& A, const FBox2D& B)
	{
		return A.bIsValid && B.bIsValid && A.Min.X < B.Max.X - .01 && A.Max.X > B.Min.X + .01 && A.Min.Y < B.Max.Y - .01 && A.Max.Y > B.Min.Y + .01;
	}
	FBox2D Box(float X, float Y, float W, float H) { return FBox2D(FVector2D(X,Y), FVector2D(X+W,Y+H)); }
	void Panel(FBottomLayout& L, const FBox2D& B, const TCHAR* Role, float Scale)
	{
		L.Paint.Rect(B.Min.X,B.Min.Y,B.GetSize().X,B.GetSize().Y,TCCss::Panel,Role);
		L.Paint.Frame(B.Min.X,B.Min.Y,B.GetSize().X,B.GetSize().Y,TCCss::Line,Border(Scale));
		L.Paint.Panels.Add(B);
	}
	void Text(FBottomLayout& L, const FString& Shown, float X, float Y, float Px, bool Bold, FLinearColor C,
		const FBox2D& Clip, const FTCMeasure& Measure, const TCHAR* Role, ETCPaintAlign Align = ETCPaintAlign::Left)
	{
		if (Shown.IsEmpty()) return;
		const float W = Measure(Shown,Px,Bold,0), H = Px * 1.35f;
		L.Paint.Text(Shown,X,Y,Px,C,Bold,Align,Role).Size = FVector2D(W,H);
		const float Left = X - (Align == ETCPaintAlign::Centre ? W / 2 : Align == ETCPaintAlign::Right ? W : 0);
		L.TextBounds.Add({Shown,Box(Left,Y,W,H),Clip});
	}
	float Wrapped(FBottomLayout& L, const FString& Shown, float X, float Y, float W, float Px, bool Bold,
		FLinearColor C, const FTCMeasure& Measure, const TCHAR* Role)
	{
		const TArray<FString> Lines = FTCPaint::Wrap(Shown,W,Px,Bold,Measure);
		const FBox2D Clip = Box(X,Y,W,Lines.Num()*Px*1.35f);
		for (const FString& Line : Lines) { Text(L,Line,X,Y,Px,Bold,C,Clip,Measure,Role); Y += Px*1.35f; }
		return Clip.GetSize().Y;
	}
	void Icon(FBottomLayout& L, const FString& Key, float X, float Y, float Side, FLinearColor C)
	{
		if (!Key.IsEmpty()) L.Icons.Add({Key,Box(X,Y,Side,Side),C});
	}
	void GridControl(FBottomLayout& L, const FControl& C, const FBox2D& B, const FTCMeasure& Measure, float Scale)
	{
		L.Paint.Rect(B.Min.X,B.Min.Y,B.GetSize().X,B.GetSize().Y,C.bOn ? TCCss::Rgba(214,178,94,.1f) : TCCss::Hex(0x24282b),TEXT("bottom:order"));
		L.Paint.Frame(B.Min.X,B.Min.Y,B.GetSize().X,B.GetSize().Y,C.bOn ? TCCss::Brass : TCCss::LineSoft,Border(Scale));
		Icon(L,C.Icon,B.GetCenter().X-11.5f,B.Min.Y+6,23,C.bEnabled ? TCCss::Text : TCCss::Dim);
		// keys.js badge() uses U+21E7. Barlow lacks that glyph, so its browser fallback is in the generated atlas.
		FString Key=C.Key; const bool Shift=Key.RemoveFromStart(TEXT("⇧"));
		Text(L,Key,B.Max.X-3,B.Min.Y+1,11,false,TCCss::Dim,B,Measure,TEXT("bottom:key"),ETCPaintAlign::Right);
		if (Shift) Icon(L,TEXT("key-shift"),B.Max.X-3-Measure(Key,11,false,0)-10,B.Min.Y+1,10,TCCss::Dim);
		if (!C.Value.IsEmpty())
		{
			const TArray<FString> Lines = FTCPaint::Wrap(TC_T(C.Value),B.GetSize().X-4,13,false,Measure);
			// Menu names use the browser's full 13px value line; taller labels add space rather than ellipsizing.
			float Y = B.Max.Y-4-Lines.Num()*17.55f;
			for (const FString& Line : Lines) { Text(L,Line,B.GetCenter().X,Y,13,false,C.bOn ? TCCss::Brass : TCCss::Text,B,Measure,TEXT("bottom:value"),ETCPaintAlign::Centre); Y += 17.55f; }
		}
		L.Hits.Add({B,C});
	}
	float Grid(FBottomLayout& L, const TArray<FControl>& Controls, float X, float Y, float W, const FTCMeasure& Measure, float Scale)
	{
		const float CellW = (W-3*GridGap)/4;
		for (int32 First=0; First<Controls.Num(); First+=4)
		{
			float H = OrderH;
			for (int32 K=First; K<FMath::Min(First+4,Controls.Num()); K++)
				H=FMath::Max(H,33.f+FTCPaint::Wrap(TC_T(Controls[K].Value),CellW-4,13,false,Measure).Num()*17.55f);
			for (int32 K=First; K<FMath::Min(First+4,Controls.Num()); K++) GridControl(L,Controls[K],Box(X+(K-First)*(CellW+GridGap),Y,CellW,H),Measure,Scale);
			Y+=H+GridGap;
		}
		return Controls.Num() ? Y-GridGap : Y;
	}
	void Shift(FBottomLayout& L, FVector2D By)
	{
		L.Paint.Shift(0,By);
		for (FBox2D& P : L.Paint.Panels) P=FBox2D(P.Min+By,P.Max+By);
		for (FIcon& I : L.Icons) I.Box=FBox2D(I.Box.Min+By,I.Box.Max+By);
		for (FHit& H : L.Hits) H.Box=FBox2D(H.Box.Min+By,H.Box.Max+By);
		for (FTextFit& T : L.TextBounds) { T.Box=FBox2D(T.Box.Min+By,T.Box.Max+By); T.Clip=FBox2D(T.Clip.Min+By,T.Clip.Max+By); }
		auto Move=[&](FBox2D& B) { if (B.bIsValid) B=FBox2D(B.Min+By,B.Max+By); };
		Move(L.Selection); Move(L.Orders); Move(L.Recruit); Move(L.Economy);
	}
}

FTCHudFit::FBottomLayout FTCHudFit::LeftPanel(float ViewH, const FString& Title, const TArray<FSelection>& Selection,
	const TArray<FString>& Details, const TArray<FControl>& Orders, const TArray<FControl>& Menu, const FTCMeasure& Measure, float Scale)
{
	using namespace TCHudBottom;
	FBottomLayout L;
	const float X=ScreenGap, W=LeftW, Inner=W-14;
	float Y=0;
	if (!Selection.IsEmpty())
	{
		Y=8;
		Text(L,TC_T(Title),X+7,Y,13,true,TCCss::Text,Box(X+7,Y,Inner,18),Measure,TEXT("bottom:selection-title")); Y+=20;
		for (const FSelection& S : Selection)
		{
			L.Paint.Rect(X+7,Y,Inner,Border(Scale),TCCss::LineSoft);
			const float Top=Y+5, NameX=X+7+48+8, NameW=Inner-48-8;
			L.Paint.Rect(X+7,Top,48,36,TCCss::Rgba(0,0,0,.22f));
			Icon(L,S.Type,X+7+10,Top+4,28,TCCss::Dim);
			const FString Count=FString::Printf(TEXT("×%d"),S.Count);
			const float CountW=Measure(Count,14,true,0);
			float NameY=Top;
			NameY+=Wrapped(L,TC_T(S.Name),NameX,NameY,NameW-CountW-4,14,true,TCCss::Text,Measure,TEXT("bottom:selection-name"));
			Text(L,Count,X+W-7,Top,14,true,TCCss::Dim,Box(X+W-7-CountW,Top,CountW,18.9f),Measure,TEXT("bottom:selection-count"),ETCPaintAlign::Right);
			FString State=TC_T(FString::Printf(TEXT("%d/%d hp"),S.Hp,S.MaxHp));
			for (const FString& Tag : S.Tags) State+=TEXT("  ")+TC_T(Tag);
			NameY+=Wrapped(L,State,NameX,NameY,NameW,13,false,TCCss::Dim,Measure,TEXT("bottom:selection-health"));
			Y=FMath::Max(Top+36,NameY)+5;
		}
		Y+=6;
		L.Selection=Box(X,0,W,Y);
		FTCPaint Foreground=MoveTemp(L.Paint); L.Paint=FTCPaint(); Panel(L,L.Selection,TEXT("bottom:selection"),Scale); L.Paint.Append(Foreground);
		Y+=6;
	}
	if (Orders.Num() || Menu.Num() || Details.Num())
	{
		const float Top=Y;
		Y+=8;
		Text(L,TC_T(FString(TEXT("Orders"))),X+7,Y,13,false,TCCss::Dim,Box(X+7,Y,Inner,18),Measure,TEXT("bottom:orders-title")); Y+=20;
		Y=Grid(L,Orders,X+7,Y,Inner,Measure,Scale);
		for (const FString& Detail : Details) { Y+=6; Y+=Wrapped(L,TC_T(Detail),X+7,Y,Inner,13,false,TCCss::Dim,Measure,TEXT("bottom:detail")); }
		if (Menu.Num()) { Y+=8; Y=Grid(L,Menu,X+7,Y,Inner,Measure,Scale); }
		Y+=7;
		L.Orders=Box(X,Top,W,Y-Top);
		FTCPaint Foreground=MoveTemp(L.Paint); L.Paint=FTCPaint(); Panel(L,L.Orders,TEXT("bottom:orders"),Scale); L.Paint.Append(Foreground);
	}
	Shift(L,FVector2D(0,ViewH-ScreenGap-Y));
	return L;
}

FTCHudFit::FBottomLayout FTCHudFit::RecruitPanel(float ViewW, float ViewH, float Right, const TArray<FString>& Headings,
	const TArray<FControl>& Tabs, const TArray<TArray<FControl>>& Lists, const TArray<FControl>& Above, const FTCMeasure& Measure, float Scale,
	const TArray<FString>& Logistics, int32 TotalCards)
{
	using namespace TCHudBottom;
	FBottomLayout L;
	const float Left=LeftW+2*ScreenGap, MaxW=FMath::Max(60.f,Right-Left), Inner=MaxW-16;
	float Y=8;
	for (const FString& H : Headings) { Y+=Wrapped(L,TC_T(H),8,Y,Inner,13,true,TCCss::Text,Measure,TEXT("bottom:recruit-heading")); Y+=6; }
	float X=8, RowH=0, Widest=0;
	for (const FControl& C : Tabs)
	{
		const FString Shown=TC_T(C.Label);
		const float IW=Measure(Shown,13,true,0)+20+(C.Icon.IsEmpty()?0:22);
		const float CW=FMath::Min(Inner,IW);
		const TArray<FString> Lines=FTCPaint::Wrap(Shown,CW-20-(C.Icon.IsEmpty()?0:22),13,true,Measure);
		const float H=Lines.Num()*17.55f+12;
		if (X>8 && X+CW>MaxW-8+.01f) { Y+=RowH+4; X=8; RowH=0; }
		const FBox2D B=Box(X,Y,CW,H);
		L.Paint.Rect(X,Y,CW,H,C.bOn?TCCss::Rgba(214,178,94,.1f):TCCss::Hex(0x24282b),TEXT("bottom:tab"));
		L.Paint.Frame(X,Y,CW,H,C.bOn?TCCss::Brass:TCCss::LineSoft,Border(Scale));
		Icon(L,C.Icon,X+10,Y+(H-17)/2,17,TCCss::Dim);
		float Ty=Y+6;
		for (const FString& Line : Lines) { Text(L,Line,X+10+(C.Icon.IsEmpty()?0:22),Ty,13,true,C.bOn?TCCss::Brass:TCCss::Dim,B,Measure,TEXT("bottom:tab-label")); Ty+=17.55f; }
		L.Hits.Add({B,C}); RowH=FMath::Max(RowH,H); Widest=FMath::Max(Widest,X+CW+8); X+=CW+4;
	}
	if (Tabs.Num()) Y+=RowH+4;
	for (const TArray<FControl>& Cards : Lists)
	{
		if (!Cards.Num()) continue;
		// #buy.fit uses the count across every group, including the closed ones, for its card container width.
		const int32 Count=FMath::Max(Cards.Num(),TotalCards);
		const float CW=Tabs.Num()?FMath::Clamp((Inner-(Tabs.Num()-1)*25-(Count-Tabs.Num())*4)/Count,50.f,100.f)
			:FMath::Clamp((Inner-4*(Cards.Num()-1))/Cards.Num(),50.f,100.f);
		const int32 Cols=FMath::Max(1,FMath::FloorToInt((Inner+4)/(CW+4)));
		for (int32 First=0; First<Cards.Num(); First+=Cols)
		{
			const int32 N=FMath::Min(Cols,Cards.Num()-First);
			float H=0;
			for (int32 K=First; K<First+N; K++)
			{
				const int32 Names=(CW>72 || Cards[K].Command.Action!=ETCUiAction::Recruit)?FTCPaint::Wrap(TC_T(Cards[K].Label),CW-6,13,true,Measure).Num():0;
				const int32 Costs=FTCPaint::Wrap(TC_T(Cards[K].Value),CW-6,14,true,Measure).Num();
				H=FMath::Max(H,4.f+FMath::Max(0,Names)*17.55f+CW*.75f+Costs*18.9f+7);
			}
			X=8+(Inner-(N*CW+(N-1)*4))/2;
			for (int32 K=First; K<First+N; K++)
			{
				const FControl& C=Cards[K]; const FBox2D B=Box(X,Y,CW,H);
				L.Paint.Rect(X,Y,CW,H,TCCss::Hex(0x24282b),TEXT("bottom:card")); L.Paint.Frame(X,Y,CW,H,TCCss::LineSoft,Border(Scale));
				float Ty=Y+4;
				// The reference's <=72px container rule keeps the full name in the hover text.
				if (CW>72 || C.Command.Action!=ETCUiAction::Recruit) Ty+=Wrapped(L,TC_T(C.Label),X+3,Ty,CW-6,13,true,C.bEnabled?TCCss::Text:TCCss::Dim,Measure,TEXT("bottom:card-name"));
				L.Paint.Rect(X+1,Ty,CW-2,CW*.75f,TCCss::Rgba(0,0,0,.22f));
				Icon(L,C.Icon,X+CW*.185f,Ty+CW*.06f,CW*.63f,C.bEnabled?TCCss::Text:TCCss::Dim);
				Ty+=CW*.75f+3;
				Wrapped(L,TC_T(C.Value),X+3,Ty,CW-6,14,true,TCCss::Text,Measure,TEXT("bottom:card-cost"));
				L.Hits.Add({B,C}); Widest=FMath::Max(Widest,X+CW+8); X+=CW+4;
			}
			Y+=H+6;
		}
	}
	if (Logistics.Num())
	{
		Y+=4;
		const float Column=(Inner-8)/2;
		float DetailX=8, DetailRowH=0;
		for (int32 I=0; I<Logistics.Num(); I++)
		{
			const FString& Detail=Logistics[I];
			if (I==0 || Detail.Contains(TEXT("#")))
			{
				if (DetailX>8) { Y+=DetailRowH+3; DetailX=8; DetailRowH=0; }
				float H=Wrapped(L,TC_T(Detail),8,Y,Column,13,true,TCCss::Dim,Measure,TEXT("bottom:logistics"));
				if (I==0 && Logistics.IsValidIndex(I+1) && Logistics[I+1].Contains(TEXT("#")))
					H=FMath::Max(H,Wrapped(L,TC_T(Logistics[++I]),16+Column,Y,Column,13,true,TCCss::Dim,Measure,TEXT("bottom:logistics")));
				Y+=H+3;
				continue;
			}
			const float DetailWidth=FMath::Min(Inner,Measure(TC_T(Detail),13,false,0));
			if (DetailX>8 && DetailX+DetailWidth>MaxW-8) { Y+=DetailRowH+3; DetailX=8; DetailRowH=0; }
			const float H=Wrapped(L,TC_T(Detail),DetailX,Y,DetailWidth,13,false,TCCss::Dim,Measure,TEXT("bottom:logistics"));
			DetailRowH=FMath::Max(DetailRowH,H);
			DetailX+=DetailWidth+8;
		}
		if (DetailX>8) Y+=DetailRowH+3;
		Widest=MaxW;
	}
	Y+=2;
	float AboveW=16;
	for (const FControl& C : Above) AboveW+=Measure(TC_T(C.Label),13,false,0)+26;
	const float W=FMath::Min(MaxW,FMath::Max(Widest,FMath::Max(AboveW,80.f)));
	L.Recruit=Box(0,0,W,Y);
	FTCPaint Foreground=MoveTemp(L.Paint); L.Paint=FTCPaint(); Panel(L,L.Recruit,TEXT("bottom:recruit"),Scale); L.Paint.Append(Foreground);
	float AX=8;
	for (int32 I=0; I<Above.Num(); I++)
	{
		const FControl& C=Above[I]; const FString Shown=TC_T(C.Label);
		const float AW=Measure(Shown,13,false,0)+18, AH=27.55f;
		const float At=I==Above.Num()-1 && I>0?FMath::Max(AX,W-8-AW):AX;
		const FBox2D B=Box(At,-6-AH,AW,AH);
		L.Paint.Rect(B.Min.X,B.Min.Y,AW,AH,TCCss::PanelSolid,TEXT("bottom:above")); L.Paint.Frame(B.Min.X,B.Min.Y,AW,AH,TCCss::LineSoft,Border(Scale));
		Text(L,Shown,At+9,B.Min.Y+5,13,false,TCCss::Dim,B,Measure,TEXT("bottom:above-label"));
		if (C.Command.Action!=ETCUiAction::None) L.Hits.Add({B,C});
		L.Paint.Panels.Add(B);
		AX=At+AW+8;
	}
	Shift(L,FVector2D(Left+(MaxW-W)/2,ViewH-ScreenGap-Y));
	return L;
}

FTCHudFit::FBottomLayout FTCHudFit::EconomyPanel(float ViewW, const FString& Money, const FString& Income, const FString& Population,
	const TArray<FControl>& Support, const FTCMeasure& Measure, float Scale)
{
	using namespace TCHudBottom;
	FBottomLayout L;
	const FString Second=TC_T(Income)+TEXT("   ")+TC_T(Population);
	const float W=FMath::Max(238.f,FMath::Max(Measure(TC_T(Money),22,false,0),Measure(Second,13,false,0))+18), X=ViewW-ScreenGap-W;
	float Y=8;
	Text(L,TC_T(Money),W-9,Y,22,false,TCCss::Brass,Box(8,Y,W-16,29.7f),Measure,TEXT("bottom:money"),ETCPaintAlign::Right); Y+=31;
	Text(L,Second,W-9,Y,13,false,TCCss::Dim,Box(8,Y,W-16,17.55f),Measure,TEXT("bottom:population"),ETCPaintAlign::Right); Y+=23;
	Y=Grid(L,Support,W-9-225,Y,225,Measure,Scale)+8;
	L.Economy=Box(0,0,W,Y);
	FTCPaint Foreground=MoveTemp(L.Paint); L.Paint=FTCPaint(); Panel(L,L.Economy,TEXT("bottom:economy"),Scale); L.Paint.Append(Foreground);
	Shift(L,FVector2D(X,ScreenGap)); return L;
}

FTCHudFit::FBottomLayout FTCHudFit::Toolbar(float MaxW, const TArray<FControl>& Controls, const FTCMeasure& Measure, float Scale)
{
	using namespace TCHudBottom;
	FBottomLayout L;
	float X=6,Y=6,H=0,W=0;
	for (const FControl& C : Controls)
	{
		const FString Shown=TC_T(C.Label);
		const float CW=FMath::Min(MaxW-12,Measure(Shown,13,false,0)+16);
		const TArray<FString> Lines=FTCPaint::Wrap(Shown,CW-16,13,false,Measure);
		const float CH=Lines.Num()*17.55f+10;
		if (X>6 && X+CW>MaxW-6) { X=6; Y+=H+4; H=0; }
		const FBox2D B=Box(X,Y,CW,CH);
		L.Paint.Rect(X,Y,CW,CH,TCCss::Hex(0x24282b)); L.Paint.Frame(X,Y,CW,CH,TCCss::LineSoft,Border(Scale));
		float TY=Y+5;
		for (const FString& LineText : Lines) { Text(L,LineText,X+8,TY,13,false,TCCss::Text,B,Measure,TEXT("bottom:toolbar-label")); TY+=17.55f; }
		if (C.Command.Action!=ETCUiAction::None) L.Hits.Add({B,C});
		W=FMath::Max(W,X+CW+6); X+=CW+4; H=FMath::Max(H,CH);
	}
	if (Controls.Num())
	{
		FTCPaint Foreground=MoveTemp(L.Paint); L.Paint=FTCPaint(); Panel(L,Box(0,0,W,Y+H+6),TEXT("bottom:toolbar"),Scale); L.Paint.Append(Foreground);
	}
	return L;
}

FTCPaint FTCHudFit::BossCard(const FTCBossBar& Boss, FVector2D View, float Top, const FBox2D& Economy,
	const TArray<FBox2D>& Avoid, double Now, const FTCMeasure& Measure, float Scale)
{
	using namespace TCHudBottom;
	auto Clear=[&](const FTCPaint& P)
	{
		for (const FBox2D& B : P.Panels)
		{
			if (B.Min.X<8 || B.Max.X>View.X-8 || B.Min.Y<8 || B.Max.Y>View.Y-8) return false;
			for (const FBox2D& A : Avoid) if (Overlap(B,A)) return false;
			// #roomBanners follows the central top stack. Reserve its ordinary paused row too.
			if (B.Min.X<View.X/2 && B.Max.X>View.X/2)
			{
				const FBox2D Pause=Box(View.X/2-65,B.Max.Y+8/Scale,130,33/Scale);
				for (const FBox2D& A : Avoid) if (Overlap(Pause,A)) return false;
			}
		}
		return true;
	};
	FTCPaint P=Boss.Layout(View.X/2,Top,View.X,Now,Measure,nullptr,Scale);
	if (Clear(P)) return P;
	const float Left=LeftW+2*ScreenGap, Right=View.X-184-2*ScreenGap;
	P=Boss.Layout((Left+Right)/2,Top,Right-Left+32,Now,Measure,nullptr,Scale);
	if (Clear(P)) return P;
	if (Economy.bIsValid)
	{
		P=Boss.Layout(Economy.GetCenter().X,Economy.Max.Y+8,Economy.GetSize().X+32,Now,Measure,nullptr,Scale);
		if (Clear(P)) return P;
	}
	// The side dock is also clear of the command card when that card is opened after the first frame.
	return P;
}

FTCHudFit::FBottomLayout FTCHudFit::SandboxToolbar(FVector2D View, float CornerRight, float ScoreLeft,
	const TArray<FControl>& Controls, const TArray<FBox2D>& Avoid, const FTCMeasure& Measure, float Scale)
{
	using namespace TCHudBottom;
	FBottomLayout L=Toolbar(View.X-16,Controls,Measure,Scale);
	if (L.Paint.Panels.IsEmpty()) return L;
	const FVector2D Size=L.Paint.Panels[0].GetSize();
	FBox2D Spot=Box((View.X-Size.X)/2,View.Y-SandboxBottom-Size.Y,Size.X,Size.Y);
	const bool Clear=Spot.Min.Y>=8 && !Avoid.ContainsByPredicate([&](const FBox2D& B) { return Overlap(Spot,B); });
	if (!Clear)
	{
		const float X=CornerRight+8, Right=ScoreLeft-4;
		L=Toolbar(FMath::Max(100.f,Right-X),Controls,Measure,Scale);
		Spot=Box(X,8,L.Paint.Panels[0].GetSize().X,L.Paint.Panels[0].GetSize().Y);
	}
	Shift(L,Spot.Min);
	return L;
}

TArray<FString> FTCHudFit::LayoutErrors(const FBottomLayout& L, FVector2D View)
{
	using namespace TCHudBottom;
	TArray<FString> Errors;
	auto Inside=[](const FBox2D& A,const FBox2D& B) { return A.Min.X>=B.Min.X-.1 && A.Min.Y>=B.Min.Y-.1 && A.Max.X<=B.Max.X+.1 && A.Max.Y<=B.Max.Y+.1; };
	const FBox2D Screen=Box(0,0,View.X,View.Y);
	for (const FTextFit& T : L.TextBounds) if (!Inside(T.Box,T.Clip) || !Inside(T.Box,Screen)) Errors.Add(TEXT("text outside its box: ")+T.Text);
	for (int32 I=0; I<L.TextBounds.Num(); I++)
	{
		const FTextFit& T=L.TextBounds[I];
		for (int32 J=I+1; J<L.TextBounds.Num(); J++) if (Overlap(T.Box,L.TextBounds[J].Box)) Errors.Add(TEXT("text overlaps: ")+T.Text+TEXT(" / ")+L.TextBounds[J].Text);
		for (const FHit& H : L.Hits) if (Overlap(T.Box,H.Box) && !Inside(T.Box,H.Box)) Errors.Add(TEXT("text crosses a control: ")+T.Text+TEXT(" / ")+H.Control.Label);
		for (const FBox2D& P : L.Paint.Panels) if (Overlap(T.Box,P) && !Inside(T.Box,P)) Errors.Add(TEXT("text crosses a panel: ")+T.Text);
	}
	for (int32 I=0; I<L.Hits.Num(); I++)
	{
		if (!Inside(L.Hits[I].Box,Screen)) Errors.Add(TEXT("control outside screen: ")+L.Hits[I].Control.Label);
		for (int32 J=I+1; J<L.Hits.Num(); J++) if (Overlap(L.Hits[I].Box,L.Hits[J].Box)) Errors.Add(TEXT("controls overlap: ")+L.Hits[I].Control.Label+TEXT(" / ")+L.Hits[J].Control.Label);
	}
	for (int32 I=0; I<L.Paint.Panels.Num(); I++)
	{
		if (!Inside(L.Paint.Panels[I],Screen)) Errors.Add(TEXT("panel outside screen"));
		for (int32 J=I+1; J<L.Paint.Panels.Num(); J++) if (Overlap(L.Paint.Panels[I],L.Paint.Panels[J])) Errors.Add(FString::Printf(TEXT("panels overlap: [%g,%g,%g,%g] / [%g,%g,%g,%g]"),L.Paint.Panels[I].Min.X,L.Paint.Panels[I].Min.Y,L.Paint.Panels[I].Max.X,L.Paint.Panels[I].Max.Y,L.Paint.Panels[J].Min.X,L.Paint.Panels[J].Min.Y,L.Paint.Panels[J].Max.X,L.Paint.Panels[J].Max.Y));
	}
	return Errors;
}

namespace
{
	constexpr double OrderNoteSeconds = 4.0, MarkerSeconds = 0.6;
	constexpr float Line = 16.f, Pad = 6.f, Gap = 8.f;
	const FLinearColor Text(1.f, 1.f, 1.f), Dim(0.75f, 0.78f, 0.72f), Warn(1.f, 0.6f, 0.2f), Good(0.75f, 0.9f, 1.f), Foe(1.f, 0.45f, 0.35f);
	const FLinearColor Panel(0.f, 0.f, 0.f, 0.55f), ButtonFill(0.18f, 0.24f, 0.16f, 0.9f);
	// recruit cards: buyable, buyable but the browser's 'broke' look cannot happen then, refused, refused for lack of MP
	// or fuel ('broke'), and the open group's tab
	const FLinearColor CardFill(0.20f, 0.27f, 0.17f, 0.92f), CardOff(0.16f, 0.16f, 0.16f, 0.85f), CardBroke(0.30f, 0.14f, 0.10f, 0.85f), TabOpen(0.32f, 0.40f, 0.22f, 0.95f);

	FString SeatName(const FTCMatchState& S, int32 Owner)
	{
		return S.Names.IsValidIndex(Owner) && !S.Names[Owner].IsEmpty() ? S.Names[Owner] : FString::Printf(TEXT("seat %d"), Owner);
	}

	// client/hud.js tagsOf: the status tags of a unit's row, from fields the snapshot carries (shared/sim.js UNIT_ROW,
	// unitFlags), in the browser's order and words (each one its own element there, so each is translated by itself:
	// the row joins them with column breaks). The supply indicator is the logistics rows' business here.
	TArray<FString> UnitTags(const FTCUnit& U, const FTCUnitDef* D)
	{
		TArray<FString> T;
		if (U.Flags & 256) T.Add(TEXT("Hidden"));
		if (U.Flags & TCFlags::Riding) T.Add(TEXT("Riding"));
		if (U.Flags & TCCargo::Flag)
		{
			const int32 N = TCCargo::Count(U.Flags), Of = D ? D->Carries : 0;
			T.Add(Of > 1 && Of <= 3 ? FString::Printf(TEXT("Carrying %d of %d squads"), N, Of) : FString(TEXT("Carrying a squad")));
		}
		if (U.Flags & TCFlags::Retreating) T.Add(TEXT("Retreating"));
		if (U.Flags & 8) T.Add(TEXT("Reinforcing"));
		if (U.Flags & 2) T.Add(TEXT("Suppressive fire"));
		if (U.Flags & 4) T.Add(TEXT("AP round loaded"));
		if (U.Supp >= 90) T.Add(TEXT("Pinned")); else if (U.Supp >= 50) T.Add(TEXT("Suppressed"));
		if (!(U.Flags & TCFlags::Garrisoned)) { if (U.Cover == 2) T.Add(TEXT("In trench")); else if (U.Cover == 3) T.Add(TEXT("By cover")); else if (U.Cover) T.Add(TEXT("In cover")); }
		if (U.Flags & 16) T.Add(TEXT("Digging")); else if (U.Flags & 1024) T.Add(TEXT("Waiting for MP to dig"));
		if (U.Flags & TCFlags::Garrisoned)
		{
			// shared/sim.js unitFlags bits 20-22: the CFG.houses type of the house it holds
			const TCWorldCore::FHouseType* H = FTCWorldObjects::GarrisonHouseType(U.Flags);
			T.Add(FString::Printf(TEXT("Garrisoned: %s"), H ? ANSI_TO_TCHAR(H->Name) : TEXT("?")));
		}
		if (U.Flags & 64) T.Add(TEXT("Attack-move"));
		// hud.js STANCE: hold fire, hold position, auto-retreat
		if (U.Flags & 2048) T.Add(TEXT("Hold fire"));
		if (U.Flags & 4096) T.Add(TEXT("Hold position"));
		if (U.Flags & 8192) T.Add(TEXT("Auto-retreat"));
		if (D && D->bBuilding && U.Built < 1.f) T.Add(FString::Printf(TEXT("Building %d%%"), FMath::RoundToInt(U.Built * 100.f)));
		// the veteran mark: the browser draws stars with the title "Veteran: n star(s)"
		if (U.Stars > 0) T.Add(U.Stars == 1 ? FString(TEXT("Veteran: 1 star")) : FString::Printf(TEXT("Veteran: %d stars"), U.Stars));
		return T;
	}

	// JavaScript's String(n) for the army numbers (0.75 per conscript squad, whole numbers otherwise)
	FString Js(double V) { return V == FMath::FloorToDouble(V) ? FString::Printf(TEXT("%.0f"), V) : FString::SanitizeFloat(V); }

	bool SameTarget(const FTCUiCommand& A, const FTCUiCommand& B)
	{
		if (A.Action != B.Action) return false;
		switch (A.Action)
		{
		case ETCUiAction::Recruit: return A.Unit == B.Unit;
		case ETCUiAction::CancelJob: return A.Building == B.Building && A.Job == B.Job;
		case ETCUiAction::RecruitGroup: return A.Group == B.Group;
		case ETCUiAction::Build: case ETCUiAction::Stance: case ETCUiAction::Ability: case ETCUiAction::Autocast:
		case ETCUiAction::Fort: case ETCUiAction::Entrench: case ETCUiAction::Formation: case ETCUiAction::Support: return A.Unit == B.Unit;
		case ETCUiAction::LobbySetting: case ETCUiAction::MatchOption: case ETCUiAction::MatchControl: return A.Unit == B.Unit && A.Group == B.Group;
		case ETCUiAction::LoadSave: case ETCUiAction::Research: return A.Unit == B.Unit;
		// round 16 (controls): the alert feed's hit boxes carry their tag (a line, the summary, Previous / Next, an entry)
		case ETCUiAction::Alert: return B.Unit.IsEmpty() || A.Unit == B.Unit;
		default: return true;
		}
	}
}

void ATCHUD::EndPlay(const EEndPlayReason::Type Reason)
{
	Minimap.Reset(); // releases the texture with the HUD
	Super::EndPlay(Reason);
}

void ATCHUD::DrawLabel(const FString& Str, const FLinearColor& Colour, float X, float Y, UFont* Font, float Scale)
{
	const FString Shown = TC_T(Str);
	NoteKept(Str, Shown);
	DrawShown(Shown, Colour, X, Y, Font, Scale);
}

// index.html's body is 500 14px Barlow Semi Condensed; larger native headings keep their relative size.
namespace
{
	float HudFontPx(UFont* Font, float Scale)
	{
		return (GEngine && Font == GEngine->GetMediumFont() ? 18.f : 14.f) * Scale;
	}
}

void ATCHUD::DrawShown(const FString& Str, const FLinearColor& Colour, float X, float Y, UFont* Font, float Scale)
{
	if (!FTCType::IsLoaded()) { AHUD::DrawText(Str, Colour, X, Y, Font, Scale); return; }
	FTCPaint P;
	P.Text(Str, X, Y, HudFontPx(Font, Scale), Colour);
	FTCType::Draw(P, Canvas, Font, UiNow);
}

void ATCHUD::ShownSize(const FString& Str, float& OutWidth, float& OutHeight, UFont* Font, float Scale) const
{
	if (!FTCType::IsLoaded()) { AHUD::GetTextSize(Str, OutWidth, OutHeight, Font, Scale); return; }
	const float Px = HudFontPx(Font, Scale);
	OutWidth = FTCType::Face(false)->Width(Str, Px);
	OutHeight = 1.2f * Px;
}

void ATCHUD::NoteKept(const FString& Str, const FString& Shown)
{
	if (Shown == Str && FTCLocale::Language() == ETCLanguage::Spanish && KeptEnglish.Num() < 400)
	{
		// one log line per text shape: runs of digits count as one (a status line's tick, a cost, a count)
		FString Shape;
		bool bLetter = false;
		for (int32 I = 0; I < Str.Len(); I++)
		{
			const TCHAR C = Str[I];
			bLetter |= (C >= 'a' && C <= 'z') || (C >= 'A' && C <= 'Z');
			if (C >= '0' && C <= '9') { if (Shape.IsEmpty() || Shape[Shape.Len() - 1] != '#') Shape.AppendChar('#'); }
			else Shape.AppendChar(C);
		}
		if (bLetter && !KeptEnglish.Contains(Shape)) { KeptEnglish.Add(Shape); UE_LOG(LogTC, Log, TEXT("locale: kept in English (%d so far): '%s'"), KeptEnglish.Num(), *Str); }
	}
}

FString ATCHUD::FitShown(const FString& Shown, float MaxWidth, UFont* Font) const
{
	float TW = 0, TH = 0;
	ShownSize(Shown, TW, TH, Font, 1.f);
	if (TW <= MaxWidth || Shown.Len() < 2) return Shown;
	// the longest head that fits with '...' after it (binary search on the length)
	int32 Lo = 0, Hi = Shown.Len();
	while (Lo < Hi)
	{
		const int32 Mid = (Lo + Hi + 1) / 2;
		ShownSize(Shown.Left(Mid).TrimEnd() + TEXT("..."), TW, TH, Font, 1.f);
		if (TW <= MaxWidth) Lo = Mid; else Hi = Mid - 1;
	}
	return Shown.Left(Lo).TrimEnd() + TEXT("...");
}

TArray<FString> ATCHUD::WrapShown(const FString& Shown, float MaxWidth, UFont* Font) const
{
	// the line's own columns first (' | ', and the HUD's '  |  '), packed greedily; a column still too wide is broken
	// at its spaces, and a single word too wide is cut
	auto Width = [&](const FString& T) { float TW = 0, TH = 0; ShownSize(T, TW, TH, Font, 1.f); return TW; };
	if (Width(Shown) <= MaxWidth) return { Shown };
	TArray<FString> Parts;
	Shown.ParseIntoArray(Parts, TEXT(" | "), true);
	TArray<FString> Out;
	FString Cur;
	auto Push = [&](const FString& Piece, const TCHAR* Sep)
	{
		const FString Try = Cur.IsEmpty() ? Piece : Cur + Sep + Piece;
		if (Cur.IsEmpty() || Width(Try) <= MaxWidth) { Cur = Try; return; }
		Out.Add(Cur);
		Cur = Piece;
	};
	for (FString P : Parts)
	{
		P.TrimStartAndEndInline();
		if (Width(P) <= MaxWidth) { Push(P, TEXT(" | ")); continue; }
		TArray<FString> Words;
		P.ParseIntoArray(Words, TEXT(" "), true);
		bool bFirst = true;
		for (const FString& W : Words) { Push(Width(W) <= MaxWidth ? W : FitShown(W, MaxWidth, Font), bFirst ? TEXT(" | ") : TEXT(" ")); bFirst = false; }
	}
	if (!Cur.IsEmpty()) Out.Add(Cur);
	return Out;
}

// ---------------------------------------------------------------------------------------------------------------------
// Round 16 (hudfit): the fit rules (TCHUD.h FTCHudFit) and the match controls where the browser has them

FString FTCHudFit::HordeRecordText(int32 Defenders, bool bBest, int32 Wave, int32 Time, const TArray<FString>& Names)
{
	// main.js: (best ? ` Record here with ${n} defender${n === 1 ? '' : 's'}: wave ${best.wave} in ${mins(best.time)}
	// (${best.names.join(', ')}).` : ` No record yet here with ${n} defender${n === 1 ? '' : 's'}.`), mins: m:ss
	const TCHAR* Plural = Defenders == 1 ? TEXT("") : TEXT("s");
	if (!bBest) return FString::Printf(TEXT(" No record yet here with %d defender%s."), Defenders, Plural);
	return FString::Printf(TEXT(" Record here with %d defender%s: wave %d in %d:%02d (%s)."), Defenders, Plural, Wave, Time / 60, Time % 60, *FString::Join(Names, TEXT(", ")));
}

FString FTCHudFit::ConnectionBannerText(bool bFullRoom, int32 SecondsLeft)
{
	// main.js: reason === 'full' ? `Trying this room again in ${left} s` : `Connection lost. Retrying in ${left} s`
	return bFullRoom ? FString::Printf(TEXT("Trying this room again in %d s"), SecondsLeft) : FString::Printf(TEXT("Connection lost. Retrying in %d s"), SecondsLeft);
}

FString FTCHudFit::HordeRecordText(const FTCMatchState& S)
{
#if defined(TC_HAS_LOBBY_HORDE_BEST)
	// main.js: n = m.players.length
	return HordeRecordText(S.Roster.Num(), S.LobbyHordeBest.bSet, S.LobbyHordeBest.Wave, S.LobbyHordeBest.Time, S.LobbyHordeBest.Names);
#else
	return FString(); // without the lobby's hordeBest (lead patch round-17/hudui/lead-patch/horde-record.patch): no line
#endif
}

FTCUiCommand ATCHUD::ControlCommand(const FString& Key, int32 Slot)
{
	static const TPair<const TCHAR*, ETCUiAction> Settings[] = {
		{ TEXT("Setting.Graphics"), ETCUiAction::Graphics }, { TEXT("Setting.Gore"), ETCUiAction::Gore }, { TEXT("Setting.EdgeScroll"), ETCUiAction::EdgeScroll },
		{ TEXT("Setting.PanSpeed"), ETCUiAction::PanSpeed }, { TEXT("Setting.Cover"), ETCUiAction::CoverToggle }, { TEXT("Setting.Volume"), ETCUiAction::Volume },
		{ TEXT("Setting.UiScale"), ETCUiAction::UiScale }, { TEXT("Setting.Language"), ETCUiAction::Language } };
	if (Key.StartsWith(TEXT("Setting.")))
		for (const auto& P : Settings) if (Key == P.Key) return FTCUiCommand(P.Value);
	FTCUiCommand C(ETCUiAction::MatchControl);
	C.Unit = Key;
	C.Group = Slot;
	return C;
}

TArray<FTCControlRow> ATCHUD::MenuSettingRows() const
{
	// client/index.html #menu after the session buttons: light.js puts Graphics and Gore before Leave (SplitMatchControls
	// places them there); then Edge scroll, Pan speed, cover-preview.js's switch (before the first .vol), Volume and the
	// UI scale. The browser's Capture mouse and Stats overlay buttons have no native switch here. The language picker
	// is the page header's (#langSel), which a native match has nowhere else: it closes the list.
	TArray<FTCControlRow> Rows;
	const ATCGameMode* M = GetWorld() ? GetWorld()->GetAuthGameMode<ATCGameMode>() : nullptr;
	const ATCPlayerController* PC = Cast<ATCPlayerController>(GetOwningPlayerController());
	if (!M || !PC) return Rows;
	auto Add = [&](const FString& Label, const TCHAR* Key, const FString& Hover, bool bOn)
	{
		FTCControlRow R;
		FTCControlItem I; I.Label = Label; I.Key = Key; I.Hover = Hover; I.bOn = bOn;
		R.Items.Add(I);
		Rows.Add(R);
	};
	Add(FTCGraphics::MenuLabel(FTCGraphics::Get().Level()), TEXT("Setting.Graphics"), FTCGraphics::MenuTip(), FTCGraphics::Get().IsLow());
	Add(FTCGraphics::GoreLabel(FTCGraphics::Get().Gore()), TEXT("Setting.Gore"), FTCGraphics::GoreTip(), false);
	if (!M->State.bSpectator)
	{
		// client/camera.js preferences, the menu's Edge scroll and Pan speed (saved)
		Add(PC->EdgeScrollLabel(), TEXT("Setting.EdgeScroll"), TEXT("Edge scroll: the view pans when the pointer is at the window's edge"), PC->EdgeScrollOn());
		Add(PC->PanSpeedLabel(), TEXT("Setting.PanSpeed"), TEXT("Pan speed: Slow, Normal or Fast for the keys and the edges"), false);
		Add(FTCCoverPreview::MenuLabel(M->Cover.IsEnabled()), TEXT("Setting.Cover"), FTCCoverPreview::MenuTip(), M->Cover.IsEnabled());
	}
	Add(PC->VolumeLabel(), TEXT("Setting.Volume"), TEXT("Effects, voices and alerts. M mutes (right click: quieter)"), false);
	Add(PC->UiScaleLabel(), TEXT("Setting.UiScale"), FTCSettingsStore::UIScaleTip(), false);
	Add(PC->LanguageLabel(), TEXT("Setting.Language"), TEXT("Language"), false);
	return Rows;
}

void ATCHUD::SplitMatchControls(TArray<FTCControlRow>& Corner, TArray<FTCControlRow>& Sandbox) const
{
	// ATCGameMode::MatchControlRows (Client/TCMatchControls.cpp) in the browser's places: the Menu button and the open
	// menu in the corner, with the settings in it (Graphics and Gore before Leave, the rest after it); the sandbox row
	// apart (#sandbox); Send next wave left out (DrawScoreStrip draws it in the Horde lead block). Rows this does not
	// know stay in the corner.
	Corner.Reset(); Sandbox.Reset();
	const ATCGameMode* M = GetWorld() ? GetWorld()->GetAuthGameMode<ATCGameMode>() : nullptr;
	if (!M) return;
	auto Has = [](const FTCControlRow& R, const TCHAR* Key) { for (const FTCControlItem& I : R.Items) if (I.Key == Key) return true; return false; };
	TArray<FTCControlRow> Settings = bSettingsInMenu && M->Controls.MenuOpen() ? MenuSettingRows() : TArray<FTCControlRow>();
	const int32 BeforeLeave = FMath::Min(2, Settings.Num());
	bool bPlaced = Settings.Num() == 0;
	for (const FTCControlRow& R : M->MatchControlRows())
	{
		if (Has(R, TEXT("NextWave"))) continue;
		if (Has(R, TEXT("SandboxUnit")) || Has(R, TEXT("SandboxSide")) || Has(R, TEXT("SandboxSpawn"))) { Sandbox.Add(R); continue; }
		if (!bPlaced && Has(R, TEXT("Leave")))
		{
			for (int32 K = 0; K < BeforeLeave; K++) Corner.Add(Settings[K]);
			Corner.Add(R);
			for (int32 K = BeforeLeave; K < Settings.Num(); K++) Corner.Add(Settings[K]);
			bPlaced = true;
			continue;
		}
		Corner.Add(R);
	}
	// no Leave (a spectator's menu): the settings after the session buttons
	if (!bPlaced) Corner.Append(Settings);
}

FVector2D ATCHUD::MeasureControlRows(UFont* Font, const TArray<FTCControlRow>& Rows)
{
	// the size DrawControlRows gives its panel at W = 0 (as wide as the widest row), with its hover line
	if (!Rows.Num()) return FVector2D::ZeroVector;
	auto Measure = [&](const FString& Lb) { float TW = 0, TH = 0; LabelSize(Lb, TW, TH, Font, 1.f); return TW + 2 * Pad; };
	float H = 2 * Pad + Line, Wide = 0.f;
	for (const FTCControlRow& R : Rows)
	{
		H += R.Items.Num() ? Line + Pad + 4.f : Line;
		float RW = (R.Text.IsEmpty() ? 0.f : Measure(R.Text)) + 2 * Pad;
		for (const FTCControlItem& It : R.Items) RW += Measure(It.Label) + Pad;
		Wide = FMath::Max(Wide, RW);
	}
	return FVector2D(Wide, H);
}

FVector2D FTCHudFit::InitialCameraBand(float ScreenH, float UiScale, bool bBuyShown)
{
	const float Border = FMath::Max(1.f, FMath::FloorToFloat(UiScale));
	// Empty #scores: 8px top gap, 17px padding, two snapped borders. #buy's tabs: 46px plus four borders.
	return FVector2D(25.f * UiScale + 2 * Border, bBuyShown ? ScreenH - (54.f * UiScale + 4 * Border) : ScreenH);
}

void ATCHUD::LabelSize(const FString& Str, float& OutWidth, float& OutHeight, UFont* Font, float Scale) const
{
	ShownSize(TC_T(Str), OutWidth, OutHeight, Font, Scale);
}

void ATCHUD::DrawMinimapPings()
{
	ATCGameMode* M = GetWorld()->GetAuthGameMode<ATCGameMode>();
	if (!M || !Minimap.Rect().bIsValid || !Canvas) return;
	const double Now = FPlatformTime::Seconds(); // the clock the relay was received on (ATCGameMode::OnPingRelay)
	const FLinearColor Ink(FColor(0x15, 0x16, 0x0f)), Chalk(FColor(0xf2, 0xec, 0xdc)); // pings.js stroke and COLOR (markers.js CHALK)
	for (const FTCPingRing& R : M->AlertFeed.Pings().Rings())
	{
		FTCPingLook L;
		FVector2D Px;
		if (!M->AlertFeed.Pings().LookOf(R, Now, L) || !Minimap.WorldToPixel(FVector2D(R.X, R.Z), Px)) continue;
		constexpr int32 Segs = 24;
		for (int32 Pass = 0; Pass < 2; Pass++)
			for (int32 K = 0; K < Segs; K++)
			{
				const double A0 = 2 * PI * K / Segs, A1 = 2 * PI * (K + 1) / Segs;
				const FVector2D B0 = Px + FVector2D(FMath::Cos(A0), FMath::Sin(A0)) * L.MinimapPx, B1 = Px + FVector2D(FMath::Cos(A1), FMath::Sin(A1)) * L.MinimapPx;
				FLinearColor C = Pass == 0 ? Ink : Chalk;
				C.A = L.MinimapAlpha;
				DrawLine(B0.X, B0.Y, B1.X, B1.Y, C, Pass == 0 ? 4.f : 2.f);
			}
	}
}

float ATCHUD::Button(UFont* Font, float X, float Y, const FString& Label, ETCUiAction Action)
{
	return Button(Font, X, Y, Label, FTCUiCommand(Action), true, ButtonFill);
}

float ATCHUD::Button(UFont* Font, float X, float Y, const FString& Label, const FTCUiCommand& Command, bool bEnabled, const FLinearColor& Fill, float MinWidth, float MaxWidth)
{
	float TW = 0, TH = 0;
	LabelSize(Label, TW, TH, Font, 1.f);
	float BW = FMath::Max(TW + 2 * Pad, MinWidth);
	if (MaxWidth > 0.f) BW = FMath::Min(BW, MaxWidth);
	const FBox2D Rect(FVector2D(X, Y), FVector2D(X + BW, Y + TH + Pad));
	DrawRect(Fill, Rect.Min.X, Rect.Min.Y, Rect.GetSize().X, Rect.GetSize().Y);
	if (TW + 2 * Pad <= BW) DrawLabel(Label, bEnabled ? Text : Dim, X + Pad, Y + Pad / 2, Font, 1.f);
	else
	{
		// round 15: a card narrower than its label (a narrow recruit card) cuts it, as the browser's names ellipsize
		const FString Shown = TC_T(Label);
		NoteKept(Label, Shown);
		DrawShown(FitShown(Shown, BW - 2 * Pad, Font), bEnabled ? Text : Dim, X + Pad, Y + Pad / 2, Font, 1.f);
	}
	Buttons.Add({ ToScreen(Rect), Command, Label, bEnabled });
	return Rect.Max.X + Pad;
}

void ATCHUD::AddPanel(float X, float Y, float W, float H, const FLinearColor& Colour)
{
	DrawRect(Colour, X, Y, W, H);
	Panels.Add(ToScreen(FBox2D(FVector2D(X, Y), FVector2D(X + W, Y + H))));
}

void ATCHUD::BeginUi(float Scale)
{
	if (bUiPushed || !Canvas || !Canvas->Canvas) return;
	UiNow = FMath::Max(0.1f, Scale);
	Canvas->Canvas->PushRelativeTransform(FScaleMatrix(FVector(UiNow, UiNow, 1.f)));
	UiOldClipX = Canvas->ClipX; UiOldClipY = Canvas->ClipY;
	Canvas->ClipX = UiOldClipX / UiNow; Canvas->ClipY = UiOldClipY / UiNow;
	bUiPushed = true;
}

void ATCHUD::EndUi()
{
	if (!bUiPushed) return;
	Canvas->Canvas->PopTransform();
	Canvas->ClipX = UiOldClipX; Canvas->ClipY = UiOldClipY;
	UiNow = 1.f;
	bUiPushed = false;
}

bool ATCHUD::ButtonAt(FVector2D Pixel, ETCUiAction& OutAction) const
{
	FTCUiCommand C;
	if (!CommandAt(Pixel, C)) return false;
	OutAction = C.Action;
	return true;
}

bool ATCHUD::CommandAt(FVector2D Pixel, FTCUiCommand& OutCommand) const
{
	// round 16: the last drawn first, so a panel drawn over another (the open match menu) takes the click
	for (int32 I = Buttons.Num() - 1; I >= 0; I--) if (Buttons[I].Rect.IsInsideOrOn(Pixel)) { OutCommand = Buttons[I].Command; return true; }
	return false;
}

bool ATCHUD::ButtonCentre(ETCUiAction Action, FVector2D& Out) const
{
	for (const FButton& B : Buttons) if (B.Command.Action == Action) { Out = B.Rect.GetCenter(); return true; }
	return false;
}

bool ATCHUD::CommandCentre(const FTCUiCommand& Command, FVector2D& Out) const
{
	for (const FButton& B : Buttons) if (SameTarget(B.Command, Command)) { Out = B.Rect.GetCenter(); return true; }
	return false;
}

bool ATCHUD::PanelAt(FVector2D Pixel) const
{
	for (const FBox2D& P : Panels) if (P.IsInsideOrOn(Pixel)) return true;
	return false;
}

FString ATCHUD::ButtonLabels() const
{
	TArray<FString> L;
	for (const FButton& B : Buttons) L.Add(B.Label);
	return FString::Join(L, TEXT(", "));
}

float ATCHUD::DrawLobby(UFont* Font, float Y, float LobbyW)
{
	// client/index.html #lobby as client/main.js renderLobby fills it: the room line, the settings (FTCLobbySettings:
	// each select is a button that cycles its choices, left click forward, right click back, greyed when this client
	// may not change it), the seats with their team, faction and AI difficulty, the Start button's text and the line
	// under it, and the saves panel (FTCPersistence) with a Load button per row for the host
	ATCGameMode* M = GetWorld()->GetAuthGameMode<ATCGameMode>();
	const FTCMatchState& S = M->State;
	if (S.RoomState.IsEmpty()) return Y; // no lobby yet: the status line says what the connection is doing
	const FTCLobbySettings& L = M->Lobby;
	struct FItem { FString Label; FTCUiCommand Command; bool bEnabled; FString Hover; };
	// MaxW (round 16): a text line's own wrap width (#modeInfo's max-width: 60ch), 0 for the panel's
	struct FLine { FString Text; FLinearColor Colour; TArray<FItem> Items; float MaxW = 0.f; };
	TArray<FLine> Lines;
	const float W = LobbyW;
	auto Measure = [&](const FString& Lb) { float TW = 0, TH = 0; LabelSize(Lb, TW, TH, Font, 1.f); return TW + 2 * Pad; };
	// buttons in rows that wrap at the panel's width, the first row after an optional text
	auto AddButtons = [&](const TArray<FItem>& Items, const FString& Lead = FString(), FLinearColor Colour = FLinearColor::White)
	{
		FLine Cur{ Lead, Colour, {} };
		float X = Lead.IsEmpty() ? 0.f : Measure(Lead);
		for (const FItem& It : Items)
		{
			const float IW = Measure(It.Label);
			if (Cur.Items.Num() && X + IW > W - 2 * Pad) { Lines.Add(Cur); Cur = FLine{ FString(), Text, {} }; X = 0; }
			Cur.Items.Add(It); X += IW + Pad;
		}
		if (Cur.Items.Num() || !Cur.Text.IsEmpty()) Lines.Add(Cur);
	};
	auto ControlItem = [](const FTCLobbyControl& C)
	{
		FTCUiCommand Cmd(ETCUiAction::LobbySetting);
		Cmd.Unit = LexToString(C.Kind);
		Cmd.Group = C.Slot;
		// round 16: a seat's selects (team, faction, AI difficulty) show their choice alone, as client/main.js renderLobby's
		// pick() <select>s in the roster row do ('Team 1', the faction's name, 'Normal'); the room's selects keep the
		// 'Title: ' their page labels give them. The title stays in the hover line.
		const bool bSeat = C.Kind == ETCLobbySetting::Team || C.Kind == ETCLobbySetting::Faction || C.Kind == ETCLobbySetting::Level;
		FString Label = bSeat ? (C.CurrentLabel.IsEmpty() ? FString(TEXT("?")) : C.CurrentLabel) : FString::Printf(TEXT("%s: %s"), *C.Title, C.CurrentLabel.IsEmpty() ? TEXT("?") : *C.CurrentLabel);
		if (C.bPending)
		{
			const FTCLobbyChoice* P = C.Find(C.PendingValue);
			Label += FString::Printf(TEXT(" (asked: %s)"), P ? *P->Label : *C.PendingValue);
		}
		return FItem{ Label, Cmd, C.bEditable, C.bEditable ? FString::Printf(TEXT("%s: left click for the next choice, right click for the previous"), *C.Title) : C.Why };
	};
	const bool bLobby = S.RoomState == TEXT("lobby");
	const FTCLobbyRoom& R = L.Room();
	const FString ModeLabel = L.HasLobby() ? FTCLobbySettings::ModeName(R.Mode) : S.Mode;
	const FString MapLabel = !L.HasLobby() ? S.LobbyMap : R.Mode == TEXT("world") ? FTCLobbySettings::WorldSizeLabel(R.WorldSize) + TEXT(" world") : FTCLobbySettings::PrettyMap(R.MapName);
	Lines.Add({ FString::Printf(TEXT("Lobby  room %s  |  %s  |  %s%s"), *S.RoomCode, *ModeLabel, *MapLabel, S.bLobbySandbox ? TEXT("  |  sandbox") : TEXT("")), Text, {} });
	// client/index.html: the page header's language picker (#langSel, title "Language"), there for everyone
	if (const ATCPlayerController* LPC = Cast<ATCPlayerController>(GetOwningPlayerController()))
		AddButtons({ FItem{ LPC->LanguageLabel(), FTCUiCommand(ETCUiAction::Language), true, TEXT("Language") } });
	// round 14: client/public-lobby.html's Tutorial button (this client has no room directory, CLI-01: it sits in the
	// room's lobby), for anyone outside a match
	if (bLobby && !L.IsRustHost()) AddButtons({ FItem{ TCTutorialEntry::Label(), FTCUiCommand(ETCUiAction::Tutorial), true, TCTutorialEntry::Title() } });
	if (L.HasLobby() && bLobby)
	{
		FString Info = FTCLobbySettings::ModeInfo(R.Mode);
		// round 17: main.js renderLobby appends the Horde record (server.js hordeBest) to #modeInfo; the page translates
		// the text sentence by sentence, so es.js's 'Record here with ...' and 'No record yet here with ...' keys apply
		if (R.Mode == TEXT("horde") && !Info.IsEmpty()) Info += FTCHudFit::HordeRecordText(S);
		// round 16: index.html #modeInfo { max-width: 60ch } in the card's text, wrapped (white-space: normal): 60 times
		// the width of the font's '0', within the panel
		float ZeroW = 0.f, ZeroH = 0.f;
		ShownSize(TEXT("0"), ZeroW, ZeroH, Font, 1.f);
		if (!Info.IsEmpty()) Lines.Add({ Info, Dim, {}, 60.f * ZeroW });
		TArray<FItem> Room;
		for (const FTCLobbyControl& C : L.RoomControls()) if (C.bShown) Room.Add(ControlItem(C));
		// client/index.html: the Weather select and the Sandbox checkbox (FTCMatchOptions), greyed for a guest
		for (const ETCMatchOption K : { ETCMatchOption::Weather, ETCMatchOption::Sandbox })
		{
			const FTCOptionControl C = M->Options.Control(K);
			if (!C.bShown) continue;
			FTCUiCommand Cmd(ETCUiAction::MatchOption);
			Cmd.Unit = LexToString(K);
			FString Label = FString::Printf(TEXT("%s: %s"), *C.Title, C.CurrentLabel.IsEmpty() ? TEXT("?") : *C.CurrentLabel);
			if (C.bPending)
			{
				const FTCLobbyChoice* P = C.Find(C.PendingValue);
				Label += FString::Printf(TEXT(" (asked: %s)"), P ? *P->Label : K == ETCMatchOption::Sandbox ? (C.PendingValue == TEXT("true") ? TEXT("On") : TEXT("Off")) : *C.PendingValue);
			}
			const FString Hover = !C.bEditable ? C.Why : K == ETCMatchOption::Weather ? FString::Printf(TEXT("%s. Left click for the next choice, right click for the previous"), *C.Tooltip) : FString::Printf(TEXT("%s. Click to switch"), *C.Tooltip);
			Room.Add({ Label, Cmd, C.bEditable, Hover });
		}
		AddButtons(Room);
		if (!M->Options.LogisticsLine().IsEmpty()) Lines.Add({ M->Options.LogisticsLine(), Dim, {} });
	}
	for (int32 I = 0; I < S.Roster.Num(); I++)
	{
		const FTCLobbyPlayer& P = S.Roster[I];
		const bool bYou = I == S.LobbySeat && !S.bSpectator;
		// client/main.js renderLobby marks only an offline player ('offline'); AI seats show their level
		const FString Who = P.bAi ? FString(TEXT("  AI")) : P.bConnected ? FString() : FString(TEXT("  offline"));
		const FString Seat = FString::Printf(TEXT("%d. %s%s%s  "), I + 1, *P.Name, bYou ? TEXT(" (you)") : TEXT(""), *Who);
		TArray<FItem> Items;
		if (L.HasLobby() && bLobby && I < R.Seats.Num())
			for (const FTCLobbyControl& C : L.SeatControls(I)) if (C.bShown) Items.Add(ControlItem(C));
		// client/main.js renderLobby: the host's Remove button on an AI seat or an offline player's seat
		const FTCOptionControl Kick = M->Options.Control(ETCMatchOption::Kick, I);
		if (bLobby && Kick.bShown)
		{
			FTCUiCommand Cmd(ETCUiAction::MatchOption);
			Cmd.Unit = LexToString(ETCMatchOption::Kick);
			Cmd.Group = I;
			Items.Add({ Kick.bPending ? FString(TEXT("Remove (asked)")) : Kick.Title, Cmd, Kick.bEditable, Kick.bEditable ? Kick.Tooltip : Kick.Why });
		}
		if (Items.Num()) AddButtons(Items, Seat, bYou ? Good : Text);
		// client/main.js renderLobby: 'Team ' + (k + 1), the teams numbered from 1 (es.js 'Team {0}')
		else Lines.Add({ FString::Printf(TEXT("%sTeam %d%s"), *Seat, P.Team + 1, P.bAi ? *FString::Printf(TEXT("  %s"), *P.Level) : TEXT("")), bYou ? Good : Text, {} });
	}
	if (S.Spectators.Num()) Lines.Add({ TEXT("Watching: ") + FString::Join(S.Spectators, TEXT(", ")), Dim, {} });
	// round 15 (Client/TCMatchControls.cpp): the Name field, Watch as a spectator / Take a seat, the invite link and
	// Copy, the Room box (Join, New) and Browse matches
	for (const FTCControlRow& Row : M->LobbyControlRows())
	{
		TArray<FItem> Items;
		for (const FTCControlItem& It : Row.Items) { FTCUiCommand C(ETCUiAction::MatchControl); C.Unit = It.Key; C.Group = It.Slot; const bool bRustWatch = L.IsRustHost() && It.Key == TEXT("Watch"); Items.Add({ It.Label, C, It.bEnabled && !bRustWatch, bRustWatch ? FString(TEXT("Rust host: no spectators.")) : It.Hover }); }
		const FLinearColor Colour = Row.Tone == FTCControlRow::Dim ? Dim : Row.Tone == FTCControlRow::Warn ? Warn : Row.Tone == FTCControlRow::Good ? Good : Text;
		if (Items.Num()) AddButtons(Items, Row.Text, Colour); else Lines.Add({ Row.Text, Colour, {} });
	}
	if (!S.LastResult.IsEmpty()) Lines.Add({ FString::Printf(TEXT("Last match: %s."), *S.LastResult), Dim, {} });
	const bool bHost = S.bAmHost && !S.bSpectator;
	// round 16: not after a Leave (main.js on left: #lobbyMsg says only "You left the match; ...", the first row here)
	if (S.RoomState == TEXT("play") && !M->Controls.Left()) Lines.Add({ TEXT("A match is running: joining it..."), Dim, {} });
	else if (S.bSpectator) Lines.Add({ TEXT("You are watching this room."), Dim, {} });
	// (a guest's "Waiting for the host to start..." is FTCLobbySettings::StartLine, the browser's #lobbyMsg text)
	if (S.bAmHost && bLobby)
	{
		// the Start button's text and enabled state are the browser's (renderLobby); Add AI as before
		TArray<FItem> Host;
		if (bHost && !L.IsRustHost()) Host.Add({ TEXT("+ Add AI player"), FTCUiCommand(ETCUiAction::AddAi), true, TEXT("+ Add AI player") }); // lobby-view.js
		const bool bStart = !L.HasLobby() || L.StartAllowed();
		Host.Add({ L.HasLobby() ? L.StartLabel() : FString(TEXT("Start match")), FTCUiCommand(ETCUiAction::Start), bStart, bStart ? FString(TEXT("Start the match")) : L.StartLine() });
		AddButtons(Host);
	}
	if (L.HasLobby() && bLobby && !L.StartLine().IsEmpty()) Lines.Add({ L.StartLine(), Warn, {} });
	if (M->Options.InLobby())
	{
		// weather-view.js: the chosen weather explained under the select
		const FTCOptionControl Wc = M->Options.Control(ETCMatchOption::Weather);
		if (Wc.bShown && !Wc.Tooltip.IsEmpty()) Lines.Add({ FString::Printf(TEXT("Weather: %s"), *Wc.Tooltip), Dim, {} });
	}
	if (M->Options.PendingCount()) Lines.Add({ FString::Printf(TEXT("%d option change(s) asked, not shown by a lobby yet"), M->Options.PendingCount()), Dim, {} });
	if (L.PendingCount()) Lines.Add({ FString::Printf(TEXT("%d change(s) asked, not shown by a lobby yet (the server answers a change it takes with a lobby, and ignores one it refuses)"), L.PendingCount()), Dim, {} });
	// the saves panel: rows newest first, a Load button on each for the host
	const FTCSaveListView Saves = M->Persistence.SaveList(S);
	if (Saves.bShown)
	{
		Lines.Add({ Saves.Heading, Text, {} });
		for (const FTCSaveRow& Row : Saves.Rows)
		{
			FTCUiCommand Load(ETCUiAction::LoadSave);
			Load.Unit = Row.Id;
			TArray<FItem> Items;
			if (Saves.bLoadButtons) Items.Add({ Saves.LoadLabel, Load, true, FString::Printf(TEXT("Load %s"), *Row.Id) });
			FLine Ln{ FString::Printf(TEXT("%s  |  %s"), *Row.Title, *Row.Detail), Dim, Items };
			Lines.Add(Ln);
		}
	}
	const FTCPersistenceStatus& St = M->Persistence.Status();
	if (St.Request != ETCPersistenceCommand::None) Lines.Add({ St.Describe(), Dim, {} });

	// round 16: each text line in the language in use, wrapped at the panel's width (or its own MaxW) as the card's
	// text wraps (white-space: normal); before, a long line (Horde's description with its record) ran past the panel
	TArray<FLine> Wrapped;
	TArray<bool> bShownText;
	for (const FLine& Ln : Lines)
	{
		if (Ln.Items.Num()) { Wrapped.Add(Ln); bShownText.Add(false); continue; }
		const FString T = TC_T(Ln.Text);
		NoteKept(Ln.Text, T);
		const float Wrap = Ln.MaxW > 0.f ? FMath::Min(Ln.MaxW, W - 2 * Pad - 6.f) : W - 2 * Pad - 6.f;
		for (const FString& Piece : FTCPaint::Wrap(T, Wrap, 0.f, false, [&](const FString& Q, float, bool, float) { float TW = 0, TH = 0; ShownSize(Q, TW, TH, Font, 1.f); return TW; }))
		{
			Wrapped.Add({ Piece, Ln.Colour, {} });
			bShownText.Add(true);
		}
	}
	Lines = MoveTemp(Wrapped);
	const float RowH = Line + Pad + 4.f;
	float H = 2 * Pad + Line; // the hover line
	for (const FLine& Ln : Lines) H += Ln.Items.Num() ? RowH : Line;
	AddPanel(8.f, Y, W, H, Panel);
	FVector2D Mouse(-1, -1);
	float MX, MY;
	const bool bMouse = GetOwningPlayerController() && GetOwningPlayerController()->GetMousePosition(MX, MY);
	if (bMouse) Mouse = FVector2D(MX, MY) / UiNow; // in the scaled panels' own pixels
	FString Hover;
	float RowY = Y + Pad;
	for (int32 Li = 0; Li < Lines.Num(); Li++)
	{
		const FLine& Ln = Lines[Li];
		if (!Ln.Items.Num()) { if (bShownText[Li]) DrawShown(Ln.Text, Ln.Colour, 14.f, RowY, Font, 1.f); else DrawLabel(Ln.Text, Ln.Colour, 14.f, RowY, Font, 1.f); RowY += Line; continue; }
		float X = 14.f;
		// a lead text before the buttons (a seat's name), or a trailing text after them (a save's row)
		const bool bLead = !Ln.Text.IsEmpty() && Ln.Items[0].Command.Action != ETCUiAction::LoadSave;
		if (bLead) { DrawLabel(Ln.Text, Ln.Colour, X, RowY + 2.f + Pad / 2, Font, 1.f); X += Measure(Ln.Text) - Pad; }
		for (const FItem& It : Ln.Items)
		{
			const float Left = X;
			X = Button(Font, X, RowY + 2.f, It.Label, It.Command, It.bEnabled, It.bEnabled ? ButtonFill : CardOff);
			if (bMouse && FBox2D(FVector2D(Left, RowY + 2.f), FVector2D(X - Pad, RowY + 2.f + Line + Pad)).IsInsideOrOn(Mouse)) Hover = It.Hover;
		}
		if (!bLead && !Ln.Text.IsEmpty()) DrawLabel(Ln.Text, Ln.Colour, X, RowY + 2.f + Pad / 2, Font, 1.f);
		RowY += RowH;
	}
	if (!Hover.IsEmpty()) DrawLabel(Hover, Good, 14.f, RowY, Font, 1.f);
	return Y + H + 4.f;
}

float ATCHUD::DrawMatchPanel(UFont* Font, float Width, float TopLimit, bool bDraw)
{
	ATCGameMode* M = GetWorld()->GetAuthGameMode<ATCGameMode>();
	ATCPlayerController* PC = Cast<ATCPlayerController>(GetOwningPlayerController());
	const FTCMatchState& S = M->State;
	TArray<FTCHudFit::FSelection> Selection;
	TArray<int32> SelectedIds=PC->Selected.Array(); SelectedIds.Sort();
	for (int32 Id : SelectedIds)
	{
		const FTCUnit* U=S.Units.Find(Id); if (!U) continue;
		int32 At=Selection.IndexOfByPredicate([&](const FTCHudFit::FSelection& R) { return R.Type==U->Type; });
		if (At==INDEX_NONE)
		{
			FTCHudFit::FSelection R; R.Type=U->Type; R.Name=FTCAlerts::UnitName(S,&M->UnitTable,U->Type,U->Owner); R.Count=0;
			At=Selection.Add(MoveTemp(R));
		}
		FTCHudFit::FSelection& R=Selection[At]; const FTCUnitDef* D=M->UnitTable.Find(U->Type);
		R.Count++; R.Hp+=U->Hp; R.MaxHp+=D?D->MaxHp():0;
		for (const FString& Tag : UnitTags(*U,D)) R.Tags.AddUnique(Tag);
	}
	TArray<FString> Details;
	if (PC->Aim!=ETCAim::None && !PC->AimHint.IsEmpty()) Details.Add(PC->AimHint);
	if (S.SeatConfirmed() && PC->Selected.Num())
	{
		TArray<FString> Groups;
		for (int32 G=1; G<=FTCControlGroups::GroupCount; G++)
			if (const int32 N=PC->Groups.Members(G).Num()) Groups.Add(FString::Printf(TEXT("%d: %d"),G,N));
		if (Groups.Num()) Details.Add(TEXT("Control group  ")+FString::Join(Groups,TEXT("   ")));
		// The keyboard instructions belong in F1 and the control tooltip, not an extra two-line footer.
	}
	// The Form submenu owns its shape and spacing values, as in hud.js. A second always-visible summary squeezed
	// selection and ability rows together and repeated those values outside the browser's layout.
	// The buttons (client/hud.js drawOrders), in rows that wrap at the panel's width: Stop and Attack-move always; for a
	// selection that is neither all buildings nor all trucks, Retreat, the three stances and (with a builder squad)
	// the Build menu; Resume deliveries for an all-truck selection; the logistics switches; End match for the host.
	// Slice 1f: the support bar (client/hud.js buildSupport and drawEcon) in its own row first, the Shell area button
	// with a salvo unit selected, the Form menu and the cover preview switch.
	// Compact (round 16): the label at fit level 1 and up (the support bar: what the browser's button shows, its key and
	// its cost; the browser draws the power's icon where this has none, and its title names it, as Hover does here)
	struct FItem { FString Label; FTCUiCommand Command; bool bEnabled; FLinearColor Fill; FString Hover; FString Compact; };
	TArray<FItem> Bar, Orders, Menu, LogisticsButtons;
	const float W = Width;
	if (!S.bSpectator && S.SeatConfirmed())
		for (const FTCSupportButton& B : M->Powers.Buttons(S, M->UnitTable, M->Recruit))
		{
			const FTCSupportDef* D = FTCSupportPowers::Find(B.Kind);
			FTCUiCommand C(ETCUiAction::Support);
			C.Unit = B.Kind;
			const bool bAimed = PC->Aim == ETCAim::Support && PC->PowerAim.Kind == ETCSupportAimKind::Support && PC->PowerAim.SupportKind == B.Kind;
			// the browser never disables a support button: a click on a refused one explains why (aimSupport)
			Bar.Add({ FString::Printf(TEXT("%s [%s] %s"), D ? *D->Name : *B.Kind, *B.Key, *B.Text), C, B.Check.bOk, bAimed ? TabOpen : B.Check.bOk ? ButtonFill : CardOff,
				B.Check.bOk ? B.Tip : FString::Printf(TEXT("%s: %s%s"), D ? *D->Name : *B.Kind, *B.Check.Reason, B.Check.bNative ? TEXT(" (this client's own check)") : TEXT("")),
				B.Key.IsEmpty() ? B.Text : FString::Printf(TEXT("[%s] %s"), *B.Key, *B.Text) });
		}
	if (!S.bSpectator)
	{
		// round 16: hud.js drawOrders fills #abil only for a selection (its key is '' with nothing selected, and the panel
		// hides when empty), so Stop and Attack-move show with a selection, as the rest of the Orders; the keys still work
		if (PC->Selected.Num())
		{
			Orders.Add({ TEXT("Stop (X)"), FTCUiCommand(ETCUiAction::Stop), true, ButtonFill, TEXT("Stop (X): halt where they are") });
			Orders.Add({ TEXT("Attack-move (G)"), FTCUiCommand(ETCUiAction::AttackMove), true, PC->Aim == ETCAim::AttackMove ? TabOpen : ButtonFill, TEXT("Attack-move (G, or Ctrl+right-click): move and fight anything met on the way") });
		}
		// client/hud.js drawOrders: Shell area while a salvo unit is selected (the badge's arrow is not in the HUD's font)
		const FTCAreaButton Area = M->Powers.AreaButton(S, M->UnitTable, PC->Selected);
		if (Area.bShown)
			Orders.Add({ FString::Printf(TEXT("Shell area (%s)"), FTCSupportPowers::AreaLabel()), FTCUiCommand(ETCUiAction::Area), true,
				PC->Aim == ETCAim::Support && PC->PowerAim.Kind == ETCSupportAimKind::Area ? TabOpen : ButtonFill, Area.Tip });
		if (TCArmyCommands::ShowsOrdersPanel(S, M->UnitTable, PC->Selected))
		{
			Orders.Add({ TEXT("Retreat (R)"), FTCUiCommand(ETCUiAction::Retreat), true, ButtonFill, TEXT("Retreat (R): run back to base, heal and reinforce there") });
			for (ETCStance St : TCStance::All)
			{
				const FTCStanceButton B = TCArmyCommands::StanceButton(S, M->UnitTable, PC->Selected, St);
				FTCUiCommand C(ETCUiAction::Stance);
				C.Unit = TCStance::Key(St);
				Orders.Add({ FString::Printf(TEXT("%s: %s"), TCStance::Name(St), *B.Text), C, B.bOk, B.bLit ? TabOpen : ButtonFill,
					B.bOk ? FString::Printf(TEXT("%s (%s): %s. Click to switch it on or off for the selection"), TCStance::Name(St), TCStance::Label(St), TCStance::Tip(St)) : B.Reason });
			}
			// client/hud.js drawOrders: the selection's abilities in PRIORITY order (a click uses, a right click switches
			// autocast), then the carrier buttons
			for (const FTCAbilityButton& B : M->Abilities.Buttons(S, M->UnitTable, PC->Selected))
			{
				const FTCAbilityDef* D = FTCAbilities::Find(B.Type);
				FTCUiCommand C(ETCUiAction::Ability);
				C.Unit = B.Type;
				const FString L = FString::Printf(TEXT("%s%s%s%s"), D ? *D->Name : *B.AbilityId, B.Key.IsEmpty() ? TEXT("") : *FString::Printf(TEXT(" (%s)"), *B.Key),
					B.Text.IsEmpty() ? TEXT("") : *FString::Printf(TEXT(" %s"), *B.Text), B.bAuto ? TEXT(" [A]") : TEXT("")); // index.html CSS #abil .ob.auto::after: 'A'
				const bool bAimed = PC->Aim == ETCAim::Ability && PC->AimAbilityType == B.Type;
				Orders.Add({ L, C, B.Check.bOk, bAimed || B.bAuto ? TabOpen : ButtonFill, B.Check.bOk ? B.Tip : FString::Printf(TEXT("%s: %s"), D ? *D->Name : *B.AbilityId, *B.Check.Reason) });
			}
			for (int32 I = 0; I < 2; I++)
			{
				const FTCCarrierButton B = I == 0 ? M->Support.MountUpButton(S, M->UnitTable, PC->Selected) : M->Support.UnloadButton(S, M->UnitTable, PC->Selected);
				if (!B.bShown) continue;
				// the browser's button shows no name, only its title's ('Montar (Shift+Q): ...' in Spanish): its head here
				Orders.Add({ FString::Printf(TEXT("%s (%s) %s"), *FTCLocale::TitleName(B.Tip), *B.Label, *B.Text), FTCUiCommand(I == 0 ? ETCUiAction::MountUp : ETCUiAction::Unload), true, ButtonFill,
					B.bOk ? B.Tip : FString::Printf(TEXT("%s%s"), *B.Reason, B.bNative ? TEXT(" (this client's own check)") : TEXT("")) });
			}
			// client/hud.js menuBtn('form') and menuHTML('form'): the shapes, Tighten, Spread, March together, Snap
			{
				const FTCFormationButton B = PC->Formation.MenuButton();
				FTCUiCommand C(ETCUiAction::Formation);
				C.Unit = B.Key;
				Orders.Add({ FString::Printf(TEXT("%s%s"), *B.Value, B.bOn ? TEXT(" (open)") : TEXT("")), C, true, B.bOn ? TabOpen : ButtonFill, B.Tip });
				for (const FTCFormationButton& F : PC->Formation.SubmenuButtons())
				{
					FTCUiCommand FC(ETCUiAction::Formation);
					FC.Unit = F.Key;
					Menu.Add({ FString::Printf(TEXT("%s%s%s"), *F.Name, F.Kbd.IsEmpty() ? TEXT("") : *FString::Printf(TEXT(" (%s)"), *F.Kbd), F.Value.IsEmpty() ? TEXT("") : *FString::Printf(TEXT("  %s"), *F.Value)),
						FC, true, F.bOn ? TabOpen : CardFill, F.Tip });
				}
			}
			// the Build menu (base buildings and fortifications, by tab) and the Trench menu (the patterns)
			const FTCBuildMenu BuildMenu = M->Build.BuildMenu(S, PC->Selected);
			const bool bField = M->Field.OffersFieldMenus(PC->Selected);
			if (BuildMenu.bOffered || bField)
			{
				Orders.Add({ TEXT("Build"), FTCUiCommand(ETCUiAction::BuildMenu), true, PC->bBuildMenuOpen ? TabOpen : ButtonFill, TEXT("Build: base buildings and field works the selected builder squads construct") });
				if (bField) Orders.Add({ TEXT("Trench"), FTCUiCommand(ETCUiAction::TrenchMenu), true, PC->bTrenchMenuOpen ? TabOpen : ButtonFill, TEXT("Trench: entrenchment patterns all selected builders dig together") });
				if (PC->bBuildMenuOpen)
				{
					for (const FTCBuildButton& B : BuildMenu.Buttons)
					{
						FTCUiCommand C(ETCUiAction::Build);
						C.Unit = B.Kind;
						Menu.Add({ FString::Printf(TEXT("%s  %s"), *B.Name, *B.Cost), C, B.Check.bOk, B.Check.bOk ? CardFill : CardOff, B.Check.bOk ? B.Tip : B.Check.Reason });
					}
					if (bField)
						for (const FTCFieldButton& B : M->Field.FortMenu(S, PC->Selected).Buttons)
						{
							FTCUiCommand C(ETCUiAction::Fort);
							C.Unit = B.Kind;
							// the key as client/keys.js labels it ("T", "Shift+Y"): the badge's arrow is not in the HUD's font
							const FTCFortDef* FD = FTCFieldworks::Find(B.Kind);
							const FString KeyL = FD ? FD->KeyLabel : FString();
							Menu.Add({ FString::Printf(TEXT("%s%s  %s"), *B.Name, KeyL.IsEmpty() ? TEXT("") : *FString::Printf(TEXT(" [%s]"), *KeyL), *B.Value), C, B.Check.bOk, B.Check.bOk ? CardFill : CardOff,
								B.Check.bOk ? B.Tip : B.Check.Reason });
						}
				}
				if (PC->bTrenchMenuOpen && bField)
					for (const FTCFieldButton& B : M->Field.TrenchMenu(S, PC->Selected).Buttons)
					{
						FTCUiCommand C(ETCUiAction::Entrench);
						C.Unit = B.Kind;
						Menu.Add({ FString::Printf(TEXT("%s  %s"), *B.Name, *B.Value), C, B.Check.bOk, B.Check.bOk ? CardFill : CardOff, B.Check.bOk ? B.Tip : B.Check.Reason });
					}
			}
		}
		if (FTCLogistics::ResumeShown(S, PC->Selected))
			Orders.Add({ FTCLogisticsCard::ResumeLabel, FTCUiCommand(ETCUiAction::Resume), true, ButtonFill, TEXT("Resume deliveries: the selected trucks go back to automatic supply runs") });
		if (M->Logistics.ControlsVisible(S))
		{
			// hud.js toolbar aria-labels 'Logistics overlay' and 'Logistics selection' (On/Off as the other switches)
			LogisticsButtons.Add({ FString::Printf(TEXT("Logistics overlay: %s"), M->Logistics.OverlaySwitch() ? TEXT("On") : TEXT("Off")), FTCUiCommand(ETCUiAction::SupplyOverlay), true, M->Logistics.OverlaySwitch() ? TabOpen : ButtonFill,
				TEXT("Supply overlay: known stores and the selected trucks' routes") });
			LogisticsButtons.Add({ FString::Printf(TEXT("Logistics selection: %s"), M->Logistics.SelectionSwitch() ? TEXT("On") : TEXT("Off")), FTCUiCommand(ETCUiAction::TruckSelect), true, M->Logistics.SelectionSwitch() ? TabOpen : ButtonFill,
				TEXT("Truck select: a drag takes only your supply trucks") });
		}
		// client/hud.js drawOrders: Take cover with infantry selected (Shift queues it), and in World Conquest Send by rail
		bool bInfantry = false;
		for (int32 Id : PC->Selected)
			if (const FTCUnit* U = S.Units.Find(Id))
				if (const FTCUnitDef* D = M->UnitTable.Find(U->Type)) bInfantry |= D->bInfantry && S.IsOwn(*U);
		if (bInfantry) Orders.Add({ FString::Printf(TEXT("Take cover (%s)"), *TCKeys::Label(TEXT("cover"))), FTCUiCommand(ETCUiAction::Cover), true, ButtonFill, TEXT("Take cover: the selected squads go to the best cover nearby (Shift queues it)") });
		if (S.Map.bWorld && PC->Selected.Num())
			Orders.Add({ FString::Printf(TEXT("Send by rail (%s)"), *TCKeys::Label(TEXT("rail"))), FTCUiCommand(ETCUiAction::Rail), true, PC->Aim == ETCAim::Rail ? TabOpen : ButtonFill, TEXT("Send by rail: click the destination; they ride to your station nearest it") });
	}
	// The game menu's settings (client/index.html #menu, cover-preview.js, light.js, camera.js; Volume, the page's
	// language picker and the UI scale slider). Round 16: they live in the match menu, as in the browser
	// (MenuSettingRows); only a match with no menu to open (no lobby in play) keeps them here, as before.
	if (!bSettingsInMenu)
		for (const FTCControlRow& R : MenuSettingRows())
			for (const FTCControlItem& It : R.Items)
				Orders.Add({ It.Label, ControlCommand(It.Key, It.Slot), true, It.bOn ? TabOpen : ButtonFill, It.Hover });
	// round 15: the host's Pause (Resume) match, Save match and End match moved to the match menu (client/main.js
	// renderMatchMenu; Client/TCMatchControls.h), with Restart, Hand <name> to AI and Leave
	const FTCMeasure Measure=FTCType::IsLoaded()?FTCType::Measure():FTCPaint::CanvasMeasure(Font);
	auto Convert=[&](const FItem& I)
	{
		FTCHudFit::FControl C;
		C.Label=I.Label; C.Hover=I.Hover; C.Command=I.Command; C.bEnabled=I.bEnabled; C.bOn=I.Fill.Equals(TabOpen);
		const FString& U=I.Command.Unit;
		switch (I.Command.Action)
		{
		case ETCUiAction::Stop: C.Icon=TEXT("stop"); C.Key=TEXT("X"); break;
		case ETCUiAction::AttackMove: C.Icon=TEXT("amove"); C.Key=TEXT("G"); break;
		case ETCUiAction::Retreat: C.Icon=TEXT("retreat"); C.Key=TEXT("R"); break;
		case ETCUiAction::Stance:
			C.Icon=U; C.Key=U==TEXT("holdFire")?TEXT("⇧F"):U==TEXT("holdPos")?TEXT("⇧G"):TEXT("⇧X");
			C.Value=I.Label.Mid(I.Label.Find(TEXT(": "))+2); break;
		case ETCUiAction::Cover: C.Icon=TEXT("takecover"); C.Key=TEXT("⇧C"); break;
		case ETCUiAction::Rail: C.Icon=TEXT("train"); C.Key=TEXT("⇧N"); break;
		case ETCUiAction::MountUp: C.Icon=TEXT("mountup"); C.Key=TEXT("⇧Q"); break;
		case ETCUiAction::Unload: C.Icon=TEXT("unload"); C.Key=TEXT("⇧E"); break;
		case ETCUiAction::Area: C.Icon=TEXT("barrage"); C.Key=TEXT("⇧B"); break;
		case ETCUiAction::Ability:
		{
			const FTCAbilityDef* D=FTCAbilities::Find(U); C.Icon=D?(D->Id==TEXT("smoke")?TEXT("smokeab"):D->Id):TEXT("unknown");
			for (const FTCAbilityButton& B : M->Abilities.Buttons(S,M->UnitTable,PC->Selected))
				if (B.Type==U) { C.Key=B.Key; C.Value=B.Text; if (B.bAuto) C.Key+=TEXT(" A"); break; }
			break;
		}
		case ETCUiAction::Formation:
			if (U==TEXT("menu")) { C.Icon=TEXT("f_wedge"); C.Value=TEXT("Form"); }
			else if (U.StartsWith(TEXT("shape:"))) C.Icon=TEXT("f_")+U.RightChop(6);
			else if (U==TEXT("tighten")) { C.Icon=TEXT("f_tight"); C.Key=TEXT("["); }
			else if (U==TEXT("spread")) { C.Icon=TEXT("f_spread"); C.Key=TEXT("]"); C.Value=FString::Printf(TEXT("%gx"),PC->Formation.Settings.Spread); }
			else { C.Icon=U==TEXT("together")?TEXT("f_together"):TEXT("f_snap"); C.Value=U==TEXT("together")?(PC->Formation.Settings.bTogether?TEXT("On"):TEXT("Off")):(PC->Formation.Settings.bSnap?TEXT("On"):TEXT("Off")); }
			break;
		case ETCUiAction::BuildMenu: C.Icon=TEXT("sandbags"); C.Value=TEXT("Build"); break;
		case ETCUiAction::TrenchMenu: C.Icon=TEXT("e_zigzag"); C.Value=TEXT("Trench"); break;
		case ETCUiAction::Build: case ETCUiAction::Fort: case ETCUiAction::Entrench:
		{
			C.Icon=I.Command.Action==ETCUiAction::Entrench?TEXT("e_")+U:U;
			const int32 Split=I.Label.Find(TEXT("  ")); if (Split>=0) C.Value=I.Label.Mid(Split+2);
			break;
		}
		case ETCUiAction::Resume: C.Icon=TEXT("truck"); C.Value=TEXT("Resume deliveries"); break;
		case ETCUiAction::SupplyOverlay: C.Icon=TEXT("supplycache"); C.Value=M->Logistics.OverlaySwitch()?TEXT("On"):TEXT("Off"); break;
		case ETCUiAction::TruckSelect: C.Icon=TEXT("truck"); C.Value=M->Logistics.SelectionSwitch()?TEXT("On"):TEXT("Off"); break;
		case ETCUiAction::Support:
			C.Icon=U;
			for (const FTCSupportButton& B : M->Powers.Buttons(S,M->UnitTable,M->Recruit)) if (B.Kind==U) { C.Key=B.Key; C.Value=B.Text; break; }
			break;
		default: C.Value=I.Label; break;
		}
		return C;
	};
	TArray<FTCHudFit::FControl> Grid, Submenu, Support;
	for (const FItem& I : Orders) Grid.Add(Convert(I));
	// hud.js order: retreat, attack-move, stop, stances, cover, menus, abilities. Stable action ranks keep ability order.
	auto Rank=[](ETCUiAction A) { switch(A) { case ETCUiAction::Retreat:return 0; case ETCUiAction::AttackMove:return 1; case ETCUiAction::Stop:return 2; case ETCUiAction::Area:return 3; case ETCUiAction::Stance:return 4; case ETCUiAction::Cover:return 5; case ETCUiAction::Rail:return 6; case ETCUiAction::MountUp:return 7; case ETCUiAction::Unload:return 8; case ETCUiAction::Formation:return 9; case ETCUiAction::BuildMenu:return 10; case ETCUiAction::TrenchMenu:return 11; case ETCUiAction::Ability:return 12; default:return 13; } };
	Grid.StableSort([&](const FTCHudFit::FControl& A,const FTCHudFit::FControl& B) { return Rank(A.Command.Action)<Rank(B.Command.Action); });
	for (const FItem& I : Menu) Submenu.Add(Convert(I));
	for (const FItem& I : Bar) Support.Add(Convert(I));
	for (const FItem& I : LogisticsButtons) Support.Add(Convert(I));
	const FString Title=PC->Selected.Num()==1?TEXT("1 unit selected"):FString::Printf(TEXT("%d units selected"),PC->Selected.Num());
	const FTCHudFit::FBottomLayout Layout=FTCHudFit::LeftPanel(Canvas->ClipY,Title,Selection,Details,Grid,Submenu,Measure,UiNow);
	float Top=Canvas->ClipY-Gap;
	for (const FBox2D& B : Layout.Paint.Panels) Top=FMath::Min(Top,(float)B.Min.Y);
	if (!bDraw) return Top;
	MatchPanelBox=FBox2D(ForceInit); for (const FBox2D& B : Layout.Paint.Panels) MatchPanelBox+=B;
	DrawBottomLayout(Layout,Font);
	if (!S.bSpectator && S.SeatConfirmed())
	{
		double Pop=0,Cap=0; const bool Known=M->Recruit.ArmyUse(S,Pop,Cap);
		const FString Money=FString::Printf(TEXT("%d MP%s"),S.Mp,M->Recruit.Fuel().IsSet()?*FString::Printf(TEXT("  %.0f Fuel"),M->Recruit.Fuel().GetValue()):TEXT(""));
		const FString Population=Known?FString::Printf(TEXT("%s/%s units"),*Js(Pop),*Js(Cap)):TEXT("");
		DrawBottomLayout(FTCHudFit::EconomyPanel(Canvas->ClipX,Money,FTCObjectives::EconomyLine(S),Population,Support,Measure,UiNow),Font);
	}
	return Top;
}

void ATCHUD::DrawRecruitPanel(UFont* Font, float Left, float Bottom)
{
	// client/hud.js the Command Card: outside Classic every unit this side can recruit, in group tabs (one open at a
	// time); in Classic and World a selected own Production Building's units and its five queue places. Each card shows
	// the browser's availability: a refused card is greyed and a click on it shows the reason instead of buying.
	ATCGameMode* M = GetWorld()->GetAuthGameMode<ATCGameMode>();
	ATCPlayerController* PC = Cast<ATCPlayerController>(GetOwningPlayerController());
	const FTCMatchState& S = M->State;
	if (S.bSpectator || !S.SeatConfirmed()) return;
	const FTCRecruitment& R = M->Recruit;
	const FTCCommandCard Card = R.Card(S, PC->Selected);
	// client/hud.js drawTech: one own finished HQ or Armory selected (FTCProgression), and drawClassicCard's World
	// Restore card when the snapshot offers recovery and nothing else takes the card (FTCWorldRecovery)
	const FTCResearchPanel Research = M->Progress.Panel(S, PC->Selected);
	const FTCWorldRecoveryCard Restore = M->WorldRec.Card(S, PC->Selected);
	TArray<int32> SelectedIds=PC->Selected.Array(); SelectedIds.Sort();
	const FTCLogisticsCard Logistics=M->Logistics.Card(S,SelectedIds);
	if (Card.Kind == ETCCardKind::Hidden && !Research.bShown && !Restore.bShown && !Logistics.bVisible) return;
	// client/hud.js drawClassicCard: an own Engineer selected (no single own building) gets the Build card; an all-truck
	// selection gets the logistics card in #buy, after ordinary recruitment when the selection is mixed.
	const FTCBuildCard BuildCard = Card.Kind == ETCCardKind::NotPorted ? M->Build.BuildCard(S, PC->Selected) : FTCBuildCard();
	// round 15: as #buy (client/index.html), between the left column and the minimap at every UI scale, never over the
	// left column: as wide as that room allows (720 px at most), however narrow; cards wider than it are cut
	const float Right = Left + FMath::Clamp((Minimap.Rect().bIsValid ? ToUi((float)Minimap.Rect().Min.X) - 12.f : Canvas->ClipX - Gap) - Left, 60.f, 720.f);
	const float Width = Right - Left;

	// client/hud.js drawRecruit: selecting one finished own Production Building opens the group it makes most of
	const int32 SelBuilding = Card.Kind == ETCCardKind::Recruit && Card.SuggestedGroup >= 0 && PC->Selected.Num() == 1 ? *PC->Selected.CreateConstIterator() : 0;
	if (SelBuilding && SelBuilding != PC->RecruitAutoFor) PC->RecruitGroupOpen = Card.SuggestedGroup;
	PC->RecruitAutoFor = SelBuilding;

	// what goes in the panel, row by row: text lines, then rows of buttons that wrap at the panel's width
	struct FItem { FString Label; FTCUiCommand Command; bool bEnabled; FLinearColor Fill; float W; FString Hover; };
	TArray<TPair<FString, FLinearColor>> Lines;
	TArray<TArray<FItem>> Rows;
	auto Measure = [&](const FString& L) { float TW = 0, TH = 0; LabelSize(L, TW, TH, Font, 1.f); return TW + 2 * Pad; };
	TArray<TArray<FItem>> Lists; // round 17: each wrapped list as given, for the browser's fit below
	auto Wrap = [&](const TArray<FItem>& Items)
	{
		Lists.Add(Items);
		TArray<FItem> Row; float X = 0;
		for (FItem I : Items)
		{
			I.W = FMath::Min(I.W, Width - 2 * Pad); // round 15: a card is never wider than the panel
			if (Row.Num() && X + I.W > Width - 2 * Pad) { Rows.Add(Row); Row.Reset(); X = 0; }
			Row.Add(I); X += I.W + Pad;
		}
		if (Row.Num()) Rows.Add(Row);
	};
	double Pop = 0, Cap = 0;
	const bool bArmy = R.ArmyUse(S, Pop, Cap);
	// hud.js drawEcon: the money, the fuel (Classic: index.html #fuelRes, '{n}' then 'Fuel') and the army count as
	// #popCount writes it, '{pop}/{cap} units' (its title: 'Army size: ...'), each in its own column
	const FString Wallet = FString::Printf(TEXT("%.0f MP%s  |  %s"), R.Mp(), R.Fuel().IsSet() ? *FString::Printf(TEXT("  |  %.0f Fuel"), R.Fuel().GetValue()) : TEXT(""),
		bArmy ? *FString::Printf(TEXT("%s/%s units"), *Js(Pop), *Js(Cap)) : TEXT("?"));
	auto CardItems = [&](const FTCRecruitGroup& G)
	{
		TArray<FItem> Items;
		for (const FTCRecruitCard& C : G.Cards)
		{
			FTCUiCommand Cmd(ETCUiAction::Recruit);
			Cmd.Unit = C.Type;
			Cmd.Building = C.Action.bHasFrom ? C.Action.From : 0;
			const FString Name = FTCAlerts::UnitName(S, &M->UnitTable, C.Type, S.You); // hud.js unitCard: name(t), the player's faction
			const FString L = FString::Printf(TEXT("%s  %s%s"), *Name, *C.Cost, C.Sub.IsEmpty() ? TEXT("") : *FString::Printf(TEXT(" (%s)"), *C.Sub));
			const FString Hover = FString::Printf(TEXT("%s: %s%s. %s"), *Name, *C.Cost, C.Sub.IsEmpty() ? TEXT("") : *FString::Printf(TEXT(", %s"), *C.Sub),
				C.Check.bOk ? TEXT("Click to recruit, Shift+click buys up to 5") : *C.Check.Reason);
			Items.Add({ L, Cmd, C.Check.bOk, C.Check.bOk ? CardFill : C.bBroke ? CardBroke : CardOff, Measure(L), Hover });
		}
		return Items;
	};
	switch (Card.Kind)
	{
	case ETCCardKind::Recruit:
	{
		// Recruit and Rally sit above the tab row. Economy is drawn in #econ, not on this card.
		TArray<FItem> Tabs;
		for (int32 I = 0; I < Card.Groups.Num(); I++)
		{
			FTCUiCommand Cmd(ETCUiAction::RecruitGroup);
			Cmd.Group = I;
			const FString L = Card.Groups[I].Name;
			Tabs.Add({ L, Cmd, true, I == PC->RecruitGroupOpen ? TabOpen : ButtonFill, Measure(L), FString::Printf(TEXT("%s: click to %s its cards"), *L, I == PC->RecruitGroupOpen ? TEXT("hide") : TEXT("show")) });
		}
		// client/hud.js drawRecruit: the card's Rally button (startRally)
		if (TCArmyCommands::ShowsRallyButton(S, R, PC->Selected))
		{
			const FString L = FString::Printf(TEXT("Rally (%s)"), TCArmyCommands::RallyLabel);
			Tabs.Add({ L, FTCUiCommand(ETCUiAction::Rally), true, PC->Aim == ETCAim::Rally ? TabOpen : ButtonFill, Measure(L), FString::Printf(TEXT("Rally (%s): choose where new recruits gather"), TCArmyCommands::RallyLabel) }); // hud.js drawRecruit b.title
		}
		Wrap(Tabs);
		if (Card.Groups.IsValidIndex(PC->RecruitGroupOpen)) Wrap(CardItems(Card.Groups[PC->RecruitGroupOpen]));
		// client/hud.js drawRecruit: one selected building's facility line, with "Cancel (75% back)" on an own site
		const FTCSiteCard Site = M->Support.SiteCard(S, M->UnitTable, PC->Selected, R.Classic());
		if (Site.bShown && !Site.bClassicCard)
		{
			if (!Site.Status.IsEmpty()) Lines.Add({ Site.Status, Dim });
			if (Site.bCancel) Wrap({ { Site.CancelText, FTCUiCommand(ETCUiAction::CancelSite), true, ButtonFill, Measure(Site.CancelText), Site.CancelText } });
		}
		break;
	}
	case ETCCardKind::Building:
	{
		const FTCUnit* B = S.Units.Find(Card.Building);
		const FTCRecruitDef* D = B ? FTCRecruitment::Find(B->Type) : nullptr;
		Lines.Add({ FString::Printf(TEXT("%s  |  %s"), D ? *D->Name : TEXT("Building"), *Wallet), Text });
		Lines.Add({ Card.QueueLine, Card.bTraining ? Good : Dim });
		for (const FTCRecruitGroup& G : Card.Groups) Wrap(CardItems(G));
		TArray<FItem> Slots;
		for (const FTCQueueSlot& Q : Card.Slots)
		{
			const FString Name = Q.Type.IsEmpty() ? FString() : FTCAlerts::UnitName(S, &M->UnitTable, Q.Type, S.You); // hud.js name(t)
			FTCUiCommand Cmd(Q.Job ? ETCUiAction::CancelJob : ETCUiAction::None);
			Cmd.Building = Card.Building; Cmd.Job = Q.Job;
			const FString L = Q.Type.IsEmpty() ? FString(TEXT("(free)")) : Q.bTraining ? FString::Printf(TEXT("%s %.0f%%"), *Name, Card.Progress * 100.0) : Q.Job ? FString::Printf(TEXT("x %s"), *Name) : Name;
			const FString Hover = Q.Job ? (Q.Check.bOk ? FString::Printf(TEXT("Cancel %s: refund %d MP, %d Fuel"), *Name, Q.RefundMp, Q.RefundFuel) : Q.Check.Reason)
				: Q.Type.IsEmpty() ? FString(TEXT("A free queue place")) : FString::Printf(TEXT("%s: %s"), Q.bTraining ? TEXT("Training") : TEXT("Waiting"), *Name);
			Slots.Add({ L, Cmd, !Q.Job || Q.Check.bOk, Q.bTraining ? TabOpen : Q.Job ? ButtonFill : CardOff, FMath::Max(Measure(L), 80.f), Hover });
		}
		Wrap(Slots);
		break;
	}
	case ETCCardKind::Site:
	{
		const FTCUnit* B = S.Units.Find(Card.Building);
		const FTCRecruitDef* D = B ? FTCRecruitment::Find(B->Type) : nullptr;
		// client/hud.js drawClassicCard for a site: its progress from the exact built the server sent, the hint and Cancel
		const FTCSiteCard Site = M->Support.SiteCard(S, M->UnitTable, PC->Selected, R.Classic());
		if (Site.bShown && !Site.Status.IsEmpty()) Lines.Add({ FString::Printf(TEXT("%s: %s"), D ? *D->Name : TEXT("Building"), *Site.Status), Text });
		else Lines.Add({ FString::Printf(TEXT("%s: under construction, %.0f%% built"), D ? *D->Name : TEXT("Building"), Card.Built * 100.f), Text });
		if (Site.bShown && !Site.Hint.IsEmpty()) Lines.Add({ Site.Hint, Dim });
		if (Site.bShown && Site.bCancel)
		{
			const FString L = Site.CancelSub.IsEmpty() ? Site.CancelText : FString::Printf(TEXT("%s (%s)"), *Site.CancelText, *Site.CancelSub);
			Wrap({ { L, FTCUiCommand(ETCUiAction::CancelSite), true, ButtonFill, Measure(L), Site.CancelTip.IsEmpty() ? L : Site.CancelTip } });
		}
		break;
	}
	default:
		if (BuildCard.bShown)
		{
			// client/hud.js drawClassicCard: the Engineers' Build card. A click arms the building (startBuild explains a
			// refusal); a key badge is its Classic key
			Lines.Add({ TEXT("Build  |  ") + Wallet, Text });
			TArray<FItem> Items;
			for (const FTCBuildButton& B : BuildCard.Buttons)
			{
				FTCUiCommand Cmd(ETCUiAction::Build);
				Cmd.Unit = B.Kind;
				const FString L = FString::Printf(TEXT("%s%s  %s"), *B.Name, B.Key.IsEmpty() ? TEXT("") : *FString::Printf(TEXT(" [%s]"), *B.Key), *B.Cost);
				Items.Add({ L, Cmd, B.Check.bOk, B.Check.bOk ? CardFill : B.bBroke ? CardBroke : CardOff, Measure(L), FString::Printf(TEXT("%s. %s"), *B.Tip, *B.Note) });
			}
			Wrap(Items);
			break;
		}
		if (Restore.bShown)
		{
			// client/hud.js drawClassicCard: 'Restore HQ' or 'Restore Engineer', its MP cost and line, disabled when short
			Lines.Add({ FString::Printf(TEXT("World recovery  |  %s"), *Wallet), Text });
			const FString L = FString::Printf(TEXT("%s  %s (%s)"), *Restore.Name, *Restore.Cost, *Restore.Sub);
			const FString Tip = Restore.bDisabled ? FString::Printf(TEXT("%s: needs %s"), *Restore.Name, *Restore.Cost) : FString::Printf(TEXT("%s: %s, %s"), *Restore.Name, *Restore.Cost, *Restore.Sub);
			Wrap({ { L, FTCUiCommand(ETCUiAction::Recover), !Restore.bDisabled, Restore.bDisabled ? CardBroke : CardFill, Measure(L), Tip } });
			break;
		}
		if (Card.Kind != ETCCardKind::Hidden && !Logistics.bVisible) Lines.Add({ FString::Printf(TEXT("Command card not built in this client yet: %s"), *Card.Why), Dim });
		break;
	}
	if (Research.bShown)
	{
		// client/hud.js drawTech: the side's tier, then the HQ's tier button or the Armory's three lines; a refused button
		// is greyed and a click on it shows the reason, a running job shows its progress
		Lines.Add({ FString::Printf(TEXT("Research  |  %s"), *Research.TierLabel), Text });
		TArray<FItem> Items;
		for (const FTCResearchButton& B : Research.Buttons)
		{
			FTCUiCommand Cmd(ETCUiAction::Research);
			Cmd.Unit = LexToString(B.Kind);
			Items.Add({ B.Text, Cmd, B.Check.bOk, B.Check.bOk ? CardFill : B.bResearching ? TabOpen : CardOff, Measure(B.Text), B.ShownTooltip() });
		}
		Wrap(Items);
	}

	const FTCMeasure TypeMeasure=FTCType::IsLoaded()?FTCType::Measure():FTCPaint::CanvasMeasure(Font);
	TArray<FTCHudFit::FControl> TabControls, Above;
	TArray<TArray<FTCHudFit::FControl>> CardLists;
	TArray<FString> Headings;
	for (const auto& L : Lines) Headings.Add(L.Key);
	if (Card.Kind==ETCCardKind::Recruit)
	{
		FTCHudFit::FControl RecruitTitle; RecruitTitle.Label=TEXT("Recruit"); Above.Add(RecruitTitle);
	}
	for (const TArray<FItem>& List : Lists)
	{
		TArray<FTCHudFit::FControl> Cards;
		for (const FItem& I : List)
		{
			FTCHudFit::FControl C;
			C.Label=I.Label; C.Hover=I.Hover; C.Command=I.Command; C.bEnabled=I.bEnabled; C.bOn=I.Fill.Equals(TabOpen);
			if (I.Command.Action==ETCUiAction::RecruitGroup)
			{
				static const TCHAR* const Icons[]={TEXT("rifle"),TEXT("at"),TEXT("tank"),TEXT("fighter"),TEXT("gunboat")};
				const int32 G=Card.Groups.IsValidIndex(I.Command.Group)?Card.Groups[I.Command.Group].Group:0;
				C.Icon=Icons[FMath::Clamp(G,0,4)]; TabControls.Add(C); continue;
			}
			if (I.Command.Action==ETCUiAction::Rally) { Above.Add(C); continue; }
			if (I.Command.Action==ETCUiAction::Recruit)
			{
				C.Label=FTCAlerts::UnitName(S,&M->UnitTable,I.Command.Unit,S.You); C.Icon=I.Command.Unit;
				const FTCPrice Price=R.PriceOf(I.Command.Unit);
				C.Value=FString::Printf(TEXT("%d MP%s"),Price.Mp,Price.Fuel?*FString::Printf(TEXT("  %d Fuel"),Price.Fuel):TEXT(""));
			}
			else
			{
				const int32 Sep=C.Label.Find(TEXT("  "));
				if (Sep>0) { C.Value=C.Label.Mid(Sep+2); C.Label=C.Label.Left(Sep); }
				C.Icon=I.Command.Unit;
			}
			Cards.Add(C);
		}
		if (Cards.Num()) CardLists.Add(Cards);
	}
	const float PanelRight=Minimap.Rect().bIsValid?ToUi((float)Minimap.Rect().Min.X)-8.f:Canvas->ClipX-Gap;
	TArray<FString> LogisticsLines;
	if (Logistics.bVisible)
	{
		LogisticsLines.Add(TEXT("Logistics"));
		for (const FTCLogisticsEntry& Entry : Logistics.Entries)
		{
			LogisticsLines.Add(TC_T(FTCAlerts::UnitName(S,&M->UnitTable,Entry.Type,S.You))+FString::Printf(TEXT(" #%d"),Entry.Id));
			for (const FString& Detail : Entry.Lines) LogisticsLines.Add(Detail);
		}
		if (Logistics.bTrucks)
		{
			LogisticsLines.Add(FTCLogisticsCard::Hint);
			FTCHudFit::FControl Resume; Resume.Label=FTCLogisticsCard::ResumeLabel; Resume.Command=FTCUiCommand(ETCUiAction::Resume);
			Above.Add(Resume);
		}
	}
	int32 TotalCards=0; for (const FTCRecruitGroup& Group : Card.Groups) TotalCards+=Group.Cards.Num();
	DrawBottomLayout(FTCHudFit::RecruitPanel(Canvas->ClipX,Canvas->ClipY,PanelRight,Headings,TabControls,CardLists,Above,TypeMeasure,UiNow,LogisticsLines,TotalCards),Font);

}

void ATCHUD::GroundLine(const ATCTerrain& T, const TArray<FVector2D>& Points, bool bClosed, const FLinearColor& Colour, float Thickness, float Lift)
{
	const int32 N = Points.Num();
	if (N < 2) return;
	FVector Prev = FVector::ZeroVector;
	bool bPrev = false;
	auto Visit = [&](FVector2D P)
	{
		const FVector Sc = Project(ATCTerrain::ToWorld(P.X, P.Y, T.HeightAt(P.X, P.Y) + Lift));
		if (bPrev && Sc.Z > 0 && Prev.Z > 0) DrawLine(Prev.X, Prev.Y, Sc.X, Sc.Y, Colour, Thickness);
		Prev = Sc; bPrev = true;
	};
	Visit(Points[0]);
	for (int32 I = 1; I < N + (bClosed ? 1 : 0); I++)
	{
		const FVector2D A = Points[I - 1], B = Points[I % N];
		const int32 Steps = FMath::Clamp(FMath::CeilToInt(FVector2D::Distance(A, B) / 2.0), 1, 2000);
		for (int32 K = 1; K <= Steps; K++) Visit(A + (B - A) * (double(K) / Steps));
	}
}

void ATCHUD::GroundRing(const ATCTerrain& T, FVector2D At, float Radius, const FLinearColor& Colour, float Thickness)
{
	TArray<FVector2D> Pts;
	constexpr int32 Segments = 32;
	for (int32 I = 0; I < Segments; I++) { const float A = 2.f * PI * I / Segments; Pts.Add(At + FVector2D(Radius * FMath::Cos(A), Radius * FMath::Sin(A))); }
	GroundLine(T, Pts, true, Colour, Thickness);
}

void ATCHUD::DrawWorldOverlays(UFont* Font)
{
	ATCGameMode* M = GetWorld()->GetAuthGameMode<ATCGameMode>();
	ATCPlayerController* PC = Cast<ATCPlayerController>(GetOwningPlayerController());
	if (!M || !PC || !M->Terrain) return;
	const ATCTerrain& T = *M->Terrain;
	const FTCMatchState& S = M->State;
	auto Label = [&](FVector2D At, const FString& L, const FLinearColor& C)
	{
		const FVector Sc = Project(ATCTerrain::ToWorld(At.X, At.Y, T.HeightAt(At.X, At.Y) + 1.f));
		FLinearColor Lc = C; Lc.A *= M->WarMap.LabelFade(); // round 14: markers.js fadeLabels while the paper war map comes
		if (Sc.Z > 0) DrawLabel(L, Lc, Sc.X + 6.f, Sc.Y - 6.f, Font, 1.f);
	};
	// round 17 (markers): buildHQ's reinforce zone, ring, flag and tag and the resource nodes' brackets and tags under
	// everything else (TCMarkersHud), then World Conquest's regions as world-regions.js draws them: the border, the
	// capture point's ring over its wash, track, arc and contested ring, its flag and its two tags. Only regions whose
	// claim point projects within three viewports of the screen are drawn (a cost limit of this client).
	{
		FTCMarkerPainter Marks = TCMarkersHud::Painter(Canvas, *M);
		TCMarkersHud::DrawHqCamps(Marks, *M, Font);
		TCMarkersHud::DrawNodes(Marks, *M, Font);
		TCMarkersHud::DrawWorldRegions(Marks, *M, Font);
	}
	// client/markers.js capturePoint: the authored capture points (slice 9)
	DrawCapturePoints(Font);
	// client/main.js frame: the armed building's footprint under the pointer, green where the browser's placement
	// check passes and red where it refuses (the server decides again)
	if (PC->Aim == ETCAim::Build && PC->bPointerGround)
	{
		const FTCPlacement P = M->Build.Placement(S, PC->AimBuild, PC->PointerGround);
		if (!P.bThrows) bLastPreviewOk = P.bOk;
		const FLinearColor C(bLastPreviewOk ? FColor(0x7a, 0xcb, 0x82) : FColor(0xdc, 0x4a, 0x3c));
		if (P.bHasFootprint && S.Map.W > 0)
		{
			const float X0 = (P.Cell % S.Map.W) * FTCMatchState::CellMetres, Z0 = (P.Cell / S.Map.W) * FTCMatchState::CellMetres, Side = P.Size * FTCMatchState::CellMetres;
			GroundLine(T, { FVector2D(X0, Z0), FVector2D(X0 + Side, Z0), FVector2D(X0 + Side, Z0 + Side), FVector2D(X0, Z0 + Side) }, true, C, 3.f);
		}
		GroundRing(T, P.At, 0.8f, C, 2.f);
	}
	// client/main.js drawWorks: the ghost of every planned or started work still to dig, faint, in its colour
	auto Cells = [&](const TArray<int32>& List, const FLinearColor& C, float Thick)
	{
		if (S.Map.W <= 0) return;
		const float Cm = FTCMatchState::CellMetres;
		for (int32 Cell : List)
		{
			const float X0 = (Cell % S.Map.W) * Cm, Z0 = (Cell / S.Map.W) * Cm;
			GroundLine(T, { FVector2D(X0, Z0), FVector2D(X0 + Cm, Z0), FVector2D(X0 + Cm, Z0 + Cm), FVector2D(X0, Z0 + Cm) }, true, C, Thick, 0.15f);
		}
	};
	for (const FTCWorkGhost& G : M->Field.WorkGhosts(S)) Cells(G.Place.Cells, FLinearColor(G.Color) * FLinearColor(1, 1, 1, 0.45f), 1.f);
	// client/main.js frame with a dig aim: the oblong marker along its direction, green or red, with the cells it
	// would dig; with an entrench aim: each planned segment's cells in its colour
	if (PC->Aim == ETCAim::Field && PC->FieldPreview.bShown && !PC->FieldPreview.bThrows)
	{
		const FTCFieldPreview& P = PC->FieldPreview;
		if (P.bMarker && P.bMarkerPlaced)
		{
			const FVector2D Along(FMath::Cos(P.MarkerDir), FMath::Sin(P.MarkerDir)), Across(-Along.Y, Along.X);
			const FVector2D A = Along * (P.MarkerLen / 2), B = Across * (P.MarkerWidth / 2);
			const FLinearColor C(P.MarkerColor);
			GroundLine(T, { P.MarkerAt - A - B, P.MarkerAt + A - B, P.MarkerAt + A + B, P.MarkerAt - A + B }, true, C, 3.f);
			GroundLine(T, { P.MarkerAt + A, P.MarkerAt + A * 1.6 }, false, C, 2.f); // the arrow past its far end
			Cells(P.Placement.Cells, C, 1.5f);
		}
		for (const FTCEntrenchPiece& Piece : P.Pieces) Cells(Piece.Place.Cells, FLinearColor(Piece.Color), 1.5f);
		if (PC->FieldAim.bHasCenter) GroundRing(T, PC->FieldAim.Center, 0.6f, FLinearColor(FColor(0xf4, 0xdc, 0x90)), 2.f);
	}
	// client/main.js strikeShape and syncStrikes: the public warnings of support calls on their way and live, in the
	// caller's colour (red for a foe's), a strip along its direction or a ring
	auto Strip = [&](FVector2D At, double Dir, double Len, double Width, const FLinearColor& C, float Thick, bool bArrow)
	{
		const FVector2D Along(FMath::Cos(Dir), FMath::Sin(Dir)), Across(-Along.Y, Along.X);
		const FVector2D A = Along * (Len / 2), B = Across * (Width / 2);
		GroundLine(T, { At - A - B, At + A - B, At + A + B, At - A + B }, true, C, Thick);
		if (bArrow) GroundLine(T, { At + A, At + A + Along * FMath::Min(6.0, Len * 0.25) }, false, C, Thick); // client/main.js aimMarker's arrow
	};
	for (const FTCStrikeMark& K : M->Powers.Strikes(S))
	{
		const FLinearColor C = K.bFoe ? FLinearColor(FColor(0xd2, 0x32, 0x1e)) : ATCUnitRenderer::SeatColor(K.Owner);
		if (K.bStrip) Strip(K.At, K.Dir, K.Len, K.Width, C, 2.f, false);
		else GroundRing(T, K.At, (float)K.Radius, C, 2.f);
		const FTCSupportDef* D = FTCSupportPowers::Find(K.Kind);
		Label(K.At, K.T > 0 ? FString::Printf(TEXT("%s in %.1f s"), D ? *D->Name : *K.Kind, K.T) : (D ? D->Name : K.Kind), C);
	}
	// client/main.js frame with a support kind or 'area' armed: its mesh under the pointer (or on the pinned centre,
	// turned toward the pointer), a strip with its arrow or a ring
	if (PC->Aim == ETCAim::Support && PC->PowerPreview.bShown && PC->PowerPreview.bPlaced)
	{
		const FTCSupportPreview& P = PC->PowerPreview;
		const FLinearColor C(P.Color);
		if (P.bStrip) Strip(P.At, P.Dir, P.Len, P.Width, C, 3.f, true);
		else GroundRing(T, P.At, (float)P.Radius, C, 2.5f);
		if (PC->PowerAim.bHasCenter) GroundRing(T, PC->PowerAim.Center, 0.6f, C, 2.f);
	}
	// client/formation-preview.js show: the slot boxes, rank ribbons, a chevron per slot and the drag's arrow
	if (PC->FacingPreview.bVisible)
	{
		const FTCFormationPreview& F = PC->FacingPreview;
		FLinearColor C(F.Color);
		C.A = F.Opacity;
		for (const TPair<FVector2D, FVector2D>& L : F.Lines) GroundLine(T, { L.Key, L.Value }, false, C, 2.f, 0.25f);
		for (const FTCFormationPreview::FArrow& A : F.Arrows) GroundLine(T, { A.Tip, A.Left, A.Right }, true, C, 2.f, 0.25f);
	}
	// client/cover-preview.js: the shields and chevrons of the cursor and the flash, each layer at its opacity (this
	// canvas has no depth test, so the through-models layer is drawn the same way, at its own opacity); round 17
	// (markers): as overlay.js ribbons with the shields' 0.3 fill (TCMarkersHud::DrawCoverLayer)
	{
		const FTCCoverView& V = M->Cover.View();
		FTCMarkerPainter Marks = TCMarkersHud::Painter(Canvas, *M);
		auto Layer = [&](const TArray<FTCCoverGlyph>& Glyphs, float Opacity, bool bDepth) { TCMarkersHud::DrawCoverLayer(Marks, Glyphs, Opacity, bDepth); };
		if (V.bVisible)
		{
			if (V.bCursorGround) Layer(M->Cover.CursorMarks().Ground, V.CursorGroundOpacity, true);
			if (V.bCursorThrough) Layer(M->Cover.CursorMarks().Through, V.CursorThroughOpacity, false);
			if (V.bFlashGround) Layer(M->Cover.FlashMarks().Ground, V.FlashGroundOpacity, true);
			if (V.bFlashThrough) Layer(M->Cover.FlashMarks().Through, V.FlashThroughOpacity, false);
		}
	}
	// client/main.js aimShape for an aimed ability: a ring of its radius under the pointer
	if (PC->Aim == ETCAim::Ability && PC->bPointerGround && PC->AimAbilityRadius > 0)
		GroundRing(T, PC->PointerGround, (float)PC->AimAbilityRadius, FLinearColor(FColor(0xff, 0xa0, 0x30)), 2.f);
	// rally points: the army's (shared/sim.js snapshotFor rally) and the selected own buildings' (their queue's rally)
	// are planLayer's flags since round 16 (DrawPlans, TCOrders::Markers)
	// client/logistics.js overlayItems and main.js drawLogisticsRoutes: known stores, the selected trucks' known routes
	if (M->Logistics.OverlayActive())
	{
		const FTCLogisticsOverlay O = M->Logistics.Overlay(S, PC->Selected, [&S](double X, double Z) { return FTCLogistics::KnownByFog(S, X, Z); });
		for (const FTCLogisticsStore& St : O.Stores)
		{
			const FVector2D At(St.X, St.Z);
			GroundRing(T, At, 4.f, FLinearColor(FTCLogistics::StoreRingColor(St.bSource)), 2.f);
			const TArray<FString> L = FTCLogistics::StoreLabels(&St);
			if (L.Num()) Label(At, L[0], FLinearColor(FTCLogistics::StoreRingColor(St.bSource)));
		}
		for (const FTCSupplyRoute& R : O.Routes) GroundLine(T, R.Points, false, FLinearColor(FTCLogistics::RouteColor(R.bManual)), 2.f, 0.6f);
	}
	// slice 9: the selection's plan lines (client/markers.js planLayer) and the unit bars (client/main.js frame)
	DrawPlans();
	DrawHealthBars();
}

void ATCHUD::DrawMinimapOverlays()
{
	// client/main.js drawMinimap's logistics layer while the overlay is on: stores, the selected trucks' routes and
	// the own units cut off from supply (red rings)
	ATCGameMode* M = GetWorld()->GetAuthGameMode<ATCGameMode>();
	ATCPlayerController* PC = Cast<ATCPlayerController>(GetOwningPlayerController());
	if (!M || !PC || !Minimap.Rect().bIsValid) return;
	const FTCMatchState& S = M->State;
	DrawMinimapPoints(); // slice 9: the capture points and the start's spawns
	DrawMinimapPings();  // slice 10: the relayed pings' rings (client/pings.js drawMinimap)
	FVector2D Px, Prev;
	if (M->bArmyRally && Minimap.WorldToPixel(M->ArmyRally, Px)) DrawRect(FLinearColor(FColor(0x9d, 0xd0, 0xff)), Px.X - 2.f, Px.Y - 2.f, 4.f, 4.f);
	// client/main.js drawMinimap, World: each region this side was sent as a disc of the claim radius in its team's
	// colour (a brass ring when locked), and the stations as dark squares with the region's fill
	if (M->WorldRec.IsWorld())
	{
		for (const FTCWorldRegionView& V : M->WorldRec.RegionViews())
		{
			FVector2D Edge;
			if (!Minimap.WorldToPixel(V.At, Px) || !Minimap.WorldToPixel(V.At + FVector2D(TCWorldRecoveryCore::PointRadius, 0), Edge)) continue;
			const float Rad = FMath::Max(2.f, (float)FVector2D::Distance(Px, Edge));
			const float Side = Rad * 1.4f;
			DrawRect(FLinearColor(V.TeamColour), Px.X - Side / 2, Px.Y - Side / 2, Side, Side);
			if (V.bLocked)
			{
				const FLinearColor C(FTCWorldRecovery::LockedColour);
				const float O = Side / 2 + 1.5f;
				DrawLine(Px.X - O, Px.Y - O, Px.X + O, Px.Y - O, C); DrawLine(Px.X + O, Px.Y - O, Px.X + O, Px.Y + O, C);
				DrawLine(Px.X + O, Px.Y + O, Px.X - O, Px.Y + O, C); DrawLine(Px.X - O, Px.Y + O, Px.X - O, Px.Y - O, C);
			}
		}
		for (const FTCWorldStationView& V : M->WorldRec.StationViews())
			if (Minimap.WorldToPixel(V.At, Px))
			{
				DrawRect(FLinearColor(FTCWorldRecovery::StationFrameColour), Px.X - 2.5f, Px.Y - 2.5f, 5.f, 5.f);
				DrawRect(FLinearColor(V.Fill), Px.X - 1.5f, Px.Y - 1.5f, 3.f, 3.f);
			}
	}
	// client/main.js drawMinimap: the strip strikes' outlines ('#ff3020' for a foe's, else the caller's colour)
	for (const FTCStrikeMark& K : M->Powers.Strikes(S))
	{
		if (!K.bMinimap || !K.bStrip) continue;
		const FVector2D Along(FMath::Cos(K.Dir), FMath::Sin(K.Dir)), Across(-Along.Y, Along.X);
		const FVector2D A = Along * (K.Len / 2), B = Across * (K.Width / 2);
		const FVector2D Corners[4] = { K.At - A - B, K.At + A - B, K.At + A + B, K.At - A + B };
		FVector2D Pix[4];
		bool bAll = true;
		for (int32 I = 0; I < 4; I++) bAll &= Minimap.WorldToPixel(Corners[I], Pix[I]);
		if (!bAll) continue;
		const FLinearColor C = K.bFoe ? FLinearColor(FColor(0xff, 0x30, 0x20)) : ATCUnitRenderer::SeatColor(K.Owner);
		for (int32 I = 0; I < 4; I++) DrawLine(Pix[I].X, Pix[I].Y, Pix[(I + 1) % 4].X, Pix[(I + 1) % 4].Y, C, 1.f);
	}
	if (!M->Logistics.OverlayActive()) return;
	const FTCLogisticsOverlay O = M->Logistics.Overlay(S, PC->Selected, [&S](double X, double Z) { return FTCLogistics::KnownByFog(S, X, Z); });
	for (const FTCLogisticsStore& St : O.Stores)
		if (Minimap.WorldToPixel(FVector2D(St.X, St.Z), Px)) DrawRect(FLinearColor(FTCLogistics::MinimapStoreColor(St.bSource)), Px.X - 2.5f, Px.Y - 2.5f, 5.f, 5.f);
	for (const FTCSupplyRoute& R : O.Routes)
		for (int32 I = 0; I < R.Points.Num(); I++)
		{
			if (!Minimap.WorldToPixel(R.Points[I], Px)) continue;
			if (I > 0) DrawLine(Prev.X, Prev.Y, Px.X, Px.Y, FLinearColor(FTCLogistics::MinimapRouteColor()), 1.f);
			Prev = Px;
		}
	for (int32 Id : M->Logistics.CutOffUnits(S))
		if (const FTCUnit* U = S.Units.Find(Id))
			if (Minimap.WorldToPixel(FVector2D(U->X, U->Z), Px))
			{
				const FLinearColor C(FTCLogistics::MinimapCutColor());
				DrawLine(Px.X - 3, Px.Y - 3, Px.X + 3, Px.Y - 3, C); DrawLine(Px.X + 3, Px.Y - 3, Px.X + 3, Px.Y + 3, C);
				DrawLine(Px.X + 3, Px.Y + 3, Px.X - 3, Px.Y + 3, C); DrawLine(Px.X - 3, Px.Y + 3, Px.X - 3, Px.Y - 3, C);
			}
}

void ATCHUD::DrawHUD()
{
	Super::DrawHUD();
	Buttons.Reset();
	Panels.Reset();
	EndUi(); // a frame starts unscaled
	Frames++;
	ATCGameMode* M = GetWorld()->GetAuthGameMode<ATCGameMode>();
	ATCPlayerController* PC = Cast<ATCPlayerController>(GetOwningPlayerController());
	if (!M || !PC || !Canvas) return;
	UFont* Font = GEngine->GetSmallFont();
	UFont* Big = GEngine->GetMediumFont() ? GEngine->GetMediumFont() : Font;
	const FTCMatchState& S = M->State;
	const bool bStrip = S.bHasMatch && S.Snapshots > 0;
	// round 15: a match's first frame starts the reference's start band (StartBand)
	if (S.bHasMatch && !bHadMatch) { bStartBand = true; StartBandFrames = 0; PC->RecruitGroupOpen=-1; }
	bHadMatch = S.bHasMatch;
	// round 14: the lobby's background over its battlefield (index.html #overlay #16191b, the backdrop at opacity 0.5)
	if (!S.bHasMatch) M->Backdrop.DrawDim(Canvas);
	TArray<FString> Lines;
	// the connection, room, seat and tick lines and the key reminder are this client's own diagnostics, with no browser
	// text: drawn as they are, neither translated nor logged as Spanish gaps (Diagnostics counts them from the top)
	int32 Diagnostics = 0;
	Lines.Add(FString::Printf(TEXT("Three Crossroads native slice | %s"), *M->ConnectionStatus()));
	Lines.Add(FString::Printf(TEXT("room %s | %s | mode %s | %s%s"), *S.RoomCode, *S.RoomState, *S.Mode, S.bAmHost ? TEXT("host") : TEXT("guest"), S.bSpectator ? TEXT(" | spectating") : TEXT("")));
	if (S.bHasMatch)
	{
		int32 Own = 0;
		for (const auto& P : S.Units) if (S.IsOwn(P.Value)) Own++;
		const FString Seat = S.bSpectator ? FString(TEXT("watching")) : S.SeatConfirmed() ? FString::Printf(TEXT("seat %d"), S.You) : FString(TEXT("seat unconfirmed"));
		// hud.js drawEcon's income line beside MP (absent for a spectator)
		const FString Income = FTCObjectives::EconomyLine(S);
		// the money as the browser writes it ('{n} MP', es.js '{0} MO'), the one piece of this diagnostic line in the
		// player's language, so it reads like the score strip and the recruit card beside it
		Lines.Add(FString::Printf(TEXT("%s | %s | tick %d | %s%s | entity rows %d (own %d, includes buildings) | selected %d"), *S.Map.Name, *Seat, S.Tick, *TC_T(FString::Printf(TEXT("%d MP"), S.Mp)),
			Income.IsEmpty() ? TEXT("") : *FString::Printf(TEXT(" %s"), *TC_T(Income)), S.Units.Num(), Own, PC->Selected.Num()));
		Diagnostics = Lines.Num();
		// the epilogue's stamp says who won; this line stays for a decided match without one
		if (S.Winner >= -1 && !S.Objectives.Epilogue().bActive) Lines.Add(S.Winner == -1 ? TEXT("Match over: draw") : FString::Printf(TEXT("Match over: team %d wins"), S.Winner));
		// client/hud.js the World Conquest score, until the score strip (which shows it too) has a snapshot to read
		const FTCWorldScoreLead Lead = M->WorldRec.ScoreLead();
		if (Lead.bShown && !bStrip)
		{
			TArray<FString> Blocks;
			for (const FTCWorldTeamScore& T : M->WorldRec.TeamScores())
			{
				TArray<FString> Out;
				for (int32 K = 0; K < T.Members.Num(); K++) if (T.Held.IsValidIndex(K) && !T.Held[K].IsEmpty()) Out.Add(FString::Printf(TEXT("seat %d %s"), T.Members[K], *T.Held[K]));
				Blocks.Add(FString::Printf(TEXT("team %.0f %s %s %s%s"), T.Team, *T.Label, *T.Number, *T.Unit, Out.Num() ? *FString::Printf(TEXT(" (%s)"), *FString::Join(Out, TEXT(", "))) : TEXT("")));
			}
			Lines.Add(FString::Printf(TEXT("%s: %s  |  %s"), *Lead.Mode, *Lead.Clock, *FString::Join(Blocks, TEXT("  |  "))));
		}
	}
	if (!Diagnostics) Diagnostics = Lines.Num();
	// DrawRoomBanners owns #pauseBanner. Repeating it in this diagnostic column drew a ghost pause sentence.
	// client/light.js notice(): the drop to Low's message in the status line for 6 s
	if (!PC->GraphicsNotice.IsEmpty() && PC->GraphicsNoticeAt >= 0 && GetWorld()->GetRealTimeSeconds() - PC->GraphicsNoticeAt < FTCGraphics::NoticeSeconds) Lines.Add(PC->GraphicsNotice);
	int32 KeyLine = INDEX_NONE;
	if (S.bHasMatch)
	{
		// the full key list is the Controls sheet (F1), as in the browser
		KeyLine = Lines.Num();
		Lines.Add(FString::Printf(TEXT("F1 controls | Space newest alert | Alt+H alert history | Shift+Space follow | H home | Ctrl+A army | . idle%s"),
			TCArmyCommands::ClassicLobby(S) ? TEXT(" | Engineers: J depot, K barracks, L motor pool, O airfield, Y flak") : TEXT("")));
	}
	float Y = 12.f;
	FTCMinimapView View;
	// round 15: the status lines as shown (each diagnostic as it is, each browser text through TC_T, the order note
	// through FTCLocale::OrderNote), then drawn in one column
	TArray<FStatusLine> Status;
	for (int32 I = 0; I < Lines.Num(); I++)
	{
		const bool bAsIs = I < Diagnostics || I == KeyLine;
		const FString Shown = bAsIs ? Lines[I] : TC_T(Lines[I]);
		if (!bAsIs) NoteKept(Lines[I], Shown);
		Status.Add({ Shown, Text, bAsIs });
	}
	if (!S.DenyReason.IsEmpty() || !S.DenyCmd.IsEmpty())
	{
		const FString Deny = FString::Printf(TEXT("Refused (%s): %s"), *S.DenyCmd, *TCOrders::DenySentence(S.DenyReason, S.DenyCmd));
		const FString Shown = TC_T(Deny);
		NoteKept(Deny, Shown);
		Status.Add({ Shown, Warn });
	}
	// What the last order or button did, for a few seconds: what was sent, or why nothing was (orange)
	const double Now = GetWorld()->GetTimeSeconds();
	if (!PC->LastOrderNote.IsEmpty() && Now - PC->LastOrderNoteTime < OrderNoteSeconds)
	{
		const FString Shown = FTCLocale::OrderNote(PC->LastOrderNote);
		NoteKept(PC->LastOrderNote, Shown);
		Status.Add({ Shown, PC->bLastOrderSent ? Good : Warn });
	}
	if (S.bHasMatch)
	{
		// the world's own overlays first, under every panel; then the score strip at the top centre
		View.YawDegrees = PC->ViewYaw();
		Minimap.ViewFromCanvas(Canvas, M->Terrain, View);
		TCMarkersHud::Verify(*M, PC->Selected); // round 17: -TCMarkersSelect= and -TCMarkersLog (verification only)
		DrawWorldOverlays(Font);
		// round 14: the paper war map over the world and its markers, under every panel (client/map-view.js)
		M->WarMap.Draw(Canvas, *M, *PC);
		BeginUi(FTCSettingsStore::UIScaleFactor(PC->UiScalePercent));
		// round 16: the match controls in the browser's places (SplitMatchControls). The settings live in the match menu
		// whenever there is one to open; the corner's closed Menu panel sets how far left the strip may reach
		bSettingsInMenu = M->Controls.MenuButton().bShown;
		TArray<FTCControlRow> CornerRows, SandboxRows;
		SplitMatchControls(CornerRows, SandboxRows);
		float CornerClosedH = 0.f;
		{
			TArray<FTCControlRow> Closed;
			for (const FTCControlRow& R : CornerRows) { bool bMenu = false; for (const FTCControlItem& I : R.Items) bMenu |= I.Key == TEXT("Menu"); if (bMenu) Closed.Add(R); }
			if (!Closed.Num()) Closed = CornerRows;
			const FVector2D C = MeasureControlRows(Font, Closed);
			CornerRight = C.X > 0.f ? 8.f + C.X : 0.f;
			CornerClosedH = C.Y;
		}
		const int32 StripFrom = Panels.Num();
		const float StripBottom = DrawScoreStrip(Font, Big);
		// camera.js measures #top's border box. DrawScoreStrip also returns the gap for the next card.
		BandTop = bStrip ? TopStackBottomPx : 0.f;
		// round 15: the strip's left edge in screen pixels (its panels are recorded in screen pixels)
		float StripLeft = Canvas->ClipX * UiNow, StripRight = 0.f, StripLow = StripBottom * UiNow;
		for (int32 K = StripFrom; K < Panels.Num(); K++) { StripLeft = FMath::Min(StripLeft, (float)Panels[K].Min.X); StripRight = FMath::Max(StripRight, (float)Panels[K].Max.X); }
		if (StripRight <= 0.f) StripRight = Canvas->ClipX * UiNow / 2;
		// client/camera.js showFollow: the follow chip under the strip
		const FString Chip = PC->FollowChip();
		if (!Chip.IsEmpty())
		{
			float TW = 0, TH = 0;
			const FString L = Chip + TEXT("  (Shift+Space or pan to stop)");
			LabelSize(L, TW, TH, Font, 1.f);
			AddPanel((Canvas->ClipX - TW) / 2 - Pad, StripBottom, TW + 2 * Pad, Line + 2.f, Panel);
			DrawLabel(L, Good, (Canvas->ClipX - TW) / 2, StripBottom + 1.f, Font, 1.f);
			StripLow = (StripBottom + Line + 2.f) * UiNow;
		}
		const float Ui = UiNow;
		EndUi();
		// the minimap (bottom right): the recruit card fits between it and the match panel
		Minimap.UiScale = Ui;
		Minimap.bDeferViewOutline = true; // round 17: client/main.js drawMinimap strokes the view last
		Minimap.Draw(Canvas, S, M->UnitTable, PC->Selected, View);
		DrawMinimapOverlays();
		Minimap.DrawViewOutline(Canvas, View);
		BeginUi(Ui);
		// round 15: the recruit card always sits beside the match panel at the bottom, between it and the minimap
		// (#buy at every UI scale); the match panel narrows only when the window leaves the card less than RecruitMinW
		const float MatchW = FTCHudFit::LeftW;
		// round 16: the match panel stays under the strip (and its notice lines, the boss bar and the follow chip) where
		// they share columns; under the sandbox panel too when that sits under the strip over it. The sandbox panel takes
		// #sandbox's place (230 px over the bottom, centred) when that is clear of the strip, the match panel and the
		// recruit card (last frame's), else it goes under the strip
		const float StripLowUi = StripLow / UiNow;
		const bool bShareStrip = StripRight > StripLeft && StripLeft / UiNow < 8.f + MatchW;
		float PanelLimit = bShareStrip ? StripLowUi + 4.f : 0.f;
		SandboxBox = FBox2D(ForceInit);
		const float MatchTop = DrawMatchPanel(Font, MatchW, PanelLimit) * UiNow; // screen pixels
		const int32 PanelsBeforeRecruit = Panels.Num();
		DrawRecruitPanel(Font, 8.f + MatchW + Gap, Canvas->ClipY - Gap);
		// round 14: camera.js band: from the top panels down to the recruit bar (#buy), the whole height when it is empty
		float RecruitTop = Canvas->ClipY * UiNow; // screen pixels, as the panels are kept
		RecruitBox = FBox2D(ForceInit);
		for (int32 K = PanelsBeforeRecruit; K < Panels.Num(); K++) { RecruitTop = FMath::Min(RecruitTop, (float)Panels[K].Min.Y); RecruitBox += Panels[K]; }
		TArray<FBox2D> Occupied;
		for (const FBox2D& B : Panels) Occupied.Add(FBox2D(B.Min/UiNow,B.Max/UiNow));
		if (Minimap.Rect().bIsValid) Occupied.Add(FBox2D(Minimap.Rect().Min/UiNow-FVector2D(4,4),Minimap.Rect().Max/UiNow+FVector2D(4,4)));
		const FTCMeasure BottomMeasure=FTCType::IsLoaded()?FTCType::Measure():FTCPaint::CanvasMeasure(Font);
		FBox2D Economy(ForceInit);
		for (const FBox2D& B : Occupied) if (B.Min.X>Canvas->ClipX/2 && B.Min.Y<=8.1f) Economy+=B;
		if (M->BossBar.Shown())
		{
			const FTCPaint Boss=FTCHudFit::BossCard(M->BossBar,FVector2D(Canvas->ClipX,Canvas->ClipY),StripLowUi,Economy,Occupied,FPlatformTime::Seconds(),BottomMeasure,UiNow);
			FTCType::Draw(Boss,Canvas,Font,UiNow);
			for (const FBox2D& B : Boss.Panels)
			{
				Panels.Add(ToScreen(B)); Occupied.Add(B);
				if (B.Min.X<Canvas->ClipX/2 && B.Max.X>Canvas->ClipX/2) TopStackBottomPx=FMath::Max(TopStackBottomPx,(float)B.Max.Y*UiNow);
			}
		}
		FTCHudFit::FBottomLayout SandboxLayout;
		if (SandboxRows.Num())
		{
			TArray<FTCHudFit::FControl> Controls;
			for (const FTCControlRow& Row : SandboxRows)
			{
				if (!Row.Text.IsEmpty()) { FTCHudFit::FControl C; C.Label=Row.Text; Controls.Add(C); }
				for (const FTCControlItem& Item : Row.Items)
				{
					FTCHudFit::FControl C; C.Label=Item.Label; C.Hover=Item.Hover; C.Command=ControlCommand(Item.Key,Item.Slot); C.bEnabled=Item.bEnabled; Controls.Add(C);
				}
			}
			SandboxLayout=FTCHudFit::SandboxToolbar(FVector2D(Canvas->ClipX,Canvas->ClipY),CornerRight,
				StripCard.bIsValid?StripCard.Min.X/UiNow:Canvas->ClipX/2,Controls,Occupied,BottomMeasure,UiNow);
			if (SandboxLayout.Paint.Panels.Num()) SandboxBox=SandboxLayout.Paint.Panels[0];
		}
		FVector2D Band(BandTop, RecruitTop);
		if (bStartBand)
		{
			// round 15: the band camera.js measured at receiveStart, until the controller has taken it (StartBand)
			const bool bBuyShown = !S.bSpectator && !TCArmyCommands::ClassicLobby(S);
			Band = FTCHudFit::InitialCameraBand(Canvas->ClipY * UiNow, UiNow, bBuyShown);
			const FVector2D Taken = PC->ClearBand();
			// kept until the strip is up too: the controller measures again when the reported top first turns positive
			// (round 14's glue for a strip that arrives with the first snapshot), which must not happen after this
			if (++StartBandFrames > 600 || (bStrip && FMath::Abs(Taken.X - Band.X) < 0.5f && FMath::Abs(Taken.Y - Band.Y) < 0.5f))
			{
				bStartBand = false;
				UE_LOG(LogTC, Log, TEXT("hud: camera band at the match start %.1f to %.1f px (client/main.js receiveStart: empty #scores, %s), taken after %d frames; this HUD's own band now %.1f to %.1f px"),
					Band.X, Band.Y, bBuyShown ? TEXT("#buy with its group tabs") : TEXT("no #buy for spectators, Classic or World"), StartBandFrames, BandTop, RecruitTop);
			}
		}
		PC->SetClearBand(Band.X, Band.Y);
		// round 16: the sandbox panel (#sandbox), then the corner: the Menu button (#util) and, open, the menu (#menu)
		// under it. The open menu is the browser's dropdown over the HUD (z-index 20): under the strip when it fits there
		// beside the strip's first card, else straight under the Menu button over the strip; drawn last of the panels,
		// so its buttons take the clicks (CommandAt reads the last drawn first)
		if (SandboxBox.bIsValid) DrawBottomLayout(SandboxLayout,Font);
		// the corner's place now (drawn after the status column and the alert feed, over them): the status column keeps
		// to the closed Menu panel (CornerRight), the open menu covering it as the browser's dropdown covers the HUD
		const FVector2D CtlSize = MeasureControlRows(Font, CornerRows);
		float CtlTop = Y + 4.f;
		if (CtlSize.X > 0.f)
		{
			const bool bOverStrip = StripRight > StripLeft && StripLeft / UiNow < 8.f + CtlSize.X + 4.f && CtlTop < StripLowUi;
			if (bOverStrip && StripLowUi + 4.f + CtlSize.Y <= Canvas->ClipY - Gap) CtlTop = StripLowUi + 4.f;
		}
		float CtlBottom = CtlSize.X > 0.f ? (CornerRight > 0.f && M->Controls.MenuOpen() ? Y + 4.f + MeasureControlRows(Font, CornerRows.Num() ? TArray<FTCControlRow>{ CornerRows[0] } : CornerRows).Y : CtlTop + CtlSize.Y) * UiNow : 0.f;
		// round 16 (lead, controls' stats box on hudfit's corner): F2's box under the closed Menu panel (stats.js place:
		// under #util; the open menu is a dropdown over it), the status column under both
		{
			const float ClosedBottom = CtlSize.X > 0.f ? (CtlTop + (CornerClosedH > 0.f ? CornerClosedH : CtlSize.Y)) * UiNow : 0.f;
			const float ClosedRight = CtlSize.X > 0.f ? (CornerRight > 0.f ? CornerRight : 8.f + CtlSize.X) * UiNow : 0.f;
			CtlBottom = FMath::Max(CtlBottom, DrawStatsBox(Font, ClosedBottom, ClosedRight));
		}
		// under the strip, the status column's last place also clears the sandbox panel (screen pixels): it starts under
		// the strip, or under the sandbox panel when it would reach that panel (#sandbox's own place 230 px over the
		// bottom, or the panel's place under the strip)
		const FBox2D SandboxScreen = SandboxBox.bIsValid ? FBox2D(SandboxBox.Min * UiNow, SandboxBox.Max * UiNow) : FBox2D(ForceInit);
		auto UnderStripTop = [&](int32 Rows)
		{
			const float Top = StripLow + 6.f;
			if (SandboxScreen.bIsValid && Top < SandboxScreen.Max.Y && Top + Rows * Line > SandboxScreen.Min.Y - 4.f) return (float)SandboxScreen.Max.Y + 6.f;
			return Top;
		};
		// round 15: the status column, unscaled (this client's own overlay, outside the browser's zoomed #hud), its lines
		// wrapped at their columns. In the top left corner beside the strip (no wider than ColumnShare of the window, so
		// the paper war map's top left stays clear) when it ends above the match panel; else in the top right corner
		// beside the strip, above the recruit card (the reference's #econ corner, which this HUD leaves empty); else
		// under the strip across the window, as before
		{
			const float Ui2 = UiNow;
			EndUi();
			auto Rows = [&](float W, bool bDiag = true) { TArray<FStatusLine> Out; for (const FStatusLine& L : Status) if (bDiag || !L.bDiag) for (const FString& P : WrapShown(L.Shown, W, Font)) Out.Add({ P, L.Colour, L.bDiag }); return Out; };
			const float LeftW = FMath::Min(StripLeft - 24.f, ColumnShare * Canvas->ClipX), RightW = FMath::Min(Canvas->ClipX - StripRight - 24.f, ColumnShare * Canvas->ClipX);
			TArray<FStatusLine> Col;
			const float LeftTop = CtlBottom > 0.f ? CtlBottom + 6.f : 12.f; // under the match controls
			float Cx = 12.f, Cy = 12.f;
			bool bPlaced = false;
			auto ClearStatus=[&](float X,float Top,float W,int32 Count)
			{
				const FBox2D Box=TCHudBottom::Box(X,Top,W,Count*Line);
				for (const FBox2D& B : Panels) if (TCHudBottom::Overlap(Box,B)) return false;
				if (SandboxScreen.bIsValid && TCHudBottom::Overlap(Box,SandboxScreen)) return false;
				return true;
			};
			if (LeftW >= ColumnMinW) { Col = Rows(LeftW); bPlaced = LeftTop + Col.Num() * Line <= MatchTop - 4.f && ClearStatus(12,LeftTop,LeftW,Col.Num()); if (bPlaced) Cy = LeftTop; }
			if (!bPlaced && RightW >= ColumnMinW) { Col = Rows(RightW); Cx = StripRight + 12.f; bPlaced = 12.f + Col.Num() * Line <= RecruitTop - 4.f && ClearStatus(Cx,12,RightW,Col.Num()); }
			if (!bPlaced)
			{
				Col = Rows(Canvas->ClipX - 24.f); Cx = 12.f; Cy = UnderStripTop(Col.Num());
				// round 16: where even that runs into the match panel or the recruit card (a large UI scale in a small
				// window), the column keeps the lines the player needs (pause, notices, refusals, the order note) and
				// leaves out this client's own diagnostics, which the log keeps; before, they were drawn over the panel
				if (Cy + Col.Num() * Line > FMath::Min(MatchTop, RecruitTop) - 4.f || !ClearStatus(Cx,Cy,Canvas->ClipX-24,Col.Num()))
				{
					Col = Rows(Canvas->ClipX - 24.f, false);
					Cy = UnderStripTop(Col.Num());
					if (!bLoggedDiagDrop) { bLoggedDiagDrop = true; UE_LOG(LogTC, Log, TEXT("hud: no room for the status column's diagnostics at %.0f%% in %.0f x %.0f; they stay in the log"), Ui2 * 100.f, Canvas->ClipX, Canvas->ClipY); }
				}
			}
			for (const FStatusLine& L : Col) { DrawShown(L.Shown, L.Colour, Cx, Cy, Font, 1.f); Cy += Line; }
			BeginUi(Ui2);
		}
		// client/alerts.js: the alert feed over the minimap and the Alt+H history
		DrawAlertFeed(Font);
		DrawEpilogue(Font, Big);
		// round 16: the corner last (#util, and #menu at z-index 20 over the HUD), on the browser's solid panel colour
		// (--panel-solid) while the menu is open, so what it covers does not show through
		if (CtlSize.X > 0.f)
		{
			if (M->Controls.MenuOpen()) DrawRect(FLinearColor(FColor(25, 28, 30, 245)), 8.f, CtlTop, CtlSize.X, CtlSize.Y);
			DrawControlRows(Font, 8.f, CtlTop, 0.f, CornerRows);
		}
	}
	else if (M->bRoomBrowserOpen)
	{
		Minimap.Reset();
		DrawControlRows(Font, 8.f, Y + 4.f, 700.f, M->BrowserRows()); // round 15: the room directory (public-lobby.html)
	}
	else
	{
		// round 15 (lead): the lobby at scale 1: client/hud.js applyScale sets --ui on #hud only, and the lobby card and
		// its report (#overlay) are never zoomed (before, a 150% UI left the report no room in a 1280 px window)
		BeginUi(1.f);
		for (const FStatusLine& L : Status) { DrawShown(L.Shown, L.Colour, 12.f, Y, Font, 1.f); Y += Line; }
		Minimap.Reset();
		// round 15: no band outside a match, so the next match's first frame starts camera.js's measure afresh
		PC->SetClearBand(0.f, 0.f);
		// round 16: the lobby panel narrows (down to LobbyPanelMinW) so the report's table fits beside it
		const float LobbyW = LobbyWidthBesideReport(Font);
		const float LobbyBottom = DrawLobby(Font, Y + 4.f, LobbyW);
		// client/report.js: the result card beside the lobby after a decided match (under it in a narrow window)
		DrawMatchReport(Font, Y + 4.f, LobbyBottom, 8.f + LobbyW + 8.f);
	}
	EndUi();
	DrawRoomBanners(Font); // round 16: #roomBanners (z-index 25: over the HUD, under the sheets)
	DrawControlsSheet(Font, Big);
	DrawStatsSheet(Font); // round 16: Menu > Stats overlay

	if (PC->bDragging && PC->bDragMoved)
	{
		const FVector2D Lo(FMath::Min(PC->DragFrom.X, PC->DragTo.X), FMath::Min(PC->DragFrom.Y, PC->DragTo.Y));
		const FVector2D Size = (PC->DragFrom - PC->DragTo).GetAbs();
		DrawRect(FLinearColor(0.6f, 1.f, 0.5f, 0.15f), Lo.X, Lo.Y, Size.X, Size.Y);
	}
	// client/main.js marker: a ring on the ground at the click (or the attacked unit), 0.3 m up, that grows from 0.5 to
	// 2.5 m and fades out over 0.6 s, drawn over the terrain, in the order's colour. Only an order that was sent gets one.
	const double Age = Now - PC->LastOrderTime;
	if (M->Terrain && S.bHasMatch && PC->LastOrderAt.X >= 0 && Age >= 0 && Age < MarkerSeconds)
	{
		const float Life = (float)(1.0 - Age / MarkerSeconds), Radius = 0.5f + (1.f - Life) * 2.f;
		const FVector2D At = PC->LastOrderAt;
		const float Up = M->Terrain->HeightAt(At.X, At.Y) + 0.3f;
		FColor C = PC->LastOrderColor;
		C.A = (uint8)FMath::RoundToInt(255.f * Life);
		const FLinearColor Colour(C);
		constexpr int32 Segments = 32;
		FVector Prev = FVector::ZeroVector;
		for (int32 I = 0; I <= Segments; I++)
		{
			const float A = 2.f * PI * I / Segments;
			const FVector P = Project(ATCTerrain::ToWorld(At.X + Radius * FMath::Cos(A), At.Y + Radius * FMath::Sin(A), Up));
			if (I > 0 && P.Z > 0 && Prev.Z > 0) DrawLine(Prev.X, Prev.Y, P.X, P.Y, Colour, 2.f);
			Prev = P;
		}
	}
}

// ---------------------------------------------------------------------------------------------------------------------
#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCHudBottomLayoutTest, "ThreeCrossroads.HUD.BottomLayout", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCHudBottomLayoutTest::RunTest(const FString&)
{
	using namespace FTCHudFit;
	if (!FTCType::IsLoaded() || !FTCLocale::IsLoaded()) { AddError(TEXT("The reference fonts and dictionary are required for the HUD fit check")); return false; }
	const FTCMeasure Measure=FTCType::Measure();
	for (ETCLanguage Language : {ETCLanguage::English,ETCLanguage::Spanish})
	for (float Scale : {1.f,1.5f})
	for (bool Open : {false,true})
	for (int32 Groups : {1,3})
	for (bool WithLogistics : {false,true})
	{
		FTCLocale::FScopedLanguage Lang(Language);
		const FVector2D View(1280/Scale,720/Scale);
		TArray<FSelection> Selected={{TEXT("rifle"),TEXT("Grenadiers"),1,100,100,{TEXT("In cover"),TEXT("Auto-retreat")}}};
		if (Groups==3)
		{
			Selected.Add({TEXT("mg"),TEXT("MG 42 Team"),1,100,100,{}});
			Selected.Add({TEXT("sniper"),TEXT("Scharfschützen"),1,100,100,{}});
		}
		TArray<FControl> Orders;
		auto Order=[&](const TCHAR* Label,const TCHAR* Icon,const TCHAR* Key,const TCHAR* Value,ETCUiAction Action)
		{
			FControl C; C.Label=Label; C.Icon=Icon; C.Key=Key; C.Value=Value; C.Command=FTCUiCommand(Action); Orders.Add(C);
		};
		Order(TEXT("Retreat"),TEXT("retreat"),TEXT("R"),TEXT(""),ETCUiAction::Retreat);
		Order(TEXT("Attack-move"),TEXT("amove"),TEXT("G"),TEXT(""),ETCUiAction::AttackMove);
		Order(TEXT("Stop"),TEXT("stop"),TEXT("X"),TEXT(""),ETCUiAction::Stop);
		Order(TEXT("Hold fire"),TEXT("holdFire"),TEXT("⇧F"),TEXT("Off"),ETCUiAction::Stance);
		Order(TEXT("Hold position"),TEXT("holdPos"),TEXT("⇧G"),TEXT("Off"),ETCUiAction::Stance);
		Order(TEXT("Auto-retreat"),TEXT("autoRetreat"),TEXT("⇧X"),TEXT("On"),ETCUiAction::Stance);
		Order(TEXT("Take cover"),TEXT("takecover"),TEXT("⇧C"),TEXT(""),ETCUiAction::Cover);
		Order(TEXT("Formation"),TEXT("f_wedge"),TEXT(""),TEXT("Form"),ETCUiAction::Formation);
		Order(TEXT("Build"),TEXT("sandbags"),TEXT(""),TEXT("Build"),ETCUiAction::BuildMenu);
		Order(TEXT("Trench"),TEXT("e_zigzag"),TEXT(""),TEXT("Trench"),ETCUiAction::TrenchMenu);
		Order(TEXT("Grenade"),TEXT("grenade"),TEXT("F A"),TEXT(""),ETCUiAction::Ability);
		FBottomLayout L=LeftPanel(View.Y,TEXT("1 unit selected"),Selected,{},Orders,{},Measure,Scale);
		const TCHAR* Names[]={TEXT("Infantry"),TEXT("Support weapons"),TEXT("Vehicles"),TEXT("Aircraft")};
		const TCHAR* Icons[]={TEXT("rifle"),TEXT("at"),TEXT("tank"),TEXT("fighter")};
		TArray<FControl> Tabs;
		for (int32 I=0; I<4; I++) { FControl C; C.Label=Names[I]; C.Icon=Icons[I]; C.Command=FTCUiCommand(ETCUiAction::RecruitGroup); C.Command.Group=I; Tabs.Add(C); }
		FControl Recruit; Recruit.Label=TEXT("Recruit"); FControl Rally; Rally.Label=TEXT("Rally (Shift+H)"); Rally.Command=FTCUiCommand(ETCUiAction::Rally);
		TArray<TArray<FControl>> Lists;
		if (Open)
		{
			TArray<FControl> Cards;
			for (const TCHAR* Name : {TEXT("Rifle Squad"),TEXT("Flamethrower Squad"),TEXT("Sniper"),TEXT("Medic Team")}) { FControl C; C.Label=Name; C.Icon=TEXT("rifle"); C.Value=TEXT("100 MP"); C.Command=FTCUiCommand(ETCUiAction::Recruit); Cards.Add(C); }
			Lists.Add(Cards);
		}
		const TArray<FString> Logistics=WithLogistics?TArray<FString>{TEXT("Logistics"),TEXT("Grenadiers #4"),TEXT("Ammunition: 100%"),TEXT("Provisions: 120 s"),TEXT("Supply line: 100%")}:TArray<FString>();
		const FBottomLayout R=RecruitPanel(View.X,View.Y,View.X-184-16,{},Tabs,Lists,{Recruit,Rally},Measure,Scale,Logistics,19);
		FControl Power; Power.Label=TEXT("Recon Flight"); Power.Icon=TEXT("recon"); Power.Key=TEXT("Z"); Power.Value=TEXT("60 MP"); Power.Command=FTCUiCommand(ETCUiAction::Support);
		TArray<FControl> Powers; for (int32 I=0; I<8; I++) Powers.Add(Power);
		const FBottomLayout E=EconomyPanel(View.X,TEXT("100 MP"),TEXT("+5/s"),TEXT("3/12 units"),Powers,Measure,Scale);
		auto Append=[&](const FBottomLayout& Other) { L.Paint.Append(Other.Paint); L.Icons.Append(Other.Icons); L.Hits.Append(Other.Hits); L.TextBounds.Append(Other.TextBounds); };
		Append(R); Append(E);
		L.Paint.Panels.Add(TCHudBottom::Box(View.X-8-184-4,View.Y-8-184-4,188,188));
		L.Paint.Panels.Add(TCHudBottom::Box(8,12,62,54));
		FTCScoreStrip Strip; Strip.Kind=TEXT("horde"); Strip.Lead.bShown=true; Strip.Lead.Mode=TEXT("Wave 1"); Strip.Lead.Clock=TEXT("8 left"); Strip.bWeather=true; Strip.Weather=TEXT("Clear: no effect");
		FTCScoreTeam Team; Team.bBar=true; Team.U0=TEXT("Bunker"); Team.Num=TEXT("99%");
		FTCScoreSeat Seat; Seat.Name=TEXT("Alder"); Seat.Held=TEXT("0 held"); Team.Seats.Add(Seat); Seat.Name=TEXT("Fern"); Seat.Held=TEXT("1 held"); Team.Seats.Add(Seat); Strip.Teams.Add(Team);
		Team=FTCScoreTeam(); Team.Role=TEXT("Horde"); Seat=FTCScoreSeat(); Seat.Name=TEXT("Horde"); Team.Seats.Add(Seat); Strip.Teams.Add(Team);
		// shared/sim.js only enables logistics in Conquest, Classic, Annihilation and World.
		if (WithLogistics)
		{
			Strip=FTCScoreStrip(); Strip.Kind=TEXT("conquest"); Strip.bWeather=true; Strip.Weather=TEXT("Clear: no effect");
			for (const TCHAR* Name : {TEXT("Alder"),TEXT("Fern")})
			{
				FTCScoreTeam Side; Side.bBar=true; Side.Num=TEXT("0 / 1200"); Side.U=TEXT("VP");
				FTCScoreSeat Player; Player.Name=Name; Player.Held=TEXT("0 pts, 0 held"); Side.Seats.Add(Player); Strip.Teams.Add(Side);
			}
		}
		const FTCPaint Scores=ScoreCard(Strip,View.X,Measure,false,Scale); L.Paint.Append(Scores);
		FTCMatchState State; State.bHasMatch=true; State.ModeRow.bPresent=true; State.ModeRow.bBoss=!WithLogistics; State.ModeRow.BossHp=5700; State.ModeRow.BossMax=6000;
		FTCBossBar Boss; Boss.Snapshot(State,0);
		const FBox2D Score=Scores.Panels[0];
		const FTCPaint BossPaint=BossCard(Boss,View,Score.Max.Y+4,E.Economy,L.Paint.Panels,0,Measure,Scale);
		L.Paint.Append(BossPaint);
		float StackBottom=Score.Max.Y;
		if (WithLogistics)
		{
			const FTCPaint Rate=MatchNoticeCard(TC_T(FString(TEXT("Scores tied"))),TEXT(""),TEXT(""),View.X,StackBottom+4,Measure,Scale);
			L.Paint.Append(Rate); StackBottom=Rate.Panels.Last().Max.Y;
		}
		for (const FBox2D& B : BossPaint.Panels) if (B.Min.X<View.X/2 && B.Max.X>View.X/2) StackBottom=FMath::Max(StackBottom,(float)B.Max.Y);
		L.Paint.Panels.Add(TCHudBottom::Box(View.X/2-57/Scale,StackBottom+8/Scale,114/Scale,33/Scale));
		TArray<FControl> Sandbox;
		for (const TCHAR* Label : {TEXT("Sandbox"),TEXT("Rifle Squad"),TEXT("Enemy"),TEXT("Spawn")}) { FControl C; C.Label=Label; Sandbox.Add(C); }
		Append(SandboxToolbar(View,70,Score.Min.X,Sandbox,L.Paint.Panels,Measure,Scale));
		const FString Case=FString::Printf(TEXT("%s %g%% %s %d groups logistics %d"),Language==ETCLanguage::Spanish?TEXT("es"):TEXT("en"),Scale*100,Open?TEXT("open"):TEXT("tabs"),Groups,WithLogistics);
		for (const FString& Error : LayoutErrors(L,View)) AddError(Case+TEXT(": ")+Error);
		TestEqual(Case+TEXT(": selection preserved"),L.Paint.TextsOf(TEXT("bottom:selection-title")),TArray<FString>{TC_T(FString(TEXT("1 unit selected")))});
		TestEqual(Case+TEXT(": full translated tabs"),R.Paint.TextsOf(TEXT("bottom:tab-label")),TArray<FString>{TC_T(FString(Names[0])),TC_T(FString(Names[1])),TC_T(FString(Names[2])),TC_T(FString(Names[3]))});
		TestTrue(Case+TEXT(": Form value is present, with no extra clipped summary"),L.Paint.TextsOf(TEXT("bottom:value")).Contains(TC_T(FString(TEXT("Form")))));
		TestEqual(Case+TEXT(": recruitment heading is above the tabs"),R.Paint.TextsOf(TEXT("bottom:above-label"))[0],TC_T(FString(TEXT("Recruit"))));
	}
	return !HasAnyErrors();
}

// Round 16 (hudfit): the fit rules at UI 150% in a 1280 x 720 window (853.3 x 480 UI pixels), the case slice 1p left
// text and settings controls retained from the earlier fit test. BottomLayout guards the measured replacement.
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCHudFitTest, "ThreeCrossroads.HUD.Fit", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCHudFitTest::RunTest(const FString&)
{
	// round 17: the Horde record after the mode's description (main.js renderLobby), and its Spanish (es.js keys)
	TestEqual(TEXT("no record, three defenders"), FTCHudFit::HordeRecordText(3, false, 0, 0, {}), FString(TEXT(" No record yet here with 3 defenders.")));
	TestEqual(TEXT("no record, one defender"), FTCHudFit::HordeRecordText(1, false, 0, 0, {}), FString(TEXT(" No record yet here with 1 defender.")));
	TestEqual(TEXT("connection banner after a loss"), FTCHudFit::ConnectionBannerText(false, 2), FString(TEXT("Connection lost. Retrying in 2 s")));
	TestEqual(TEXT("connection banner for a full room"), FTCHudFit::ConnectionBannerText(true, 10), FString(TEXT("Trying this room again in 10 s")));
	TestEqual(TEXT("a record"), FTCHudFit::HordeRecordText(2, true, 7, 545, { TEXT("Ann"), TEXT("Bo") }), FString(TEXT(" Record here with 2 defenders: wave 7 in 9:05 (Ann, Bo).")));
	if (FTCLocale::IsLoaded())
	{
		TestEqual(TEXT("es: no record"), FTCLocale::Translate(TEXT("No record yet here with 3 defenders."), ETCLanguage::Spanish), FString(TEXT("Todavía no hay récord aquí con 3 defensores.")));
		TestEqual(TEXT("es: connection lost"), FTCLocale::Translate(FTCHudFit::ConnectionBannerText(false, 4), ETCLanguage::Spanish), FString(TEXT("Conexión perdida. Reintentando en 4 s")));
		TestEqual(TEXT("es: full room"), FTCLocale::Translate(FTCHudFit::ConnectionBannerText(true, 10), ETCLanguage::Spanish), FString(TEXT("Reintentando esta sala en 10 s")));
		TestEqual(TEXT("es: a record"), FTCLocale::Translate(TEXT("Record here with 2 defenders: wave 7 in 9:05 (Ann, Bo)."), ETCLanguage::Spanish), FString(TEXT("Récord aquí con 2 defensores: oleada 7 en 9:05 (Ann, Bo).")));
	}
	// the settings in the match menu: their keys are the HUD's own actions; the session controls stay match controls
	const TPair<const TCHAR*, ETCUiAction> Keys[] = { { TEXT("Setting.Graphics"), ETCUiAction::Graphics }, { TEXT("Setting.Gore"), ETCUiAction::Gore },
		{ TEXT("Setting.EdgeScroll"), ETCUiAction::EdgeScroll }, { TEXT("Setting.PanSpeed"), ETCUiAction::PanSpeed }, { TEXT("Setting.Cover"), ETCUiAction::CoverToggle },
		{ TEXT("Setting.Volume"), ETCUiAction::Volume }, { TEXT("Setting.UiScale"), ETCUiAction::UiScale }, { TEXT("Setting.Language"), ETCUiAction::Language } };
	for (const auto& K : Keys) TestEqual(FString::Printf(TEXT("%s's action"), K.Key), (int32)ATCHUD::ControlCommand(K.Key, -1).Action, (int32)K.Value);
	const FTCUiCommand Hand = ATCHUD::ControlCommand(TEXT("HandAi"), 2);
	TestTrue(TEXT("HandAi stays a match control with its slot"), Hand.Action == ETCUiAction::MatchControl && Hand.Unit == TEXT("HandAi") && Hand.Group == 2);
	TestTrue(TEXT("an unknown setting key stays a match control (RunControl reports it)"), ATCHUD::ControlCommand(TEXT("Setting.Nothing"), -1).Action == ETCUiAction::MatchControl);
	return true;
}
#endif
