use super::repeat::Repeat;

pub fn next_index(
    current: usize,
    length: usize,
    repeat: Repeat,
    random: Option<u64>,
) -> Option<usize> {
    if current >= length {
        return None;
    }
    if repeat == Repeat::One {
        return Some(current);
    }
    if let Some(random) = random {
        return Some((random % length as u64) as usize);
    }
    if current + 1 < length {
        return Some(current + 1);
    }
    (repeat == Repeat::All).then_some(0)
}
