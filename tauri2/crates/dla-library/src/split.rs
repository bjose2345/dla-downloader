//! The parts of a split archive set: `<code>__<id>.part2.rar` or
//! `<code>__<id>.zip.002`. A set's parts are only usable together, so the
//! library lists, downloads and names them as one.

/// The set a split archive's part belongs to, and the part's number:
/// `RJ1__a.part2.rar` is part 2 of `RJ1__a`, `RJ1__a.zip.003` part 3 of
/// `RJ1__a`. `None` for a file that is not a part.
pub fn split_part(name: &str) -> Option<(&str, u32)> {
    let lower = name.to_ascii_lowercase();
    if let Some(stem) = lower
        .strip_suffix(".exe")
        .or_else(|| lower.strip_suffix(".rar"))
    {
        let (base, number) = stem.rsplit_once(".part")?;
        return part_number(number, 1).map(|number| (&name[..base.len()], number));
    }
    let (rest, number) = lower.rsplit_once('.')?;
    let base = rest
        .strip_suffix(".zip")
        .or_else(|| rest.strip_suffix(".7z"))?;
    if number.len() != 3 {
        return None;
    }
    part_number(number, 3).map(|number| (&name[..base.len()], number))
}

fn part_number(digits: &str, min_len: usize) -> Option<u32> {
    if digits.len() < min_len || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::split_part;

    #[test]
    fn reads_the_set_and_number_of_each_part() {
        assert_eq!(split_part("RJ1__a.part1.exe"), Some(("RJ1__a", 1)));
        assert_eq!(split_part("RJ1__a.part02.rar"), Some(("RJ1__a", 2)));
        assert_eq!(split_part("RJ1__a.PART10.RAR"), Some(("RJ1__a", 10)));
        assert_eq!(split_part("RJ1__a.zip.003"), Some(("RJ1__a", 3)));
        assert_eq!(split_part("RJ1__a.7z.001"), Some(("RJ1__a", 1)));
    }

    #[test]
    fn leaves_whole_archives_alone() {
        assert_eq!(split_part("RJ1__a.zip"), None);
        assert_eq!(split_part("RJ1__a.exe"), None);
        assert_eq!(split_part("RJ1__a.rar"), None);
        assert_eq!(split_part("RJ1__a.part.rar"), None);
        assert_eq!(split_part("RJ1__a.zip.1"), None);
        assert_eq!(split_part("notes.txt.001"), None);
    }
}
