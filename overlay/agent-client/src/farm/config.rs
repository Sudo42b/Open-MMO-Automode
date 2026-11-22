//! Farm-mode configuration: a default hunting plan plus time windows that
//! override parts of it. Lives under `[npcs.farm]` in config.toml.

use chrono::{Datelike, Local, NaiveTime, Timelike};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct FarmConfig {
    pub plan: PlanConfig,
    pub schedule: Vec<ScheduleWindow>,
    /// JSON status snapshot rewritten every turn; `{name}` expands to the
    /// character name. Empty disables it.
    pub status_file: String,
    /// A floor with no monster in sight for this long is considered cleared.
    pub clear_secs: u64,
    /// The same attack target is re-sent at most this often.
    pub reissue_secs: u64,
    /// Monsters farther than this are left alone.
    pub engage_range: f32,
    /// Ground loot farther than this is left alone.
    pub loot_range: f32,
}

impl Default for FarmConfig {
    fn default() -> Self {
        Self {
            plan: PlanConfig::default(),
            schedule: Vec::new(),
            status_file: "logs/farm/{name}.json".to_string(),
            clear_secs: 20,
            reissue_secs: 12,
            engage_range: 30.0,
            loot_range: 20.0,
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default)]
pub struct PlanConfig {
    /// Dungeon id (`old_crypt`) or display name (`Old Crypt`).
    pub dungeon: String,
    /// Inclusive floor range hunted, counted downward from 1.
    pub floors: [u8; 2],
    /// Drink a potion below this HP percent.
    pub potion_pct: u32,
    /// Leave the dungeon below this HP percent.
    pub emergency_pct: u32,
    pub potion: String,
    /// Restock up to this many potions in town.
    pub potion_target: u32,
    /// Go to town when potions fall to this count (and gold allows).
    pub min_potions: u32,
    pub food: String,
    pub food_target: u32,
    /// Eat below this satiation (max 1000, hungry under 300).
    pub eat_below: u32,
    pub return_scroll: String,
    pub return_scroll_target: u32,
    /// Ground items worth walking to.
    pub loot: Vec<String>,
    /// Bag items sold to the merchant on every town visit.
    pub sell: Vec<String>,
    /// Never sold, even when listed in `sell`.
    pub keep: Vec<String>,
    /// Open the floor chest when the bag holds its key.
    pub open_chests: bool,
    /// Stop hunting: return to town and idle there.
    pub rest: bool,
}

impl Default for PlanConfig {
    fn default() -> Self {
        Self {
            dungeon: "old_crypt".to_string(),
            floors: [1, 3],
            potion_pct: 30,
            emergency_pct: 20,
            potion: "healing_potion".to_string(),
            potion_target: 5,
            min_potions: 1,
            food: "jerky".to_string(),
            food_target: 6,
            eat_below: 300,
            return_scroll: "scroll_of_return".to_string(),
            return_scroll_target: 2,
            loot: vec![
                "small_sword".to_string(),
                "goblin_sword".to_string(),
                "dagger".to_string(),
                "apple".to_string(),
                "cheese".to_string(),
                "coin_pile".to_string(),
            ],
            sell: vec![
                "small_sword".to_string(),
                "goblin_sword".to_string(),
                "dagger".to_string(),
            ],
            keep: vec![
                "scroll_of_enchant_armor".to_string(),
                "scroll_of_enchant_weapon".to_string(),
            ],
            open_chests: true,
            rest: false,
        }
    }
}

impl PlanConfig {
    pub fn first_floor(&self) -> u8 {
        self.floors[0].max(1)
    }

    pub fn last_floor(&self) -> u8 {
        self.floors[1].max(self.first_floor())
    }

    pub fn sells(&self, def_id: &str) -> bool {
        self.sell.iter().any(|s| s == def_id) && !self.keep.iter().any(|k| k == def_id)
    }

    pub fn loots(&self, def_id: &str) -> bool {
        self.loot.iter().any(|s| s == def_id)
    }

    pub fn label(&self) -> String {
        if self.rest {
            return "rest".to_string();
        }
        format!("{} {}-{}", self.dungeon, self.first_floor(), self.last_floor())
    }
}

/// One override window. Fields left out keep the default plan's value.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct ScheduleWindow {
    /// `HH:MM-HH:MM` local time; a window may cross midnight.
    pub at: String,
    /// `mon`..`sun`; empty means every day.
    #[serde(default)]
    pub days: Vec<String>,
    pub dungeon: Option<String>,
    pub floors: Option<[u8; 2]>,
    pub potion_pct: Option<u32>,
    pub emergency_pct: Option<u32>,
    pub potion_target: Option<u32>,
    pub food_target: Option<u32>,
    pub rest: Option<bool>,
}

