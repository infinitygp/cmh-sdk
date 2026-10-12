//! Where a module sits on the base, derived from its mount path.
//!
//! The base is a grid of 6 x 4 cells (seen with the USB cable at the
//! top-left; columns count from the left, rows from the top). It has 27
//! contact pads, each wired to one port of its hub tree and each sitting at
//! the bottom, left or right side of a cell. A module touches exactly one
//! pad with a contact whose place on the module is fixed, so the pad
//! identifies both the cells the module covers and how it is turned
//! (docs/experiments/0013 and 0014).

use crate::devices::ModuleKind;

/// Width of the base in cells.
pub const BASE_COLUMNS: u8 = 6;
/// Height of the base in cells.
pub const BASE_ROWS: u8 = 4;

/// Clockwise rotation of a module from its normal mounting.
///
/// Normal means: logo bar of the 15 LCD Keys at the bottom, single knob of
/// the 3 Knobs at the top, faders upright, rollers lying horizontally with
/// their contact end on the right, dial display upright.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Rotation {
    Deg0,
    Deg90,
    Deg180,
    Deg270,
}

impl Rotation {
    const ALL: [Rotation; 4] = [
        Rotation::Deg0,
        Rotation::Deg90,
        Rotation::Deg180,
        Rotation::Deg270,
    ];

    /// Clockwise angle in degrees.
    pub const fn degrees(self) -> u16 {
        match self {
            Rotation::Deg0 => 0,
            Rotation::Deg90 => 90,
            Rotation::Deg180 => 180,
            Rotation::Deg270 => 270,
        }
    }

    const fn quarter_turns(self) -> u8 {
        (self.degrees() / 90) as u8
    }
}

/// The cells a mounted module covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Placement {
    /// Leftmost covered column, `1..=BASE_COLUMNS`.
    pub column: u8,
    /// Topmost covered row, `1..=BASE_ROWS`.
    pub row: u8,
    /// Number of covered columns as mounted.
    pub columns: u8,
    /// Number of covered rows as mounted.
    pub rows: u8,
    pub rotation: Rotation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    Bottom,
    Left,
    Top,
    Right,
}

impl Side {
    /// The side this one faces after a quarter turn clockwise.
    const fn turned(self) -> Side {
        match self {
            Side::Bottom => Side::Left,
            Side::Left => Side::Top,
            Side::Top => Side::Right,
            Side::Right => Side::Bottom,
        }
    }
}

struct Pad {
    path: &'static [u8],
    column: u8,
    row: u8,
    side: Side,
}

const fn pad(path: &'static [u8], column: u8, row: u8, side: Side) -> Pad {
    Pad {
        path,
        column,
        row,
        side,
    }
}

/// Every pad of the base with the mount path it reports. All 27 were
/// measured by putting a module on them.
const PADS: [Pad; 27] = [
    pad(&[1, 2, 4], 2, 2, Side::Bottom),
    pad(&[1, 1, 1], 4, 2, Side::Bottom),
    pad(&[1, 4, 3], 6, 2, Side::Bottom),
    pad(&[2, 2], 4, 3, Side::Bottom),
    pad(&[1, 3, 2], 6, 3, Side::Bottom),
    pad(&[1, 2, 1], 1, 4, Side::Bottom),
    pad(&[4, 4], 2, 4, Side::Bottom),
    pad(&[4, 3], 3, 4, Side::Bottom),
    pad(&[2, 3], 4, 4, Side::Bottom),
    pad(&[1, 1, 2], 5, 4, Side::Bottom),
    pad(&[1, 3, 4], 6, 4, Side::Bottom),
    pad(&[1, 2, 3], 1, 2, Side::Left),
    pad(&[3, 1], 3, 2, Side::Left),
    pad(&[3, 4], 5, 2, Side::Left),
    pad(&[1, 2, 2], 1, 4, Side::Left),
    pad(&[4, 1], 2, 4, Side::Left),
    pad(&[4, 2], 3, 4, Side::Left),
    pad(&[2, 4], 4, 4, Side::Left),
    pad(&[1, 1, 3], 5, 4, Side::Left),
    pad(&[3, 3], 4, 1, Side::Right),
    pad(&[1, 4, 1], 6, 1, Side::Right),
    pad(&[3, 2], 4, 2, Side::Right),
    pad(&[1, 4, 2], 6, 2, Side::Right),
    pad(&[2, 1], 4, 3, Side::Right),
    pad(&[1, 3, 1], 6, 3, Side::Right),
    pad(&[1, 1, 4], 4, 4, Side::Right),
    pad(&[1, 3, 3], 6, 4, Side::Right),
];

