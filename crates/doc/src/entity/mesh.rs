use super::{Deserialize, Face3d, Mat3, PolylineFitVertex, PolylineRecord, Serialize, Vec3};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MeshKind {
    Polyface,
    Polygon,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MeshVertex {
    pub vertex: PolylineFitVertex,
    pub indices: [i64; 4],
}

impl MeshVertex {
    pub fn is_face(&self) -> bool {
        self.vertex.flags & 128 != 0 && self.vertex.flags & 64 == 0
    }
}

/// A mesh stays one POLYLINE. Faces are a display projection, not separate database entities.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PolylineMesh {
    pub kind: MeshKind,
    /// Group 70 bits other than the topology bits (16 and 64).
    pub flags: i64,
    pub m_count: i64,
    pub n_count: i64,
    pub m_density: i64,
    pub n_density: i64,
    pub smooth_type: i64,
    pub vertices: Vec<MeshVertex>,
    pub seqend: Option<PolylineRecord>,
}

impl PolylineMesh {
    pub fn dxf_flags(&self) -> i64 {
        (self.flags & !(16 | 64))
            | match self.kind {
                MeshKind::Polyface => 64,
                MeshKind::Polygon => 16,
            }
    }

    pub fn transform(&mut self, m: &Mat3) {
        for record in self.vertices.iter_mut().filter(|v| !v.is_face()) {
            let p = m.apply(record.vertex.position.xy());
            record.vertex.position = p.to3(record.vertex.position.z * m.scale_factor());
        }
    }

    pub fn faces(&self) -> Vec<Face3d> {
        let vertices: Vec<Vec3> = self.vertices.iter().filter(|v| !v.is_face() && v.vertex.flags & 16 == 0).map(|v| v.vertex.position).collect();
        match self.kind {
            MeshKind::Polyface => self.polyface_faces(&vertices),
            MeshKind::Polygon => self.polygon_faces(&vertices),
        }
    }

    fn polyface_faces(&self, vertices: &[Vec3]) -> Vec<Face3d> {
        let mut out = Vec::new();
        for face in self.vertices.iter().filter(|v| v.is_face()) {
            let mut corners = Vec::with_capacity(4);
            let mut hidden = 0u8;
            let mut valid = true;
            for &index in face.indices.iter().filter(|&&i| i != 0) {
                let at = usize::try_from(index.unsigned_abs()).ok().and_then(|n| n.checked_sub(1));
                let Some(point) = at.and_then(|n| vertices.get(n)) else {
                    valid = false;
                    break;
                };
                if index < 0 {
                    hidden |= 1 << corners.len();
                }
                corners.push(*point);
            }
            match (valid, corners.as_slice()) {
                (true, [a, b, c]) => out.push(Face3d { corners: [*a, *b, *c, *c], hidden_edges: hidden | if hidden & 4 != 0 { 8 } else { 0 } }),
                (true, [a, b, c, d]) => out.push(Face3d { corners: [*a, *b, *c, *d], hidden_edges: hidden }),
                _ => {}
            }
        }
        out
    }

    fn polygon_faces(&self, vertices: &[Vec3]) -> Vec<Face3d> {
        let dims = |m, n| Some((usize::try_from(m).ok()?, usize::try_from(n).ok()?));
        let fits = |(m, n): (usize, usize)| m >= 2 && n >= 2 && m.checked_mul(n).is_some_and(|count| count <= vertices.len());
        let exact = |(m, n): (usize, usize)| fits((m, n)) && m * n == vertices.len();
        let Some((m, n)) = [dims(self.m_count, self.n_count), dims(self.m_density, self.n_density)]
            .into_iter()
            .flatten()
            .find(|d| exact(*d))
            .or_else(|| dims(self.m_count, self.n_count).filter(|d| fits(*d)))
        else {
            return Vec::new();
        };
        let at = |i: usize, j: usize| vertices.get((i % m) * n + j % n).copied();
        let rows = if self.flags & 1 != 0 { m } else { m - 1 };
        let cols = if self.flags & 32 != 0 { n } else { n - 1 };
        let mut out = Vec::new();
        for i in 0..rows {
            for j in 0..cols {
                if let (Some(a), Some(b), Some(c), Some(d)) = (at(i, j), at(i, j + 1), at(i + 1, j + 1), at(i + 1, j)) {
                    out.push(Face3d { corners: [a, b, c, d], hidden_edges: 0 });
                }
            }
        }
        out
    }
}
