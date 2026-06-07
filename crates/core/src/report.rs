//! Stable, serializable view of a [`Plan`] for machine consumption.
//!
//! This is the JSON contract that the CLI's `--json` mode emits and that a
//! future GUI (or an automated test) consumes. It is deliberately decoupled
//! from the internal [`Plan`]/`PlannedAction` types so the wire format stays
//! stable even as the engine evolves. Nothing here mutates the filesystem.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::plan::{Disposal, Plan};

#[derive(Serialize, Debug, PartialEq, Eq)]
pub struct CategorySummary {
    pub category: String,
    pub count: usize,
    pub bytes: u64,
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
    pub skipped_protected: usize,
    /// Per-category rollups, ordered by category name for stable output.
    pub by_category: Vec<CategorySummary>,
    pub items: Vec<ItemReport>,
}

impl ScanReport {
    pub fn from_plan(plan: &Plan) -> Self {
        let mut by_cat: BTreeMap<&str, (usize, u64)> = BTreeMap::new();
        let mut items = Vec::with_capacity(plan.actions.len());

        for a in &plan.actions {
            let e = by_cat.entry(a.category.as_str()).or_insert((0, 0));
            e.0 += 1;
            e.1 += a.size_bytes;
            items.push(ItemReport {
                path: a.path.as_path().display().to_string(),
                size_bytes: a.size_bytes,
                category: a.category.clone(),
                disposal: disposal_label(a.disposal),
            });
        }

        let by_category = by_cat
            .into_iter()
            .map(|(category, (count, bytes))| CategorySummary {
                category: category.to_string(),
                count,
                bytes,
            })
            .collect();

        ScanReport {
            total_count: plan.count(),
            total_bytes: plan.total_bytes(),
            requires_confirmation: plan.requires_confirmation(),
            skipped_protected: plan.skipped_protected,
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
