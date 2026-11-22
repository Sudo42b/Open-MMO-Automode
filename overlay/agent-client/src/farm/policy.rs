//! The farm policy: a pure function from a state snapshot to the same JSON
//! actions an LLM would answer with. Keeping it pure makes it testable
//! without a server.

use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::config::{FarmConfig, PlanConfig};

/// Town square; the shops and the respawn inn are within `TOWN_RADIUS`.
pub const TOWN: (f32, f32) = (-1475.2, 4741.6);
pub const TOWN_RADIUS: f32 = 120.0;

const POTION_PRICE: i64 = 600;
const FOOD_PRICE: i64 = 60;
const RETURN_SCROLL_PRICE: i64 = 800;
const REISSUE_MOVE: Duration = Duration::from_secs(30);
const REISSUE_RETURN: Duration = Duration::from_secs(15);
const MAX_SHOP_ACTIONS: usize = 6;

/// The driver's own chase gives up beyond `walk::MAX_CHASE_DISTANCE` (20 m),
/// so anything farther is approached with a `move` first.
const CHASE_LIMIT: f32 = 19.5;
const REISSUE_APPROACH: Duration = Duration::from_secs(6);

pub struct MonsterView {
    pub id: String,
    pub x: f32,
    pub z: f32,
    pub dist: f32,
    pub health: u32,
    pub floor: i8,
}

pub struct GroundView {
    pub id: u64,
    pub def_id: String,
    pub dist: f32,
    pub floor: i8,
}

pub struct Snapshot {
    pub in_game: bool,
    pub hp: u32,
    pub max_hp: u32,
    pub floor: i8,
    pub pos: (f32, f32),
    pub gold: i64,
    pub satiation: Option<u32>,
    pub bag: Vec<(String, u32)>,
    pub monsters: Vec<MonsterView>,
    pub ground: Vec<GroundView>,
    /// Official NPC names in sight.
    pub merchants: Vec<String>,
    pub is_night: bool,
    /// Display name the mover resolves (`Old Crypt`).
    pub dungeon_name: String,
    /// `crypt_key` → the floor-5 chest wants `crypt_key_5`.
    pub key_prefix: String,
}

impl Snapshot {
    pub fn count(&self, def_id: &str) -> u32 {
        self.bag
            .iter()
            .filter(|(d, _)| d == def_id)
            .map(|(_, q)| *q)
            .sum()
    }

    pub fn hp_pct(&self) -> u32 {
        if self.max_hp == 0 {
            return 100;
        }
        self.hp * 100 / self.max_hp
    }

    pub fn in_dungeon(&self) -> bool {
        self.floor < 0
    }

    pub fn depth(&self) -> u8 {
        self.floor.unsigned_abs()
    }

    pub fn near_town(&self) -> bool {
        dist(self.pos, TOWN) <= TOWN_RADIUS
    }
}

fn dist(a: (f32, f32), b: (f32, f32)) -> f32 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

#[derive(Default)]
pub struct Memory {
    pub last_attack: Option<(String, Instant)>,
    pub floor_empty_since: Option<Instant>,
    pub last_floor: Option<i8>,
    pub returning: bool,
    pub last_return_at: Option<Instant>,
    pub last_move_at: Option<Instant>,
    pub chest_opened_floor: Option<i8>,
    pub was_dead: bool,
    pub deaths: u32,
    pub potions_used: u32,
    pub attacks: u32,
    pub town_trips: u32,
    pub last_thought: String,
}

pub struct Decision {
    pub thought: String,
    pub actions: Vec<Value>,
}

impl Decision {
    fn one(thought: impl Into<String>, action: Value) -> Self {
        Self {
            thought: thought.into(),
            actions: vec![action],
        }
    }

    fn wait(thought: impl Into<String>) -> Self {
        Self {
            thought: thought.into(),
            actions: Vec::new(),
        }
    }
}

fn due(last: Option<Instant>, now: Instant, every: Duration) -> bool {
    last.is_none_or(|t| now.duration_since(t) >= every)
}

