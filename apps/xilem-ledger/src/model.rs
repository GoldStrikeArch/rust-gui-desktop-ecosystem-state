//! The number model: locale-aware parsing/formatting, the row type,
//! validation and totals. No xilem types appear here on purpose — this is
//! the part a framework gives you nothing for, and it is all plain Rust.

use rust_decimal::Decimal;
use std::str::FromStr;

// --- MARK: locale ---

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Locale {
    EnUs,
    FrFr,
}

impl Locale {
    pub fn decimal(self) -> char {
        match self {
            Self::EnUs => '.',
            Self::FrFr => ',',
        }
    }
    pub fn group(self) -> char {
        match self {
            Self::EnUs => ',',
            // ASCII space, not U+202F: fontique 0.6 with masonry's default
            // font set renders the narrow no-break space as tofu.
            Self::FrFr => ' ',
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::EnUs => "en-US",
            Self::FrFr => "fr-FR",
        }
    }
    pub fn other(self) -> Self {
        match self {
            Self::EnUs => Self::FrFr,
            Self::FrFr => Self::EnUs,
        }
    }
}

/// Characters that may appear in a *partial* number while the user types.
/// This is the "filter while typing" predicate: it accepts anything that
/// could still grow into a valid number, and nothing else.
pub fn typing_filter(s: &str, loc: Locale, allow_negative: bool, allow_fraction: bool) -> String {
    let mut out = String::with_capacity(s.len());
    let mut seen_decimal = false;
    for (i, c) in s.chars().enumerate() {
        let keep = match c {
            '-' if allow_negative && i == 0 => true,
            '(' if allow_negative && i == 0 => true,
            ')' if allow_negative => true,
            c if c.is_ascii_digit() => true,
            c if c == loc.decimal() && allow_fraction && !seen_decimal => {
                seen_decimal = true;
                true
            }
            c if c == loc.group() => true,
            _ => false,
        };
        if keep {
            out.push(c);
        }
    }
    out
}

/// Locale-aware, paste-tolerant parse.
///
/// Accepts `1,234.56`, `1.234,56`, `1 234,56`, `$1,234.56`, `1.234,56 EUR`,
/// `(12.50)` (accounting negative) and bare `1234.5`. Ambiguity rule: if both
/// `.` and `,` occur, the *last* one is the decimal separator; if only one
/// occurs, it is a grouping separator only when it is followed by exactly
/// three digits and it is the active locale's grouping character.
pub fn parse_decimal(raw: &str, loc: Locale) -> Option<Decimal> {
    let mut s: String = raw.trim().to_string();
    // Accounting negative.
    let mut negative = false;
    if s.starts_with('(') && s.ends_with(')') && s.len() >= 3 {
        negative = true;
        s = s[1..s.len() - 1].to_string();
    }
    // Strip currency symbols / codes / all whitespace kinds.
    let s: String = s
        .replace("EUR", "")
        .replace("USD", "")
        .chars()
        .filter(|c| !matches!(c, '$' | '\u{20ac}' | '\u{a3}' | '\u{a5}') && !c.is_whitespace())
        .collect();
    if s.is_empty() {
        return None;
    }
    if s.starts_with('-') {
        negative = !negative;
    }
    let body: String = s.chars().filter(|c| *c != '-' && *c != '+').collect();
    if body.is_empty() || !body.chars().all(|c| c.is_ascii_digit() || c == '.' || c == ',') {
        return None;
    }

    let last_dot = body.rfind('.');
    let last_comma = body.rfind(',');
    let dec_pos = match (last_dot, last_comma) {
        (Some(d), Some(c)) => Some(d.max(c)),
        (Some(d), None) => decide_single(&body, d, '.', loc),
        (None, Some(c)) => decide_single(&body, c, ',', loc),
        (None, None) => None,
    };

    let mut digits = String::new();
    for (i, c) in body.char_indices() {
        if c.is_ascii_digit() {
            digits.push(c);
        } else if Some(i) == dec_pos {
            digits.push('.');
        }
        // any other separator is grouping -> dropped
    }
    if digits.starts_with('.') {
        digits.insert(0, '0');
    }
    let mut d = Decimal::from_str(&digits).ok()?;
    if negative {
        d.set_sign_negative(true);
    }
    Some(d)
}

/// Single-separator disambiguation: grouping iff exactly three digits follow
/// *and* the character is the locale's grouping character *and* there is at
/// least one digit before it.
fn decide_single(body: &str, pos: usize, ch: char, loc: Locale) -> Option<usize> {
    let after = body.len() - pos - 1;
    let before = pos;
    if ch == loc.group() && after == 3 && before > 0 {
        None
    } else {
        Some(pos)
    }
}

