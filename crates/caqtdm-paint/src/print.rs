//! `printPS` / `print2Painter` from `caqtdm_lib.h`.
//!
//! Qt 5 sets `QPrinter::NativeFormat` and writes the file through the print
//! engine. That page is PDF. The orientation is portrait, the resolution is
//! 300 dpi, and the widget is scaled with the same `pageRect` formula. On
//! Linux the file is handed to CUPS `lp`, on macOS to `lpr`, and on Windows
//! to the shell print verb.

/// A4, the paper `QPrinter` uses when `printPS` does not select a device.
pub const PRINT_DPI: i32 = 300;
pub const PRINT_POINT: f64 = 12.0;

#[derive(Debug, Clone, PartialEq)]
pub struct PrintLayout {
    pub orientation: &'static str,
    pub dpi: i32,
    pub paper_width_px: i32,
    pub paper_height_px: i32,
    pub page_x: i32,
    pub page_y: i32,
    pub page_width_px: i32,
    pub page_height_px: i32,
    pub line_spacing_px: i32,
    pub scale: f64,
    pub origin_x: f64,
    pub origin_y: f64,
    pub path_y: i32,
    pub paper_width_pt: f64,
    pub paper_height_pt: f64,
    pub image_x_pt: f64,
    pub image_y_pt: f64,
    pub image_w_pt: f64,
    pub image_h_pt: f64,
}

/// Portrait A4 at 300 dpi. Margins stay 0 because `printPS` never calls
/// `setPageMargins`, so `pageRect` and `paperRect` are the paper.
pub fn print_ps_layout(widget_w: i32, widget_h: i32) -> PrintLayout {
    let dpi = PRINT_DPI;
    let (paper_width_pt, paper_height_pt) = a4_points();
    let paper_width_px = (paper_width_pt * f64::from(dpi) / 72.0).round() as i32;
    let paper_height_px = (paper_height_pt * f64::from(dpi) / 72.0).round() as i32;
    let line_spacing_px = (PRINT_POINT * f64::from(dpi) / 72.0).round() as i32;
    let widget_w = widget_w.max(1);
    let widget_h = widget_h.max(1);
    let xscale = f64::from(paper_width_px) / f64::from(widget_w);
    let yscale = f64::from(paper_height_px) / f64::from(widget_h + line_spacing_px);
    let scale = xscale.min(yscale);
    let origin_x = f64::from(paper_width_px) / 2.0 + scale * (f64::from(-widget_w) / 2.0);
    let origin_y = f64::from(paper_height_px) / 2.0
        + scale * (f64::from(-widget_h) / 2.0 + f64::from(line_spacing_px));
    let pt_per_px = 72.0 / f64::from(dpi);
    let image_w_pt = f64::from(widget_w) * scale * pt_per_px;
    let image_h_pt = f64::from(widget_h) * scale * pt_per_px;
    let image_x_pt = origin_x * pt_per_px;
    let image_y_pt = paper_height_pt - (origin_y + f64::from(widget_h) * scale) * pt_per_px;
    PrintLayout {
        orientation: "portrait",
        dpi,
        paper_width_px,
        paper_height_px,
        page_x: 0,
        page_y: 0,
        page_width_px: paper_width_px,
        page_height_px: paper_height_px,
        line_spacing_px,
        scale,
        origin_x,
        origin_y,
        path_y: line_spacing_px,
        paper_width_pt,
        paper_height_pt,
        image_x_pt,
        image_y_pt,
        image_w_pt,
        image_h_pt,
    }
}

fn a4_points() -> (f64, f64) {
    (210.0 * 72.0 / 25.4, 297.0 * 72.0 / 25.4)
}

