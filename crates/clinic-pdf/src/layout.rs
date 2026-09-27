//! Positions every piece of text on A5 pages. Coordinates here are millimetres measured from
//! the TOP of the page (easier to reason about); they are flipped when converted to PDF ops.

use clinic_core::money::Paise;
use printpdf::{Color, FontId, Line, LinePoint, Mm, Op, ParsedFont, PdfFontHandle, Point, Pt, Rgb, TextItem};

use crate::LoadedFonts;
use crate::receipt::{ReceiptData, ReceiptLine};

pub const PAGE_WIDTH_MM: f32 = 148.0;
pub const PAGE_HEIGHT_MM: f32 = 210.0;
const MARGIN_MM: f32 = 10.0;
const RIGHT_MM: f32 = PAGE_WIDTH_MM - MARGIN_MM;
const BOTTOM_LIMIT_MM: f32 = PAGE_HEIGHT_MM - MARGIN_MM - 8.0; // room for the page number

// Item table columns (x in mm; numbers are right-aligned at these positions).
const ITEM_X: f32 = MARGIN_MM;
const QTY_RIGHT: f32 = 72.0;
const RATE_RIGHT: f32 = 94.0;
const DISC_RIGHT: f32 = 114.0;
const AMOUNT_RIGHT: f32 = RIGHT_MM;
const ITEM_MAX_WIDTH: f32 = QTY_RIGHT - 10.0 - ITEM_X;

const BODY: f32 = 9.0;
const SMALL: f32 = 7.5;
const MM_PER_PT: f32 = 25.4 / 72.0;

const BLACK: (f32, f32, f32) = (0.1, 0.1, 0.1);
const GREY: (f32, f32, f32) = (0.42, 0.45, 0.5);
const RED: (f32, f32, f32) = (0.75, 0.1, 0.1);

pub struct FontIds {
    pub regular: FontId,
    pub bold: FontId,
}

#[derive(Clone, Copy)]
enum Weight {
    Regular,
    Bold,
}

#[derive(Clone, Copy)]
enum Align {
    Left,
    Right,
    Center,
}

struct Pages<'a> {
    fonts: &'a LoadedFonts,
    ids: &'a FontIds,
    done: Vec<Vec<Op>>,
    ops: Vec<Op>,
    /// Distance from the top of the page to the top of the next line, in mm.
    y: f32,
}

impl<'a> Pages<'a> {
    fn new(fonts: &'a LoadedFonts, ids: &'a FontIds) -> Self {
        Self { fonts, ids, done: Vec::new(), ops: Vec::new(), y: MARGIN_MM }
    }

    /// Returned references live for `'a` (not for the `&self` borrow), so callers can keep
    /// using them while pushing ops.
    fn font(&self, weight: Weight) -> (&'a ParsedFont, &'a FontId) {
        let (fonts, ids) = (self.fonts, self.ids);
        match weight {
            Weight::Regular => (&fonts.regular, &ids.regular),
            Weight::Bold => (&fonts.bold, &ids.bold),
        }
    }

    fn line_height(size: f32) -> f32 {
        size * MM_PER_PT * 1.45
    }

    /// Starts a new page when fewer than `needed_mm` remain. Returns true if it did.
    fn ensure_space(&mut self, needed_mm: f32) -> bool {
        if self.y + needed_mm <= BOTTOM_LIMIT_MM {
            return false;
        }
        self.done.push(std::mem::take(&mut self.ops));
        self.y = MARGIN_MM;
        true
    }

    fn text(&mut self, s: &str, x: f32, size: f32, weight: Weight, align: Align, color: (f32, f32, f32)) {
        let (font, id) = self.font(weight);
        let width = text_width_mm(font, s, size);
        let left = match align {
            Align::Left => x,
            Align::Right => x - width,
            Align::Center => x - width / 2.0,
        };
        let baseline = self.y + size * MM_PER_PT; // top of line + ascent (approx.)
        self.ops.extend([
            Op::SetFillColor { col: Color::Rgb(Rgb::new(color.0, color.1, color.2, None)) },
            Op::StartTextSection,
            Op::SetFont { font: PdfFontHandle::External(id.clone()), size: Pt(size) },
            Op::SetTextCursor { pos: Point::new(Mm(left), Mm(PAGE_HEIGHT_MM - baseline)) },
            Op::ShowText { items: vec![TextItem::Text(s.to_string())] },
            Op::EndTextSection,
        ]);
    }

