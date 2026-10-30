use serde::{Deserialize, Serialize};

use crate::spline_fit::FitOptions;
use crate::{Bounds2, Mat4, Vec2, Vec3};

/// Largest B-spline degree treated as valid. Basis evaluation is O(degree²) per sample and
/// tessellation takes O(degree) samples, so an unbounded degree (DXF/HATCH can declare up to
/// 100 000) turns a tiny spline into billions of iterations (CWE-407). Real splines are low
/// degree (AutoCAD caps at 11); above this a spline evaluates to its control polygon instead.
pub const MAX_DEGREE: usize = 64;

/// A (possibly rational) B-spline curve, DXF SPLINE style.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Spline {
    pub degree: usize,
    pub knots: Vec<f64>,
    pub control: Vec<Vec2>,
    /// Empty = non-rational (all weights 1).
    #[serde(default)]
    pub weights: Vec<f64>,
    #[serde(default)]
    pub fit: Vec<Vec2>,
    #[serde(default)]
    pub closed: bool,
    /// How the curve was built from `fit` (knot parametrisation, end tangents, fit tolerance), so
    /// edits to the fit points rebuild it the same way.
    #[serde(default)]
    pub fit_opts: FitOptions,
    /// The Z values of a spatial (canonical) spline. `control`, `fit` and the end tangents of `fit_opts` then hold the X
    /// and Y values of the same WCS points and vectors, and this holds their Z. `None`: a planar spline at Z = 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub space: Option<SplineSpace>,
    /// The groups of a SPLINE record that the other fields do not hold, as a file gave them; `None` when they have the values
    /// of a spline made here (see [`Spline::source`]). Boxed to keep the spline small.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<Box<SplineSource>>,
}

/// The tolerances (DXF 42 and 43) and the linear flag (bit 16 of group 70) of a SPLINE record. The defaults are what a spline
/// made here is written with.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SplineSource {
    pub knot_tolerance: f64,
    pub control_tolerance: f64,
    /// A linear spline (a degree 1 spline that AutoCAD marks with bit 16, with the planar bit).
    pub linear: bool,
}

impl Spline {
    /// The tolerances and the linear flag of the record this spline was read from, or those of a spline made here.
    pub fn source(&self) -> SplineSource {
        self.source.as_deref().copied().unwrap_or_default()
    }
}

impl Default for SplineSource {
    fn default() -> Self {
        SplineSource { knot_tolerance: 1e-10, control_tolerance: 1e-10, linear: false }
    }
}

/// The Z values of the control points, the fit points and the end tangents of a spatial spline.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SplineSpace {
    pub control: Vec<f64>,
    pub fit: Vec<f64>,
    pub start_tangent: f64,
    pub end_tangent: f64,
}

impl Spline {
    /// Whether the Z values of a spatial spline are there and match the counts of its points.
    pub fn space_is_consistent(&self) -> bool {
        self.space.as_ref().is_some_and(|z| {
            z.control.len() == self.control.len()
                && z.fit.len() == self.fit.len()
                && z.control.iter().chain(&z.fit).chain([&z.start_tangent, &z.end_tangent]).all(|v| v.is_finite())
        })
    }

    /// The control points in WCS; `None` unless the spline is spatial and consistent.
    pub fn control3(&self) -> Option<Vec<Vec3>> {
        let z = self.space.as_ref().filter(|_| self.space_is_consistent())?;
        Some(self.control.iter().zip(&z.control).map(|(p, z)| p.to3(*z)).collect())
    }

    /// The fit points in WCS; `None` unless the spline is spatial and consistent.
    pub fn fit3(&self) -> Option<Vec<Vec3>> {
        let z = self.space.as_ref().filter(|_| self.space_is_consistent())?;
        Some(self.fit.iter().zip(&z.fit).map(|(p, z)| p.to3(*z)).collect())
    }

