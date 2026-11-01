//! Pure Windows sensor policy, separate from Win32 calls for regression testing.
use super::sensors::Reading;

/// A full-screen window must not hide a failure to determine notification state.
/// The bool in `notification_busy` is the existing presentation/quiet-time policy;
/// D3D/App hints alone are deliberately not trusted as full-screen evidence.
pub fn combine(
    window_fullscreen: Reading<bool>,
    notification_busy: Reading<bool>,
) -> (Reading<bool>, Reading<bool>) {
    match notification_busy {
        Reading::Known(true) => (Reading::Known(true), Reading::Known(false)),
        Reading::Known(false) => {
            let interruptible = match window_fullscreen {
                Reading::Known(value) => Reading::Known(!value),
                other => other,
            };
            (window_fullscreen, interruptible)
        }
        unknown => (window_fullscreen, unknown),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_failure_cannot_be_hidden_by_fullscreen() {
        for fullscreen in [false, true] {
            let (_, interruptible) = combine(
                Reading::Known(fullscreen),
                Reading::Failed("notification query failed"),
            );
            assert_eq!(interruptible, Reading::Failed("notification query failed"));
        }
    }

    #[test]
    fn absent_session_stays_unknown_even_with_fullscreen() {
        assert_eq!(
            combine(Reading::Known(true), Reading::Unavailable("not present")).1,
            Reading::Unavailable("not present")
        );
    }

    #[test]
    fn valid_presentation_and_window_policy_is_preserved() {
        assert_eq!(
            combine(Reading::Known(false), Reading::Known(false)),
            (Reading::Known(false), Reading::Known(true))
        );
        assert_eq!(
            combine(Reading::Known(true), Reading::Known(false)),
            (Reading::Known(true), Reading::Known(false))
        );
        assert_eq!(
            combine(Reading::Known(false), Reading::Known(true)),
            (Reading::Known(true), Reading::Known(false))
        );
    }

    #[test]
    fn unavailable_window_is_not_a_normal_desktop() {
        assert_eq!(
            combine(Reading::Failed("window"), Reading::Known(false)).1,
            Reading::Failed("window")
        );
    }
}