/// Format with grouping and a fixed number of decimals.
pub fn format_decimal(v: Decimal, dp: u32, loc: Locale) -> String {
    let r = v.round_dp(dp);
    let neg = r.is_sign_negative() && !r.is_zero();
    let s = r.abs().to_string();
    let (int_part, frac_part) = match s.split_once('.') {
        Some((i, f)) => (i.to_string(), f.to_string()),
        None => (s, String::new()),
    };
    let mut frac = frac_part;
    while frac.len() < dp as usize {
        frac.push('0');
    }
    frac.truncate(dp as usize);

    let mut grouped = String::new();
    for (i, c) in int_part.chars().enumerate() {
        if i > 0 && (int_part.len() - i) % 3 == 0 {
            grouped.push(loc.group());
        }
        grouped.push(c);
    }
    let mut out = String::new();
    if neg {
        out.push('-');
    }
    out.push_str(&grouped);
    if dp > 0 {
        out.push(loc.decimal());
        out.push_str(&frac);
    }
    out
}

/// Split a formatted number into (integer-side, separator+fraction) so the
/// two halves can be laid out in two fixed-width boxes and line up on the
/// decimal point regardless of the font's digit metrics.
pub fn split_at_decimal(formatted: &str, loc: Locale) -> (String, String) {
    match formatted.rfind(loc.decimal()) {
        Some(i) => (formatted[..i].to_string(), formatted[i..].to_string()),
        None => (formatted.to_string(), String::new()),
    }
}

// --- MARK: rows ---

pub const CATEGORIES: &[&str] = &[
    "Travel", "Meals", "Lodging", "Software", "Hardware", "Training", "Office", "Other",
];

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Field {
    Date,
    Desc,
    Cat,
    Qty,
    Price,
}

impl Field {
    pub const ORDER: [Self; 5] = [Self::Date, Self::Desc, Self::Cat, Self::Qty, Self::Price];
    pub fn name(self) -> &'static str {
        match self {
            Self::Date => "Date",
            Self::Desc => "Description",
            Self::Cat => "Category",
            Self::Qty => "Qty",
            Self::Price => "Unit price",
        }
    }
    pub fn numeric(self) -> bool {
        matches!(self, Self::Qty | Self::Price)
    }
    /// Arrow-key step (and its x10 shift variant) for numeric fields.
    pub fn step(self) -> Decimal {
        match self {
            Self::Qty => Decimal::ONE,
            Self::Price => Decimal::new(1, 2),
            _ => Decimal::ZERO,
        }
    }
    pub fn dp(self) -> u32 {
        match self {
            Self::Price => 2,
            _ => 0,
        }
    }
}

/// One expense row. Committed values are typed (`Decimal`); `draft` holds the
/// in-progress *text* of whichever cell is being edited, so an invalid edit
/// can be shown without destroying the committed value.
#[derive(Clone, Debug)]
pub struct Row {
    pub date: String,
    pub desc: String,
    pub cat: String,
    pub qty: Decimal,
    pub price: Decimal,
    pub reimb: bool,
    /// Uncommitted text per field (only set while editing).
    pub draft: [Option<String>; 5],
    /// Per-field error message, set at commit time.
    pub err: [Option<String>; 5],
}

impl Row {
    pub fn amount(&self) -> Decimal {
        (self.qty * self.price).round_dp(2)
    }

    pub fn field_index(f: Field) -> usize {
        match f {
            Field::Date => 0,
            Field::Desc => 1,
            Field::Cat => 2,
            Field::Qty => 3,
            Field::Price => 4,
        }
    }

    /// The text a cell should display: the uncommitted draft if there is one,
    /// otherwise the normalised/formatted committed value.
    pub fn text(&self, f: Field, loc: Locale) -> String {
        if let Some(d) = &self.draft[Self::field_index(f)] {
            return d.clone();
        }
        self.committed_text(f, loc)
    }

    pub fn committed_text(&self, f: Field, loc: Locale) -> String {
        match f {
            Field::Date => self.date.clone(),
            Field::Desc => self.desc.clone(),
            Field::Cat => self.cat.clone(),
            Field::Qty => format_decimal(self.qty, 0, loc),
            Field::Price => format_decimal(self.price, 2, loc),
        }
    }

    /// Commit the draft of `f`. Returns `Err(message)` and keeps the previous
    /// committed value if the draft is invalid.
    pub fn commit(&mut self, f: Field, loc: Locale) -> Result<(), String> {
        let idx = Self::field_index(f);
        let Some(draft) = self.draft[idx].take() else {
            return Ok(());
        };
        let res = self.apply(f, &draft, loc);
        match &res {
            Ok(()) => self.err[idx] = None,
            Err(m) => {
                self.err[idx] = Some(m.clone());
                // keep the bad text visible so the user can fix it
                self.draft[idx] = Some(draft);
            }
        }
        res
    }

