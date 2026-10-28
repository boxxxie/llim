use std::collections::{BinaryHeap, HashSet};

use chrono::Utc;

use crate::{
    domain::{BackfillJob, BackfillScore, Issue, RateLimitStatus},
    id::{IssueId, UserId},
};

#[derive(Debug, Default)]
pub struct BackfillPriorityQueue {
    heap: BinaryHeap<BackfillJob>,
    queued_ids: HashSet<IssueId>,
    completed_count: usize,
}

impl BackfillPriorityQueue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn enqueue(&mut self, job: BackfillJob) -> bool {
        if self.queued_ids.contains(&job.issue_id) {
            return false;
        }
        self.queued_ids.insert(job.issue_id.clone());
        self.heap.push(job);
        true
    }

    pub fn enqueue_issue(
        &mut self,
        issue: &Issue,
        viewer_id: Option<&UserId>,
        tier_override: Option<u8>,
    ) -> bool {
        let score = issue.backfill_score(viewer_id, tier_override);
        self.enqueue(BackfillJob {
            issue_id: issue.id.clone(),
            score,
        })
    }

    pub fn promote_to_tier_zero(&mut self, issue_id: IssueId) {
        self.queued_ids.insert(issue_id.clone());
        let score = BackfillScore {
            tier: 0,
            priority_rank: 0,
            updated_at_millis: Utc::now().timestamp_millis(),
            issue_id: issue_id.clone(),
        };
        self.heap.push(BackfillJob { issue_id, score });
    }

    pub fn pop_batch(
        &mut self,
        max_batch_size: usize,
        rate_limit: Option<&RateLimitStatus>,
    ) -> Vec<IssueId> {
        let mut popped = Vec::new();
        while popped.len() < max_batch_size {
            let Some(top) = self.heap.peek() else {
                break;
            };

            if let Some(rl) = rate_limit {
                if !rl.allows_backfill() && top.score.tier > 0 {
                    break;
                }
            }

            let Some(job) = self.heap.pop() else {
                break;
            };

            if self.queued_ids.remove(&job.issue_id) {
                popped.push(job.issue_id);
                self.completed_count += 1;
            }
        }
        popped
    }

    pub fn remaining(&self) -> usize {
        self.queued_ids.len()
    }

    pub fn completed(&self) -> usize {
        self.completed_count
    }

    pub fn total(&self) -> usize {
        self.completed_count + self.queued_ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.queued_ids.is_empty()
    }
}