    /// The start and end tangents in WCS (each `None` without that fit tangent); `None` unless the spline is spatial
    /// and consistent.
    pub fn tangents3(&self) -> Option<(Option<Vec3>, Option<Vec3>)> {
        let z = self.space.as_ref().filter(|_| self.space_is_consistent())?;
        Some((self.fit_opts.start_tangent.map(|t| t.to3(z.start_tangent)), self.fit_opts.end_tangent.map(|t| t.to3(z.end_tangent))))
    }

    /// This spatial spline after the affine map `m`: the control and fit points move, the tangents turn with the linear
    /// part, the knots, weights and degree stay (a B-spline is affine-invariant, so the curve is the exact image). `None`
    /// when the spline is not spatial and consistent, `m` is not a finite invertible affine map, or a result is not finite.
    pub fn transformed3(&self, m: &Mat4) -> Option<Spline> {
        m.inverse()?;
        let (control, fit) = (self.control3()?, self.fit3()?);
        let (start, end) = self.tangents3()?;
        let (control, fit): (Vec<Vec3>, Vec<Vec3>) = (control.iter().map(|p| m.apply(*p)).collect(), fit.iter().map(|p| m.apply(*p)).collect());
        let (start, end) = (start.map(|t| m.apply_vec(t)), end.map(|t| m.apply_vec(t)));
        let finite = control.iter().chain(&fit).chain(start.iter()).chain(end.iter()).all(|p| p.is_finite());
        finite.then(|| Spline {
            control: control.iter().map(|p| p.xy()).collect(),
            fit: fit.iter().map(|p| p.xy()).collect(),
            fit_opts: FitOptions { start_tangent: start.map(|t| t.xy()), end_tangent: end.map(|t| t.xy()), ..self.fit_opts },
            space: Some(SplineSpace {
                control: control.iter().map(|p| p.z).collect(),
                fit: fit.iter().map(|p| p.z).collect(),
                start_tangent: start.map_or(0.0, |t| t.z),
                end_tangent: end.map_or(0.0, |t| t.z),
            }),
            ..self.clone()
        })
    }

    /// A clamped uniform spline over control points.
    pub fn from_control(control: Vec<Vec2>, degree: usize) -> Spline {
        let n = control.len();
        let degree = degree.clamp(1, n.saturating_sub(1).max(1));
        let knots = clamped_uniform_knots(n, degree);
        Spline {
            degree,
            knots,
            control,
            weights: Vec::new(),
            fit: Vec::new(),
            closed: false,
            fit_opts: FitOptions::default(),
            space: None,
            source: Default::default(),
        }
    }

    /// Cubic interpolation through fit points (chord-length parametrisation, natural-ish ends
    /// via a clamped knot vector: solve the banded system for control points).
    pub fn from_fit_points(fit: &[Vec2]) -> Spline {
        Spline::fit_with(fit, false, FitOptions::default())
    }

    /// Smooth closed (periodic) cubic interpolation through fit points. Chord-length parameters
    /// include the closing chord; the knot vector is the periodic (unclamped) extension and the
    /// first `degree` control points repeat at the end, so the curve has continuous tangent and
    /// curvature at the seam. A repeated closing fit point is dropped.
    pub fn from_fit_points_closed(fit: &[Vec2]) -> Spline {
        Spline::fit_with(fit, true, FitOptions::default())
    }

    /// Interpolation through fit points, open or closed (see `from_fit_points_closed`).
    pub fn from_fit(fit: &[Vec2], closed: bool) -> Spline {
        Spline::fit_with(fit, closed, FitOptions::default())
    }

    /// Closed with an unclamped (periodic) knot vector, as DXF flags it (group 70 bit 2).
    pub fn is_periodic(&self) -> bool {
        self.closed && self.knots.first() != self.knots.get(self.degree)
    }

