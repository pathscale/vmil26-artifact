//! The parameterised workload: five knobs, one generator.
//!
//! The base benchmark had two hardcoded operations. This is the same idea with
//! the constants pulled out, because two points cannot show where a trend
//! crosses zero and the interesting question about an interned-string ABI is
//! exactly where it stops paying.
//!
//! # What this is for
//!
//! Finding the configurations where interning **loses**. Interning trades a
//! per-name copy for per-use freedom, so there is a repetition ratio below
//! which it costs more than it saves, and a benchmark that never reaches that
//! ratio is not evidence about the design, it is a demonstration of it. Every
//! axis here is chosen to push toward the losing side:
//!
//! | axis | favourable end | adversarial end |
//! | --- | --- | --- |
//! | vocabulary | 8 distinct names | one per element, nothing to reuse |
//! | name length | `"c7"` | 50-plus bytes of Tailwind |
//! | chattiness | few large writes | many small ones |
//! | shape | wide fan | deep chain |
//! | events | none | a dispatch storm |
//!
//! Vocabulary and name length are swept as a full cross product rather than one
//! at a time, because the worst case for the atom tier is **long and distinct
//! together**: a long name is expensive to intern and a distinct one never
//! earns the cost back. Varying them separately never visits that corner.
//!
//! # One definition, executed twice
//!
//! Arm C's guest links this module and calls [`Program::build`] directly. Arm
//! A's JavaScript transcribes *this generator*, not each scenario, so adding a
//! configuration to the sweep adds no transcription at all and the identity
//! assertion covers every one of them. That is a much stronger check than the
//! base benchmark had, where a single hand-written scenario was transcribed
//! once.
//!
//! The configuration crosses into the guest as six integers, once, inside
//! `prepare`, which is outside every measured window. Nothing about the program
//! itself crosses.

use crate::Rng;

/// How many distinct class values the page draws from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Vocabulary {
    /// `n` distinct values, reused round-robin across the tree.
    Distinct(usize),
    /// One value per element. The adversarial case: interning pays the copy
    /// for every name and gets nothing back, because nothing is ever reused.
    UniquePerElement,
}

impl Vocabulary {
    /// The value index element `i` uses.
    pub fn index_for(self, element: usize) -> usize {
        match self {
            Vocabulary::Distinct(n) => element % n.max(1),
            Vocabulary::UniquePerElement => element,
        }
    }

    /// How many distinct values a tree of `elements` will actually use.
    pub fn distinct_over(self, elements: usize) -> usize {
        match self {
            Vocabulary::Distinct(n) => n.max(1).min(elements),
            Vocabulary::UniquePerElement => elements,
        }
    }

    pub fn as_ints(self) -> (i32, i32) {
        match self {
            Vocabulary::Distinct(n) => (0, n as i32),
            Vocabulary::UniquePerElement => (1, 0),
        }
    }

    pub fn from_ints(kind: i32, size: i32) -> Self {
        if kind == 1 {
            Vocabulary::UniquePerElement
        } else {
            Vocabulary::Distinct(size.max(1) as usize)
        }
    }
}

/// How long the class values are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameLength {
    /// `"c0"`, `"c1"`, ... Two to five bytes.
    Short,
    /// Tailwind-shaped: a discriminator plus six utility tokens, around 50 to
    /// 70 bytes. This is what a real component library puts in a `class`
    /// attribute, and it is the length at which interning a name that is never
    /// reused is most expensive.
    Long,
}

impl NameLength {
    pub fn as_int(self) -> i32 {
        match self {
            NameLength::Short => 0,
            NameLength::Long => 1,
        }
    }

    pub fn from_int(value: i32) -> Self {
        if value == 1 {
            NameLength::Long
        } else {
            NameLength::Short
        }
    }
}

/// Utility tokens for the long names. A fixed pool, so the JavaScript
/// transcription is a literal array and cannot drift by generation rule.
pub const UTILITIES: [&str; 24] = [
    "flex",
    "items-center",
    "justify-between",
    "px-4",
    "py-2",
    "rounded-lg",
    "border",
    "border-slate-200",
    "bg-white",
    "text-sm",
    "font-medium",
    "shadow-sm",
    "hover:bg-slate-50",
    "focus:outline-none",
    "transition-colors",
    "duration-150",
    "gap-2",
    "w-full",
    "max-w-md",
    "truncate",
    "cursor-pointer",
    "select-none",
    "leading-tight",
    "tracking-wide",
];

