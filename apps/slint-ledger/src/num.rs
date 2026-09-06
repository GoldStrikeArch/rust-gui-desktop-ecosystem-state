// Locale-aware number parsing/formatting. Slint has no number type at the
// widget boundary and no locale API in its public Rust surface
// (`SlintContext::set_locale` exists in i-slint-core but isn't re-exported),
// so this is all hand-rolled.

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Loc {
    EnUs,
    FrFr,
}

impl Loc {
    pub fn name(self) -> &'static str {
        match self {
            Loc::EnUs => "en-US",
            Loc::FrFr => "fr-FR",
        }
    }
    pub fn dec(self) -> char {
        match self {
            Loc::EnUs => '.',
            Loc::FrFr => ',',
        }
    }
    pub fn grp(self) -> char {
        match self {
            Loc::EnUs => ',',
            Loc::FrFr => ' ',
        }
    }
}

/// `1234.5, 2, en-US` -> `1,234.50`; `fr-FR` -> `1 234,50`.
pub fn fmt(v: f64, decimals: usize, loc: Loc) -> String {
    let neg = v < 0.0;
    let scaled = (v.abs() * 10f64.powi(decimals as i32)).round() as i128;
    let pow = 10i128.pow(decimals as u32);
    let int_part = (scaled / pow).to_string();
    let mut grouped = String::new();
    for (i, c) in int_part.chars().enumerate() {
        if i > 0 && (int_part.len() - i) % 3 == 0 {
            grouped.push(loc.grp());
        }
        grouped.push(c);
    }
    let mut out = String::new();
    if neg {
        out.push('-');
    }
    out.push_str(&grouped);
    if decimals > 0 {
        out.push(loc.dec());
        out.push_str(&format!("{:0width$}", scaled % pow, width = decimals));
    }
    out
}

/// Accepts `1,234.56`, `1 234,56`, `$1,234.56`, `1.234,56 €`, `(12.50)`,
/// `-12.5`. Returns None when the string cannot be a number.
pub fn parse(s: &str, loc: Loc) -> Option<f64> {
    let mut t: String = s.trim().to_string();
    let mut neg = false;
    // accounting negative
    if t.starts_with('(') && t.ends_with(')') && t.len() >= 3 {
        neg = true;
        t = t[1..t.len() - 1].to_string();
    }
    // currency symbols, spaces (incl. NBSP/narrow NBSP used as group sep)
    t.retain(|c| !matches!(c, '$' | '€' | '£' | '¥' | ' ' | '\u{a0}' | '\u{202f}'));
    if let Some(rest) = t.strip_prefix('-') {
        neg = !neg;
        t = rest.to_string();
    }
    if t.is_empty() {
        return None;
    }
    let last_dot = t.rfind('.');
    let last_com = t.rfind(',');
    let dec_sep = match (last_dot, last_com) {
        (Some(d), Some(c)) => Some(if d > c { '.' } else { ',' }),
        (Some(_), None) => {
            // one kind only: 3 trailing digits is ambiguous -> ask the locale
            let tail = t.len() - last_dot.unwrap() - 1;
            if tail == 3 && loc.grp() == '.' { None } else { Some('.') }
        }
        (None, Some(_)) => {
            let tail = t.len() - last_com.unwrap() - 1;
            if tail == 3 && loc.grp() == ',' { None } else { Some(',') }
        }
        (None, None) => None,
    };
    let mut cleaned = String::new();
    for (i, c) in t.char_indices() {
        match c {
            '0'..='9' => cleaned.push(c),
            '.' | ',' => {
                if Some(c) == dec_sep && Some(i) == t.rfind(c) {
                    cleaned.push('.');
                }
                // otherwise it is a grouping separator: drop it
            }
            _ => return None,
        }
    }
    let v: f64 = cleaned.parse().ok()?;
    Some(if neg { -v } else { v })
}

/// Typing filter. `kind`: 1 integer, 2 decimal, 3 ISO date mask.
/// Returns true when `current + ch` can still grow into a valid value.
pub fn allow(kind: i32, current: &str, ch: &str, loc: Loc) -> bool {
    let mut it = ch.chars();
    let (Some(c), None) = (it.next(), it.next()) else {
        return true; // multi-char / no char: not a literal insertion
    };
    // control characters and Slint's private-use key codes (arrows, Tab,
    // Backspace, …) are never literal insertions
    if (c as u32) < 0x20 || (0xE000..=0xF8FF).contains(&(c as u32)) {
        return true;
    }
    match kind {
        1 => c.is_ascii_digit() && current.chars().filter(|c| c.is_ascii_digit()).count() < 3,
        2 => {
            if c == '-' {
                return current.is_empty();
            }
            if c == loc.dec() {
                return !current.contains(loc.dec());
            }
            if c == loc.grp() {
                return !current.is_empty();
            }
            c.is_ascii_digit()
        }
        3 => c.is_ascii_digit() || c == '-',
        _ => true,
    }
}

/// `YYYY-MM-DD` with a real calendar check.
pub fn valid_date(s: &str) -> bool {
    let p: Vec<&str> = s.split('-').collect();
    if p.len() != 3 || p[0].len() != 4 || p[1].len() != 2 || p[2].len() != 2 {
        return false;
    }
    let (Ok(y), Ok(m), Ok(d)) = (
        p[0].parse::<u32>(),
        p[1].parse::<u32>(),
        p[2].parse::<u32>(),
    ) else {
        return false;
    };
    if !(1..=12).contains(&m) || d == 0 {
        return false;
    }
    let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let len = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    d <= len[(m - 1) as usize]
}
