# Guest memory layout

All addresses are 64-bit values; all multibyte fields use little-endian encoding. Architecture configuration is centralized in `vm-core::Architecture`.

| Region | Start | Size | Permissions |
| --- | --- | --- | --- |
| Code, reserved for a future backend | `0x0000000000400000` | 4096 | R-X |
| Read-only data | `0x0000000001000000` | 4096 | R-- |
| Globals | `0x0000000002000000` | 4096 | RW- |
| Heap | `0x0000555500000000` | 65536 | RW- |
| Stack, reserved | `0x00007FFFFFFFE000` | 4096 | RW- |

Globals +0 contains the eight-byte player root pointer. Globals +8 contains the bakery activities pointer; globals +16 contains the bakery visitors pointer. Null and the unmapped gaps fault.

## Fresh allocation order

PlayerName occupies heap +0 through +31. PlayerState immediately follows at heap +32 and occupies 136 bytes. Generator[], Upgrade[], Achievement[], Statistics, TemporaryEffects, ShopState, PrestigeState, RecipeBonus, LuckyCharm, BakeryActivities, BakeryOrder[], and BakeryVisitors follow in deterministic order with eight-byte alignment. Existing object layouts and adjacency are unchanged.

## PlayerState

| Offset | Type | Field |
| --- | --- | --- |
| `0x00` | u64 | Available cookies |
| `0x08` | u64 | Lifetime/total cookies baked |
| `0x10` | u64 | Base cookies per click |
| `0x18` | u64 | Base passive cookies per second |
| `0x20` | u64 | Prestige level |
| `0x28` | u64 | Reserved flags |
| `0x30` | pointer | PlayerName |
| `0x38` | pointer | Generator[] |
| `0x40` | pointer | Upgrade[] |
| `0x48` | pointer | Achievement[] |
| `0x50` | u64 | Generator catalogue count |
| `0x58` | u64 | Upgrade catalogue count |
| `0x60` | u64 | Achievement catalogue count |
| `0x68` | pointer | Statistics |
| `0x70` | pointer | TemporaryEffects |
| `0x78` | pointer | ShopState |
| `0x80` | pointer | PrestigeState |

Current catalogue traversal is bounded to nine generators, eight upgrades, and eight achievements. Pointer redirection remains observable through the fields that game operations dereference.

Each generator occupies 32 bytes: ownership at +0, current price at +8, base unit production at +16, and its upgrade multiplier at +24. Upgrade and achievement entries are eight-byte owned/unlocked flags.

Statistics occupies 48 bytes: manual clicks, spent cookies, played ticks, passive remainder, stale charm pointer, and charm-traded flag.

TemporaryEffects occupies 32 bytes: boost expiry tick, boost multiplier, next gift tick, and one reserved field.

ShopState occupies 40 bytes: label[24], multiplier at +24, and a pointer at +32 to RecipeBonus. Click power dereferences that pointer and the charm pointer, which gives corruption a direct effect on game behavior.

PrestigeState contains total earned points and reset count. RecipeBonus and LuckyCharm initially each contain the little-endian u64 value 1. Coupon reuse leaves the old charm pointer unchanged.

## Bakery activities

The pointer at globals +8 references a 128-byte allocation. All fields below are little-endian u64 values except the last pointer.

| Offset | Field |
| --- | --- |
| `0x00` | Selected recipe index (Classic 0, Chocolate chip 1, Vanilla crunch 2) |
| `0x08` | Recipe click multiplier |
| `0x10` | Recipe passive production multiplier |
| `0x18`, `0x20`, `0x28` | Three recipe-unlocked flags |
| `0x30` | Total orders completed |
| `0x38` | Baking rushes won |
| `0x40` | Rush state: idle 0, running 1, reward ready 2, missed 3, offer available 4 |
| `0x48` | Rush target clicks |
| `0x50` | Rush clicks so far |
| `0x58` | Rush cookie reward |
| `0x60` | Rush deadline (absolute guest tick) |
| `0x68` | Next random rush offer tick |
| `0x70` | Last cookie shower reward |
| `0x78` | Pointer to BakeryOrder[] |

The order array has three 64-byte entries. Each contains kind at +0 (bake 0, click 1, generator 2), target at +8, starting counter at +16, reward at +24, generator index at +32, customer index at +40, completed-order count at +48, and a reserved field at +56. Baking and clicking progress subtract the starting counter from lifetime cookies or manual clicks; generator orders read current ownership. Claiming replaces the goal, baseline, reward, and customer, using the saved deterministic RNG.

The recipe multipliers affect real clicking and passive production without modifying the stored base click/CPS values. Active rush clicks update the guest progress field; deadline expiry is driven only by guest ticks. All writes appear in write history. Unknown enum values and invalid pointers fault safely.

## Bakery visitors

Globals +16 points to an 80-byte allocation. Each field is a little-endian u64.

| Offset | Field |
| --- | --- |
| `0x00` | Visitor kind: absent 0, sugar ants 1, crumb mouse 2, cookie gobbler 3 |
| `0x08` | Remaining hits |
| `0x10` | Next random arrival tick |
| `0x18` | Next bite tick |
| `0x20` | Cookies eaten per bite |
| `0x28` | Cookies held by the current visitor |
| `0x30` | Total cookies stolen |
| `0x38` | Visitors defeated |
| `0x40` | Departure tick |
| `0x48` | Total cookies recovered |

Bites occur once per second and are capped at the available balance. Defeating a visitor returns its held cookies without increasing lifetime production. All scheduling uses saved guest ticks and seeded randomness, with no offline progression. The recipe-label and charm-trading routines remain in the headless model for compatibility and corruption tests; their controls are not shown in the game UI.