/// How many utilities a long name carries.
pub const UTILITIES_PER_NAME: usize = 6;

/// The class value for a given vocabulary index.
///
/// The `c{index}` prefix is not decoration: without it, six tokens drawn from a
/// pool of twenty-four collide, and two "distinct" vocabulary entries would be
/// the same string. The distinct-name count is the denominator of the
/// repetition ratio this whole sweep exists to measure, so a collision would
/// corrupt the headline number rather than merely look untidy.
pub fn class_value(index: usize, length: NameLength) -> String {
    match length {
        NameLength::Short => format!("c{index}"),
        NameLength::Long => {
            let mut value = format!("c{index}");
            for slot in 0..UTILITIES_PER_NAME {
                value.push(' ');
                value.push_str(UTILITIES[(index * 7 + slot * 11) % UTILITIES.len()]);
            }
            value
        }
    }
}

/// Same total mutation, delivered in different numbers of calls.
///
/// The axis that separates a per-call boundary cost from a per-byte one: both
/// ends write the same number of text bytes, so a binding whose cost is per
/// byte should not care and one whose cost is per call should.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chattiness {
    /// One text write per element.
    ManySmall,
    /// One write per [`Chattiness::BATCH`] elements, each that many times
    /// larger.
    FewLarge,
}

impl Chattiness {
    pub const BATCH: usize = 16;

    pub fn as_int(self) -> i32 {
        match self {
            Chattiness::ManySmall => 0,
            Chattiness::FewLarge => 1,
        }
    }

    pub fn from_int(value: i32) -> Self {
        if value == 1 {
            Chattiness::FewLarge
        } else {
            Chattiness::ManySmall
        }
    }
}

/// Where a new element is attached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeShape {
    /// Every element is the child of the previous one. A single chain.
    Deep,
    /// Every element is a child of the root. One enormous fan.
    Wide,
    /// A mixture, biased toward recent parents, which produces chains broken
    /// by occasional branching. The achieved depth and fanout are measured and
    /// reported rather than claimed, because "realistic" is an assertion about
    /// a distribution and the report should carry the distribution.
    Realistic,
}

impl TreeShape {
    pub fn as_int(self) -> i32 {
        match self {
            TreeShape::Deep => 0,
            TreeShape::Wide => 1,
            TreeShape::Realistic => 2,
        }
    }

    pub fn from_int(value: i32) -> Self {
        match value {
            0 => TreeShape::Deep,
            1 => TreeShape::Wide,
            _ => TreeShape::Realistic,
        }
    }
}

/// One point in the sweep.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Config {
    pub vocabulary: Vocabulary,
    pub names: NameLength,
    pub chattiness: Chattiness,
    pub shape: TreeShape,
    /// Elements in the tree. Held constant across the sweep so that a
    /// difference between configurations is the axis and not the size.
    pub elements: usize,
    /// Elements that get a listener attached.
    pub listeners: usize,
    /// Events delivered after the tree is built.
    pub dispatches: usize,
}

impl Config {
    /// The baseline every one-axis variation is a change from.
    pub const ELEMENTS: usize = 2_000;

    pub fn baseline() -> Self {
        Self {
            vocabulary: Vocabulary::Distinct(80),
            names: NameLength::Short,
            chattiness: Chattiness::ManySmall,
            shape: TreeShape::Realistic,
            elements: Self::ELEMENTS,
            listeners: 0,
            dispatches: 0,
        }
    }

    /// The six integers that cross into the guest, once, in `prepare`.
    pub fn as_ints(self) -> [i32; 7] {
        let (kind, size) = self.vocabulary.as_ints();
        [
            kind,
            size,
            self.names.as_int(),
            self.chattiness.as_int(),
            self.shape.as_int(),
            self.elements as i32,
            self.listeners as i32,
        ]
    }

    pub fn from_ints(v: [i32; 7]) -> Self {
        Self {
            vocabulary: Vocabulary::from_ints(v[0], v[1]),
            names: NameLength::from_int(v[2]),
            chattiness: Chattiness::from_int(v[3]),
            shape: TreeShape::from_int(v[4]),
            elements: v[5].max(0) as usize,
            listeners: v[6].max(0) as usize,
            dispatches: 0,
        }
    }

