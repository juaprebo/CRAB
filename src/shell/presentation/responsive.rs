//! Space allocation for normal and compact terminal windows.
use ratatui::layout::{Constraint, Layout, Rect};

pub struct Screen {
    pub header: Rect,
    pub body: Rect,
    pub viz: Rect,
    pub now: Rect,
    pub footer: Rect,
    pub compact: bool,
}

impl Screen {
    pub fn new(area: Rect) -> Self {
        let compact = area.width < 80 || area.height < 24;
        let constraints = if compact {
            [
                Constraint::Length(u16::from(area.height >= 6)),
                Constraint::Min(0),
                Constraint::Length(0),
                Constraint::Length(if area.height >= 6 {
                    2
                } else {
                    area.height.min(1)
                }),
                Constraint::Length(u16::from(area.height >= 7)),
            ]
        } else {
            [
                Constraint::Length(1),
                Constraint::Min(4),
                Constraint::Length((area.height / 3).clamp(7, 18)),
                Constraint::Length(4),
                Constraint::Length(1),
            ]
        };
        let [header, body, viz, now, footer] = Layout::vertical(constraints).areas(area);
        Self {
            header,
            body,
            viz,
            now,
            footer,
            compact,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compact_mode_prioritizes_the_list_in_short_or_narrow_windows() {
        for (width, height) in [(60, 20), (100, 12), (79, 40)] {
            let screen = Screen::new(Rect::new(0, 0, width, height));
            assert!(screen.compact);
            assert_eq!(screen.viz.height, 0);
            assert_eq!(screen.now.height, 2);
            assert_eq!(screen.body.height, height - 4);
        }
        let screen = Screen::new(Rect::new(0, 0, 80, 24));
        assert!(!screen.compact);
        assert_eq!(screen.viz.height, 8);
        assert_eq!(screen.now.height, 4);
    }
    #[test]
    fn layouts_stay_within_bounds_and_never_overlap_even_when_tiny() {
        for width in [0, 1, 20, 29, 30, 39, 40, 60, 79, 80, 81, 140] {
            for height in [0, 1, 2, 3, 4, 5, 6, 7, 12, 23, 24, 25, 40] {
                let area = Rect::new(3, 5, width, height);
                let screen = Screen::new(area);
                let rects = [
                    screen.header,
                    screen.body,
                    screen.viz,
                    screen.now,
                    screen.footer,
                ];
                let mut bottom = area.y;
                for rect in rects {
                    assert!(rect.x >= area.x && rect.right() <= area.right());
                    assert!(rect.y >= bottom && rect.bottom() <= area.bottom());
                    bottom = rect.bottom();
                }
                assert_eq!(bottom, area.bottom());
                if height >= 7 {
                    assert!(screen.body.height >= 3);
                }
            }
        }
    }
}