/// Footprint of a module in its normal mounting and the spot of its contact:
/// the cell (counted from the module's top-left) and the side of that cell.
struct Shape {
    columns: u8,
    rows: u8,
    contact: (u8, u8, Side),
}

const fn shape(kind: ModuleKind) -> Option<Shape> {
    // Most modules carry the contact at the bottom of their bottom-right
    // cell; the rollers carry it at the right end.
    let (columns, rows, side) = match kind {
        ModuleKind::Keys15 => (4, 3, Side::Bottom),
        ModuleKind::Faders5 => (4, 2, Side::Bottom),
        ModuleKind::Knobs3 | ModuleKind::DialDisplay => (2, 2, Side::Bottom),
        ModuleKind::Rollers2 => (4, 1, Side::Right),
        ModuleKind::Base => return None,
    };
    Some(Shape {
        columns,
        rows,
        contact: (columns - 1, rows - 1, side),
    })
}

/// Works out which cells `kind` covers when it reports `mount_path`
/// ([`Manager::mount_path`](crate::Manager::mount_path)).
///
/// Returns `None` for the base itself, for an unknown path, and for a pad
/// that this module cannot be seated on.
pub fn placement(kind: ModuleKind, mount_path: &[u8]) -> Option<Placement> {
    let shape = shape(kind)?;
    let pad = PADS.iter().find(|p| p.path == mount_path)?;
    Rotation::ALL.into_iter().find_map(|rotation| {
        let (mut columns, mut rows) = (shape.columns, shape.rows);
        let (mut cx, mut cy, mut side) = shape.contact;
        for _ in 0..rotation.quarter_turns() {
            // A quarter turn clockwise swaps the footprint and carries the
            // cell (x, y) to (rows - 1 - y, x).
            (cx, cy) = (rows - 1 - cy, cx);
            (columns, rows) = (rows, columns);
            side = side.turned();
        }
        if side != pad.side {
            return None;
        }
        let column = pad.column.checked_sub(cx).filter(|c| *c >= 1)?;
        let row = pad.row.checked_sub(cy).filter(|r| *r >= 1)?;
        (column + columns - 1 <= BASE_COLUMNS && row + rows - 1 <= BASE_ROWS).then_some(Placement {
            column,
            row,
            columns,
            rows,
            rotation,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ModuleKind::{DialDisplay, Faders5, Keys15, Knobs3, Rollers2};
    use Rotation::{Deg0, Deg90, Deg270};

    fn at(column: u8, row: u8, columns: u8, rows: u8, rotation: Rotation) -> Option<Placement> {
        Some(Placement {
            column,
            row,
            columns,
            rows,
            rotation,
        })
    }

    /// Every placement that was physically made and logged
    /// (docs/experiments/0005, 0011, 0013, 0014, 0015).
    #[test]
    fn reproduces_every_measured_placement() {
        let measured: &[(ModuleKind, &[u8], Option<Placement>)] = &[
            // Normal layout.
            (Keys15, &[2, 2], at(1, 1, 4, 3, Deg0)),
            (DialDisplay, &[1, 4, 3], at(5, 1, 2, 2, Deg0)),
            (Knobs3, &[1, 3, 4], at(5, 3, 2, 2, Deg0)),
            (Rollers2, &[1, 1, 4], at(1, 4, 4, 1, Deg0)),
            (Faders5, &[1, 1, 3], at(5, 1, 2, 4, Deg90)),
            // Knobs, top row.
            (Knobs3, &[1, 2, 4], at(1, 1, 2, 2, Deg0)),
            (Knobs3, &[1, 2, 3], at(1, 1, 2, 2, Deg90)),
            (Knobs3, &[1, 1, 1], at(3, 1, 2, 2, Deg0)),
            (Knobs3, &[3, 1], at(3, 1, 2, 2, Deg90)),
            (Knobs3, &[3, 3], at(3, 1, 2, 2, Deg270)),
            (Knobs3, &[1, 4, 3], at(5, 1, 2, 2, Deg0)),
            (Knobs3, &[3, 4], at(5, 1, 2, 2, Deg90)),
            (Knobs3, &[1, 4, 1], at(5, 1, 2, 2, Deg270)),
            // Knobs, bottom row.
            (Knobs3, &[4, 4], at(1, 3, 2, 2, Deg0)),
            (Knobs3, &[1, 2, 2], at(1, 3, 2, 2, Deg90)),
            (Knobs3, &[2, 3], at(3, 3, 2, 2, Deg0)),
            (Knobs3, &[4, 2], at(3, 3, 2, 2, Deg90)),
            (Knobs3, &[2, 1], at(3, 3, 2, 2, Deg270)),
            (Knobs3, &[1, 1, 3], at(5, 3, 2, 2, Deg90)),
            (Knobs3, &[1, 3, 1], at(5, 3, 2, 2, Deg270)),
            // Knobs, half a place down.
            (Knobs3, &[2, 2], at(3, 2, 2, 2, Deg0)),
            (Knobs3, &[3, 2], at(3, 2, 2, 2, Deg270)),
            (Knobs3, &[1, 3, 2], at(5, 2, 2, 2, Deg0)),
            (Knobs3, &[1, 4, 2], at(5, 2, 2, 2, Deg270)),
            // Knobs, half a place sideways in the bottom row.
            (Knobs3, &[4, 3], at(2, 3, 2, 2, Deg0)),
            (Knobs3, &[4, 1], at(2, 3, 2, 2, Deg90)),
            (Knobs3, &[1, 1, 2], at(4, 3, 2, 2, Deg0)),
            (Knobs3, &[2, 4], at(4, 3, 2, 2, Deg90)),
            // The other modules away from their usual place.
            (DialDisplay, &[1, 2, 3], at(1, 1, 2, 2, Deg90)),
            (Faders5, &[1, 1, 1], at(1, 1, 4, 2, Deg0)),
            (Keys15, &[1, 3, 2], at(3, 1, 4, 3, Deg0)),
            // Rollers.
            (Rollers2, &[1, 3, 3], at(3, 4, 4, 1, Deg0)),
            (Rollers2, &[3, 3], at(1, 1, 4, 1, Deg0)),
            (Rollers2, &[1, 2, 1], at(1, 1, 1, 4, Deg90)),
        ];
        for (kind, path, expected) in measured {
            assert_eq!(placement(*kind, path), *expected, "{kind} at {path:?}");
        }
    }

    #[test]
    fn pad_paths_are_the_27_distinct_ports() {
        let mut paths: Vec<&[u8]> = PADS.iter().map(|p| p.path).collect();
        paths.sort();
        paths.dedup();
        assert_eq!(paths.len(), 27);
        // The base controller occupies the 28th port of the hub tree.
        assert!(!paths.contains(&&[1u8, 4, 4][..]));
        assert!(paths.iter().all(|p| p[0] >= 1 && p[0] <= 4));
    }

    #[test]
    fn placements_stay_on_the_base() {
        for kind in ModuleKind::ALL {
            for pad in &PADS {
                if let Some(p) = placement(kind, pad.path) {
                    assert!(p.column >= 1 && p.column + p.columns - 1 <= BASE_COLUMNS);
                    assert!(p.row >= 1 && p.row + p.rows - 1 <= BASE_ROWS);
                }
            }
        }
    }

    #[test]
    fn rejects_what_cannot_be() {
        // The base is not mounted anywhere.
        assert_eq!(placement(ModuleKind::Base, &[1, 4, 4]), None);
        assert_eq!(placement(Knobs3, &[1, 4, 4]), None);
        // Unknown path.
        assert_eq!(placement(Knobs3, &[9, 9]), None);
        assert_eq!(placement(Knobs3, &[]), None);
        // A 2x2 module would hang over the left edge on the bottom pad of
        // column 1; only the upright rollers use that pad.
        assert_eq!(placement(Knobs3, &[1, 2, 1]), None);
        // The keys do not fit above row 3 on a bottom pad of row 2.
        assert_eq!(placement(Keys15, &[1, 2, 4]), None);
    }

    #[test]
    fn nothing_but_the_rollers_can_be_upside_down() {
        // A half turn needs a pad at the top of a cell for the usual
        // contact; the base has none.
        for kind in [Keys15, Faders5, Knobs3, DialDisplay] {
            for pad in &PADS {
                let rotation = placement(kind, pad.path).map(|p| p.rotation);
                assert_ne!(rotation, Some(Rotation::Deg180));
            }
        }
    }
}
