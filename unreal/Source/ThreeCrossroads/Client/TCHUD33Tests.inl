// Shared behavioral regression, run by Unreal automation and the standalone value-type shim.
// On the old queue the adapter submits the same captions, with no association to observe.
namespace TCHud33Test
{
 struct FSegment { FVector2D Start,End; };
 struct FObserved { FVector2D Anchor; TArray<FSegment> Segments; bool bEndpoint=false; };
 inline FObserved Observe()
 {
#ifdef TC_HUD33_ASSOCIATION
  const auto& Link=TCHud31::WorldLabelAssociation();
  FObserved Out; Out.Anchor=Link.Anchor; Out.bEndpoint=Link.bEndpoint;
  for (const auto& S:Link.Segments) Out.Segments.Add({S.Start,S.End});
  return Out;
#else
  return {};
#endif
 }
 inline void Submit(const FBox2D& Bounds,FVector2D Anchor,bool Known,TFunction<void()> Paint)
 {
#ifdef TC_HUD33_ASSOCIATION
  TCHud31::SubmitAnchoredWorldLabel(nullptr,Bounds,Anchor,Known,MoveTemp(Paint));
#else
  (void)Anchor; (void)Known;
  TCHud31::SubmitWorldLabel(nullptr,Bounds,MoveTemp(Paint));
#endif
 }
 template<class Check> void Run(Check CheckIt,const TArray<double>& EnglishWidths={},const TArray<double>& SpanishWidths={})
 {
  const FBox2D View({0,0},{1280,720});
  const TArray<FBox2D> Panels={FBox2D({496,12},{784,393}),FBox2D({842,12},{1268,368}),
   FBox2D({790,370},{1268,411}),FBox2D({986,426},{1268,708})};
  // Physical viewport is fixed, including at UI150. Widths cover complete EN/ES countdowns.
  for (double Scale : {1.0,1.5}) for (bool Spanish : {false,true})
  {
   TArray<FBox2D> Occupied=Panels;
   int Painted=0,Linked=0;
   TCHud31::BeginWorldLabels(nullptr,View);
   const FBox2D Icon({802,426},{840,466});
   TCHud31::ProtectWorldInformation(nullptr,Icon); Occupied.Add(Icon);
   const FVector2D Anchors[]={{815,377},{637,457},{862,512}};
   const FVector2D Origins[]={{775,330},{540,386},{765,440}};
   const TArray<double> DefaultWidths={80,Spanish?290.0:195.0,Spanish?265.0:195.0};
   const TArray<double>& Measured=Spanish?SpanishWidths:EnglishWidths;
   const TArray<double>& Widths=Measured.Num()==3?Measured:DefaultWidths;
   const double Heights[]={32.5,29.84,29.84};
   for (int I=0;I<3;I++)
   {
    const FBox2D Bounds(Origins[I],Origins[I]+FVector2D(Widths[I]*Scale,Heights[I]*Scale));
    Submit(Bounds,Anchors[I],true,[&,Bounds,I]()
    {
     auto Offset=TCHud31::WorldLabelOffset();
     const FBox2D Placed(Bounds.Min+Offset,Bounds.Max+Offset);
     CheckIt("complete caption clears panels, icons and captions",TCHud31::WorldLabelVisible(Placed,Occupied));
     CheckIt("complete caption stays in physical viewport",Placed.Min.X>=6&&Placed.Min.Y>=6&&Placed.Max.X<=1274&&Placed.Max.Y<=714);
     const auto Link=Observe();
     CheckIt("association keeps actual ground anchor",Link.Anchor.X==Anchors[I].X&&Link.Anchor.Y==Anchors[I].Y);
     CheckIt("panel-covered anchor has no false endpoint",Link.bEndpoint==TCHud31::WorldLabelVisible(FBox2D(Anchors[I]-FVector2D(6,6),Anchors[I]+FVector2D(6,6)),Panels,0));
     CheckIt("caption has a visible ground leader",Link.Segments.Num()>0);
     if (Link.Segments.Num()>0)
     {
      Linked++;
      const auto Start=Link.Segments[0].Start;
      CheckIt("leader begins on placed caption edge",Start.X>=Placed.Min.X&&Start.X<=Placed.Max.X&&Start.Y>=Placed.Min.Y&&Start.Y<=Placed.Max.Y&&
       (Start.X==Placed.Min.X||Start.X==Placed.Max.X||Start.Y==Placed.Min.Y||Start.Y==Placed.Max.Y));
      for (const auto& Segment:Link.Segments) for (int K=1;K<20;K++)
      {
       const auto P=Segment.Start+(Segment.End-Segment.Start)*(K/20.0);
       CheckIt("leader never paints through a HUD panel",TCHud31::WorldLabelVisible(FBox2D(P,P),Panels,0));
      }
     }
     Occupied.Add(Placed); Painted++;
    });
   }
   TCHud31::FinishWorldLabels(Panels);
   CheckIt("income and both support captions survive",Painted==3);
   CheckIt("all three captions retain ground association",Linked==3);
  }
  // No input eligibility means no position ornament, even when a caption remains public.
  int Hidden=0;
  TCHud31::BeginWorldLabels(nullptr,View);
  Submit(FBox2D({500,100},{700,130}),{600,600},false,[&]()
  {const auto L=Observe();CheckIt("hidden anchor cannot emit a leader or endpoint",L.Segments.Num()==0&&!L.bEndpoint&&L.Anchor.X==0&&L.Anchor.Y==0);Hidden++;});
  TCHud31::FinishWorldLabels(Panels);
  CheckIt("hidden-anchor caption still survives",Hidden==1);
  int Clean=0;
  TCHud31::BeginWorldLabels(nullptr,View);
  TCHud31::SubmitWorldLabel(nullptr,FBox2D({500,450},{700,480}),[&]()
  {const auto L=Observe();CheckIt("previous-frame anchor does not leak",L.Segments.Num()==0&&!L.bEndpoint);Clean++;});
  TCHud31::FinishWorldLabels({}); TCHud31::FinishWorldLabels({});
  CheckIt("frame reset does not replay captions",Clean==1);
  CheckIt("association cannot escape paint callback",Observe().Segments.Num()==0&&!Observe().bEndpoint);
 }
}
