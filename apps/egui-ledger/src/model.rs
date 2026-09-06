//! Money, locale and parsing — the "number model" of SPEC-10.
//!
//! The value that travels between the widget and the model is a
//! `rust_decimal::Decimal`; the value that travels between the *keyboard* and
//! the widget is a `String`, because egui's `TextEdit` is `&mut String` and
//! nothing else. Everything below is the glue in between.

use rust_decimal::Decimal;
use std::str::FromStr;

pub const CATEGORIES: [&str; 6] =
    ["Travel", "Meals", "Hardware", "Software", "Office", "Other"];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Locale {
    EnUs,
    FrFr,
}

impl Locale {
    pub fn dec(self) -> char {
        match self {
            Self::EnUs => '.',
            Self::FrFr => ',',
        }
    }
    /// fr-FR groups with a space. We use an ASCII space, NOT the typographically
    /// correct U+202F / U+2009: epaint gives those a `FontTweak::thin_space_width`
    /// of 0.5 space *even in the monospace face*, which would break the
    /// decimal-point alignment this spec measures. Recorded in FRICTION.md.
    pub fn grp(self) -> char {
        match self {
            Self::EnUs => ',',
            Self::FrFr => ' ',
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::EnUs => "en-US",
            Self::FrFr => "fr-FR",
        }
    }
}

/// Format with exactly `dp` decimals and locale grouping.
///
/// Exactly `dp` decimals is what makes the column line up on the separator:
/// with a monospaced (equal-advance) digit face, right-aligning a set of
/// strings that all have the same number of fractional digits puts every
/// decimal separator in the same pixel column. See `decimal_align` in
/// FRICTION.md.
pub fn fmt_dec(v: Decimal, dp: u32, loc: Locale) -> String {
    let neg = v.is_sign_negative() && !v.is_zero();
    let plain = v.abs().round_dp(dp).to_string();
    let (int, frac) = match plain.split_once('.') {
        Some((i, f)) => (i.to_owned(), f.to_owned()),
        None => (plain, String::new()),
    };
    let mut frac = frac;
    while frac.len() < dp as usize {
        frac.push('0');
    }
    frac.truncate(dp as usize);

    let mut grouped = String::new();
    for (n, c) in int.chars().rev().enumerate() {
        if n > 0 && n % 3 == 0 {
            grouped.push(loc.grp());
        }
        grouped.push(c);
    }
    let int: String = grouped.chars().rev().collect();

    let mut out = String::new();
    if neg {
        out.push('-');
    }
    out.push_str(&int);
    if dp > 0 {
        out.push(loc.dec());
        out.push_str(&frac);
    }
    out
}

/// Parse a user- or clipboard-supplied number.
///
/// Handles: currency symbols and spaces, accounting negatives `(12.50)`,
/// and both separator conventions regardless of the active locale (so a
/// pasted `1.234,56 €` parses while the app is in en-US).
pub fn parse_dec(src: &str, loc: Locale) -> Option<Decimal> {
    let mut t = src.trim().to_owned();
    let mut neg = false;
    if t.starts_with('(') && t.ends_with(')') && t.len() >= 3 {
        neg = true;
        t = t[1..t.len() - 1].to_owned();
    }
    // Drop currency symbols, spaces (incl. NBSP/thin space) and '+'.
    t.retain(|c| c.is_ascii_digit() || c == '.' || c == ',' || c == '-');
    if t.starts_with('-') {
        neg = !neg;
        t.remove(0);
    }
    if t.contains('-') || t.is_empty() {
        return None;
    }

    let last_dot = t.rfind('.');
    let last_com = t.rfind(',');
    let dec_pos = match (last_dot, last_com) {
        // Both present: the later one is the decimal separator.
        (Some(d), Some(c)) => Some(d.max(c)),
        // Only one: a lone separator with exactly 3 digits after it is
        // grouping unless it is this locale's decimal separator.
        (Some(d), None) => {
            let after = t.len() - d - 1;
            if after == 3 && loc.dec() != '.' { None } else { Some(d) }
        }
        (None, Some(c)) => {
            let after = t.len() - c - 1;
            if after == 3 && loc.dec() != ',' { None } else { Some(c) }
        }
        (None, None) => None,
    };

    let cleaned = match dec_pos {
        Some(p) => {
            let (head, tail) = t.split_at(p);
            let head: String = head.chars().filter(|c| c.is_ascii_digit()).collect();
            let tail: String = tail[1..].chars().filter(|c| c.is_ascii_digit()).collect();
            if tail.is_empty() {
                head
            } else {
                format!("{head}.{tail}")
            }
        }
        None => t.chars().filter(|c| c.is_ascii_digit()).collect(),
    };
    if cleaned.is_empty() || cleaned == "." {
        return None;
    }
    let v = Decimal::from_str(&cleaned).ok()?;
    Some(if neg { -v } else { v })
}

