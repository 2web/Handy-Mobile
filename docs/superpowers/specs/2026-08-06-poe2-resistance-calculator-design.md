# Path of Exile 2 Resistance Calculator — Design

**Date:** 2026-08-06
**Status:** Approved (design), pending spec review

## Goal

Sum what the player's captured gear contributes to each resistance, compare it against the
cap, and say where the missing points could come from — in an "Equipment" tab beside
Progress and Items.

## Context

Two ports have shipped into Handy: item capture (R3) and the log tracker (R1). The player
has captured nine items — sceptre, body armour, helmet, focus, boots, gloves, two rings, an
amulet — eight of them through the background clipboard watcher. That is a nearly complete
set of worn equipment, missing only a belt.

This is the feature the original `poe2-helper` spec called E2. It was designed there as
"sum the contributions of worn items", and everything below refines that against what the
real data turned out to look like.

## The discovery that shapes this feature

Summing the player's actual items and comparing with the character panel from an earlier
screenshot:

| | Sum of items | Character panel | Difference |
|---|---|---|---|
| Fire | 95% | 71% | **−24** |
| Cold | 95% | 71% | **−24** |
| Lightning | 82% | 58% | **−24** |
| Chaos | 29% | 29% | **0** |

Chaos matches exactly. All three elemental resistances are over by the same 24 points. That
is the campaign's resistance penalty, which does not apply to chaos.

**This is why the naive calculator is dangerous rather than merely imprecise.** It would
report fire at 95% against a 75% cap — a comfortable surplus — when the true value is 71%,
a shortfall. In a hardcore league that is an advisor telling the player not to bother fixing
resistances before a boss who then kills them permanently. A number that is confidently
wrong in the reassuring direction is worse than no number.

(The panel figures come from a screenshot taken on 2026-08-05, and the gear may have changed
since. But chaos matching to the point while three elementals are off by an identical amount
is too orderly to be coincidence.)

## Scope & decisions (locked with user)

- **The penalty is a setting the player fills in once**, `poe2_resistance_penalty`, defaulting
  to unset. While it is unset the tab shows the equipment contribution and states plainly
  that the character panel will read lower. Once set, the tab shows the final figure and
  compares it to the cap.
  The alternative — a table of penalties by act, baked into the code — was rejected: those
  values shift between leagues, and a stale table would be wrong silently. One number the
  player checked against their own panel is both accurate and verifiable.
- **A third tab, "Equipment"** (`Экипировка` in Russian), beside Progress and Items. The tab
  is named after what it shows — the gear — rather than after the arithmetic it performs.
- **Resistances only.** Not life, not armour, not damage.
- **Worn gear is inferred from what was captured**, by slot, most recent wins.

## What counts as worn

The program never sees the character's equipment; it sees what the player copied. So worn
gear is inferred:

- Each item class maps to a slot. Rings hold two items; every other slot holds one.
- Within a slot, the most recently captured items fill it — two for rings, one elsewhere.
- Anything older in that slot is ignored, and so is anything in a class that maps to no
  slot at all (currency, gems, flasks).
- The player can exclude a specific item from the calculation, for the case where they
  copied something they are not wearing. Excluding is per item and reversible.

Slot map: `Body Armours`, `Helmets`, `Gloves`, `Boots`, `Belts`, `Amulets` → one each;
`Rings` → two; `Foci`, `Shields`, `Quivers` → one shared off-hand slot; every weapon class
(`Sceptres`, `Wands`, `Bows`, `Staves`, `One Hand Maces`, `Two Hand Maces`, `Crossbows`,
`Spears`, `Flails`, `Daggers`, `Claws`) → one shared weapon slot.

An unknown item class contributes nothing and is listed as unrecognised rather than silently
dropped — the game adds classes every league.

## What is summed

Every modifier on a worn item, whatever its kind: affixes, implicits, and **runes**. Runes
matter — in this player's gear they carry `+18%` fire on the body armour, `+18%` lightning
on the helmet, `+14%` cold on the focus.