    fn advance(&mut self, size: f32) {
        self.y += Self::line_height(size);
    }

    fn rule(&mut self) {
        let y = PAGE_HEIGHT_MM - (self.y + 1.2);
        self.ops.extend([
            Op::SetOutlineColor { col: Color::Rgb(Rgb::new(GREY.0, GREY.1, GREY.2, None)) },
            Op::SetOutlineThickness { pt: Pt(0.5) },
            Op::DrawLine {
                line: Line {
                    points: vec![
                        LinePoint { p: Point::new(Mm(MARGIN_MM), Mm(y)), bezier: false },
                        LinePoint { p: Point::new(Mm(RIGHT_MM), Mm(y)), bezier: false },
                    ],
                    is_closed: false,
                },
            },
        ]);
        self.y += 3.0;
    }

    fn finish(mut self) -> Vec<Vec<Op>> {
        self.done.push(self.ops);
        let total = self.done.len();
        let mut pages = Vec::with_capacity(total);
        for (index, ops) in self.done.into_iter().enumerate() {
            let mut page = Pages { fonts: self.fonts, ids: self.ids, done: Vec::new(), ops, y: PAGE_HEIGHT_MM - MARGIN_MM - 3.0 };
            if total > 1 {
                page.text(&format!("Page {} of {}", index + 1, total), PAGE_WIDTH_MM / 2.0, SMALL, Weight::Regular, Align::Center, GREY);
            }
            pages.push(page.ops);
        }
        pages
    }
}

/// Width of `text` in mm at `size_pt`, from the font's glyph advance widths.
pub(crate) fn text_width_mm(font: &ParsedFont, text: &str, size_pt: f32) -> f32 {
    let units: u32 = text
        .chars()
        .filter_map(|c| font.lookup_glyph_index(c as u32))
        .filter_map(|gid| font.get_glyph_width(gid))
        .map(u32::from)
        .sum();
    units as f32 * size_pt / f32::from(font.units_per_em.max(1)) * MM_PER_PT
}

/// Splits `text` at spaces into as few lines as fit `max_mm` (one line whenever it fits). A single
/// word wider than the page is shortened with "…".
pub(crate) fn wrap_text(font: &ParsedFont, text: &str, size_pt: f32, max_mm: f32) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let candidate = if current.is_empty() { word.to_string() } else { format!("{current} {word}") };
        if current.is_empty() || text_width_mm(font, &candidate, size_pt) <= max_mm {
            current = candidate;
        } else {
            lines.push(std::mem::take(&mut current));
            current = word.to_string();
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines.into_iter().map(|line| fit_text(font, &line, size_pt, max_mm)).collect()
}

/// The clinic address on one line ("12 MG Road, Pune 411001"), from the address lines in Settings.
pub(crate) fn address_text(lines: &[String]) -> String {
    lines.iter().map(|l| l.trim().trim_end_matches(',')).filter(|l| !l.is_empty()).collect::<Vec<_>>().join(", ")
}

/// Shortens `text` with "…" until it fits `max_mm`.
pub(crate) fn fit_text(font: &ParsedFont, text: &str, size_pt: f32, max_mm: f32) -> String {
    if text_width_mm(font, text, size_pt) <= max_mm {
        return text.to_string();
    }
    let mut chars: Vec<char> = text.chars().collect();
    while !chars.is_empty() {
        chars.pop();
        let candidate: String = chars.iter().collect::<String>().trim_end().to_string() + "…";
        if text_width_mm(font, &candidate, size_pt) <= max_mm {
            return candidate;
        }
    }
    "…".to_string()
}

fn money(p: Paise) -> String {
    p.to_indian_string()
}

