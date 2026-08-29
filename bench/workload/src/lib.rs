//! The workload, defined once so that no two arms can disagree about it.
//!
//! Every arm builds the same rows in the same order with the same labels. That
//! is not a convention maintained by care; it is maintained by there being one
//! definition. The harness generates arm A's JavaScript from these tables and
//! arm C's guest links this crate directly, so a change to a word list moves
//! both arms or neither.
//!
//! # Provenance
//!
//! The three word lists, the label shape, and both operations come from
//! `frameworks/keyed/vanillajs/src/Main.js` in krausest/js-framework-benchmark
//! (Apache-2.0). See the README. They are reproduced verbatim, including the
//! duplicate in [`COLOURS`], so that a label produced here is drawn from the
//! same multiset as a label produced upstream.
//!
//! # The determinism change
//!
//! Upstream draws with `Math.random()`. This crate cannot: the entire claim
//! being measured is a difference between arms, and two arms that built
//! different trees would not be comparable at all. Ten thousand rows of
//! different text is a different number of bytes across the boundary, so an
//! unseeded draw does not add noise to the measurement, it invalidates it.
//!
//! So the draw is a fixed-seed [`Rng`] and every arm consumes the identical
//! sequence. `tests/identical_trees.rs` asserts the trees match rather than
//! trusting that they do.

#![forbid(unsafe_code)]

pub mod scenario;

/// Verbatim from upstream `Main.js`. 25 entries.
pub const ADJECTIVES: [&str; 25] = [
    "pretty",
    "large",
    "big",
    "small",
    "tall",
    "short",
    "long",
    "handsome",
    "plain",
    "quaint",
    "clean",
    "elegant",
    "easy",
    "angry",
    "crazy",
    "helpful",
    "mushy",
    "odd",
    "unsightly",
    "adorable",
    "important",
    "inexpensive",
    "cheap",
    "expensive",
    "fancy",
];

/// Verbatim from upstream `Main.js`. 11 entries.
///
/// `"brown"` appears twice, at index 5 and index 7. That is upstream's list as
/// it stands, not a transcription error here, and it is kept because removing
/// it would change the distribution of labels and make these numbers
/// incomparable with upstream's. It also means the list holds 10 distinct
/// colours in 11 slots, so brown is drawn twice as often as any other.
pub const COLOURS: [&str; 11] = [
    "red", "yellow", "blue", "green", "pink", "brown", "purple", "brown", "white", "black",
    "orange",
];

/// Verbatim from upstream `Main.js`. **13 entries, not 25.**
///
/// Called out because the brief that commissioned this repo said 25, and a
/// reader checking the workload against that brief should find the discrepancy
/// resolved here rather than assume a truncation. Upstream's list is thirteen
/// words long; this is all of it.
pub const NOUNS: [&str; 13] = [
    "table", "chair", "house", "bbq", "desk", "car", "pony", "cookie", "sandwich", "burger",
    "pizza", "mouse", "keyboard",
];

/// The suffix `partial-update` appends, from upstream's `updateData`.
pub const UPDATE_SUFFIX: &str = " !!!";

/// Every tenth row is updated, from upstream's `updateData` (`i += 10`).
pub const UPDATE_STRIDE: usize = 10;

/// The seed. Any fixed value works; this one is recorded in the JSON output so
/// a run can be reproduced from its own report.
pub const SEED: u32 = 0x5EED_1234;

/// xorshift32, chosen because JavaScript can execute it exactly.
///
/// This has to run identically in Rust and in JavaScript, and JavaScript has no
/// 64-bit integer arithmetic outside `BigInt`. A 64-bit generator would have to
/// be emulated in the JS arm, and an emulation that is subtly wrong produces
/// different labels, a different byte count, and a difference between arms that
/// looks like a finding. Every operation here is a 32-bit shift or xor, which
/// JavaScript performs exactly with `<<`, `>>>` and `^` given a `>>> 0` to
/// return to unsigned after each step.
///
/// Statistical quality is irrelevant. The generator's whole job is to be the
/// same sequence twice.
#[derive(Debug, Clone)]
pub struct Rng {
    state: u32,
}

impl Rng {
    /// Seeded at [`SEED`].
    #[must_use]
    pub fn new() -> Self {
        Self::with_seed(SEED)
    }

    /// A zero seed is a fixed point of xorshift, so it is rejected rather than
    /// silently producing an infinite run of zeroes and a table of one label.
    #[must_use]
    pub fn with_seed(seed: u32) -> Self {
        Self {
            state: if seed == 0 { SEED } else { seed },
        }
    }