Three text shapes carry resistance, and all three appear in the real data or are known to
exist in the game:

- `+N% to <Fire|Cold|Lightning|Chaos> Resistance` — one element.
- `+N% to all Elemental Resistances` — fire, cold and lightning each gain N. Chaos does not.
- `+N% to <A> and <B> Resistance` — both named elements gain N. **Not present in the current
  data and therefore unverified**, but the shape exists in the game and the calculator must
  handle it rather than discover it later as a silent undercount.

The value used is the rolled one, not the tier bounds.

## The penalty

One integer, applied to fire, cold and lightning, never to chaos. Stored as a positive
number the player reads off their own panel: they see 95 in the tab and 71 in the game, so
they enter 24.

The tab explains where to get it: open the character panel in the game, compare one
elemental resistance with what the tab shows, enter the difference.

While unset, every elemental figure is labelled as the equipment contribution and the cap
comparison is withheld — not shown as "met" or "short", because neither is known.

## What the tab shows

Per resistance: the equipment total, the final figure after the penalty, the cap, and the
shortfall or surplus. Chaos has no penalty line.

Below that, the part that matters in solo self-found, where nothing can be bought:
**which slots could supply what is missing.** For each resistance short of the cap, the tab
names the worn items contributing nothing to it, and the slots that are empty entirely.

For this player right now, that reads: no belt at all, and neither ring nor amulet carries
lightning. "Get more lightning resistance" is useless advice in a league without trade;
"you have no belt, and a belt commonly rolls up to 40%" is actionable.

## Failure behaviour

| Situation | Behaviour |
|---|---|
| No items captured | The tab says so and points at the Items tab |
| Penalty unset | Contribution shown, cap comparison withheld, how to find the penalty explained |
| Item class not in the slot map | Contributes nothing, listed as unrecognised |
| Two items in a one-item slot | The most recent wins; the other is listed as superseded |
| Advanced descriptions were off when captured | Values still parse; the tab notes that some items lack tiers |
| A modifier's value failed to parse | Contributes nothing and is listed, never guessed at |

## Testing

- Slot inference: most recent wins; rings take two; a weapon class and an off-hand class do
  not collide; an unknown class is reported rather than dropped.
- Summing: single-element, all-elemental, and the two-element shape; runes counted; implicits
  counted; the rolled value used rather than a tier bound.
- The penalty: applied to three elements and never to chaos; absent penalty withholds the
  cap comparison rather than assuming zero.
- Gap advice: an item with no lightning modifier is named for lightning; an empty slot is
  named; a slot already contributing is not named.
- An acceptance test over this player's nine real items asserting the totals this document
  records: fire 95, cold 95, lightning 82, chaos 29, including the +14 from two
  all-elemental sources.

## Definition of done

1. The Equipment tab lists fire, cold, lightning and chaos with the equipment total.
2. With the penalty set, each elemental figure shows the final value and the distance to the
   cap; chaos shows its distance without a penalty.
3. With the penalty unset, no cap comparison is shown, and the tab explains how to obtain it.
4. Runes and implicits are counted; the real nine items total 95 / 95 / 82 / 29.
5. Slot inference picks the most recent per slot, two rings, and reports superseded and
   unrecognised items rather than dropping them.
6. For each resistance below the cap, the tab names empty slots and worn items contributing
   nothing to it.
7. Excluding an item removes it from every total, and the exclusion survives a restart.
8. Labels come from i18n with English and Russian present.

## Out of scope

- Life, energy shield, armour, evasion, spirit, damage — this tab is about surviving a hit
  of a known element, and mixing everything in would bury that.
- Modifiers that raise the maximum resistance (`+1% to maximum Fire Resistance`). The player
  has none; the cap stays 75 until one appears.
- Comparing two items and advising which to wear.
- Reading the character panel from a screenshot — that remains E3, and it would replace the
  penalty setting with a measured figure.
- Any use of the game's own API or trade site.