pub fn layout_receipt(data: &ReceiptData, fonts: &LoadedFonts, ids: &FontIds) -> Vec<Vec<Op>> {
    let mut pages = Pages::new(fonts, ids);
    header(&mut pages, data);
    table_header(&mut pages);
    let mut section = "";
    for line in &data.lines {
        if !line.section.is_empty() && line.section != section {
            section_heading(&mut pages, data, &line.section);
            section = line.section.as_str();
        }
        item_line(&mut pages, data, line);
    }
    totals(&mut pages, data);
    pages.finish()
}

fn header(p: &mut Pages<'_>, data: &ReceiptData) {
    let center = PAGE_WIDTH_MM / 2.0;
    p.text(&data.clinic_name, center, 14.0, Weight::Bold, Align::Center, BLACK);
    p.advance(14.0);
    // The whole address on one line; wrapped only when it is wider than the page.
    let address = address_text(&data.clinic_address_lines);
    for line in wrap_text(&p.fonts.regular, &address, SMALL + 0.5, RIGHT_MM - MARGIN_MM) {
        p.text(&line, center, SMALL + 0.5, Weight::Regular, Align::Center, GREY);
        p.advance(SMALL + 0.5);
    }
    // Phone only: no GSTIN or other GST wording on invoices.
    if let Some(phone) = &data.clinic_phone {
        p.text(&format!("Ph {phone}"), center, SMALL + 0.5, Weight::Regular, Align::Center, GREY);
        p.advance(SMALL + 0.5);
    }
    if let Some(banner) = &data.status_banner {
        p.y += 1.5;
        p.text(banner, center, 12.0, Weight::Bold, Align::Center, RED);
        p.advance(12.0);
    }
    p.rule();
    p.text(&format!("Bill No: {}", data.bill_no), MARGIN_MM, BODY, Weight::Bold, Align::Left, BLACK);
    p.text(&data.date_time, RIGHT_MM, BODY, Weight::Regular, Align::Right, BLACK);
    p.advance(BODY);
    if let Some(client) = &data.client_label {
        p.text(&format!("Client: {client}"), MARGIN_MM, BODY, Weight::Regular, Align::Left, BLACK);
        p.advance(BODY);
    }
    p.rule();
}

fn table_header(p: &mut Pages<'_>) {
    p.text("Item", ITEM_X, BODY, Weight::Bold, Align::Left, BLACK);
    p.text("Qty", QTY_RIGHT, BODY, Weight::Bold, Align::Right, BLACK);
    p.text("MRP", RATE_RIGHT, BODY, Weight::Bold, Align::Right, BLACK);
    p.text("Disc", DISC_RIGHT, BODY, Weight::Bold, Align::Right, BLACK);
    p.text("Amount", AMOUNT_RIGHT, BODY, Weight::Bold, Align::Right, BLACK);
    p.advance(BODY);
    p.rule();
}

fn section_heading(p: &mut Pages<'_>, data: &ReceiptData, title: &str) {
    if p.ensure_space(Pages::line_height(SMALL) + Pages::line_height(BODY)) {
        p.text(&format!("{} (continued)", data.bill_no), MARGIN_MM, SMALL, Weight::Regular, Align::Left, GREY);
        p.advance(SMALL);
        table_header(p);
    }
    p.y += 1.0;
    p.text(&title.to_uppercase(), ITEM_X, SMALL, Weight::Bold, Align::Left, GREY);
    p.advance(SMALL);
}

