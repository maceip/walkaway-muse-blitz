//! Native geometry for the browser's one tab strip; no JavaScript animation.
//!
//! Width tokens and the closed-form spring are ported from maceip/liquid-tabs
//! (`tab-strip-port`, f23ba1819f7b3a3c4587aebb75e7adde59dbad58), specifically
//! `swift/Sources/CompactTabStrip/TabStripGeometry.swift` and
//! `web/src/geometry.ts`. The pinching quadratic neck is ported from the local
//! MIT-licensed `liquid-effect-animation-main/src/components/ui/gooey-nav.tsx`
//! (SHA-256 e50081d89b7db841a1227aea06eb40ab3a5da224f8462ab86217721d5c737c18).
//! That snapshot names StarKnightt/liquid-effect-animation in its package
//! metadata, but the exact file's upstream provenance is not independently
//! verified. Both MIT notices are in the browser-local THIRD_PARTY_NOTICES.txt.
//!
//! Adaptations: classic curved tab shoulders instead of glass capsules, CSS
//! clip paths for Blitz, and a bounded, selected-centered overflow window.
//! The host owns selection and the frame clock. Passing no animation renders
//! the exact resting geometry (including for reduced motion).

pub const TAB_HEIGHT: f64 = 32.0;
pub const MINIMUM_WIDTH: f64 = 120.0;
pub const MAXIMUM_WIDTH: f64 = 240.0;
pub const SELECTED_MAXIMUM_WIDTH: f64 = 280.0;
pub const GAP: f64 = 4.0;
const COMPACT_MINIMUM_WIDTH: f64 = 92.0;
const OVERFLOW_WIDTH: f64 = 26.0;
const SEPARATION: f64 = 16.0;
const NECK_BREAK: f64 = 0.22;

/// Fractional selected-tab position, advanced by the host's monotonic clock.
/// Retargeting keeps position and velocity, including rapid repeated clicks.
#[derive(Clone, Debug)]
pub struct TabAnimation {
    position: f64,
    target: f64,
    velocity: f64,
}

impl Default for TabAnimation {
    fn default() -> Self {
        Self::new(0)
    }
}

impl TabAnimation {
    pub fn new(selected: usize) -> Self {
        Self {
            position: selected as f64,
            target: selected as f64,
            velocity: 0.0,
        }
    }

    pub fn select(&mut self, selected: usize) {
        self.target = selected as f64;
    }

    /// Reset after a structural change that renumbers tab indices, or when
    /// reduced motion is enabled. This does not change any browser tab state.
    pub fn snap(&mut self, selected: usize) {
        *self = Self::new(selected);
    }

    pub fn position(&self) -> f64 {
        self.position
    }

    /// Speed-dependent deformation, not a color-only transition.
    pub fn stretch(&self) -> f64 {
        (self.velocity.abs() / 18.0).min(1.0)
    }

    pub fn is_animating(&self) -> bool {
        self.position != self.target || self.velocity != 0.0
    }

    /// Advance in seconds. Returns whether another frame is needed. Hosts
    /// should paint this frame even when the return value becomes false.
    pub fn tick(&mut self, dt_seconds: f64) -> bool {
        if !dt_seconds.is_finite() || dt_seconds <= 0.0 || !self.is_animating() {
            return self.is_animating();
        }
        // Exact underdamped solution from advanceSpring; no unstable Euler
        // integration. Cap suspended-window time just as the upstream rAF does.
        let dt = dt_seconds.min(0.05);
        let omega = 2.0 * std::f64::consts::PI / 0.25;
        let damping = 0.85;
        let x = self.position - self.target;
        let v = self.velocity;
        let a = damping * omega;
        let b = omega * (1.0 - damping * damping).sqrt();
        let c = (v + a * x) / b;
        let (sin, cos) = (b * dt).sin_cos();
        let decay = (-a * dt).exp();
        self.position = self.target + decay * (x * cos + c * sin);
        self.velocity = decay * (v * cos - (a * c + b * x) * sin);
        // Upstream thresholds are pixel-space; these are tab-index units.
        if (self.position - self.target).abs() < 0.0001 && self.velocity.abs() < 0.0005 {
            self.position = self.target;
            self.velocity = 0.0;
        }
        self.is_animating()
    }
}

