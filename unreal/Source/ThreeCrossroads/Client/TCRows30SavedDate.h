// ISO saved instants accepted by new Date(savedAt), independent of the viewer's timezone.
#pragma once
#include <cstdint>
#include <string_view>
namespace TCRows30SavedDate
{
inline bool ParseIsoUtc(std::string_view S, int64_t& Out)
{
 auto Number=[&](size_t At,size_t N,int& V) { V=0; if(At+N>S.size()) return false; for(size_t I=At;I<At+N;I++) { if(S[I]<'0'||S[I]>'9') return false; V=V*10+S[I]-'0'; } return true; };
 int Y,M,D,H=0,Min=0,Sec=0,Ms=0,Offset=0;
 if(S.size()<10 || S[4]!='-' || S[7]!='-' || !Number(0,4,Y) || !Number(5,2,M) || !Number(8,2,D)) return false;
 if(M<1 || M>12 || D<1 || D>31) return false;
 if(S.size()!=10)
 {
  if(S.size()<20 || S[10]!='T' || S[13]!=':' || S[16]!=':' || !Number(11,2,H) || !Number(14,2,Min) || !Number(17,2,Sec)) return false;
  size_t At=19;
  if(At<S.size() && S[At]=='.')
  { size_t Start=++At; int Digits=0; while(At<S.size() && S[At]>='0' && S[At]<='9') { if(Digits<3) Ms=Ms*10+S[At]-'0'; ++Digits; ++At; } if(At==Start) return false; while(Digits++<3) Ms*=10; }
  if(At<S.size() && S[At]=='Z') ++At;
  else
  { if(At>=S.size() || (S[At]!='+' && S[At]!='-')) return false; int Sign=S[At++]=='+'?1:-1, OH,OM;
    if(At+5!=S.size() || S[At+2]!=':' || !Number(At,2,OH) || !Number(At+3,2,OM) || OH>23 || OM>59) return false;
    Offset=Sign*(OH*60+OM); At+=5; }
  if(At!=S.size() || H>24 || Min>59 || Sec>59 || (H==24 && (Min || Sec || Ms))) return false;
 }
 // Gregorian days relative to 1970-01-01, including year zero and Date.parse's day overflow.
 int Year=Y-(M<=2); const int Era=(Year>=0?Year:Year-399)/400; const unsigned YO=unsigned(Year-Era*400);
 const unsigned DOY=(153*(M+(M>2?-3:9))+2)/5+D-1, DOE=YO*365+YO/4-YO/100+DOY;
 const int64_t Days=int64_t(Era)*146097+DOE-719468;
 Out=(((Days*24+H)*60+Min-Offset)*60+Sec)*1000+Ms; return true;
}
}
