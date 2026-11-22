//! One JSON file per character, rewritten every turn, so a shell script or a
//! dashboard can read the farmer's state without the watch panel.

use std::path::{Path, PathBuf};

use super::config::PlanConfig;
use super::policy::{Decision, Memory, Snapshot};

pub fn path_for(template: &str, label: &str) -> Option<PathBuf> {
    if template.trim().is_empty() {
        return None;
    }
    let safe: String = label
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    Some(PathBuf::from(template.replace("{name}", &safe)))
}

pub fn write(
    path: &Path,
    label: &str,
    snap: &Snapshot,
    plan: &PlanConfig,
    mem: &Memory,
    decision: &Decision,
) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let body = serde_json::json!({
        "character": label,
        "updated": chrono::Local::now().to_rfc3339(),
        "plan": plan.label(),
        "hp": snap.hp,
        "max_hp": snap.max_hp,
        "floor": snap.floor,
        "x": snap.pos.0,
        "z": snap.pos.1,
        "gold": snap.gold,
        "satiation": snap.satiation,
        "potions": snap.count(&plan.potion),
        "food": snap.count(&plan.food),
        "return_scrolls": snap.count(&plan.return_scroll),
        "monsters_in_sight": snap.monsters.len(),
        "returning": mem.returning,
        "deaths": mem.deaths,
        "attacks": mem.attacks,
        "potions_used": mem.potions_used,
        "town_trips": mem.town_trips,
        "thought": decision.thought,
        "actions": decision.actions,
    });
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(&body)?)?;
    std::fs::rename(&tmp, path)
}
