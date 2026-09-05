//! Stable, serializable view of a [`Plan`] for machine consumption.
//!
//! This is the JSON contract that the CLI's `--json` mode emits and that the
//! GUI consumes. It is deliberately decoupled
//! from the internal [`Plan`]/`PlannedAction` types so the wire format stays
//! stable even as the engine evolves. Nothing here mutates the filesystem.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::plan::{Disposal, Plan};

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct CategorySummary {
    pub category: String,
    /// Human-facing name (from the category registry; falls back to the id).
    pub name: String,
    /// Short explanation (from the registry; empty if the id is unknown).
    pub description: String,
    pub count: usize,
    pub bytes: u64,
    /// May a Smart Scan tick this for you? Copied from the registry, so the
    /// frontend cannot invent a default the backend did not sanction.
    ///
    /// **False for a category the registry does not know**, which is the safe
    /// direction: an unrecognized id is exactly the case where nothing should be
    /// pre-selected on the user's behalf.
    pub smart_scan_default: bool,
}

#[derive(Serialize, Debug)]
pub struct ItemReport {
    /// Absolute, canonical path.
    pub path: String,
    pub size_bytes: u64,
    pub category: String,
    pub disposal: &'static str,
}

#[derive(Serialize, Debug)]
pub struct ScanReport {
    pub total_count: usize,
    pub total_bytes: u64,
    /// True if executing this plan would cross a mass-delete threshold.
    pub requires_confirmation: bool,
    /// Candidates dropped by the safety guard (denylist/allowlist).
    ///
    /// A decision, not a gap — these were seen and refused.
    pub skipped_protected: usize,
    /// Places the walk could not see into: a directory it could not open, or an
    /// entry it could not measure.
    pub skipped_unreadable: usize,
    /// True when the scan describes less than what is there.
    ///
    /// Derived rather than free-standing, so a caller cannot compute it wrongly
    /// or forget to. When this is set, `total_bytes` and `total_count` are
    /// **floors**: the common cause is a cleaner root behind Full Disk Access,
    /// where the alternative is reporting an empty Trash to someone whose Trash
    /// is full.
    ///
    /// Consumers, stated exactly rather than aspirationally: the CLI's
    /// `plan_summary` acts on it today. **The Clean screen does not yet** — it
    /// still renders the total unqualified and still says the Mac is tidy when
    /// the plan is empty. Every other module's view already surfaces its own
    /// `partial`, so Clean is the one screen presenting a floor as a total, and
    /// the notice that closes it is a screenshot change behind the visual gate.
    pub partial: bool,
    /// Per-category rollups, ordered by category name for stable output.
    pub by_category: Vec<CategorySummary>,
    /// One record per planned file.
    ///
    /// Skipped entirely when empty, so a caller that does not need per-file
    /// detail (the GUI, which renders only the rollups) pays nothing for it —
    /// on a real home this list is ~165k records per scan.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<ItemReport>,
}

impl ScanReport {
    /// A report carrying the per-file `items` list. Used by the CLI's `--json`,
    /// whose output contract includes them.
    pub fn from_plan(plan: &Plan) -> Self {
        Self::build(plan, true)
    }

    /// A report with rollups only and no per-file list.
    ///
    /// The GUI renders `by_category` and nothing else, so shipping one record
    /// per file across the IPC boundary is pure cost.
    pub fn from_plan_without_items(plan: &Plan) -> Self {
        Self::build(plan, false)
    }

    fn build(plan: &Plan, with_items: bool) -> Self {
        let mut by_cat: BTreeMap<&str, (usize, u64)> = BTreeMap::new();
        let mut items = Vec::with_capacity(if with_items { plan.actions.len() } else { 0 });

        for a in &plan.actions {
            let e = by_cat.entry(a.category.as_str()).or_insert((0, 0));
            e.0 += 1;
            e.1 += a.size_bytes;
            if with_items {
                items.push(ItemReport {
                    path: a.path.as_path().display().to_string(),
                    size_bytes: a.size_bytes,
                    category: a.category.clone(),
                    disposal: disposal_label(a.disposal),
                });
            }
        }

        let by_category = by_cat
            .into_iter()
            .map(|(category, (count, bytes))| {
                let meta = crate::categories::by_id(category);
                CategorySummary {
                    category: category.to_string(),
                    name: meta.map(|c| c.name).unwrap_or(category).to_string(),
                    description: meta.map(|c| c.description).unwrap_or("").to_string(),
                    count,
                    bytes,
                    smart_scan_default: meta.is_some_and(|c| c.smart_scan_default),
                }
            })
            .collect();

        ScanReport {
            total_count: plan.count(),
            total_bytes: plan.total_bytes(),
            requires_confirmation: plan.requires_confirmation(),
            skipped_protected: plan.skipped_protected,
            skipped_unreadable: plan.skipped_unreadable,
            partial: plan.skipped_unreadable > 0,
            by_category,
            items,
        }
    }

    /// Serialize to pretty JSON. Infallible in practice (the DTO is plain data).
    pub fn to_json_pretty(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
    }
}

fn disposal_label(disposal: Disposal) -> &'static str {
    match disposal {
        Disposal::Trash => "trash",
        Disposal::Permanent => "permanent",
    }
}
