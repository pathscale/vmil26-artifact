//! Which configurations get run, and why each one is in the list.
//!
//! Not a full cross product. 4 vocabularies by 2 name lengths by 2
//! chattinesses by 3 shapes by 2 event settings is 96 points, most of which
//! answer no question, and a matrix nobody reads is not more rigorous than one
//! somebody does.
//!
//! What is here instead:
//!
//! - **A full grid over vocabulary and name length**, because the worst case
//!   for an atom tier is long *and* distinct together and varying one axis at a
//!   time never visits that corner. This grid is also the crossover
//!   experiment.
//! - **A ratio ladder**, extra vocabulary sizes chosen to land at repetition
//!   ratios between 1 and 6, where the crossover is expected. The user-facing
//!   grid brackets it; this resolves it.
//! - **One-axis variations** off the baseline for chattiness, shape and
//!   events, which are questions about a different thing and do not need to be
//!   crossed with the vocabulary grid to be answered.
//!
//! Every point is tagged with the axis it exercises so the JSON matrix can be
//! read a column at a time.

use workload::scenario::{Chattiness, Config, NameLength, TreeShape, Vocabulary};

/// One point in the sweep.
#[derive(Debug, Clone)]
pub struct Point {
    /// Stable slug, used as the matrix key.
    pub id: String,
    /// What this point is for, in a sentence.
    pub rationale: &'static str,
    /// Which axis it varies.
    pub axis: &'static str,
    pub config: Config,
}

/// How many events a storm delivers.
///
/// Enough that dispatch dominates the configuration's cost rather than being
/// lost in the tree build, and small enough that twenty-four timed iterations
/// of it stay quick.
pub const STORM: usize = 2_000;

/// Listeners attached for the event points.
pub const LISTENERS: usize = 500;

fn id_for(config: &Config) -> String {
    let vocabulary = match config.vocabulary {
        Vocabulary::Distinct(n) => format!("v{n}"),
        Vocabulary::UniquePerElement => "vunique".to_string(),
    };
    let names = match config.names {
        NameLength::Short => "short",
        NameLength::Long => "long",
    };
    let chattiness = match config.chattiness {
        Chattiness::ManySmall => "many",
        Chattiness::FewLarge => "few",
    };
    let shape = match config.shape {
        TreeShape::Deep => "deep",
        TreeShape::Wide => "wide",
        TreeShape::Realistic => "real",
    };
    // Listeners are in the id even when no storm follows, so that "listeners
    // attached, nothing dispatched" is a distinct point rather than colliding
    // with the no-events baseline and being deduplicated away. Without it there
    // is no control separating what registration costs from what delivery
    // costs, and the event axis has one point instead of two.
    let events = match (config.listeners, config.dispatches) {
        (0, _) => String::new(),
        (listeners, 0) => format!("-listen{listeners}"),
        (listeners, dispatches) => format!("-listen{listeners}-storm{dispatches}"),
    };
    format!("{vocabulary}-{names}-{chattiness}-{shape}{events}")
}

