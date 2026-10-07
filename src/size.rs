/// Count a megabyte as a million bytes, the smaller of the units upload limits use, so the size
/// warning comes early rather than late.
pub const BYTES_PER_MB: u64 = BYTES_PER_UNIT * BYTES_PER_UNIT;

const BYTES_PER_UNIT: u64 = 1000;
const SIZE_UNITS: [&str; 4] = ["kB", "MB", "GB", "TB"];
/// The smallest value that would round up to 1000.0 at one decimal place.
const ROUNDS_TO_NEXT_UNIT: f64 = 999.95;

/// Show a byte count in the largest decimal unit that keeps it under 1000.
pub fn format_size(bytes: u64) -> String {
    if bytes < BYTES_PER_UNIT {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64 / BYTES_PER_UNIT as f64;
    let mut unit_index = 0;
    while value >= ROUNDS_TO_NEXT_UNIT && unit_index + 1 < SIZE_UNITS.len() {
        value /= BYTES_PER_UNIT as f64;
        unit_index += 1;
    }
    format!("{value:.1} {}", SIZE_UNITS[unit_index])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_below_a_kilobyte_show_exact_bytes() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(999), "999 B");
    }

    #[test]
    fn larger_sizes_use_decimal_units() {
        assert_eq!(format_size(1_000), "1.0 kB");
        assert_eq!(format_size(1_500_000), "1.5 MB");
        assert_eq!(format_size(42_300_000), "42.3 MB");
        assert_eq!(format_size(2_000_000_000), "2.0 GB");
    }

    #[test]
    fn size_that_would_round_to_1000_moves_to_the_next_unit() {
        assert_eq!(format_size(999_949), "999.9 kB");
        assert_eq!(format_size(999_950), "1.0 MB");
    }

    #[test]
    fn largest_unit_keeps_growing_past_1000() {
        assert_eq!(format_size(2_000_000_000_000_000), "2000.0 TB");
    }

    #[test]
    fn a_megabyte_is_a_million_bytes() {
        assert_eq!(BYTES_PER_MB, 1_000_000);
        assert_eq!(format_size(BYTES_PER_MB), "1.0 MB");
    }
}