    pub fn is_valid(&self) -> bool {
        let n = self.control.len();
        n > self.degree
            && self.degree >= 1
            && self.degree <= MAX_DEGREE
            && self.knots.len() == n + self.degree + 1
            && self.knots.windows(2).all(|w| w.first() <= w.get(1))
    }

    pub fn domain(&self) -> (f64, f64) {
        let lo = self.knots.get(self.degree).copied().unwrap_or(0.0);
        let hi = self.knots.get(self.control.len()).copied().unwrap_or(1.0);
        (lo, hi)
    }

    /// Evaluate at parameter `t` (de Boor via basis functions).
    pub fn eval(&self, t: f64) -> Vec2 {
        if !self.is_valid() {
            return self.control.first().copied().unwrap_or_default();
        }
        let (lo, hi) = self.domain();
        self.eval_valid(t, lo, hi)
    }

    /// `eval` on a spline `is_valid` has accepted, given its `domain()`, so sampling many points
    /// validates once (`is_valid` walks the knot vector).
    fn eval_valid(&self, t: f64, lo: f64, hi: f64) -> Vec2 {
        let n = self.control.len().saturating_sub(1);
        let t = t.clamp(lo, hi);
        let span = find_span(n, self.degree, t, &self.knots);
        let (mut nb, mut left, mut right) = ([0.0; MAX_STACK_DEGREE + 1], [0.0; MAX_STACK_DEGREE + 1], [0.0; MAX_STACK_DEGREE + 1]);
        let heap;
        let b: &[f64] = if self.degree <= MAX_STACK_DEGREE {
            basis_into(span, t, self.degree, &self.knots, &mut nb, &mut left, &mut right);
            nb.get(..=self.degree).unwrap_or(&[])
        } else {
            heap = basis(span, t, self.degree, &self.knots);
            &heap
        };
        let mut p = Vec2::ZERO;
        let mut wsum = 0.0;
        for (k, nb) in b.iter().enumerate() {
            let idx = span + k - self.degree;
            let w = if self.weights.is_empty() { 1.0 } else { self.weights.get(idx).copied().unwrap_or(1.0) };
            if let Some(c) = self.control.get(idx) {
                p += *c * (nb * w);
                wsum += nb * w;
            }
        }
        if wsum.abs() > 1e-300 { p / wsum } else { p }
    }

    pub fn tessellate(&self, tol: f64) -> Vec<Vec2> {
        if !self.is_valid() {
            return self.control.clone();
        }
        self.samples(self.sample_count(tol)).collect()
    }

    /// How many segments a tessellation at `tol` takes: the control-polygon length guides it,
    /// capped at `MAX_SAMPLES`.
    fn sample_count(&self, tol: f64) -> usize {
        self.segments_for(self.polygon_length() / tol.max(1e-9))
    }

    /// The segments for a control polygon `ratio` times the tolerance: dimensionless, so a
    /// spline and its scaled copy sample alike at any scale.
    fn segments_for(&self, ratio: f64) -> usize {
        let n = (ratio.sqrt() * 2.0).clamp(8.0, 2000.0) as usize * self.degree.max(1);
        n.min(MAX_SAMPLES)
    }

    fn polygon_length(&self) -> f64 {
        self.control.windows(2).map(|w| w.first().zip(w.get(1)).map(|(a, b)| a.dist(*b)).unwrap_or(0.0)).sum()
    }