impl ScheduleWindow {
    fn apply(&self, base: &PlanConfig) -> PlanConfig {
        let mut p = base.clone();
        if let Some(d) = &self.dungeon {
            p.dungeon = d.clone();
        }
        if let Some(f) = self.floors {
            p.floors = f;
        }
        if let Some(v) = self.potion_pct {
            p.potion_pct = v;
        }
        if let Some(v) = self.emergency_pct {
            p.emergency_pct = v;
        }
        if let Some(v) = self.potion_target {
            p.potion_target = v;
        }
        if let Some(v) = self.food_target {
            p.food_target = v;
        }
        if let Some(v) = self.rest {
            p.rest = v;
        }
        p
    }

    fn matches(&self, weekday: u32, minute_of_day: u32) -> bool {
        if !self.days.is_empty() {
            let today = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"][weekday as usize];
            if !self.days.iter().any(|d| d.eq_ignore_ascii_case(today)) {
                return false;
            }
        }
        let Some((start, end)) = parse_window(&self.at) else {
            return false;
        };
        if start <= end {
            (start..end).contains(&minute_of_day)
        } else {
            minute_of_day >= start || minute_of_day < end
        }
    }
}

fn parse_window(spec: &str) -> Option<(u32, u32)> {
    let (a, b) = spec.split_once('-')?;
    let m = |s: &str| {
        NaiveTime::parse_from_str(s.trim(), "%H:%M")
            .ok()
            .map(|t| t.hour() * 60 + t.minute())
    };
    Some((m(a)?, m(b)?))
}

impl FarmConfig {
    /// The plan in force now: the last matching window wins over earlier ones.
    pub fn plan_now(&self) -> PlanConfig {
        let now = Local::now();
        self.plan_at(
            now.weekday().num_days_from_monday(),
            now.hour() * 60 + now.minute(),
        )
    }

    pub fn plan_at(&self, weekday: u32, minute_of_day: u32) -> PlanConfig {
        self.schedule
            .iter()
            .filter(|w| w.matches(weekday, minute_of_day))
            .fold(self.plan.clone(), |p, w| w.apply(&p))
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        for w in &self.schedule {
            if parse_window(&w.at).is_none() {
                anyhow::bail!("[farm] schedule `at` must be HH:MM-HH:MM, got {:?}", w.at);
            }
            for d in &w.days {
                let d = d.to_ascii_lowercase();
                if !["mon", "tue", "wed", "thu", "fri", "sat", "sun"].contains(&d.as_str()) {
                    anyhow::bail!("[farm] schedule day must be mon..sun, got {d:?}");
                }
            }
        }
        if self.plan.floors[0] == 0 {
            anyhow::bail!("[farm] plan.floors starts at 1");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(toml_src: &str) -> FarmConfig {
        toml::from_str(toml_src).expect("farm config parses")
    }

    #[test]
    fn default_plan_without_windows() {
        let c = FarmConfig::default();
        assert_eq!(c.plan_at(0, 600), c.plan);
    }

    #[test]
    fn window_overrides_dungeon_and_floors() {
        let c = cfg(
            r#"
            [plan]
            dungeon = "old_crypt"
            floors = [1, 3]
            [[schedule]]
            at = "22:00-06:00"
            dungeon = "orc_warrens"
            floors = [5, 6]
            "#,
        );
        assert_eq!(c.plan_at(2, 23 * 60).dungeon, "orc_warrens");
        assert_eq!(c.plan_at(2, 23 * 60).floors, [5, 6]);
        assert_eq!(c.plan_at(2, 5 * 60).dungeon, "orc_warrens");
        assert_eq!(c.plan_at(2, 12 * 60).dungeon, "old_crypt");
    }

    #[test]
    fn window_respects_days_and_rest() {
        let c = cfg(
            r#"
            [[schedule]]
            at = "09:00-18:00"
            days = ["mon", "tue"]
            rest = true
            "#,
        );
        assert!(c.plan_at(0, 10 * 60).rest);
        assert!(!c.plan_at(5, 10 * 60).rest);
        assert!(!c.plan_at(0, 20 * 60).rest);
    }

    #[test]
    fn later_window_wins() {
        let c = cfg(
            r#"
            [[schedule]]
            at = "00:00-23:59"
            floors = [1, 2]
            [[schedule]]
            at = "12:00-13:00"
            floors = [4, 5]
            "#,
        );
        assert_eq!(c.plan_at(3, 12 * 60 + 30).floors, [4, 5]);
        assert_eq!(c.plan_at(3, 8 * 60).floors, [1, 2]);
    }

    #[test]
    fn bad_window_rejected() {
        let c = cfg("[[schedule]]\nat = \"9-18\"\n");
        assert!(c.validate().is_err());
    }
}
