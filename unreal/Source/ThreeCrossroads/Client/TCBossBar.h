// The Horde boss bar (parity CLI-65): the Kaiju's health across the top of the screen while one is on the map.
//
// Ported from the pinned reference client (reference/ww2-rts at e4e579a):
//   client/hud.js      drawBoss, run on every snapshot (hud.update): shown while s.mode.boss is set, the name line
//                      `KAIJU  ${Math.ceil(b[0] / b[1] * 100)}%` and the fill at b[0] / b[1] * 100 percent;
//   client/index.html  <div id="top">: the score strip, then #boss, then #status, centred, 4 px apart; #boss's CSS:
//                      width min(640px, var(--vw) - 32px), padding 6px 12px 8px, the solid panel and its 1 px border;
//                      .boss-name 15px 700, letter-spacing 0.2em, #e8c060; .boss-bar 18px tall, 4px under the name,
//                      a 1 px black border on #2a1010; the fill a linear-gradient(#e04a3a, #8e1c14) whose width
//                      eases over 0.3 s (transition: width 0.3s, the CSS 'ease' curve).
// The server sets mode.boss in Horde only (shared/sim.js hordeTick: [ceil(sum of the bosses' hp), sum of their hpPer]
// while any boss unit is alive, else null), so no other mode shows it; this reads only FTCMatchState::ModeRow, the
// snapshot's own mode row. The page's observer passes the name line through spanish() (es.js has no key for it, so it
// stays English); HTML collapses its two spaces into one when it is drawn.
#pragma once

#include "CoreMinimal.h"
#include "Client/TCEndReport.h"

class FTCMatchState;

namespace TCBossRules
{
	constexpr float MaxWidth = 640.f, SideRoom = 32.f, PadTop = 6.f, PadSide = 12.f, PadBottom = 8.f;
	constexpr float NamePx = 15.f, NameLine = 1.35f, Tracking = 0.2f; // em
	constexpr float BarGap = 4.f, BarH = 18.f, TopGap = 4.f;
	constexpr double Transition = 0.3;
}

class FTCBossBar
{
public:
	// hud.js drawBoss, from each snapshot (Now: the client clock in seconds, for the width's transition).
	void Snapshot(const FTCMatchState& S, double Now);
	void Reset();
	bool Shown() const { return bShown; }
	// The name line's text node (before HTML collapses its spaces) and the fill's target percentage.
	const FString& Text() const { return Name; }
	double Percent() const { return Target; }
	// The fill's width in percent at Now: the CSS transition from where it was toward the new value.
	double WidthAt(double Now) const;
	// The bar centred on CentreX with its top at Top (the score strip's bottom plus #top's 4 px gap), for a HUD
	// ViewW pixels wide. Empty when hidden.
	FTCPaint Layout(float CentreX, float Top, float ViewW, double Now, const FTCMeasure& Measure, float* OutBottom = nullptr) const;

private:
	bool bShown = false, bEverSet = false;
	FString Name;
	double Target = 100.0, From = 100.0, Started = -1e9;
};
