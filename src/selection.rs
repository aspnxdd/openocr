use winit::dpi::PhysicalPosition;

#[derive(Debug)]
pub struct Selection {
    pub start: Option<PhysicalPosition<f64>>,
    pub end: Option<PhysicalPosition<f64>>,
    pub is_selecting: bool,
}

impl Selection {
    pub fn new() -> Self {
        Self {
            start: None,
            end: None,
            is_selecting: false,
        }
    }

    pub fn rect(&self) -> Option<(u32, u32, u32, u32)> {
        match (self.start, self.end) {
            (Some(s), Some(e)) => {
                let x = s.x.min(e.x) as u32;
                let y = s.y.min(e.y) as u32;
                let w = (s.x - e.x).abs() as u32;
                let h = (s.y - e.y).abs() as u32;
                if w == 0 || h == 0 {
                    return None;
                }
                Some((x, y, w, h))
            }
            _ => None,
        }
    }
}

pub fn draw(buffer: &mut [u32], width: u32, height: u32, selection: &Selection) {
    let overlay_color = 0x88000000u32;
    let border_color = 0xFFFFFF00u32; // yellow border

    buffer.fill(overlay_color);

    if let Some((sx, sy, sw, sh)) = selection.rect() {
        for y in sy..(sy + sh).min(height) {
            for x in sx..(sx + sw).min(width) {
                buffer[(y * width + x) as usize] = 0x00000000;
            }
        }

        let border = 2u32;
        for y in sy.saturating_sub(border)..=(sy + sh + border).min(height - 1) {
            for x in sx.saturating_sub(border)..=(sx + sw + border).min(width - 1) {
                let on_border = x < sx || x >= sx + sw || y < sy || y >= sy + sh;
                if on_border {
                    buffer[(y * width + x) as usize] = border_color;
                }
            }
        }
    }
}