/// PDF page `print2Painter` would draw: the widget pixmap scaled into the
/// page, then the local caption and the full path.
pub fn render_print_pdf(
    width: u32,
    height: u32,
    rgba: &[u8],
    caption: &str,
    full_path: &str,
) -> Vec<u8> {
    let layout = print_ps_layout(width as i32, height as i32);
    let mut rgb = Vec::with_capacity((width * height * 3) as usize);
    for pixel in rgba.chunks(4) {
        let (r, g, b) = if pixel.len() >= 3 {
            (pixel[0], pixel[1], pixel[2])
        } else {
            (0, 0, 0)
        };
        rgb.extend_from_slice(&[r, g, b]);
    }
    let stream = format!(
        "q\n{w:.2} 0 0 {h:.2} {x:.2} {y:.2} cm\n/Im0 Do\nQ\nBT\n/F1 12 Tf\n1 0 0 1 4 {top:.2} Tm\n({caption}) Tj\n1 0 0 1 4 {path:.2} Tm\n({full}) Tj\nET\n",
        w = layout.image_w_pt,
        h = layout.image_h_pt,
        x = layout.image_x_pt,
        y = layout.image_y_pt,
        top = layout.paper_height_pt - 12.0,
        path = layout.paper_height_pt - 12.0 - f64::from(layout.path_y) * 72.0 / f64::from(layout.dpi),
        caption = pdf_text(caption),
        full = pdf_text(full_path),
    );
    let mut objects = Vec::new();
    objects.push(b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
    objects.push(b"<< /Type /Pages /Count 1 /Kids [3 0 R] >>".to_vec());
    objects.push(
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {:.2} {:.2}] /Contents 4 0 R /Resources << /XObject << /Im0 5 0 R >> /Font << /F1 6 0 R >> >> >>",
            layout.paper_width_pt, layout.paper_height_pt
        )
        .into_bytes(),
    );
    objects.push(format!("<< /Length {} >>\nstream\n{stream}endstream", stream.len()).into_bytes());
    let mut image = format!(
        "<< /Type /XObject /Subtype /Image /Width {width} /Height {height} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Length {} >>\nstream\n",
        rgb.len()
    )
    .into_bytes();
    image.extend_from_slice(&rgb);
    image.extend_from_slice(b"\nendstream");
    objects.push(image);
    objects.push(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec());

    let mut out = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", index + 1).as_bytes());
        out.extend_from_slice(object);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n", objects.len() + 1).as_bytes());
    out.extend_from_slice(b"0000000000 65535 f \n");
    for offset in offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    out
}

fn pdf_text(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn print_ps_is_portrait_a4_and_scales_like_print2painter() {
        let page = print_ps_layout(100, 100);
        assert_eq!(page.orientation, "portrait");
        assert_eq!(page.dpi, 300);
        assert!(page.paper_height_px > page.paper_width_px);
        assert_eq!(page.paper_width_px, 2480);
        assert_eq!(page.paper_height_px, 3508);
        assert_eq!(page.page_x, 0);
        assert_eq!(page.line_spacing_px, 50);
        let yscale = f64::from(page.page_height_px) / f64::from(100 + 50);
        assert!((page.scale - yscale).abs() < 1e-9);
        let wide = print_ps_layout(2_000, 10);
        assert!((wide.scale - f64::from(wide.page_width_px) / 2_000.0).abs() < 1e-9);
        let small = print_ps_layout(2, 2);
        let pdf = render_print_pdf(
            2,
            2,
            &[255, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 0, 0, 255],
            "1970-01-01 00:00:00, panel.ui",
            "/tmp/panel.ui",
        );
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.starts_with("%PDF-1.4"));
        assert!(text.contains("/MediaBox [0 0 595.28 841.89]"));
        assert!(text.contains("1970-01-01 00:00:00, panel.ui"));
        assert!(text.contains("/tmp/panel.ui"));
        assert!(text.contains(&format!(
            "{:.2} 0 0 {:.2} {:.2} {:.2} cm",
            small.image_w_pt, small.image_h_pt, small.image_x_pt, small.image_y_pt
        )));
    }
}