pub fn decide(
    now: Instant,
    snap: &Snapshot,
    plan: &PlanConfig,
    cfg: &FarmConfig,
    mem: &mut Memory,
) -> Decision {
    if !snap.in_game {
        return Decision::wait("not in game yet");
    }
    if snap.hp == 0 {
        if !mem.was_dead {
            mem.deaths += 1;
            mem.was_dead = true;
        }
        mem.last_attack = None;
        mem.returning = true;
        return Decision::one("dead; respawn", json!({"type": "respawn"}));
    }
    mem.was_dead = false;
    if mem.last_floor != Some(snap.floor) {
        mem.last_floor = Some(snap.floor);
        mem.floor_empty_since = None;
        mem.chest_opened_floor = None;
    }

    let potions = snap.count(&plan.potion);
    let food = snap.count(&plan.food);
    let scrolls = snap.count(&plan.return_scroll);

    if snap.hp_pct() < plan.potion_pct && potions > 0 {
        mem.potions_used += 1;
        return Decision::one(
            format!("hp {}% < {}%: drink", snap.hp_pct(), plan.potion_pct),
            json!({"type": "use", "item": plan.potion}),
        );
    }
    if snap.satiation.is_some_and(|s| s < plan.eat_below) && food > 0 {
        return Decision::one("hungry: eat", json!({"type": "use", "item": plan.food}));
    }

    if snap.in_dungeon() {
        return in_dungeon(now, snap, plan, cfg, mem, potions, food, scrolls);
    }
    on_surface(now, snap, plan, mem, potions, food, scrolls)
}

#[allow(clippy::too_many_arguments)]
fn in_dungeon(
    now: Instant,
    snap: &Snapshot,
    plan: &PlanConfig,
    cfg: &FarmConfig,
    mem: &mut Memory,
    potions: u32,
    food: u32,
    scrolls: u32,
) -> Decision {
    let emergency = snap.hp_pct() < plan.emergency_pct
        || (potions == 0 && snap.hp_pct() < plan.potion_pct.max(50));
    let supplies_low = (potions <= plan.min_potions && snap.gold >= POTION_PRICE)
        || (food == 0 && snap.gold >= FOOD_PRICE);
    if emergency || supplies_low || plan.rest || mem.returning {
        let why = if emergency {
            "emergency"
        } else if plan.rest {
            "schedule says rest"
        } else {
            "supplies low"
        };
        if !mem.returning {
            mem.returning = true;
            mem.town_trips += 1;
        }
        if !due(mem.last_return_at, now, REISSUE_RETURN) {
            return Decision::wait(format!("{why}: return pending"));
        }
        mem.last_return_at = Some(now);
        mem.last_attack = None;
        if scrolls > 0 {
            return Decision::one(
                format!("{why}: read return scroll"),
                json!({"type": "use", "item": plan.return_scroll}),
            );
        }
        return Decision::one(
            format!("{why}: no scroll, walk out"),
            json!({"type": "move", "target": snap.dungeon_name, "depth": 0}),
        );
    }

    let mut live: Vec<&MonsterView> = snap
        .monsters
        .iter()
        .filter(|m| m.floor == snap.floor && m.health > 0 && m.dist <= cfg.engage_range)
        .collect();
    live.sort_by(|a, b| a.dist.total_cmp(&b.dist));
    if let Some(target) = live.first() {
        mem.floor_empty_since = None;
        if target.dist > CHASE_LIMIT {
            if !due(mem.last_move_at, now, REISSUE_APPROACH) {
                return Decision::wait(format!("closing in on {}", target.id));
            }
            mem.last_move_at = Some(now);
            mem.last_attack = None;
            // Aim at the monster's own cell: it stands on walkable ground,
            // while a point interpolated towards it may sit inside a wall.
            return Decision::one(
                format!("{} is {:.0}m away: approach first", target.id, target.dist),
                json!({"type": "move", "x": target.x, "z": target.z, "depth": snap.depth()}),
            );
        }
        let current = mem
            .last_attack
            .as_ref()
            .filter(|(id, _)| live.iter().any(|m| &m.id == id))
            .cloned();
        let (id, since) = match current {
            Some((id, at)) => (id, Some(at)),
            None => (target.id.clone(), None),
        };
        if since.is_some_and(|at| now.duration_since(at) < Duration::from_secs(cfg.reissue_secs)) {
            return Decision::wait(format!("fighting {id}"));
        }
        mem.last_attack = Some((id.clone(), now));
        mem.attacks += 1;
        return Decision::one(
            format!("attack {id} ({:.1}m)", target.dist),
            json!({"type": "attack", "target": id}),
        );
    }
    mem.last_attack = None;

    if plan.open_chests && mem.chest_opened_floor != Some(snap.floor) {
        let key = format!("{}_{}", snap.key_prefix, snap.depth());
        if snap.count(&key) > 0 {
            mem.chest_opened_floor = Some(snap.floor);
            return Decision::one(
                format!("holding {key}: open the floor chest"),
                json!({"type": "open_chest"}),
            );
        }
    }

    let mut loot: Vec<&GroundView> = snap
        .ground
        .iter()
        .filter(|g| g.floor == snap.floor && g.dist <= cfg.loot_range && plan.loots(&g.def_id))
        .collect();
    loot.sort_by(|a, b| a.dist.total_cmp(&b.dist));
    if let Some(g) = loot.first() {
        return Decision::one(
            format!("pick up {} ({:.1}m)", g.def_id, g.dist),
            json!({"type": "pickup", "item": g.id}),
        );
    }

    // Floors are only ever walked downward: the mover's stair routing climbs
    // unreliably (a `move` to a shallower floor can hang the driver), so the
    // way back up is always a return scroll, which also restocks in town.
    let depth = snap.depth();
    let (first, last) = (plan.first_floor(), plan.last_floor());
    if depth < first {
        if !due(mem.last_move_at, now, REISSUE_MOVE) {
            return Decision::wait("heading into the floor range");
        }
        mem.last_move_at = Some(now);
        return Decision::one(
            format!("floor {depth} above {first}-{last}: go down to {first}"),
            json!({"type": "move", "target": snap.dungeon_name, "depth": first}),
        );
    }
    let empty_since = *mem.floor_empty_since.get_or_insert(now);
    let quiet = now.duration_since(empty_since);
    if depth < last {
        if quiet < Duration::from_secs(cfg.clear_secs) {
            return Decision::wait(format!("floor {depth} quiet; waiting for spawns"));
        }
        if !due(mem.last_move_at, now, REISSUE_MOVE) {
            return Decision::wait("floor change pending");
        }
        mem.last_move_at = Some(now);
        mem.floor_empty_since = None;
        let next = depth + 1;
        return Decision::one(
            format!("floor {depth} cleared: move to {next}"),
            json!({"type": "move", "target": snap.dungeon_name, "depth": next}),
        );
    }
    if depth == last && quiet < Duration::from_secs(cfg.linger_secs) {
        return Decision::wait(format!("last floor {depth} quiet; lingering for spawns"));
    }
    let why = if depth > last {
        format!("floor {depth} is below {first}-{last}")
    } else {
        format!("floor {depth} stayed quiet {}s", quiet.as_secs())
    };
    mem.returning = true;
    mem.town_trips += 1;
    mem.last_return_at = Some(now);
    mem.floor_empty_since = None;
    if scrolls > 0 {
        return Decision::one(
            format!("{why}: return scroll, restock, re-enter"),
            json!({"type": "use", "item": plan.return_scroll}),
        );
    }
    Decision::one(
        format!("{why}: no scroll, walk out"),
        json!({"type": "move", "target": snap.dungeon_name, "depth": 0}),
    )
}

