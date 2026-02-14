use std::collections::BTreeMap;

use sentinelpipe_core::{Finding, RunConfig};
use sentinelpipe_pipeline::Selector;

/// Category-diverse top-K selector.
///
/// - group by category
/// - sort each category by risk
/// - round-robin pick across categories
pub struct DiverseTopKSelector;

impl Selector for DiverseTopKSelector {
    fn select(&self, cfg: &RunConfig, findings: Vec<Finding>) -> Vec<Finding> {
        let mut grouped: BTreeMap<String, Vec<Finding>> = BTreeMap::new();
        for finding in findings {
            grouped
                .entry(finding.category.clone())
                .or_default()
                .push(finding);
        }

        for group in grouped.values_mut() {
            group.sort_by(|a, b| {
                b.total_risk
                    .partial_cmp(&a.total_risk)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }

        let limit = cfg.top_k.unwrap_or(usize::MAX);
        let mut selected = Vec::new();

        while selected.len() < limit {
            let mut progress = false;
            for group in grouped.values_mut() {
                if selected.len() >= limit {
                    break;
                }
                if let Some(f) = group.first().cloned() {
                    selected.push(f);
                    group.remove(0);
                    progress = true;
                }
            }
            if !progress {
                break;
            }
        }

        selected
    }
}