    /// Distinct names the page uses: the class values, plus `"class"` itself,
    /// plus the one tag name.
    pub fn distinct_names(self) -> usize {
        self.vocabulary.distinct_over(self.elements) + 2
    }

    /// Total name uses: one class value and one tag per element, plus the
    /// attribute name each time.
    pub fn name_uses(self) -> usize {
        self.elements * 3
    }

    /// **The headline denominator.** Total name uses over distinct names.
    ///
    /// Interning trades `distinct_names` extra crossings for
    /// `(name_uses - distinct_names) * mean_name_length` bytes, so this ratio
    /// is what decides whether that trade is worth making.
    pub fn repetition_ratio(self) -> f64 {
        self.name_uses() as f64 / self.distinct_names() as f64
    }

    /// Mean bytes per distinct name, which is the other half of the trade.
    pub fn mean_name_length(self) -> f64 {
        let distinct = self.vocabulary.distinct_over(self.elements);
        if distinct == 0 {
            return 0.0;
        }
        let total: usize = (0..distinct)
            .map(|index| class_value(index, self.names).len())
            .sum();
        // Plus `class` and the tag name, which are also interned.
        (total + "class".len() + TAG.len()) as f64 / (distinct + 2) as f64
    }
}

/// One tag for every element.
///
/// A real page uses many, but a varying tag vocabulary would move the
/// distinct-name count on an axis that is not being swept, and the whole point
/// here is that vocabulary changes only when the vocabulary knob is turned.
pub const TAG: &str = "div";

/// The text a chattiness write carries, for element `i`.
///
/// Content, not vocabulary: no design can intern it, which is what makes the
/// chattiness axis a clean per-call versus per-byte test.
pub fn text_chunk(index: usize) -> String {
    format!("n{index} ")
}

/// One operation in the generated program.
///
/// The DOM's own vocabulary, not any particular binding's spelling of it. Each
/// arm renders these into its own calls.
///
/// # Why every element gets its own text node
///
/// Setting text on an *element* replaces its children with a single text node.
/// An earlier version of this generator wrote the chattiness phase's text
/// straight onto the elements, which quietly demolished the tree the shape axis
/// had just built: by the end of a run a deep chain and a wide fan were both
/// mostly a flat row of text. The shape axis was varying a shape that no longer
/// existed when the measurement was taken.
///
/// So the build gives every element an empty text node, and the text phase
/// rewrites *those* in place. A text node has no element children to destroy,
/// the shape survives, and the empty node costs zero bytes to create.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Create an element with the fixed tag and push it on the element list.
    Create,
    /// Set `class` on the most recently created element to vocabulary entry `n`.
    ///
    /// The index, not the string. The string is built once, in the arm's
    /// preparation step, and looked up here. An earlier version rendered it per
    /// step and so measured a string formatter: the short-to-long contrast
    /// moved by the same three milliseconds whether 552 or 136,008 extra bytes
    /// crossed the boundary, because almost none of that time was the boundary.
    SetClass(usize),
    /// Create an empty text node for the element of the same index.
    CreateText,
    /// Append text node `index` to element `index`.
    AppendTextTo { index: usize },
    /// Append element `child` to element `parent`, where `parent` may be
    /// [`MOUNT`].
    Append { parent: usize, child: usize },
    /// Rewrite text node `target` in place.
    SetText { target: usize, text: String },
    /// Attach a listener to element `target`.
    AddListener { target: usize },
}

/// The program a configuration expands to.
pub struct Program {
    pub steps: Vec<Step>,
    /// Every distinct class value, indexed as [`Step::SetClass`] indexes them.
    ///
    /// Built here so that executing the program is a table lookup and not a
    /// string format. See [`Step::SetClass`] for what happened when it was not.
    pub vocabulary: Vec<String>,
    /// Achieved tree statistics, measured while generating rather than assumed.
    pub max_depth: usize,
    pub leaves: usize,
    pub mean_fanout: f64,
}

/// The mount point, as a parent index.
pub const MOUNT: usize = usize::MAX;