/// Native upstream resting-width policy, with non-finite input sanitized.
/// `available` includes the normal 4px gaps when there are three or more tabs.
pub fn resting_widths(available: f64, count: usize, selected: usize) -> Vec<f64> {
    if count == 0 {
        return Vec::new();
    }
    let available = finite_or(available, MINIMUM_WIDTH * count as f64).max(0.0);
    if count == 1 {
        return vec![available.clamp(MINIMUM_WIDTH, SELECTED_MAXIMUM_WIDTH)];
    }
    if count == 2 {
        let neighbor = (available / 2.0).clamp(MINIMUM_WIDTH, MAXIMUM_WIDTH);
        let active = (available - neighbor)
            .max(neighbor)
            .min(SELECTED_MAXIMUM_WIDTH);
        return (0..count)
            .map(|index| {
                if index == selected.min(count - 1) {
                    active
                } else {
                    neighbor
                }
            })
            .collect();
    }
    let width =
        ((available - (count - 1) as f64 * GAP) / count as f64).clamp(MINIMUM_WIDTH, MAXIMUM_WIDTH);
    vec![width; count]
}

#[derive(Clone, Debug)]
pub struct TabSlot {
    pub index: usize,
    pub x: f64,
    pub width: f64,
    pub left_open: f64,
    pub right_open: f64,
    pub stretch: f64,
}

#[derive(Clone, Debug)]
pub struct TabSeam {
    pub x: f64,
    pub gap: f64,
    pub span: f64,
    pub left_index: usize,
    pub right_index: usize,
}

#[derive(Clone, Debug, Default)]
pub struct TabLayout {
    pub slots: Vec<TabSlot>,
    pub seams: Vec<TabSeam>,
    /// A hidden adjacent tab's real index, usable with the existing tab ID
    /// action. Overflow controls do not create a second selection protocol.
    pub previous: Option<usize>,
    pub next: Option<usize>,
}

/// Layout inside the track only; the caller reserves a separate new-tab button.
/// Tabs first compress to 92px, then overflow into a selected-centered window.
/// Every hidden tab remains reachable through neighboring tab-index controls.
pub fn layout(
    available: f64,
    count: usize,
    selected: usize,
    animation: Option<&TabAnimation>,
) -> TabLayout {
    if count == 0 {
        return TabLayout::default();
    }
    let available = finite_or(available, 320.0).clamp(1.0, 100_000.0);
    let selected = selected.min(count - 1);
    let overflow = count as f64 * COMPACT_MINIMUM_WIDTH + 2.0 * SEPARATION > available;
    let edge = if overflow {
        OVERFLOW_WIDTH.min(available / 4.0)
    } else {
        0.0
    };
    let room = (available - 2.0 * edge).max(1.0);
    let capacity = (((room - 2.0 * SEPARATION).max(0.0) / COMPACT_MINIMUM_WIDTH) as usize)
        .max(1)
        .min(count);
    let visible = if overflow { capacity } else { count };
    let start = selected.saturating_sub(visible / 2).min(count - visible);
    let end = start + visible;
    let position = animation
        .map(TabAnimation::position)
        .unwrap_or(selected as f64)
        .clamp(start as f64, (end - 1) as f64);
    let stretch = animation.map(TabAnimation::stretch).unwrap_or(0.0);
    let openness = |seam: usize| {
        if seam == start || seam == end {
            1.0
        } else {
            // At rest only the two seams next to the active tab open. During
            // travel the opening, neck, and curved shoulders follow the spring.
            (1.5 - (position - (seam as f64 - 0.5)).abs()).clamp(0.0, 1.0)
        }
    };
    let gaps: Vec<f64> = (start + 1..end)
        .map(|seam| -1.0 + (SEPARATION + 1.0) * openness(seam))
        .collect();
    let width_budget = (room - gaps.iter().sum::<f64>()).max(1.0);
    let upstream_room = width_budget
        + if visible > 2 {
            (visible - 1) as f64 * GAP
        } else {
            0.0
        };
    let local = position - start as f64;
    let low = local.floor() as usize;
    let high = (low + 1).min(visible - 1);
    let fraction = local - low as f64;
    let low_widths = resting_widths(upstream_room, visible, low);
    let high_widths = resting_widths(upstream_room, visible, high);
    let mut widths: Vec<f64> = low_widths
        .iter()
        .zip(&high_widths)
        .map(|(a, b)| a + (b - a) * fraction)
        .collect();
    let compression = (width_budget / widths.iter().sum::<f64>()).min(1.0);
    for width in &mut widths {
        *width *= compression;
    }
    let mut result = TabLayout {
        previous: (start > 0).then(|| start - 1),
        next: (end < count).then_some(end),
        ..Default::default()
    };
    let mut x = edge;
    for (local_index, width) in widths.into_iter().enumerate() {
        let index = start + local_index;
        if local_index > 0 {
            let gap = gaps[local_index - 1];
            x += gap;
            result.seams.push(TabSeam {
                x: x - SEPARATION,
                gap,
                span: SEPARATION,
                left_index: index - 1,
                right_index: index,
            });
        }
        result.slots.push(TabSlot {
            index,
            x,
            width,
            left_open: openness(index),
            right_open: openness(index + 1),
            stretch: stretch * (1.0 - (position - index as f64).abs()).max(0.0),
        });
        x += width;
    }
    result
}