    /// The next 32 bits.
    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        x
    }

    /// Upstream's selection, with the draw replaced and the shape kept.
    ///
    /// Upstream is `Math.round(Math.random() * 1000) % max`, which yields 0..=1000
    /// and then folds it into range. That fold is biased whenever `max` does not
    /// divide 1001, and the bias is deliberately preserved here: correcting it
    /// would change which labels appear and how often, and the point of copying
    /// upstream's lists is to stay comparable with upstream's labels. Only the
    /// source of randomness differs.
    pub fn index(&mut self, max: usize) -> usize {
        (self.next_u32() % 1001) as usize % max
    }
}

impl Default for Rng {
    fn default() -> Self {
        Self::new()
    }
}

/// One row: an id and the label to show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub id: u32,
    pub label: String,
}

/// The row source, holding the id counter and the generator.
///
/// Ids are sequential from 1 across the life of the store, matching upstream's
/// `this.id++`. An arm that builds 10,000 rows and then updates them draws from
/// one store, so the ids and labels are a single continuing sequence rather than
/// two independent ones.
#[derive(Debug, Clone)]
pub struct Store {
    rng: Rng,
    next_id: u32,
}

impl Store {
    #[must_use]
    pub fn new() -> Self {
        Self {
            rng: Rng::new(),
            next_id: 1,
        }
    }

    /// `"{adjective} {colour} {noun}"`, one space between each.
    ///
    /// The three draws happen in this order and the arms depend on it: the
    /// generator is a sequence, so drawing colour before adjective would give
    /// every subsequent row a different label.
    pub fn next_label(&mut self) -> String {
        let adjective = ADJECTIVES[self.rng.index(ADJECTIVES.len())];
        let colour = COLOURS[self.rng.index(COLOURS.len())];
        let noun = NOUNS[self.rng.index(NOUNS.len())];
        let mut label = String::with_capacity(adjective.len() + colour.len() + noun.len() + 2);
        label.push_str(adjective);
        label.push(' ');
        label.push_str(colour);
        label.push(' ');
        label.push_str(noun);
        label
    }

    /// The next row.
    pub fn next_row(&mut self) -> Row {
        let id = self.next_id;
        self.next_id += 1;
        Row {
            id,
            label: self.next_label(),
        }
    }

    /// `count` rows.
    pub fn build(&mut self, count: usize) -> Vec<Row> {
        (0..count).map(|_| self.next_row()).collect()
    }
}

impl Default for Store {
    fn default() -> Self {
        Self::new()
    }
}

/// The two operations, and the table sizes they run against.
pub mod op {
    /// `create-1000`: 1,000 rows into an empty table.
    pub const CREATE_ROWS: usize = 1_000;

    /// `partial-update` runs against a table of this size.
    pub const UPDATE_TABLE_ROWS: usize = 10_000;

    /// How many rows `partial-update` touches: every tenth of 10,000.
    pub const UPDATE_TOUCHED: usize = UPDATE_TABLE_ROWS / super::UPDATE_STRIDE;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lists_match_upstreams_lengths() {
        assert_eq!(ADJECTIVES.len(), 25);
        assert_eq!(COLOURS.len(), 11);
        // Thirteen. The brief said 25; upstream says 13. See NOUNS.
        assert_eq!(NOUNS.len(), 13);
    }

    #[test]
    fn brown_really_is_in_there_twice() {
        assert_eq!(COLOURS.iter().filter(|c| **c == "brown").count(), 2);
    }

    #[test]
    fn the_sequence_is_reproducible() {
        assert_eq!(Store::new().build(50), Store::new().build(50));
    }

    #[test]
    fn ids_are_sequential_from_one() {
        let rows = Store::new().build(5);
        assert_eq!(
            rows.iter().map(|r| r.id).collect::<Vec<_>>(),
            vec![1, 2, 3, 4, 5]
        );
    }

    #[test]
    fn labels_are_three_words_from_the_lists() {
        for row in Store::new().build(200) {
            let words: Vec<&str> = row.label.split(' ').collect();
            assert_eq!(words.len(), 3, "{:?}", row.label);
            assert!(ADJECTIVES.contains(&words[0]));
            assert!(COLOURS.contains(&words[1]));
            assert!(NOUNS.contains(&words[2]));
        }
    }

    /// A zero seed is absorbing for xorshift. If it were accepted, every label
    /// would be identical and the byte counts would silently collapse.
    #[test]
    fn a_zero_seed_does_not_freeze_the_generator() {
        let mut rng = Rng::with_seed(0);
        let first = rng.next_u32();
        assert_ne!(first, 0);
        assert_ne!(rng.next_u32(), first);
    }
}
