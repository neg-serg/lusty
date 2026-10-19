//! WCAG 2.1 contrast math and the floors the picker's colours must meet.
//!
//! The popup paints text on an opaque background and highlights cells with
//! backgrounds of their own, so every foreground/background pair is a
//! legibility claim. Rather than eyeballing it, the ratios are computed here
//! and asserted in tests: a colour change that drops below its floor fails the
//! build instead of shipping an unreadable frame.

/// WCAG 2.1 AA floor for normal-size text.
pub const MIN_TEXT: f64 = 4.5;
/// WCAG 2.1 AA floor for non-text UI components (and large text).
pub const MIN_UI: f64 = 3.0;
/// Floor for purely decorative-but-perceivable paint (frame lines, a tint that
/// is only a second cue). Below this a colour is effectively invisible. The
/// renderer never compares against it: it is the floor the invariant tests
/// hold such colours to.
#[cfg_attr(not(test), allow(dead_code))]
pub const MIN_DECOR: f64 = 1.5;

/// Parse an SGR parameter string such as `48;2;r;g;b` (or `38;2;r;g;b`) back
/// into RGB, so the tests read exactly the constants the renderer paints.
pub fn sgr_rgb(sgr: &str) -> Option<[u8; 3]> {
    let mut parts = sgr.split(';');
    match parts.next()? {
        "38" | "48" => {}
        _ => return None,
    }
    if parts.next()? != "2" {
        return None;
    }
    let mut channel = || parts.next()?.parse::<u8>().ok();
    let rgb = [channel()?, channel()?, channel()?];
    if parts.next().is_some() {
        return None;
    }
    Some(rgb)
}

/// WCAG 2.1 relative luminance of an sRGB colour, 0.0 ..= 1.0.
pub fn relative_luminance([r, g, b]: [u8; 3]) -> f64 {
    fn channel(value: u8) -> f64 {
        let c = value as f64 / 255.0;
        if c <= 0.039_28 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
}

/// WCAG 2.1 contrast ratio between two colours, 1.0 (identical) ..= 21.0
/// (black on white). Order does not matter.
pub fn contrast_ratio(a: [u8; 3], b: [u8; 3]) -> f64 {
    let (la, lb) = (relative_luminance(a), relative_luminance(b));
    let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 0.01
    }

    #[test]
    fn luminance_matches_the_wcag_reference_points() {
        assert!(close(relative_luminance([0, 0, 0]), 0.0));
        assert!(close(relative_luminance([255, 255, 255]), 1.0));
        // The linearisation knee: just below and just above 0.03928.
        assert!(relative_luminance([10, 10, 10]) < relative_luminance([11, 11, 11]));
    }

    #[test]
    fn contrast_ratio_spans_one_to_twenty_one() {
        assert!(close(contrast_ratio([0, 0, 0], [0, 0, 0]), 1.0));
        assert!(close(contrast_ratio([0, 0, 0], [255, 255, 255]), 21.0));
        // Symmetry: the ratio does not depend on the argument order.
        assert!(close(
            contrast_ratio([0, 95, 175], [0, 0, 0]),
            contrast_ratio([0, 0, 0], [0, 95, 175])
        ));
    }

    #[test]
    fn sgr_rgb_reads_back_the_renderer_spelling() {
        assert_eq!(sgr_rgb("48;2;0;95;175"), Some([0, 95, 175]));
        assert_eq!(sgr_rgb("38;2;255;255;255"), Some([255, 255, 255]));
        assert_eq!(sgr_rgb("48;2;1;2"), None, "short triple");
        assert_eq!(sgr_rgb("48;2;1;2;3;4"), None, "trailing junk");
        assert_eq!(sgr_rgb("48;5;17"), None, "indexed colour, not rgb");
        assert_eq!(sgr_rgb("1;48;2;1;2;3"), None, "attributes are not colours");
    }
}