/// Port of GooeyNav's neckPath, with an explicit height instead of a stretched
/// 100-unit SVG viewBox. Two concave quadratic curves thin to a clean break.
/// The path is usable directly in CSS `clip-path: path('...')`.
pub fn neck_path(gap: f64, span: f64, height: f64) -> String {
    if !gap.is_finite()
        || !span.is_finite()
        || !height.is_finite()
        || gap <= 0.0
        || span <= 0.0
        || height <= 0.0
    {
        return String::new();
    }
    let waist = height * (1.0 - gap / (span * NECK_BREAK));
    if waist <= 0.0 {
        return String::new();
    }
    let start = span - gap;
    let mid = start + gap / 2.0;
    let top_control = height - waist;
    format!(
        "M{start:.3} 0 Q{mid:.3} {top_control:.3} {span:.3} 0 L{span:.3} {height:.3} Q{mid:.3} {waist:.3} {start:.3} {height:.3} Z"
    )
}

/// A real curved tab silhouette: convex top corners flow into concave bottom
/// shoulders. Seam openness and spring speed deform the path on each frame.
pub fn tab_path(width: f64, height: f64, left_open: f64, right_open: f64, stretch: f64) -> String {
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return String::new();
    }
    let l = finite_or(left_open, 1.0).clamp(0.0, 1.0);
    let r = finite_or(right_open, 1.0).clamp(0.0, 1.0);
    let stretch = finite_or(stretch, 0.0).clamp(0.0, 1.0);
    let shoulder = (8.0 + 5.0 * stretch).min(width / 8.0).min(height / 3.0);
    let radius = 7.0_f64.min(width / 10.0).min(height / 3.0);
    let lx = shoulder * l;
    let rx = width - shoulder * r;
    let top_l = lx + radius * l;
    let top_r = rx - radius * r;
    let knee = height - radius;
    let lc = lx * 0.2;
    let rc = width - (width - rx) * 0.2;
    let ly = radius * l;
    let ry = radius * r;
    format!(
        "M0 {height:.3} C{lc:.3} {height:.3} {lx:.3} {height:.3} {lx:.3} {knee:.3} L{lx:.3} {ly:.3} Q{lx:.3} 0 {top_l:.3} 0 L{top_r:.3} 0 Q{rx:.3} 0 {rx:.3} {ry:.3} L{rx:.3} {knee:.3} C{rx:.3} {height:.3} {rc:.3} {height:.3} {width:.3} {height:.3} Z"
    )
}