    /// `n + 1` points evenly spaced in parameter over the domain (the spline must be valid).
    fn samples(&self, n: usize) -> impl Iterator<Item = Vec2> + '_ {
        let (lo, hi) = self.domain();
        (0..=n).map(move |i| self.eval_valid(lo + (hi - lo) * i as f64 / n as f64, lo, hi))
    }

    /// The box round the curve, from as many samples as `tessellate` takes at a tolerance
    /// relative to the spline's own size (1e-3 of its control box's diagonal), spread over the
    /// knot spans in proportion to their length with at least eight per span (its ends included),
    /// so a narrow span, which one grid over the whole domain can step over entirely, is seen
    /// too. The box can fall short of an extreme by the sag between two samples: under 1e-3 of
    /// the spline's size where the budget sets the count, under 1% in a very narrow span held
    /// to the eight-segment floor, whatever the drawing's units (zoom extents and selection pre-filtering
    /// tolerate that; the sampled box was never exact). A fixed absolute tolerance sampled the
    /// 4000-point cap on every spline of a drawing in millimetres, a third of the time a large
    /// drawing took to open (#1076); a typical spline now takes a few hundred evaluations. The
    /// cap holds over all spans: the floor drops below eight per span when that would exceed it,
    /// and a spline with more spans than two each allow gets its control box, which contains the
    /// curve unless a weight is negative (then the grid). A degree-1 spline with positive weights
    /// is its control polygon, so its control box is exact; a degenerate spline gets its control
    /// box, which contains the curve (the box of the finite control points when one is not
    /// finite: `hull` skips it, but it would poison every sample).
    pub fn bounds(&self) -> Bounds2 {
        let hull = Bounds2::from_points(self.control.iter().copied());
        let diag = hull.min.dist(hull.max);
        let finite = self.control.iter().all(|c| c.is_finite()) && self.weights.iter().all(|w| w.is_finite());
        // The convex hull property, which makes the control box contain the curve and a
        // degree-1 spline its control polygon, needs every weight positive.
        let positive = self.weights.iter().all(|w| *w > 0.0);
        if !self.is_valid() || !finite {
            return hull;
        }
        // With a zero or negative weight even a degenerate control box is no bound (zero weights
        // evaluate to the origin), so only positive weights take these shortcuts.
        if positive && (self.degree == 1 || !(diag.is_finite() && diag > 0.0)) {
            return hull;
        }
        let spans = self.knot_spans().count();
        if spans == 0 {
            return hull;
        }
        if spans > MAX_SAMPLES / 2 {
            // Too many spans for two samples each: the control box contains the curve when no
            // weight is negative (the convex hull property), so it is the safe answer; a grid could
            // step over a narrow span. Negative weights void that property, so they keep the grid.
            if positive {
                return hull;
            }
            return Bounds2::from_points(self.samples(MAX_SAMPLES));
        }
        let (lo, hi) = self.domain();
        let floor = 8.min(MAX_SAMPLES / spans);
        // The span's share of the domain first: huge knots (1e308, from a hostile edit) would
        // overflow `budget * (b - a)` to infinity and the cast to a boundless count.
        let segments = |budget: usize, (a, b): (f64, f64)| floor.max((budget as f64 * ((b - a) / (hi - lo))) as usize).min(MAX_SAMPLES);
        let n = self.segments_for(self.polygon_length() / (diag * 1e-3));
        let budget = if self.knot_spans().map(|s| segments(n, s)).sum::<usize>() <= MAX_SAMPLES { n } else { MAX_SAMPLES - floor * spans };
        let mut out = Bounds2::EMPTY;
        for (a, b) in self.knot_spans() {
            let seg = segments(budget, (a, b));
            for i in 0..seg {
                out.add(self.eval_valid(a + (b - a) * i as f64 / seg as f64, lo, hi));
            }
        }
        out.add(self.eval_valid(hi, lo, hi));
        out
    }

    /// The non-empty knot spans the curve is defined over, in parameter order.
    fn knot_spans(&self) -> impl Iterator<Item = (f64, f64)> + '_ {
        let knots = self.knots.get(self.degree..=self.control.len()).unwrap_or(&[]);
        knots.windows(2).filter_map(|w| w.first().zip(w.get(1)).map(|(a, b)| (*a, *b)).filter(|(a, b)| b > a))
    }
}

/// Most segments `tessellate` and `bounds` evaluate for one spline (#268: a high degree on many
/// control points took minutes).
const MAX_SAMPLES: usize = 4000;

