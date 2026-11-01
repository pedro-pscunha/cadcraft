//! Groups of a newer release that a user DXF of an older one leaves out. The records read from a DWG carry them, and AutoCAD LT
//! 2025 discards a file that has them (Eztec Firenze as an R2000 DXF): an MTEXT background fill before R2004 ("Falta código de
//! grupo DXF: 90") and its text frame (bit 0x10 of group 90) before R2018, a solid's history subclass (AcDb3dSolid, group 350)
//! before R2007 ("Fim prematuro do objeto"), and a dimension's groups 73 to 75 before R2007 and 280 before R2010 ("Código de grupo
//! DXF inesperado: 280", DP Inc and Planeta-Toulouse). The DWG bridge keeps them: acadrust's DWG writer applies the same release
//! rules. The Raw save check compares a Raw record through the same rules ([`mtext_fill`], [`holds`]).

use cadcraft_dxf::{Tag, Version};

/// The background flags (MTEXT group 90) a DXF of `target` holds: none before R2004, without the text frame (0x10) before R2018.
/// 0 is no group 90.
pub(crate) fn mtext_fill(flags: i64, target: Version) -> i64 {
    if target < Version::R2004 {
        0
    } else if target < Version::R2018 {
        flags & !0x10
    } else {
        flags
    }
}

/// Whether a fill with these (kept) flags has its colour, scale and transparency (63, 421, 45, 441): a fill of its own colour (bit
/// 1), or from R2018 a text frame; a fill of the window colour has none.
pub(crate) fn mtext_fill_colour(flags: i64, target: Version) -> bool {
    flags & 1 != 0 || (target >= Version::R2018 && flags & 0x10 != 0)
}

/// Whether a DXF of `target` holds group `code` of subclass `class` in a record of `kind` (the MTEXT fill aside: [`mtext_fill`]).
pub(crate) fn holds(kind: &str, class: &str, code: i32, target: Version) -> bool {
    match kind {
        "3DSOLID" if class == "AcDb3dSolid" => target >= Version::R2007,
        "DIMENSION" | "ARC_DIMENSION" | "LARGE_RADIAL_DIMENSION" if class == "AcDbDimension" => match code {
            73..=75 => target >= Version::R2007,
            280 => target >= Version::R2010,
            _ => true,
        },
        _ => true,
    }
}

pub(super) fn strip(tags: &mut Vec<Tag>, target: Version) {
    if target >= Version::R2018 {
        return;
    }
    let mut out = Vec::with_capacity(tags.len());
    let mut kind = String::new();
    let mut subclass = String::new();
    // In the current MTEXT: whether its own groups end (an embedded object, 101, reuses 45), and whether the fill's colour, scale and
    // transparency stay.
    let (mut embedded, mut colour) = (false, true);
    for t in std::mem::take(tags) {
        match t.code {
            0 => {
                kind = t.str();
                subclass.clear();
                (embedded, colour) = (false, true);
            }
            100 => subclass = t.str().trim().to_string(),
            101 => embedded = true,
            _ => {}
        }
        let own = t.code < 1000;
        if kind == "MTEXT" && subclass == "AcDbMText" && own && !embedded {
            match t.code {
                90 => {
                    let flags = mtext_fill(t.i64(), target);
                    colour = mtext_fill_colour(flags, target);
                    if flags != 0 {
                        out.push(Tag::i(90, flags));
                    }
                    continue;
                }
                63 | 421 | 45 | 441 if !colour => continue,
                _ => {}
            }
        }
        if own && !holds(&kind, &subclass, t.code, target) {
            continue;
        }
        out.push(t);
    }
    *tags = out;
}
