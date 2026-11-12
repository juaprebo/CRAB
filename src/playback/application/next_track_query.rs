use crate::library::domain::library::LocalTrack;
use crate::playback::domain::{playback_order::next_index, repeat::Repeat};

pub struct NextTrackQuery;
impl NextTrackQuery {
    pub fn execute<'a>(
        &self,
        tracks: &'a [LocalTrack],
        current: usize,
        repeat: Repeat,
        random: Option<u64>,
    ) -> Option<(usize, &'a LocalTrack)> {
        let index = next_index(current, tracks.len(), repeat, random)?;
        Some((index, &tracks[index]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn respects_end_repeat_and_shuffle_without_a_player() {
        let tracks = vec![
            crate::testutil::track("/a.mp3"),
            crate::testutil::track("/b.mp3"),
        ];
        assert_eq!(
            NextTrackQuery
                .execute(&tracks, 0, Repeat::Off, None)
                .unwrap()
                .0,
            1
        );
        assert!(
            NextTrackQuery
                .execute(&tracks, 1, Repeat::Off, None)
                .is_none()
        );
        assert_eq!(
            NextTrackQuery
                .execute(&tracks, 1, Repeat::All, None)
                .unwrap()
                .0,
            0
        );
        assert_eq!(
            NextTrackQuery
                .execute(&tracks, 0, Repeat::One, Some(1))
                .unwrap()
                .0,
            0
        );
        assert_eq!(
            NextTrackQuery
                .execute(&tracks, 0, Repeat::Off, Some(3))
                .unwrap()
                .0,
            1
        );
        assert!(
            NextTrackQuery
                .execute(&[], 0, Repeat::All, Some(1))
                .is_none()
        );
        assert!(
            NextTrackQuery
                .execute(&tracks, usize::MAX, Repeat::All, None)
                .is_none()
        );
    }
}