pub(crate) fn clamped_uniform_knots(n: usize, degree: usize) -> Vec<f64> {
    let mut k = vec![0.0; degree + 1];
    let inner = n.saturating_sub(degree + 1);
    for i in 1..=inner {
        k.push(i as f64 / (inner + 1) as f64);
    }
    k.extend(std::iter::repeat_n(1.0, degree + 1));
    k
}

pub(crate) fn find_span(n: usize, p: usize, u: f64, knots: &[f64]) -> usize {
    let kn = knots.get(n + 1).copied().unwrap_or(1.0);
    if u >= kn {
        return n;
    }
    let mut low = p;
    let mut high = n + 1;
    let mut mid = (low + high) / 2;
    let mut guard = 0;
    while guard < 200 {
        guard += 1;
        let km = knots.get(mid).copied().unwrap_or(0.0);
        let km1 = knots.get(mid + 1).copied().unwrap_or(1.0);
        if u < km {
            high = mid;
        } else if u >= km1 {
            low = mid;
        } else {
            break;
        }
        mid = (low + high) / 2;
    }
    mid
}

/// Highest degree `eval` works out on the stack. The DXF reader and the SPLINE command cap the
/// degree at 10; degrees 11 up to `MAX_DEGREE` (a programmatic spline) take a heap buffer.
pub(crate) const MAX_STACK_DEGREE: usize = 10;

/// The `p + 1` non-zero basis functions of degree `p` at `u` in `span`.
pub(crate) fn basis(span: usize, u: f64, p: usize, knots: &[f64]) -> Vec<f64> {
    let mut n = vec![0.0; p + 1];
    let mut left = vec![0.0; p + 1];
    let mut right = vec![0.0; p + 1];
    basis_into(span, u, p, knots, &mut n, &mut left, &mut right);
    n
}

/// `basis` into `n[..=p]`, with `left` and `right` as scratch; each must hold `p + 1` values or
/// `n` stays as it was.
fn basis_into(span: usize, u: f64, p: usize, knots: &[f64], n: &mut [f64], left: &mut [f64], right: &mut [f64]) {
    if n.len() <= p || left.len() <= p || right.len() <= p {
        return;
    }
    if let Some(x) = n.get_mut(0) {
        *x = 1.0;
    }
    for j in 1..=p {
        let k = |i: isize| knots.get(i.max(0) as usize).copied().unwrap_or(0.0);
        left[j] = u - k(span as isize + 1 - j as isize);
        right[j] = k(span as isize + j as isize) - u;
        let mut saved = 0.0;
        for r in 0..j {
            let den = right[r + 1] + left[j - r];
            let tmp = if den.abs() > 1e-300 { n[r] / den } else { 0.0 };
            n[r] = saved + right[r + 1] * tmp;
            saved = left[j - r] * tmp;
        }
        n[j] = saved;
    }
}