impl Program {
    /// Expand a configuration into its program.
    ///
    /// Deterministic given [`crate::SEED`]: the same configuration produces the
    /// same program in the guest and in JavaScript, which is what lets the two
    /// arms be compared at all.
    pub fn build(config: Config) -> Program {
        let mut rng = Rng::new();
        let mut steps = Vec::new();

        let mut parents: Vec<usize> = Vec::with_capacity(config.elements);
        let mut children = vec![0usize; config.elements];
        let mut depth = vec![0usize; config.elements];
        let mut max_depth = 0;

        for element in 0..config.elements {
            steps.push(Step::Create);
            steps.push(Step::SetClass(config.vocabulary.index_for(element)));
            steps.push(Step::CreateText);
            steps.push(Step::AppendTextTo { index: element });

            let parent = match config.shape {
                // A single chain: the previous element, or the mount for the
                // first.
                TreeShape::Deep => {
                    if element == 0 {
                        MOUNT
                    } else {
                        element - 1
                    }
                }
                // One fan: everything on the mount.
                TreeShape::Wide => MOUNT,
                // Biased toward recent parents. Drawn from the generator so
                // that the shape is reproducible, and measured below so that
                // the report carries what was actually built.
                TreeShape::Realistic => {
                    if element == 0 {
                        MOUNT
                    } else if rng.index(2) == 0 {
                        element - 1
                    } else {
                        // A recent ancestor rather than a uniform one: real
                        // documents branch near the leaves far more than at
                        // the root.
                        let window = element.min(32);
                        element - 1 - rng.index(window)
                    }
                }
            };

            steps.push(Step::Append {
                parent,
                child: element,
            });

            if parent != MOUNT {
                children[parent] += 1;
                depth[element] = depth[parent] + 1;
                max_depth = max_depth.max(depth[element]);
            }
            parents.push(parent);
        }

        // Text, carrying the chattiness axis. Both ends write the same bytes.
        match config.chattiness {
            Chattiness::ManySmall => {
                for element in 0..config.elements {
                    steps.push(Step::SetText {
                        target: element,
                        text: text_chunk(element),
                    });
                }
            }
            Chattiness::FewLarge => {
                let mut element = 0;
                while element < config.elements {
                    let end = (element + Chattiness::BATCH).min(config.elements);
                    let mut text = String::new();
                    for chunk in element..end {
                        text.push_str(&text_chunk(chunk));
                    }
                    // Written into the first text node of the batch. The
                    // resulting document differs from the many-small case,
                    // which is the point: the same bytes arrive through a
                    // sixteenth as many calls.
                    steps.push(Step::SetText {
                        target: element,
                        text,
                    });
                    element = end;
                }
            }
        }

        // Listeners, spread evenly rather than clustered, so a dispatch storm
        // walks the whole tree instead of one hot subtree.
        if let Some(stride) = config
            .elements
            .checked_div(config.listeners)
            .map(|stride| stride.max(1))
        {
            let mut target = 0;
            let mut attached = 0;
            while target < config.elements && attached < config.listeners {
                steps.push(Step::AddListener { target });
                target += stride;
                attached += 1;
            }
        }

        let internal = children.iter().filter(|count| **count > 0).count();
        let leaves = config.elements - internal;
        let mean_fanout = if internal == 0 {
            0.0
        } else {
            children.iter().sum::<usize>() as f64 / internal as f64
        };

        Program {
            steps,
            vocabulary: (0..config.vocabulary.distinct_over(config.elements))
                .map(|index| class_value(index, config.names))
                .collect(),
            max_depth,
            leaves,
            mean_fanout,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_program_is_reproducible() {
        let config = Config::baseline();
        assert_eq!(Program::build(config).steps, Program::build(config).steps);
    }

    #[test]
    fn long_names_are_distinct_and_long() {
        let mut seen = std::collections::HashSet::new();
        for index in 0..800 {
            let value = class_value(index, NameLength::Long);
            assert!(value.len() > 45, "{value:?} is not long");
            assert!(seen.insert(value.clone()), "{value:?} collided");
        }
    }

    #[test]
    fn short_names_are_distinct_and_short() {
        let mut seen = std::collections::HashSet::new();
        for index in 0..800 {
            let value = class_value(index, NameLength::Short);
            assert!(value.len() <= 4, "{value:?} is not short");
            assert!(seen.insert(value.clone()));
        }
    }

    /// The adversarial case bottoms out at **3, not 1**, and that is a property
    /// of documents rather than a limitation here.
    ///
    /// Even with every class value distinct, the tag name and the attribute
    /// name are reused on every element: `n` elements make `3n` name uses of
    /// `n + 2` distinct names. Reaching 1 would need a distinct tag and a
    /// distinct attribute name per element, which is not a page anyone writes.
    /// So 3 is the floor a real document can reach, and the sweep reaches it.
    #[test]
    fn unique_names_bottom_out_at_three_not_one() {
        let config = Config {
            vocabulary: Vocabulary::UniquePerElement,
            ..Config::baseline()
        };
        let ratio = config.repetition_ratio();
        assert!((ratio - 3.0).abs() < 0.01, "ratio was {ratio}");
    }

    #[test]
    fn a_small_vocabulary_gives_a_high_ratio() {
        let config = Config {
            vocabulary: Vocabulary::Distinct(8),
            ..Config::baseline()
        };
        assert!(config.repetition_ratio() > 100.0);
    }

    /// Both ends of the chattiness axis must write the same bytes, or the axis
    /// measures size rather than call count.
    #[test]
    fn chattiness_holds_the_byte_total_constant() {
        let bytes = |chattiness| {
            Program::build(Config {
                chattiness,
                ..Config::baseline()
            })
            .steps
            .iter()
            .filter_map(|step| match step {
                Step::SetText { text, .. } => Some(text.len()),
                _ => None,
            })
            .sum::<usize>()
        };
        assert_eq!(bytes(Chattiness::ManySmall), bytes(Chattiness::FewLarge));
    }

    #[test]
    fn chattiness_changes_the_call_count() {
        let writes = |chattiness| {
            Program::build(Config {
                chattiness,
                ..Config::baseline()
            })
            .steps
            .iter()
            .filter(|step| matches!(step, Step::SetText { .. }))
            .count()
        };
        assert!(writes(Chattiness::ManySmall) > writes(Chattiness::FewLarge) * 8);
    }

    #[test]
    fn the_shapes_are_actually_different_shapes() {
        let deep = Program::build(Config {
            shape: TreeShape::Deep,
            ..Config::baseline()
        });
        let wide = Program::build(Config {
            shape: TreeShape::Wide,
            ..Config::baseline()
        });
        let realistic = Program::build(Config {
            shape: TreeShape::Realistic,
            ..Config::baseline()
        });

        assert_eq!(deep.max_depth, Config::ELEMENTS - 1, "a chain");
        assert_eq!(wide.max_depth, 0, "a fan");
        assert!(
            realistic.max_depth > 4 && realistic.max_depth < Config::ELEMENTS - 1,
            "realistic depth was {}",
            realistic.max_depth
        );
        assert!(
            realistic.leaves > Config::ELEMENTS / 5,
            "realistic leaves was {}",
            realistic.leaves
        );
    }

    #[test]
    fn listeners_are_spread_across_the_tree() {
        let program = Program::build(Config {
            listeners: 100,
            ..Config::baseline()
        });
        let targets: Vec<usize> = program
            .steps
            .iter()
            .filter_map(|step| match step {
                Step::AddListener { target } => Some(*target),
                _ => None,
            })
            .collect();
        assert_eq!(targets.len(), 100);
        assert!(targets.iter().max().unwrap() > &(Config::ELEMENTS / 2));
    }

    /// Six integers must round-trip, or the guest builds a different tree from
    /// the one the harness thinks it asked for and the identity assertion
    /// fails somewhere far from the cause.
    #[test]
    fn the_configuration_survives_the_boundary() {
        for vocabulary in [Vocabulary::Distinct(8), Vocabulary::UniquePerElement] {
            for names in [NameLength::Short, NameLength::Long] {
                for shape in [TreeShape::Deep, TreeShape::Wide, TreeShape::Realistic] {
                    let config = Config {
                        vocabulary,
                        names,
                        shape,
                        chattiness: Chattiness::FewLarge,
                        listeners: 7,
                        ..Config::baseline()
                    };
                    assert_eq!(Config::from_ints(config.as_ints()), config);
                }
            }
        }
    }
}