fn merchant_for(snap: &Snapshot) -> &'static str {
    let seen = |n: &str| snap.merchants.iter().any(|m| m == n);
    match (snap.is_night, seen("Rica"), seen("Wick")) {
        (_, true, false) => "Rica",
        (_, false, true) => "Wick",
        (true, _, _) => "Wick",
        (false, _, _) => "Rica",
    }
}

fn on_surface(
    now: Instant,
    snap: &Snapshot,
    plan: &PlanConfig,
    mem: &mut Memory,
    potions: u32,
    food: u32,
    scrolls: u32,
) -> Decision {
    mem.last_attack = None;
    mem.floor_empty_since = None;

    if plan.rest {
        mem.returning = false;
        if dist(snap.pos, TOWN) > 30.0 && due(mem.last_move_at, now, REISSUE_MOVE) {
            mem.last_move_at = Some(now);
            return Decision::one(
                "resting: walk to the square",
                json!({"type": "move", "x": TOWN.0, "z": TOWN.1}),
            );
        }
        return Decision::wait("resting in town");
    }

    if !snap.near_town() {
        if !due(mem.last_move_at, now, REISSUE_MOVE) {
            return Decision::wait("walking");
        }
        mem.last_move_at = Some(now);
        let goal = if mem.returning { "town" } else { "dungeon" };
        return Decision::one(
            format!("outside town: head for the {goal}"),
            if mem.returning {
                json!({"type": "move", "x": TOWN.0, "z": TOWN.1})
            } else {
                json!({"type": "move", "target": snap.dungeon_name, "depth": plan.first_floor()})
            },
        );
    }

    let merchant = merchant_for(snap);
    let mut actions = Vec::new();
    for (def_id, _) in &snap.bag {
        if plan.sells(def_id) && !actions.iter().any(|a: &Value| a["item"] == def_id.as_str()) {
            actions.push(json!({"type": "sell", "item": def_id, "qty": "all", "target": merchant}));
        }
    }
    let mut gold = snap.gold;
    let mut want = |item: &str, have: u32, target: u32, price: i64, actions: &mut Vec<Value>| {
        let mut n = target.saturating_sub(have);
        while n > 0 && gold >= price && actions.len() < MAX_SHOP_ACTIONS {
            actions.push(json!({"type": "buy", "item": item, "target": merchant}));
            gold -= price;
            n -= 1;
        }
    };
    want(&plan.potion, potions, plan.potion_target, POTION_PRICE, &mut actions);
    want(&plan.food, food, plan.food_target, FOOD_PRICE, &mut actions);
    want(
        &plan.return_scroll,
        scrolls,
        plan.return_scroll_target,
        RETURN_SCROLL_PRICE,
        &mut actions,
    );
    if !actions.is_empty() {
        actions.truncate(MAX_SHOP_ACTIONS);
        return Decision {
            thought: format!("in town: trade with {merchant}"),
            actions,
        };
    }

    mem.returning = false;
    if potions == 0 && snap.gold < POTION_PRICE {
        return Decision::wait("no potions and no gold; staying in town");
    }
    if !due(mem.last_move_at, now, REISSUE_MOVE) {
        return Decision::wait("dungeon trip pending");
    }
    mem.last_move_at = Some(now);
    Decision::one(
        format!("stocked: enter {} floor {}", snap.dungeon_name, plan.first_floor()),
        json!({"type": "move", "target": snap.dungeon_name, "depth": plan.first_floor()}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap() -> Snapshot {
        Snapshot {
            in_game: true,
            hp: 40,
            max_hp: 45,
            floor: -2,
            pos: (-1450.0, 4700.0),
            gold: 5000,
            satiation: Some(600),
            bag: vec![
                ("healing_potion".into(), 4),
                ("jerky".into(), 3),
                ("scroll_of_return".into(), 2),
            ],
            monsters: Vec::new(),
            ground: Vec::new(),
            merchants: Vec::new(),
            is_night: false,
            dungeon_name: "Old Crypt".into(),
            key_prefix: "crypt_key".into(),
        }
    }

    fn run(s: &Snapshot, mem: &mut Memory) -> Decision {
        decide(
            Instant::now(),
            s,
            &PlanConfig::default(),
            &FarmConfig::default(),
            mem,
        )
    }

    #[test]
    fn drinks_when_low() {
        let mut s = snap();
        s.hp = 10;
        let d = run(&s, &mut Memory::default());
        assert_eq!(d.actions[0]["type"], "use");
        assert_eq!(d.actions[0]["item"], "healing_potion");
    }

    #[test]
    fn attacks_nearest_and_holds_target() {
        let mut s = snap();
        s.monsters = vec![
            mon("m7", 12.0, -2),
            mon("m3", 4.0, -2),
            mon("m9", 1.0, -3),
        ];
        let mut mem = Memory::default();
        let d = run(&s, &mut mem);
        assert_eq!(d.actions[0]["type"], "attack");
        assert_eq!(d.actions[0]["target"], "m3");
        let again = run(&s, &mut mem);
        assert!(again.actions.is_empty(), "same target is not re-sent at once");
    }

    #[test]
    fn approaches_far_monster_before_attacking() {
        let mut s = snap();
        s.monsters = vec![mon("m5", 29.0, -2)];
        let mut mem = Memory::default();
        let d = run(&s, &mut mem);
        assert_eq!(d.actions[0]["type"], "move");
        assert_eq!(d.actions[0]["depth"], 2);
        let x = d.actions[0]["x"].as_f64().unwrap() as f32;
        assert!((x - (s.pos.0 + 29.0)).abs() < 0.01, "walks to the monster's cell, got {x}");
        assert!(run(&s, &mut mem).actions.is_empty(), "approach is not re-sent at once");
        s.monsters = vec![mon("m5", 9.0, -2)];
        assert_eq!(run(&s, &mut mem).actions[0]["type"], "attack");
    }

    fn mon(id: &str, dist: f32, floor: i8) -> MonsterView {
        MonsterView {
            id: id.into(),
            x: -1450.0 + dist,
            z: 4700.0,
            dist,
            health: 10,
            floor,
        }
    }

    #[test]
    fn returns_with_scroll_when_potions_run_out() {
        let mut s = snap();
        s.bag = vec![("scroll_of_return".into(), 1), ("jerky".into(), 2)];
        let d = run(&s, &mut Memory::default());
        assert_eq!(d.actions[0]["type"], "use");
        assert_eq!(d.actions[0]["item"], "scroll_of_return");
    }

    #[test]
    fn walks_out_without_scroll() {
        let mut s = snap();
        s.bag = vec![("jerky".into(), 2)];
        let d = run(&s, &mut Memory::default());
        assert_eq!(d.actions[0]["type"], "move");
        assert_eq!(d.actions[0]["depth"], 0);
    }

    #[test]
    fn opens_chest_with_key() {
        let mut s = snap();
        s.floor = -5;
        s.bag.push(("crypt_key_5".into(), 1));
        let d = run(&s, &mut Memory::default());
        assert_eq!(d.actions[0]["type"], "open_chest");
    }

    #[test]
    fn moves_down_after_floor_is_quiet() {
        let s = snap();
        let mut mem = Memory::default();
        mem.floor_empty_since = Some(Instant::now() - Duration::from_secs(60));
        mem.last_floor = Some(-2);
        let d = run(&s, &mut mem);
        assert_eq!(d.actions[0]["type"], "move");
        assert_eq!(d.actions[0]["depth"], 3);
    }

    #[test]
    fn last_floor_lingers_then_returns_by_scroll_instead_of_climbing() {
        let mut s = snap();
        s.floor = -3;
        let mut mem = Memory::default();
        mem.last_floor = Some(-3);
        mem.floor_empty_since = Some(Instant::now() - Duration::from_secs(40));
        assert!(run(&s, &mut mem).actions.is_empty(), "still lingering at 40s");
        mem.floor_empty_since = Some(Instant::now() - Duration::from_secs(120));
        let d = run(&s, &mut mem);
        assert_eq!(d.actions[0]["type"], "use");
        assert_eq!(d.actions[0]["item"], "scroll_of_return");
        assert!(mem.returning);
        assert!(
            !d.actions.iter().any(|a| a["type"] == "move" && a["depth"] == 1),
            "never asks the mover to climb"
        );
    }

    #[test]
    fn below_range_returns_by_scroll() {
        let mut s = snap();
        s.floor = -5;
        let mut mem = Memory::default();
        let d = run(&s, &mut mem);
        assert_eq!(d.actions[0]["item"], "scroll_of_return");
    }

    #[test]
    fn shops_in_town_then_enters() {
        let mut s = snap();
        s.floor = 0;
        s.pos = TOWN;
        s.bag = vec![("small_sword".into(), 2), ("healing_potion".into(), 1)];
        let mut mem = Memory::default();
        let d = run(&s, &mut mem);
        assert_eq!(d.actions[0]["type"], "sell");
        assert_eq!(d.actions[0]["item"], "small_sword");
        assert!(d.actions.iter().any(|a| a["type"] == "buy" && a["item"] == "healing_potion"));
        assert!(d.actions.iter().all(|a| a["target"] == "Rica"));

        s.bag = vec![
            ("healing_potion".into(), 5),
            ("jerky".into(), 6),
            ("scroll_of_return".into(), 2),
        ];
        let d = run(&s, &mut mem);
        assert_eq!(d.actions[0]["type"], "move");
        assert_eq!(d.actions[0]["target"], "Old Crypt");
        assert_eq!(d.actions[0]["depth"], 1);
    }

    #[test]
    fn night_shopping_goes_to_wick() {
        let mut s = snap();
        s.floor = 0;
        s.pos = TOWN;
        s.is_night = true;
        s.bag = vec![("healing_potion".into(), 0)];
        let d = run(&s, &mut Memory::default());
        assert!(d.actions.iter().all(|a| a["target"] == "Wick"));
    }

    #[test]
    fn respawns_when_dead() {
        let mut s = snap();
        s.hp = 0;
        let mut mem = Memory::default();
        let d = run(&s, &mut mem);
        assert_eq!(d.actions[0]["type"], "respawn");
        assert_eq!(mem.deaths, 1);
    }

    #[test]
    fn rest_window_leaves_dungeon() {
        let s = snap();
        let mut plan = PlanConfig::default();
        plan.rest = true;
        let d = decide(
            Instant::now(),
            &s,
            &plan,
            &FarmConfig::default(),
            &mut Memory::default(),
        );
        assert_eq!(d.actions[0]["item"], "scroll_of_return");
    }
}
