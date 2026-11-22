//! Farm mode: a rule-based stand-in for the LLM. It answers every prompt
//! from the shared state alone, so one character can hunt, restock and
//! respawn unattended on a time schedule, at no LLM cost.

mod config;
mod policy;
mod status;

use std::sync::{Arc, Mutex as SyncMutex};
use std::time::Instant;

use async_trait::async_trait;
use tokio::sync::Mutex;
use tracing::{info, warn};

pub use config::{FarmConfig, PlanConfig};

use crate::driver::LlmBackend;
use crate::state::SharedState;
use policy::{GroundView, Memory, MonsterView, Snapshot};

pub struct FarmBackend {
    state: Arc<Mutex<SharedState>>,
    cfg: FarmConfig,
    label: String,
    mem: SyncMutex<Memory>,
    status_path: Option<std::path::PathBuf>,
}

impl FarmBackend {
    pub fn new(
        state: Arc<Mutex<SharedState>>,
        cfg: FarmConfig,
        label: &str,
    ) -> anyhow::Result<Self> {
        cfg.validate()?;
        let status_path = status::path_for(&cfg.status_file, label);
        info!(
            "[{label}] farm mode: {} ({} schedule windows)",
            cfg.plan.label(),
            cfg.schedule.len()
        );
        Ok(Self {
            state,
            cfg,
            label: label.to_string(),
            mem: SyncMutex::new(Memory::default()),
            status_path,
        })
    }

    async fn snapshot(&self, plan: &PlanConfig) -> Snapshot {
        let (dungeon_name, key_prefix) = resolve_dungeon(&plan.dungeon);
        let s = self.state.lock().await;
        let me = s.self_player.as_ref();
        let pos = me.map_or((0.0, 0.0), |p| (p.position.x, p.position.z));
        let floor = s.self_floor_level;
        let d = |x: f32, z: f32| (x - pos.0).hypot(z - pos.1);
        Snapshot {
            in_game: s.in_game && me.is_some(),
            hp: me.map_or(0, |p| p.health),
            max_hp: me.map_or(0, |p| p.max_health),
            floor,
            pos,
            gold: s.self_gold.unwrap_or(0),
            satiation: s.self_hunger.map(|(v, _)| v),
            bag: s
                .self_bag
                .iter()
                .map(|i| (i.item_def_id.clone(), i.quantity))
                .collect(),
            monsters: s
                .nearby_monsters
                .values()
                .map(|m| MonsterView {
                    id: m.id.clone(),
                    dist: d(m.position.x, m.position.z),
                    health: m.health,
                    floor: m.floor_level,
                })
                .collect(),
            ground: s
                .ground_items_in_sight()
                .into_iter()
                .map(|(dist, g)| GroundView {
                    id: g.instance_id,
                    def_id: g.item_def_id.clone(),
                    dist,
                    floor: g.floor_level,
                })
                .collect(),
            merchants: s
                .nearby_players
                .values()
                .filter(|p| p.is_official_npc)
                .map(|p| p.name.clone())
                .collect(),
            is_night: s.is_night.unwrap_or(false),
            dungeon_name,
            key_prefix,
        }
    }
}

/// Accepts the registry id or the display name; the mover wants the name.
fn resolve_dungeon(asked: &str) -> (String, String) {
    let asked_l = asked.trim().to_ascii_lowercase();
    onlinerpg_shared::dungeon::entrances()
        .iter()
        .find(|d| d.id.eq_ignore_ascii_case(&asked_l) || d.name.to_ascii_lowercase() == asked_l)
        .map(|d| (d.name.clone(), d.key_prefix.clone()))
        .unwrap_or_else(|| (asked.trim().to_string(), String::new()))
}

#[async_trait]
impl LlmBackend for FarmBackend {
    async fn send_message(&self, _prompt: &str) -> anyhow::Result<String> {
        let plan = self.cfg.plan_now();
        let snap = self.snapshot(&plan).await;
        let decision = {
            let mut mem = self.mem.lock().unwrap_or_else(|e| e.into_inner());
            if mem.last_thought != plan.label() {
                info!("[{}] farm plan: {}", self.label, plan.label());
                mem.last_thought = plan.label();
            }
            let d = policy::decide(Instant::now(), &snap, &plan, &self.cfg, &mut mem);
            if let Some(path) = &self.status_path {
                if let Err(e) = status::write(path, &self.label, &snap, &plan, &mem, &d) {
                    warn!("[{}] farm status write failed: {e}", self.label);
                }
            }
            d
        };
        Ok(serde_json::json!({
            "thought": decision.thought,
            "actions": decision.actions,
        })
        .to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_registry_dungeons_by_id_and_name() {
        let (name, key) = resolve_dungeon("old_crypt");
        assert_eq!(name, "Old Crypt");
        assert_eq!(key, "crypt_key");
        assert_eq!(resolve_dungeon("old crypt").0, "Old Crypt");
        assert_eq!(resolve_dungeon("Nowhere").0, "Nowhere");
    }
}
