//! Groups of a newer release that a user DXF of an older one leaves out. The records read from a DWG carry them, and AutoCAD LT
//! 2025 discards a file that has them (Eztec Firenze as an R2000 DXF): an MTEXT background fill before R2004 ("Falta código de
//! grupo DXF: 90") and its text frame (bit 0x10 of group 90) before R2018, and a solid's history subclass (AcDb3dSolid, group 350)
//! before R2007 ("Fim prematuro do objeto"), and an arc dimension's groups 73 to 75 before R2007 and 280 before R2010 ("Código de grupo
//! DXF inesperado: 280", DP Inc and Planeta-Toulouse). The DWG bridge keeps them: acadrust's DWG writer applies the same release rules.

use cadcraft_dxf::{Tag, Version};

pub(super) fn strip(tags: &mut Vec<Tag>, target: Version) {
    if target >= Version::R2018 {
        return;
    }
    let mut out = Vec::with_capacity(tags.len());
    let mut kind = String::new();
    let mut subclass = String::new();
    // In the current MTEXT: whether its own groups end (an embedded object, 101, reuses 45), and whether the fill's colour, scale and
    // transparency stay (a fill of the window colour has none).
    let (mut embedded, mut fill) = (false, true);
    for t in std::mem::take(tags) {
        match t.code {
            0 => {
                kind = t.str();
                subclass.clear();
                (embedded, fill) = (false, true);
            }
            100 => subclass = t.str().trim().to_string(),
            101 => embedded = true,
            _ => {}
        }
        let own = t.code < 1000;
        if kind == "MTEXT" && subclass == "AcDbMText" && own && !embedded {
            match t.code {
                90 => {
                    let flags = if target < Version::R2004 { 0 } else { t.i64() & !0x10 };
                    fill = flags & 1 != 0;
                    if flags != 0 {
                        out.push(Tag::i(90, flags));
                    }
                    continue;
                }
                63 | 421 | 45 | 441 if !fill => continue,
                _ => {}
            }
        }
        if kind == "3DSOLID" && target < Version::R2007 && (subclass == "AcDb3dSolid" && own) {
            continue;
        }
        // A dimension's text line spacing (73 to 75, R2007) and version (280, R2010); the written DIMENSION already leaves them out,
        // an arc dimension kept as read does not.
        let dimension = matches!(kind.as_str(), "DIMENSION" | "ARC_DIMENSION" | "LARGE_RADIAL_DIMENSION");
        if dimension
            && subclass == "AcDbDimension"
            && (matches!(t.code, 73..=75) && target < Version::R2007 || t.code == 280 && target < Version::R2010)
        {
            continue;
        }
        out.push(t);
    }
    *tags = out;
}
