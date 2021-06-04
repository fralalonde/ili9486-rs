use crate::ILI9486;

use core::iter;

use embedded_graphics::{
    drawable::Pixel,
    geometry::{Point, Size},
    pixelcolor::{
        raw::{RawData, RawU16},
        Rgb565,
    },
    primitives::Rectangle,
    style::{PrimitiveStyle, Styled},
    DrawTarget,
};

impl<IFACE, RESET> DrawTarget<Rgb565> for ILI9486<IFACE, RESET>
where
    IFACE: display_interface::WriteOnlyDataCommand,
{
    type Error = display_interface::DisplayError;

    fn size(&self) -> Size {
        Size::new(self.width as u32, self.height as u32)
    }

    fn draw_pixel(&mut self, pixel: Pixel<Rgb565>) -> Result<(), Self::Error> {
        let Pixel(pos, color) = pixel;

        if pos.x < 0 || pos.y < 0 || pos.x >= self.width as i32 || pos.y >= self.height as i32 {
            return Ok(());
        }

        self.draw_raw_slice(
            pos.x as u16,
            pos.y as u16,
            pos.x as u16,
            pos.y as u16,
            &[RawU16::from(color).into_inner()],
        )
    }

    fn draw_rectangle(
        &mut self,
        item: &Styled<Rectangle, PrimitiveStyle<Rgb565>>,
    ) -> Result<(), Self::Error> {
        let Point { x: x0, y: y0 } = item.primitive.top_left;
        let Point { x: x1, y: y1 } = item.primitive.bottom_right;
        let w = self.width as i32;
        let h = self.height as i32;
        if x0 >= w || y0 >= h {
            return Ok(());
        }
        fn clamp(v: i32, max: i32) -> u16 {
            if v < 0 {
                0
            } else if v > max {
                max as u16
            } else {
                v as u16
            }
        }
        let x0 = clamp(x0, w - 1);
        let y0 = clamp(y0, h - 1);
        let x1 = clamp(x1, w - 1);
        let y1 = clamp(y1, h - 1);
        self.draw_raw_iter(
            x0,
            y0,
            x1,
            y1,
            item.into_iter()
                .filter(|p| {
                    let Point { x, y } = p.0;
                    x >= 0 && y >= 0 && x < w && y < h
                })
                .map(|p| RawU16::from(p.1).into_inner()),
        )
    }

    fn clear(&mut self, color: Rgb565) -> Result<(), Self::Error> {
        let color = RawU16::from(color).into_inner();

        self.draw_raw_iter(
            0,
            0,
            (self.width - 1) as u16,
            (self.height - 1) as u16,
            iter::repeat(color).take(self.width * self.height),
        )
    }
}
