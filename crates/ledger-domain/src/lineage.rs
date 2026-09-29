//! Which copy of a document came from which.
//!
//! Enough history travels inside the document for one copy to prove it
//! descends from another, so promoting a backup is a decision the app can
//! check rather than a guess that it was current.

use crate::text::new_id;
use serde::{Deserialize, Serialize};

/// How many past revisions travel with the document.
///
/// Bounded because this rides along on every read and write. Sixty-four is far
/// more than the gap between a source of truth and a backup that is refreshed
/// on every write; a backup further behind than this reads as
/// [`Relation::TooFarApart`], which is a question for a person rather than a
/// guess by the app.
pub const HISTORY: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Lineage {
    /// Increments on every write. Useful to show; never trusted on its own,
    /// because two forked copies can reach the same number.
    pub generation: u64,
    /// Identifies this exact revision.
    pub revision: String,
    /// Which store this revision was written to.
    pub writer: String,
    /// Revisions this one descends from, newest first, oldest dropped.
    #[serde(default)]
    pub history: Vec<String>,
}

/// What one document is to another.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Relation {
    /// The same revision.
    Same,
    /// This one descends from the other: the other can be fast-forwarded.
    Ahead,
    /// The other descends from this one: this can be fast-forwarded.
    Behind,
    /// Neither descends from the other. Both hold writes the other does not.
    Forked,
    /// Further apart than the history carried can account for. Might be a
    /// long-neglected backup, might be a fork; the app cannot tell.
    TooFarApart,
}

impl Lineage {
    /// The first revision of a document.
    pub fn start(writer: &str) -> Self {
        Self {
            generation: 1,
            revision: new_id(),
            writer: writer.to_string(),
            history: Vec::new(),
        }
    }

    /// The revision that follows this one.
    pub fn next(&self, writer: &str) -> Self {
        let mut history = Vec::with_capacity(self.history.len() + 1);
        history.push(self.revision.clone());
        history.extend(self.history.iter().take(HISTORY - 1).cloned());

        Self {
            generation: self.generation.saturating_add(1),
            revision: new_id(),
            writer: writer.to_string(),
            history,
        }
    }

    /// Whether this lineage's history has been trimmed, and so cannot prove
    /// the absence of a relationship.
    fn trimmed(&self) -> bool {
        self.history.len() >= HISTORY
    }

    pub fn relation_to(&self, other: &Lineage) -> Relation {
        if self.revision == other.revision {
            return Relation::Same;
        }
        if self.history.contains(&other.revision) {
            return Relation::Ahead;
        }
        if other.history.contains(&self.revision) {
            return Relation::Behind;
        }
        // With complete history on both sides, no shared revision means they
        // genuinely diverged. With either side trimmed, it cannot be said.
        if self.trimmed() || other.trimmed() {
            Relation::TooFarApart
        } else {
            Relation::Forked
        }
    }
}

/// Compare two documents that may not both carry a lineage.
///
/// A document written by something that does not stamp lineage — the app this
/// replaces, or a hand edit — cannot be placed, and says so.
pub fn compare(ours: Option<&Lineage>, theirs: Option<&Lineage>) -> Relation {
    match (ours, theirs) {
        (Some(a), Some(b)) => a.relation_to(b),
        (None, None) => Relation::TooFarApart,
        _ => Relation::TooFarApart,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chain(writer: &str, length: usize) -> Lineage {
        let mut lineage = Lineage::start(writer);
        for _ in 1..length {
            lineage = lineage.next(writer);
        }
        lineage
    }

    #[test]
    fn a_fresh_document_starts_at_one() {
        let start = Lineage::start("s3");
        assert_eq!(start.generation, 1);
        assert!(start.history.is_empty());
        assert_eq!(start.writer, "s3");
    }

    #[test]
    fn each_write_advances_the_generation_and_remembers_the_last() {
        let first = Lineage::start("s3");
        let second = first.next("s3");
        assert_eq!(second.generation, 2);
        assert_eq!(second.history[0], first.revision);
        assert_ne!(second.revision, first.revision);
    }

    #[test]
    fn a_copy_of_itself_is_the_same_revision() {
        let one = Lineage::start("s3");
        assert_eq!(one.relation_to(&one.clone()), Relation::Same);
    }

    #[test]
    fn a_backup_left_behind_can_be_fast_forwarded() {
        let backup = Lineage::start("s3");
        let truth = backup.next("s3").next("s3");

        assert_eq!(truth.relation_to(&backup), Relation::Ahead);
        assert_eq!(backup.relation_to(&truth), Relation::Behind);
    }

    #[test]
    fn two_copies_that_both_moved_on_are_a_fork() {
        // The case promotion must never resolve by itself: each holds writes
        // the other does not.
        let shared = Lineage::start("s3");
        let ours = shared.next("s3");
        let theirs = shared.next("drive");

        assert_eq!(ours.relation_to(&theirs), Relation::Forked);
        assert_eq!(theirs.relation_to(&ours), Relation::Forked);
    }

    #[test]
    fn matching_generations_do_not_mean_matching_documents() {
        // Why a bare counter is not enough on its own.
        let shared = Lineage::start("s3");
        let ours = shared.next("s3");
        let theirs = shared.next("drive");

        assert_eq!(ours.generation, theirs.generation);
        assert_eq!(ours.relation_to(&theirs), Relation::Forked);
    }

    #[test]
    fn a_backup_within_the_window_is_still_placed() {
        let backup = Lineage::start("s3");
        let mut truth = backup.clone();
        for _ in 0..HISTORY - 1 {
            truth = truth.next("s3");
        }
        assert_eq!(truth.relation_to(&backup), Relation::Ahead);
    }

    #[test]
    fn a_backup_older_than_the_window_is_reported_as_unknowable() {
        // Honest rather than convenient: past this point the app cannot tell a
        // neglected backup from a fork, so it does not claim to.
        let backup = Lineage::start("s3");
        let mut truth = backup.clone();
        for _ in 0..HISTORY + 5 {
            truth = truth.next("s3");
        }
        assert_eq!(truth.relation_to(&backup), Relation::TooFarApart);
    }

    #[test]
    fn history_never_grows_past_the_bound() {
        let long = chain("s3", HISTORY * 3);
        assert_eq!(long.history.len(), HISTORY);
    }

    #[test]
    fn a_document_with_no_lineage_cannot_be_placed() {
        let one = Lineage::start("s3");
        assert_eq!(compare(Some(&one), None), Relation::TooFarApart);
        assert_eq!(compare(None, None), Relation::TooFarApart);
    }
}