fn item_line(p: &mut Pages<'_>, data: &ReceiptData, line: &ReceiptLine) {
    let not_supplied = line.not_supplied_qty > 0;
    let detail = if not_supplied {
        Some(format!("Not supplied (out of stock) – prescribed {}", line.not_supplied_qty))
    } else {
        line.detail.clone()
    };
    let needed = Pages::line_height(BODY) + detail.as_ref().map_or(0.0, |_| Pages::line_height(SMALL));
    if p.ensure_space(needed) {
        p.text(&format!("{} (continued)", data.bill_no), MARGIN_MM, SMALL, Weight::Regular, Align::Left, GREY);
        p.advance(SMALL);
        table_header(p);
    }
    let name = fit_text(&p.fonts.regular, &line.name, BODY, ITEM_MAX_WIDTH);
    p.text(&name, ITEM_X, BODY, Weight::Regular, Align::Left, BLACK);
    p.text(&line.qty.to_string(), QTY_RIGHT, BODY, Weight::Regular, Align::Right, BLACK);
    let rate = if not_supplied { "–".to_string() } else { money(line.unit_price) };
    p.text(&rate, RATE_RIGHT, BODY, Weight::Regular, Align::Right, BLACK);
    let discount = if line.discount == Paise::ZERO { "–".to_string() } else { format!("-{}", money(line.discount)) };
    p.text(&discount, DISC_RIGHT, BODY, Weight::Regular, Align::Right, BLACK);
    p.text(&money(line.amount), AMOUNT_RIGHT, BODY, Weight::Regular, Align::Right, BLACK);
    p.advance(BODY);
    if let Some(detail) = detail {
        let color = if not_supplied { RED } else { GREY };
        let detail = fit_text(&p.fonts.regular, &detail, SMALL, RIGHT_MM - ITEM_X - 4.0);
        p.text(&detail, ITEM_X + 2.0, SMALL, Weight::Regular, Align::Left, color);
        p.advance(SMALL);
    }
}