fn finite_or(value: f64, fallback: f64) -> f64 {
    if value.is_finite() { value } else { fallback }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_width_policy_is_preserved() {
        assert!(resting_widths(800.0, 0, 0).is_empty());
        assert_eq!(resting_widths(900.0, 1, 0), [280.0]);
        assert_eq!(resting_widths(520.0, 2, 0), [280.0, 240.0]);
        assert_eq!(resting_widths(520.0, 2, 1), [240.0, 280.0]);
        assert_eq!(resting_widths(608.0, 3, 1), [200.0; 3]);
        assert!(resting_widths(f64::NAN, 4, 0).iter().all(|w| w.is_finite()));
    }

    #[test]
    fn spring_changes_geometry_and_settles_exactly() {
        let mut animation = TabAnimation::new(0);
        let before = layout(800.0, 3, 0, Some(&animation));
        animation.select(2);
        assert!(animation.tick(1.0 / 60.0));
        assert!(animation.position() > 0.0 && animation.position() < 2.0);
        assert!(animation.stretch() > 0.0);
        let moving = layout(800.0, 3, 2, Some(&animation));
        assert_ne!(before.slots[1].right_open, moving.slots[1].right_open);
        assert_ne!(
            tab_path(200.0, TAB_HEIGHT, 1.0, 1.0, 0.0),
            tab_path(200.0, TAB_HEIGHT, 1.0, 1.0, animation.stretch())
        );
        for _ in 0..180 {
            animation.tick(1.0 / 60.0);
        }
        assert!(!animation.is_animating());
        assert_eq!(animation.position(), 2.0);
        assert_eq!(animation.stretch(), 0.0);
    }

    #[test]
    fn retargeting_preserves_motion_and_invalid_time_is_harmless() {
        let mut animation = TabAnimation::new(0);
        animation.select(4);
        animation.tick(0.03);
        let old_position = animation.position();
        let old_stretch = animation.stretch();
        animation.select(1);
        assert_eq!(animation.position(), old_position);
        assert_eq!(animation.stretch(), old_stretch);
        animation.tick(f64::NAN);
        animation.tick(f64::INFINITY);
        animation.tick(-1.0);
        assert_eq!(animation.position(), old_position);
        animation.snap(1);
        assert!(!animation.is_animating());
        assert!(!animation.tick(0.02));
    }

    #[test]
    fn spring_is_refresh_rate_independent() {
        let mut a = TabAnimation::new(0);
        let mut b = a.clone();
        a.select(3);
        b.select(3);
        a.tick(0.04);
        for _ in 0..4 {
            b.tick(0.01);
        }
        assert!((a.position() - b.position()).abs() < 1e-12);
    }

    #[test]
    fn quadratic_neck_pinches_and_breaks_at_upstream_threshold() {
        assert!(neck_path(0.0, 16.0, 32.0).is_empty());
        let full = neck_path(0.5, 16.0, 32.0);
        let thin = neck_path(3.0, 16.0, 32.0);
        assert_eq!(full.matches('Q').count(), 2);
        assert_ne!(full, thin);
        assert!(neck_path(16.0 * NECK_BREAK, 16.0, 32.0).is_empty());
        assert!(neck_path(16.0, 16.0, 32.0).is_empty());
        assert!(neck_path(f64::NAN, 16.0, 32.0).is_empty());
        assert!(neck_path(1.0, f64::INFINITY, 32.0).is_empty());
    }

    #[test]
    fn shoulders_are_curves_not_rounded_rectangle_colors() {
        let curved = tab_path(180.0, 32.0, 1.0, 1.0, 0.0);
        assert_eq!(curved.matches('C').count(), 2);
        assert_eq!(curved.matches('Q').count(), 2);
        assert!(curved.ends_with('Z'));
        assert_ne!(curved, tab_path(180.0, 32.0, 0.0, 0.0, 0.0));
        assert!(tab_path(f64::NAN, 32.0, 1.0, 1.0, 0.0).is_empty());
        assert!(!tab_path(1.0, 1.0, f64::NAN, 2.0, f64::NAN).contains("NaN"));
    }

    #[test]
    fn overflow_keeps_selected_visible_and_all_slots_inside_track() {
        for width in [80.0, 240.0, 320.0, 800.0, 1600.0] {
            for count in [1, 2, 3, 8, 40, 1000] {
                for selected in [0, count / 2, count - 1] {
                    let result = layout(width, count, selected, None);
                    assert!(result.slots.iter().any(|s| s.index == selected));
                    for slot in &result.slots {
                        assert!(slot.x >= 0.0 && slot.width > 0.0);
                        assert!(
                            slot.x + slot.width <= width + 0.001,
                            "{width} {count} {slot:?}"
                        );
                    }
                    if let Some(previous) = result.previous {
                        assert_eq!(previous + 1, result.slots[0].index);
                    }
                    if let Some(next) = result.next {
                        assert_eq!(next, result.slots.last().unwrap().index + 1);
                    }
                }
            }
        }
    }

    #[test]
    fn reduced_motion_matches_settled_animation() {
        let a = layout(700.0, 5, 3, None);
        let b = layout(700.0, 5, 3, Some(&TabAnimation::new(3)));
        for (left, right) in a.slots.iter().zip(&b.slots) {
            assert_eq!(left.x, right.x);
            assert_eq!(left.width, right.width);
            assert_eq!(left.left_open, right.left_open);
            assert_eq!(left.right_open, right.right_open);
        }
        assert!(layout(700.0, 0, 0, None).slots.is_empty());
        assert!(
            layout(f64::NAN, 5, usize::MAX, None)
                .slots
                .iter()
                .all(|s| s.x.is_finite())
        );
    }
}
