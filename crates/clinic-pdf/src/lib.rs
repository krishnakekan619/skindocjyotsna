//! Receipt PDFs for SkinDocJyotsna.
//!
//! The PDF is laid out in Rust with an embedded font, so it looks the same on Windows and
//! macOS and never depends on the installed fonts or on the WebView.

mod layout;
mod receipt;

use printpdf::{Mm, ParsedFont, PdfDocument, PdfPage, PdfSaveOptions};

pub use receipt::{ReceiptData, ReceiptLine, ReceiptPayment, sample_receipt};

#[derive(Debug, thiserror::Error)]
pub enum PdfError {
    #[error("embedded font '{0}' could not be loaded")]
    Font(&'static str),
}

const REGULAR_TTF: &[u8] = include_bytes!("../fonts/NotoSans-Regular.ttf");
const BOLD_TTF: &[u8] = include_bytes!("../fonts/NotoSans-Bold.ttf");

pub(crate) struct LoadedFonts {
    pub regular: ParsedFont,
    pub bold: ParsedFont,
}

pub(crate) fn load_fonts() -> Result<LoadedFonts, PdfError> {
    Ok(LoadedFonts {
        regular: ParsedFont::from_bytes(REGULAR_TTF, 0, &mut Vec::new()).ok_or(PdfError::Font("NotoSans-Regular"))?,
        bold: ParsedFont::from_bytes(BOLD_TTF, 0, &mut Vec::new()).ok_or(PdfError::Font("NotoSans-Bold"))?,
    })
}

/// Renders an A5 portrait receipt (more pages are added automatically for long bills).
pub fn render_receipt_pdf(data: &ReceiptData) -> Result<Vec<u8>, PdfError> {
    let fonts = load_fonts()?;
    // Metadata carries only the bill number, never the client's name.
    let mut doc = PdfDocument::new(&format!("Receipt {}", data.bill_no));
    let ids = layout::FontIds { regular: doc.add_font(&fonts.regular), bold: doc.add_font(&fonts.bold) };
    let pages: Vec<PdfPage> = layout::layout_receipt(data, &fonts, &ids)
        .into_iter()
        .map(|ops| PdfPage::new(Mm(layout::PAGE_WIDTH_MM), Mm(layout::PAGE_HEIGHT_MM), ops))
        .collect();
    let mut warnings = Vec::new();
    Ok(doc.with_pages(pages).save(&PdfSaveOptions::default(), &mut warnings))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_receipt_renders_a_pdf() -> Result<(), PdfError> {
        let bytes = render_receipt_pdf(&sample_receipt())?;
        assert!(bytes.starts_with(b"%PDF"), "output must be a PDF");
        assert!(bytes.len() > 2_000, "PDF looks too small: {} bytes", bytes.len());
        Ok(())
    }

    #[test]
    fn embedded_fonts_cover_every_character_on_the_sample_receipt() -> Result<(), PdfError> {
        let fonts = load_fonts()?;
        let sample = sample_receipt();
        let mut text: Vec<String> = vec![sample.clinic_name.clone(), sample.bill_no.clone(), "₹ Not supplied –…".into()];
        text.extend(sample.clinic_address_lines.iter().cloned());
        text.extend(sample.lines.iter().map(|line| line.name.clone()));
        text.extend(sample.lines.iter().filter_map(|line| line.detail.clone()));
        for s in &text {
            for c in s.chars().filter(|c| !c.is_whitespace()) {
                assert!(fonts.regular.lookup_glyph_index(c as u32).is_some(), "regular font lacks {c:?}");
                assert!(fonts.bold.lookup_glyph_index(c as u32).is_some(), "bold font lacks {c:?}");
            }
        }
        Ok(())
    }
}