fn totals(p: &mut Pages<'_>, data: &ReceiptData) {
    let mut rows: Vec<(String, String, bool)> = if data.breakdown.is_empty() {
        vec![("Subtotal".into(), money(data.subtotal), false)]
    } else {
        data.breakdown.iter().map(|row| (row.label.clone(), money(row.amount), false)).collect()
    };
    if data.discount != Paise::ZERO {
        rows.push((data.discount_label.clone(), format!("-{}", money(data.discount)), false));
    }
    if data.round_off != Paise::ZERO {
        rows.push(("Round off".into(), money(data.round_off), false));
    }
    rows.push(("TOTAL".into(), format!("₹ {}", money(data.total)), true));

    let payments: Vec<String> = data.payments.iter().map(|pay| format!("{} {}", pay.method, money(pay.amount))).collect();
    let needed = rows.len() as f32 * Pages::line_height(BODY) + 30.0;
    if p.ensure_space(needed) {
        p.text(&format!("{} (continued)", data.bill_no), MARGIN_MM, SMALL, Weight::Regular, Align::Left, GREY);
        p.advance(SMALL);
    }
    p.rule();
    let label_right = RATE_RIGHT - 4.0;
    for (label, value, strong) in rows {
        let (size, weight) = if strong { (11.0, Weight::Bold) } else { (BODY, Weight::Regular) };
        if strong {
            p.y += 1.0;
        }
        p.text(&label, label_right, size, weight, Align::Right, BLACK);
        p.text(&value, AMOUNT_RIGHT, size, weight, Align::Right, BLACK);
        p.advance(size);
    }
    p.rule();
    if !payments.is_empty() {
        p.text(&format!("Paid: {}", payments.join(" + ")), MARGIN_MM, BODY, Weight::Regular, Align::Left, BLACK);
        p.advance(BODY);
    }
    if let (Some(received), Some(change)) = (data.amount_received, data.change_due) {
        p.text(&format!("Received {}   Change {}", money(received), money(change)), MARGIN_MM, BODY, Weight::Regular, Align::Left, BLACK);
        p.advance(BODY);
    }
    if let Some(by) = &data.billed_by {
        p.text(&format!("Billed by: {by}"), MARGIN_MM, SMALL, Weight::Regular, Align::Left, GREY);
        p.advance(SMALL);
    }
    if let Some(footer) = &data.footer {
        p.y += 3.0;
        for line in wrap_text(&p.fonts.regular, footer, BODY, RIGHT_MM - MARGIN_MM) {
            p.text(&line, PAGE_WIDTH_MM / 2.0, BODY, Weight::Regular, Align::Center, BLACK);
            p.advance(BODY);
        }
    }
    if let Some(notice) = &data.notice {
        p.y += 2.0;
        let notice = fit_text(&p.fonts.regular, notice, SMALL, RIGHT_MM - MARGIN_MM);
        p.text(&notice, PAGE_WIDTH_MM / 2.0, SMALL, Weight::Regular, Align::Center, GREY);
        p.advance(SMALL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PdfError, load_fonts, sample_receipt};
    use printpdf::PdfDocument;

    fn ids(fonts: &LoadedFonts) -> FontIds {
        let mut doc = PdfDocument::new("test");
        FontIds { regular: doc.add_font(&fonts.regular), bold: doc.add_font(&fonts.bold) }
    }

    #[test]
    fn text_width_grows_with_text_and_size() -> Result<(), PdfError> {
        let fonts = load_fonts()?;
        assert_eq!(text_width_mm(&fonts.regular, "", BODY), 0.0);
        let short = text_width_mm(&fonts.regular, "₹ 10", BODY);
        let long = text_width_mm(&fonts.regular, "₹ 10,000.00", BODY);
        assert!(short > 0.0 && long > short);
        assert!(text_width_mm(&fonts.regular, "₹ 10", 18.0) > short);
        Ok(())
    }

    #[test]
    fn the_address_is_one_line_when_it_fits_and_wraps_only_when_needed() -> Result<(), PdfError> {
        let fonts = load_fonts()?;
        let width = RIGHT_MM - MARGIN_MM;
        let address = address_text(&["Shop 4, Sai Plaza,".into(), " FC Road ".into(), "Pune 411004".into()]);
        assert_eq!(address, "Shop 4, Sai Plaza, FC Road, Pune 411004");
        assert_eq!(wrap_text(&fonts.regular, &address, SMALL + 0.5, width), vec![address.clone()]);

        let long = "Ground Floor, Shop No. 4 and 5, Sai Krupa Commercial Complex, Opposite Big Municipal Garden, Near Main Bus Stand, Fergusson College Road, Shivajinagar, Pune, Maharashtra 411004";
        let lines = wrap_text(&fonts.regular, long, SMALL + 0.5, width);
        assert!(lines.len() > 1, "too wide for one line");
        assert!(lines.iter().all(|l| text_width_mm(&fonts.regular, l, SMALL + 0.5) <= width));
        for pair in lines.windows(2) {
            let next_word = pair[1].split(' ').next().unwrap_or_default();
            let joined = format!("{} {next_word}", pair[0]);
            assert!(text_width_mm(&fonts.regular, &joined, SMALL + 0.5) > width, "each line is filled before wrapping");
        }
        assert_eq!(lines.join(" "), long, "nothing lost");
        assert!(wrap_text(&fonts.regular, "", SMALL, width).is_empty());
        Ok(())
    }

    #[test]
    fn long_names_are_shortened_to_fit_the_column() -> Result<(), PdfError> {
        let fonts = load_fonts()?;
        let name = "Extra Long Medicine Name With Many Words 500mg Tablets Strip Of Fifteen";
        let fitted = fit_text(&fonts.regular, name, BODY, ITEM_MAX_WIDTH);
        assert!(fitted.ends_with('…'));
        assert!(text_width_mm(&fonts.regular, &fitted, BODY) <= ITEM_MAX_WIDTH);
        assert_eq!(fit_text(&fonts.regular, "Paracetamol", BODY, ITEM_MAX_WIDTH), "Paracetamol");
        Ok(())
    }

    #[test]
    fn short_bill_fits_one_page_and_long_bill_paginates() -> Result<(), PdfError> {
        let fonts = load_fonts()?;
        let ids = ids(&fonts);
        let mut data = sample_receipt();
        assert_eq!(layout_receipt(&data, &fonts, &ids).len(), 1);

        let template = data.lines[0].clone();
        data.lines = (0..60).map(|_| template.clone()).collect();
        let pages = layout_receipt(&data, &fonts, &ids);
        assert!(pages.len() >= 2, "60 lines should need more than one A5 page");
        assert!(pages.iter().all(|ops| !ops.is_empty()));
        Ok(())
    }
}