/// Filter a string *while it is being typed* so it can still become a number.
/// egui has no input mask or validator: the only hook is "the `TextEdit` gave
/// me back a `String`, now fix it".
pub fn filter_typing(s: &mut String, loc: Locale, allow_neg: bool, dp: u32) {
    let dec = loc.dec();
    let grp = loc.grp();
    let mut out = String::with_capacity(s.len());
    let mut seen_dec = false;
    for (i, c) in s.chars().enumerate() {
        match c {
            '-' if allow_neg && i == 0 => out.push(c),
            c if c.is_ascii_digit() => out.push(c),
            c if c == dec && dp > 0 && !seen_dec && !out.is_empty() => {
                seen_dec = true;
                out.push(c);
            }
            c if c == grp && !out.is_empty() => out.push(c),
            _ => {}
        }
    }
    // At most `dp` fractional digits.
    if let Some(p) = out.find(loc.dec()) {
        out.truncate((p + 1 + dp as usize).min(out.len()));
    }
    *s = out;
}

/// A masked `YYYY-MM-DD` field: keep digits, re-insert the dashes.
pub fn filter_date(s: &mut String) {
    let digits: String = s.chars().filter(|c| c.is_ascii_digit()).take(8).collect();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i == 4 || i == 6 {
            out.push('-');
        }
        out.push(c);
    }
    *s = out;
}

pub fn date_valid(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return false;
    }
    let n = |a: usize, z: usize| s[a..z].parse::<u32>().ok();
    match (n(0, 4), n(5, 7), n(8, 10)) {
        (Some(y), Some(m), Some(d)) => (1900..=2999).contains(&y) && (1..=12).contains(&m) && (1..=31).contains(&d),
        _ => false,
    }
}

#[derive(Clone)]
pub struct Row {
    pub date: String,
    pub desc: String,
    pub cat: usize,
    pub qty: Decimal,
    pub qty_buf: String,
    pub qty_err: Option<String>,
    pub price: Decimal,
    pub price_buf: String,
    pub price_err: Option<String>,
    pub reimb: bool,
}

impl Row {
    pub fn amount(&self) -> Decimal {
        (self.qty * self.price).round_dp(2)
    }

    pub fn reformat(&mut self, loc: Locale) {
        self.qty_buf = fmt_dec(self.qty, 0, loc);
        self.price_buf = fmt_dec(self.price, 2, loc);
    }

    /// Tab-separated form used by ⌘C / ⌘V.
    pub fn to_tsv(&self, loc: Locale) -> String {
        format!(
            "{}\t{}\t{}\t{}\t{}\t{}",
            self.date,
            self.desc,
            CATEGORIES[self.cat],
            fmt_dec(self.qty, 0, loc),
            fmt_dec(self.price, 2, loc),
            if self.reimb { "yes" } else { "no" }
        )
    }

    pub fn from_tsv(&mut self, tsv: &str, loc: Locale) -> bool {
        let f: Vec<&str> = tsv.trim_end_matches(['\n', '\r']).split('\t').collect();
        if f.len() < 5 {
            return false;
        }
        self.date = f[0].trim().to_owned();
        filter_date(&mut self.date);
        self.desc = f[1].trim().to_owned();
        if let Some(i) = CATEGORIES.iter().position(|c| c.eq_ignore_ascii_case(f[2].trim())) {
            self.cat = i;
        }
        if let Some(q) = parse_dec(f[3], loc) {
            self.qty = q.round_dp(0);
        }
        if let Some(p) = parse_dec(f[4], loc) {
            self.price = p.round_dp(2);
        }
        if let Some(r) = f.get(5) {
            self.reimb = matches!(r.trim(), "yes" | "true" | "1" | "y");
        }
        self.qty_err = None;
        self.price_err = None;
        self.reformat(loc);
        true
    }
}

pub fn seed(loc: Locale) -> Vec<Row> {
    const DATA: [(&str, &str, usize, i64, &str, bool); 12] = [
        ("2026-01-04", "Flight LHR-SFO", 0, 1, "842.30", true),
        ("2026-01-05", "Airport transfer", 0, 2, "38.75", true),
        ("2026-01-06", "Team dinner", 1, 1, "213.40", true),
        ("2026-01-08", "USB-C dock", 2, 3, "129.99", false),
        ("2026-01-09", "Monitor arm", 2, 2, "74.50", false),
        ("2026-01-11", "IDE licence", 3, 1, "199.00", true),
        ("2026-01-12", "Coffee, offsite", 1, 12, "4.25", false),
        ("2026-01-14", "Printer paper", 4, 6, "9.99", false),
        ("2026-01-15", "Returned dock", 2, 1, "-129.99", false),
        ("2026-01-18", "Conference pass", 0, 1, "1250.00", true),
        ("2026-01-20", "Whiteboard pens", 4, 4, "3.60", false),
        ("2026-01-22", "Cloud credits", 3, 1, "1875.25", true),
    ];
    DATA.into_iter()
        .map(|(date, desc, cat, qty, price, reimb)| {
            let mut r = Row {
                date: date.to_owned(),
                desc: desc.to_owned(),
                cat,
                qty: Decimal::from(qty),
                qty_buf: String::new(),
                qty_err: None,
                price: Decimal::from_str(price).unwrap(),
                price_buf: String::new(),
                price_err: None,
                reimb,
            };
            r.reformat(loc);
            r
        })
        .collect()
}
