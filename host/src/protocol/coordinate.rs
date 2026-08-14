//! Coordinate transformation utilities for mapping client touch / pointer inputs
//! to the host virtual display geometry.

#[derive(Debug, Clone, Copy)]
pub struct CoordinateTransformer {
    pub display_width: u32,
    pub display_height: u32,
}

impl CoordinateTransformer {
    pub fn new(display_width: u32, display_height: u32) -> Self {
        Self {
            display_width: display_width.max(1),
            display_height: display_height.max(1),
        }
    }

    /// Map normalized coordinate [0..65535] to display pixel coordinates (f64, f64)
    pub fn normalized_to_display(&self, norm_x: u16, norm_y: u16) -> (f64, f64) {
        let x = (norm_x as f64 / 65535.0) * (self.display_width as f64);
        let y = (norm_y as f64 / 65535.0) * (self.display_height as f64);
        (x, y)
    }

    /// Map display pixel coordinates to normalized [0..65535]
    pub fn display_to_normalized(&self, px_x: f64, px_y: f64) -> (u16, u16) {
        let norm_x = ((px_x / self.display_width as f64).clamp(0.0, 1.0) * 65535.0) as u16;
        let norm_y = ((px_y / self.display_height as f64).clamp(0.0, 1.0) * 65535.0) as u16;
        (norm_x, norm_y)
    }

    /// Transform coordinates for orientation (0 = 0 deg, 1 = 90 deg, 2 = 180 deg, 3 = 270 deg)
    pub fn transform_orientation(&self, norm_x: u16, norm_y: u16, orientation: u32) -> (u16, u16) {
        match orientation % 4 {
            0 => (norm_x, norm_y),
            1 => (norm_y, 65535 - norm_x),
            2 => (65535 - norm_x, 65535 - norm_y),
            3 => (65535 - norm_y, norm_x),
            _ => (norm_x, norm_y),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_coordinate_mapping() {
        let transformer = CoordinateTransformer::new(1920, 1080);
        let (x, y) = transformer.normalized_to_display(0, 0);
        assert_eq!((x, y), (0.0, 0.0));

        let (x, y) = transformer.normalized_to_display(65535, 65535);
        assert!((x - 1920.0).abs() < 0.1);
        assert!((y - 1080.0).abs() < 0.1);

        let (x, y) = transformer.normalized_to_display(32767, 32767);
        assert!((x - 960.0).abs() < 1.0);
        assert!((y - 540.0).abs() < 1.0);
    }
}