/// Every configuration the sweep runs.
pub fn points() -> Vec<Point> {
    let mut points = Vec::new();
    let baseline = Config::baseline();

    // --- The grid. Vocabulary crossed with name length. ---
    //
    // 8, 80 and 800 are the sizes the brief names; unique-per-element is the
    // adversarial case where interning pays for every name and nothing is ever
    // reused.
    for vocabulary in [
        Vocabulary::Distinct(8),
        Vocabulary::Distinct(80),
        Vocabulary::Distinct(800),
        Vocabulary::UniquePerElement,
    ] {
        for names in [NameLength::Short, NameLength::Long] {
            let config = Config {
                vocabulary,
                names,
                ..baseline
            };
            points.push(Point {
                id: id_for(&config),
                rationale: "Vocabulary crossed with name length. Long and distinct together \
                            is the worst case for an atom tier: expensive to intern and \
                            never earned back.",
                axis: "vocabulary x name-length",
                config,
            });
        }
    }

    // --- The ratio ladder. ---
    //
    // The grid's rungs are an order of magnitude apart: R jumps from 73 at 80
    // distinct names straight to 7.5 at 800. The crossover sits in that gap, and
    // interpolating across a 10x span makes the headline number swing by a
    // factor of two between runs on ordinary timing noise. These rungs land at
    // R of roughly 40, 20 and 12, which brackets it closely enough that the
    // interpolation is stable.
    //
    // Both name lengths, because the crossover is at a different ratio for each
    // and a ladder under only one of them leaves the other interpolating across
    // the wide gap again.
    for distinct in [150, 300, 500] {
        for names in [NameLength::Short, NameLength::Long] {
            let config = Config {
                vocabulary: Vocabulary::Distinct(distinct),
                names,
                ..baseline
            };
            points.push(Point {
                id: id_for(&config),
                rationale: "Fine rung on the ratio ladder. The grid's rungs are 10x apart, \
                            which is too coarse to interpolate a crossover through without \
                            it swinging on timing noise.",
                axis: "vocabulary x name-length",
                config,
            });
        }
    }

    // Below the 800-name rung, resolving the approach to the floor of 3.
    for distinct in [1_000, 1_333, 1_600] {
        let config = Config {
            vocabulary: Vocabulary::Distinct(distinct),
            names: NameLength::Long,
            ..baseline
        };
        points.push(Point {
            id: id_for(&config),
            rationale: "Resolves the repetition ratio between the 800-name grid point and \
                        the unique-name one, where the trade has already turned over and \
                        the question is how steeply.",
            axis: "vocabulary x name-length",
            config,
        });
    }

    // --- Chattiness. ---
    //
    // Both ends write the same bytes in different numbers of calls, so a
    // per-call boundary cost shows up here and a per-byte one does not.
    // Crossed with vocabulary only at its two extremes: chattiness is a
    // question about call count, and asking it four times over would not
    // answer it better.
    for chattiness in [Chattiness::ManySmall, Chattiness::FewLarge] {
        for vocabulary in [Vocabulary::Distinct(8), Vocabulary::UniquePerElement] {
            let config = Config {
                chattiness,
                vocabulary,
                ..baseline
            };
            let id = id_for(&config);
            if points.iter().any(|point| point.id == id) {
                continue;
            }
            points.push(Point {
                id,
                rationale: "Same total text delivered in many small writes or few large \
                            ones. Separates a per-call boundary cost from a per-byte one.",
                axis: "chattiness",
                config,
            });
        }
    }

    // --- Tree shape. ---
    for shape in [TreeShape::Deep, TreeShape::Wide, TreeShape::Realistic] {
        for vocabulary in [Vocabulary::Distinct(8), Vocabulary::UniquePerElement] {
            let config = Config {
                shape,
                vocabulary,
                ..baseline
            };
            let id = id_for(&config);
            if points.iter().any(|point| point.id == id) {
                continue;
            }
            points.push(Point {
                id,
                rationale: "Deep chain against wide fan at equal node count, with the \
                            engine's own realistic shape between them.",
                axis: "tree-shape",
                config,
            });
        }
    }

    // --- Events. ---
    //
    // The claim is that a click copies zero bytes. A storm tests whether the
    // deferred-dispatch queue becomes the cost instead.
    for (listeners, dispatches) in [(LISTENERS, 0), (LISTENERS, STORM)] {
        let config = Config {
            listeners,
            dispatches,
            ..baseline
        };
        let id = id_for(&config);
        if points.iter().any(|point| point.id == id) {
            continue;
        }
        points.push(Point {
            id,
            rationale: "Listeners attached, then a dispatch storm. Tests whether delivery \
                        rather than data becomes the cost.",
            axis: "event-rate",
            config,
        });
    }

    points
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_point_has_a_distinct_id() {
        let points = points();
        let mut ids: Vec<&str> = points.iter().map(|point| point.id.as_str()).collect();
        ids.sort_unstable();
        let count = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), count, "duplicate ids in the sweep");
    }

    /// The sweep is pointless if it never visits the case interning loses.
    #[test]
    fn the_adversarial_corner_is_in_the_sweep() {
        assert!(points().iter().any(|point| {
            point.config.vocabulary == Vocabulary::UniquePerElement
                && point.config.names == NameLength::Long
        }));
    }

    /// And pointless again if every point sits on one side of the crossover.
    ///
    /// The floor is **3, not 1**, and that is a property of the workload rather
    /// than a gap in the sweep. Even when every class *value* is distinct, the
    /// tag name and the attribute name are reused on every element, so a page
    /// of `n` elements makes `3n` name uses of `n + 2` distinct names. Reaching
    /// R = 1 would need a distinct tag and a distinct attribute name per
    /// element too, which is not a page anyone writes. Three is the real
    /// adversarial floor for a document, and the sweep reaches it.
    #[test]
    fn the_ratios_bracket_the_expected_crossover() {
        let ratios: Vec<f64> = points()
            .iter()
            .map(|point| point.config.repetition_ratio())
            .collect();
        assert!(
            ratios.iter().any(|ratio| *ratio < 3.5),
            "nothing near the bottom: {ratios:?}"
        );
        assert!(
            ratios.iter().any(|ratio| *ratio > 100.0),
            "nothing near the top: {ratios:?}"
        );
        // And something in between, or the ladder resolves nothing.
        assert!(ratios.iter().any(|ratio| (2.0..12.0).contains(ratio)));
    }

    #[test]
    fn all_five_axes_are_represented() {
        let points = points();
        for axis in [
            "vocabulary x name-length",
            "chattiness",
            "tree-shape",
            "event-rate",
        ] {
            assert!(
                points.iter().any(|point| point.axis == axis),
                "no points for {axis}"
            );
        }
        assert!(points.iter().any(|point| point.config.dispatches > 0));
    }
}
