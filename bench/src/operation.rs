//! The two operations, and why only these two.
//!
//! Upstream's suite has ten. Nine of them measure a framework's reconciler,
//! which is not what this repo is about, and running them would produce a table
//! that invites exactly the framework comparison this harness is not making.
//!
//! The two kept were chosen because they are the best and worst case for an
//! interned-string ABI, so reporting both makes the interning claim falsifiable
//! rather than flattering:
//!
//! - `create-1000` is dominated by static tag and attribute names. Eleven
//!   distinct strings serve twenty-six thousand operations. This is where an
//!   interned tier should win by the largest margin it will ever win by.
//! - `partial-update` is dominated by dynamic text. Every one of the thousand
//!   writes carries content that has never been seen before and cannot be
//!   interned by any design. This is where an interned tier should show no
//!   advantage at all.
//!
//! A report showing only the first would be true and misleading. The pair is
//! the honest unit.

use serde::Serialize;
use workload::scenario::Config;

/// What an arm is being asked to build.
///
/// The upstream operations are kept alongside the sweep rather than replaced by
/// it. They are the licensed, cited baseline whose provenance the whole
/// repository rests on, and the paper's original table has to stay
/// reproducible. The sweep is the new primary result; these two are the fixed
/// point it is anchored to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Workload {
    Upstream(Operation),
    Sweep(Config),
}

impl Workload {
    /// Rows the arm must build before the measured window opens.
    pub fn setup_rows(self) -> usize {
        match self {
            Workload::Upstream(operation) => operation.table_rows_before(),
            // A sweep point's setup is expanding the program, which touches no
            // document at all.
            Workload::Sweep(_) => 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Operation {
    /// Build 1,000 rows into an empty table.
    Create1000,
    /// On a 10,000-row table, change the text of every tenth row.
    PartialUpdate,
}

impl Operation {
    pub const ALL: [Operation; 2] = [Operation::Create1000, Operation::PartialUpdate];

    pub fn name(self) -> &'static str {
        match self {
            Operation::Create1000 => "create-1000",
            Operation::PartialUpdate => "partial-update",
        }
    }

    /// What the operation does, for the report. A JSON file that has to be read
    /// beside a README is a JSON file that will be read without one.
    pub fn description(self) -> &'static str {
        match self {
            Operation::Create1000 => {
                "Build 1,000 rows into an empty table. Dominated by static tag \
                 and attribute names: 11 distinct strings across the whole \
                 operation."
            }
            Operation::PartialUpdate => {
                "On a 10,000-row table, change the text of every 10th row \
                 (1,000 text mutations). Dominated by dynamic text, which no \
                 design can intern."
            }
        }
    }

    /// The rows that exist before the operation starts.
    pub fn table_rows_before(self) -> usize {
        match self {
            Operation::Create1000 => 0,
            Operation::PartialUpdate => workload::op::UPDATE_TABLE_ROWS,
        }
    }

    /// The rows the operation itself touches.
    pub fn rows_touched(self) -> usize {
        match self {
            Operation::Create1000 => workload::op::CREATE_ROWS,
            Operation::PartialUpdate => workload::op::UPDATE_TOUCHED,
        }
    }
}