    fn apply(&mut self, f: Field, text: &str, loc: Locale) -> Result<(), String> {
        match f {
            Field::Date => {
                let t = text.trim();
                if !valid_date(t) {
                    return Err("date must be YYYY-MM-DD".into());
                }
                self.date = t.to_string();
            }
            Field::Desc => {
                let t = text.trim();
                if t.is_empty() || t.chars().count() > 60 {
                    return Err("description: 1-60 characters".into());
                }
                self.desc = t.to_string();
            }
            Field::Cat => {
                let t = text.trim();
                let hit = CATEGORIES
                    .iter()
                    .find(|c| c.eq_ignore_ascii_case(t))
                    .or_else(|| {
                        CATEGORIES
                            .iter()
                            .find(|c| c.to_lowercase().starts_with(&t.to_lowercase()))
                            .filter(|_| !t.is_empty())
                    });
                match hit {
                    Some(c) => self.cat = (*c).to_string(),
                    None => return Err("unknown category".into()),
                }
            }
            Field::Qty => {
                let v = parse_decimal(text, loc).ok_or("not a number")?;
                if v.fract() != Decimal::ZERO {
                    return Err("qty must be a whole number".into());
                }
                if v < Decimal::ONE || v > Decimal::from(999) {
                    return Err("qty must be 1-999".into());
                }
                self.qty = v;
            }
            Field::Price => {
                let v = parse_decimal(text, loc).ok_or("not a number")?;
                let lim = Decimal::new(9_999_999, 2);
                if v < -lim || v > lim {
                    return Err("price out of range".into());
                }
                self.price = v.round_dp(2);
            }
        }
        Ok(())
    }

    pub fn to_tsv(&self, loc: Locale) -> String {
        format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}",
            self.date,
            self.desc,
            self.cat,
            format_decimal(self.qty, 0, loc),
            format_decimal(self.price, 2, loc),
            format_decimal(self.amount(), 2, loc),
            if self.reimb { "yes" } else { "no" }
        )
    }

    /// Fill this row from a TSV line. Best-effort: unknown fields are skipped.
    pub fn from_tsv(&mut self, line: &str, loc: Locale) -> bool {
        let f: Vec<&str> = line.trim_end_matches(['\n', '\r']).split('\t').collect();
        if f.len() < 5 {
            return false;
        }
        let mut ok = true;
        for (i, field) in Field::ORDER.iter().enumerate() {
            self.draft[Row::field_index(*field)] = Some(f[i].to_string());
            ok &= self.commit(*field, loc).is_ok();
        }
        if let Some(r) = f.get(6) {
            self.reimb = r.eq_ignore_ascii_case("yes") || *r == "true" || *r == "1";
        }
        ok
    }
}

fn valid_date(t: &str) -> bool {
    let b = t.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return false;
    }
    if !b.iter().enumerate().all(|(i, c)| {
        if i == 4 || i == 7 {
            true
        } else {
            c.is_ascii_digit()
        }
    }) {
        return false;
    }
    let m: u32 = t[5..7].parse().unwrap_or(0);
    let d: u32 = t[8..10].parse().unwrap_or(0);
    (1..=12).contains(&m) && (1..=31).contains(&d)
}

/// A date mask: keep digits, re-insert the dashes at 4 and 7.
pub fn mask_date(s: &str) -> String {
    let digits: String = s.chars().filter(|c| c.is_ascii_digit()).take(8).collect();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i == 4 || i == 6 {
            out.push('-');
        }
        out.push(c);
    }
    out
}

pub fn seed_rows() -> Vec<Row> {
    let raw: &[(&str, &str, &str, i64, i64)] = &[
        ("2026-01-08", "Flight LHR-JFK", "Travel", 1, 48250),
        ("2026-01-09", "Airport transfer", "Travel", 2, 3475),
        ("2026-01-09", "Hotel, 3 nights", "Lodging", 3, 21900),
        ("2026-01-10", "Team dinner", "Meals", 6, 4180),
        ("2026-01-11", "Conference pass", "Training", 1, 129500),
        ("2026-01-11", "Duplicate charge refund", "Travel", 1, -3475),
        ("2026-01-12", "USB-C dock", "Hardware", 1, 18999),
        ("2026-01-13", "IDE licence", "Software", 4, 9900),
        ("2026-01-14", "Printer paper", "Office", 12, 599),
        ("2026-01-15", "Taxi to client", "Travel", 2, 2250),
        ("2026-01-16", "Lunch with vendor", "Meals", 3, 1875),
        ("2026-01-17", "Standing desk mat", "Office", 1, 7499),
    ];
    raw.iter()
        .map(|(d, s, c, q, p)| Row {
            date: (*d).into(),
            desc: (*s).into(),
            cat: (*c).into(),
            qty: Decimal::from(*q),
            price: Decimal::new(*p, 2),
            reimb: *p > 0,
            draft: [None, None, None, None, None],
            err: [None, None, None, None, None],
        })
        .collect()
}

pub struct Totals {
    pub subtotal: Decimal,
    pub vat: Decimal,
    pub total: Decimal,
}

pub fn totals(rows: &[Row], vat_pct: Decimal) -> Totals {
    let subtotal: Decimal = rows.iter().map(|r| r.amount()).sum();
    let vat = (subtotal * vat_pct / Decimal::from(100)).round_dp(2);
    Totals {
        subtotal,
        vat,
        total: subtotal + vat,
    }
}

/// (row index, field, message) for every current error.
pub fn error_summary(rows: &[Row]) -> Vec<(usize, Field, String)> {
    let mut out = Vec::new();
    for (i, r) in rows.iter().enumerate() {
        for f in Field::ORDER {
            if let Some(m) = &r.err[Row::field_index(f)] {
                out.push((i, f, m.clone()));
            }
        }
    }
    out
}