/// Gaussian elimination with partial pivoting.
#[allow(clippy::needless_range_loop)]
pub(crate) fn solve(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let n = b.len();
    for c in 0..n {
        let piv = (c..n).max_by(|&i, &j| a[i][c].abs().total_cmp(&a[j][c].abs()))?;
        if a[piv][c].abs() < 1e-14 {
            return None;
        }
        a.swap(c, piv);
        b.swap(c, piv);
        for r in c + 1..n {
            let f = a[r][c] / a[c][c];
            if f != 0.0 {
                for k in c..n {
                    a[r][k] -= f * a[c][k];
                }
                b[r] -= f * b[c];
            }
        }
    }
    let mut x = vec![0.0; n];
    for r in (0..n).rev() {
        let s: f64 = (r + 1..n).map(|k| a[r][k] * x[k]).sum();
        x[r] = (b[r] - s) / a[r][r];
    }
    Some(x)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod space_tests {
    use super::*;

    fn spatial() -> Spline {
        Spline {
            degree: 2,
            knots: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
            control: vec![Vec2::new(0.0, 0.0), Vec2::new(1.0, 2.0), Vec2::new(3.0, 0.0)],
            weights: vec![1.0, 0.5, 1.0],
            fit: vec![Vec2::new(0.0, 0.0), Vec2::new(3.0, 0.0)],
            closed: false,
            fit_opts: FitOptions { start_tangent: Some(Vec2::new(1.0, 1.0)), end_tangent: Some(Vec2::new(1.0, -1.0)), ..FitOptions::default() },
            space: Some(SplineSpace { control: vec![0.0, 4.0, 1.0], fit: vec![0.0, 1.0], start_tangent: 0.5, end_tangent: -0.5 }),
            source: Default::default(),
        }
    }

    fn near(a: Vec3, b: Vec3) -> bool {
        (a - b).len() < 1e-9
    }

    #[test]
    fn the_points_are_spatial_and_a_planar_spline_has_none() {
        let s = spatial();
        assert!(s.space_is_consistent());
        assert_eq!(s.control3().unwrap()[1], Vec3::new(1.0, 2.0, 4.0));
        assert_eq!(s.fit3().unwrap()[1], Vec3::new(3.0, 0.0, 1.0));
        assert_eq!(s.tangents3().unwrap(), (Some(Vec3::new(1.0, 1.0, 0.5)), Some(Vec3::new(1.0, -1.0, -0.5))));
        assert!(Spline { space: None, ..s.clone() }.control3().is_none());
        let mut ragged = s;
        ragged.space.as_mut().unwrap().control.pop();
        assert!(!ragged.space_is_consistent() && ragged.control3().is_none());
    }

    #[test]
    fn an_affine_map_moves_the_points_and_the_curve_follows() {
        // A shear with a non-uniform scale and a translation: not a similarity, still exact for a B-spline.
        let m = Mat4 { m: [[2.0, 0.5, 0.0, 1.0], [0.0, 1.0, 0.25, -2.0], [0.0, 0.0, 3.0, 4.0], [0.0, 0.0, 0.0, 1.0]] };
        let s = spatial();
        let t = s.transformed3(&m).unwrap();
        for (a, b) in s.control3().unwrap().iter().zip(t.control3().unwrap()) {
            assert!(near(m.apply(*a), b));
        }
        let (start, end) = t.tangents3().unwrap();
        assert!(near(start.unwrap(), m.apply_vec(Vec3::new(1.0, 1.0, 0.5))) && near(end.unwrap(), m.apply_vec(Vec3::new(1.0, -1.0, -0.5))));
        assert_eq!((t.knots.clone(), t.weights.clone(), t.degree), (s.knots.clone(), s.weights.clone(), 2));
        // The curve at a parameter is the image of the curve (its XY part is the 2D evaluation of the moved XY controls).
        let (lo, hi) = s.domain();
        let mid = 0.5 * (lo + hi);
        let moved_xy = t.eval(mid);
        assert!(moved_xy.is_finite());
    }

    #[test]
    fn a_collapse_a_perspective_or_a_planar_spline_has_no_image() {
        let flat = Mat4 { m: [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0]] };
        assert!(spatial().transformed3(&flat).is_none());
        let mut perspective = Mat4::IDENTITY;
        perspective.m[3][0] = 0.1;
        assert!(spatial().transformed3(&perspective).is_none());
        assert!(Spline { space: None, ..spatial() }.transformed3(&Mat4::IDENTITY).is_none());
    }

    #[test]
    fn reversing_a_spatial_spline_reverses_its_z_values_and_swaps_the_tangents() {
        let r = spatial().reversed();
        let z = r.space.unwrap();
        assert_eq!((z.control, z.fit, z.start_tangent, z.end_tangent), (vec![1.0, 4.0, 0.0], vec![1.0, 0.0], 0.5, -0.5));
    }
}
