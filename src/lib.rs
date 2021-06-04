#![no_std]

use embedded_hal::blocking::delay::{DelayMs, DelayUs};
use embedded_hal::digital::v2::OutputPin;

use core::iter::once;
use display_interface::DataFormat::{U16BEIter, U8Iter};
use display_interface::WriteOnlyDataCommand;

#[cfg(feature = "graphics")]
mod graphics;

#[cfg(feature = "graphics-core")]
mod graphics_core;

pub use embedded_hal::spi::MODE_0 as SPI_MODE;

pub use display_interface::DisplayError;

type Result<T = (), E = DisplayError> = core::result::Result<T, E>;

/// Trait that defines display size information
pub trait DisplaySize {
    /// Width in pixels
    const WIDTH: usize;
    /// Height in pixels
    const HEIGHT: usize;
}

/// Generic display size of 240x320 pixels
pub struct DisplaySize240x320;

impl DisplaySize for DisplaySize240x320 {
    const WIDTH: usize = 240;
    const HEIGHT: usize = 320;
}

/// Generic display size of 320x480 pixels
pub struct DisplaySize320x480;

impl DisplaySize for DisplaySize320x480 {
    const WIDTH: usize = 320;
    const HEIGHT: usize = 480;
}

/// The default orientation is Portrait
#[derive(Eq, PartialEq, Copy, Clone)]
pub enum Orientation {
    Portrait,
    Landscape,
}

impl Default for Orientation {
    fn default() -> Self {
        Orientation::Portrait
    }
}

#[derive(Eq, PartialEq, Copy, Clone)]
pub enum Flip {
    No,
    FlipHorizontal,
    FlipVertical,
    Rotate180,
}

impl Default for Flip {
    fn default() -> Self {
        Flip::No
    }
}

#[derive(Default, Copy, Clone)]
pub struct DisplayMode {
    pub orientation: Orientation,
    pub flip: Flip,
    pub inverted_rgb: bool,
}

const BGR_PIXEL_ORDER: u8 = 1 << 3;
const ROW_COLUMN_EXCHANGE: u8 = 1 << 5;
const COLUMN_ORDER_SWAP: u8 = 1 << 6;
const ROW_ORDER_SWAP: u8 = 1 << 7;

impl From<DisplayMode> for u8 {
    fn from(mode: DisplayMode) -> Self {
        let mut mode_code = 0;
        if mode.orientation == Orientation::Landscape {
            mode_code |= ROW_COLUMN_EXCHANGE;
        }
        if mode.inverted_rgb {
            mode_code |= BGR_PIXEL_ORDER;
        }
        match mode.flip {
            Flip::No => {}
            Flip::FlipHorizontal => mode_code |= if mode.orientation == Orientation::Portrait { COLUMN_ORDER_SWAP } else { ROW_ORDER_SWAP },
            Flip::FlipVertical => mode_code |= if mode.orientation == Orientation::Landscape { COLUMN_ORDER_SWAP } else { ROW_ORDER_SWAP },
            Flip::Rotate180 => mode_code |= COLUMN_ORDER_SWAP | ROW_ORDER_SWAP,
        }
        mode_code
    }
}

/// There are two method for drawing to the screen:
/// [Ili9341::draw_raw_iter] and [Ili9341::draw_raw_slice]
///
/// In both cases the expected pixel format is rgb565.
///
/// The hardware makes it efficient to draw rectangles on the screen.
///
/// What happens is the following:
///
/// - A drawing window is prepared (with the 2 opposite corner coordinates)
/// - The starting point for drawint is the top left corner of this window
/// - Every pair of bytes received is intepreted as a pixel value in rgb565
/// - As soon as a pixel is received, an internal counter is incremented,
///   and the next word will fill the next pixel (the adjacent on the right, or
///   the first of the next row if the row ended)
pub struct ILI9486<IFACE, RESET> {
    interface: IFACE,
    reset: RESET,
    width: usize,
    height: usize,
    mode: DisplayMode,
}

impl<IFACE, RESET> ILI9486<IFACE, RESET>
    where
        IFACE: WriteOnlyDataCommand,
        RESET: OutputPin,
{
    fn reset(&mut self, delay: &mut impl DelayUs<u32>) -> Result<(), DisplayError> {
        // Do hardware reset by holding reset low for at least 10us and then releasing it
        self.reset.set_low().map_err(|_| DisplayError::RSError)?;
        delay.delay_us(150);
        self.reset.set_high().map_err(|_| DisplayError::RSError)?;
        delay.delay_us(150);
        Ok(())
    }

    pub fn new<DELAY, SIZE>(
        interface: IFACE,
        reset: RESET,
        delay: &mut DELAY,
        display_mode: DisplayMode,
        _display_size: SIZE,
    ) -> Result<Self>
        where
            DELAY: DelayUs<u32>,
            SIZE: DisplaySize,
    {
        let mut ili9486 = ILI9486 {
            interface,
            reset,
            width: SIZE::WIDTH,
            height: SIZE::HEIGHT,
            mode: DisplayMode::default(),
        };

        // for _ in 0..3 {
        //     ili9486.reset(delay)?
        // }
        ili9486.reset(delay)?;
        ili9486.command(Command::InterfaceModeControl, &[0x00]).unwrap();
        ili9486.command(Command::SleepOut, &[]).unwrap();
        delay.delay_us(120);

        ili9486.command(Command::PixelFormatSet, &[0x55])?;
        ili9486.command(Command::DisplayInversionOff, &[])?;

        ili9486.command(Command::PowerControl1, &[0x09, 0x09])?;
        ili9486.command(Command::PowerControl2, &[0x41, 0x00])?;
        ili9486.command(Command::PowerControl3, &[0x33])?;
        ili9486.command(Command::PowerControl4, &[0x00, 0x36])?;

        ili9486.set_display_mode(display_mode)?;
        ili9486.command(Command::DigitalGammaControl1, &[0x00, 0x2C, 0x2C, 0x0B, 0x0C, 0x04, 0x4C, 0x64, 0x36, 0x03, 0x0E, 0x01, 0x10, 0x01, 0x00])?;
        ili9486.command(Command::DigitalGammaControl2, &[0x0F, 0x37, 0x37, 0x0C, 0x0F, 0x05, 0x50, 0x32, 0x36, 0x04, 0x0B, 0x00, 0x19, 0x14, 0x0F])?;
        ili9486.command(Command::DisplayFunctionControl, &[0, /*ISC=2*/2, /*Display Height h=*/59])?;

        ili9486.command(Command::SleepOut, &[])?;
        delay.delay_us(120);

        ili9486.command(Command::DisplayOn, &[])?;
        ili9486.command(Command::IdleModeOff, &[])?;
        ili9486.command(Command::NormalDisplayMode, &[])?;

        Ok(ili9486)
    }
}


impl<IFACE, RESET> ILI9486<IFACE, RESET>
    where
        IFACE: WriteOnlyDataCommand,
{
    fn command(&mut self, cmd: Command, args: &[u8]) -> Result {
        self.interface.send_commands(U8Iter(&mut once(cmd as u8)))?;
        self.interface.send_data(U8Iter(&mut args.iter().cloned()))
    }

    fn write_iter<I: IntoIterator<Item=u16>>(&mut self, data: I) -> Result {
        self.command(Command::MemoryWrite, &[])?;
        self.interface.send_data(U16BEIter(&mut data.into_iter()))
    }

    fn set_window(&mut self, x0: u16, y0: u16, x1: u16, y1: u16) -> Result {
        self.command(
            Command::ColumnAddressSet,
            &[
                (x0 >> 8) as u8,
                (x0 & 0xff) as u8,
                (x1 >> 8) as u8,
                (x1 & 0xff) as u8,
            ],
        )?;
        self.command(
            Command::PageAddressSet,
            &[
                (y0 >> 8) as u8,
                (y0 & 0xff) as u8,
                (y1 >> 8) as u8,
                (y1 & 0xff) as u8,
            ],
        )
    }

    /// Configures the screen for hardware-accelerated vertical scrolling.
    pub fn configure_vertical_scroll(
        &mut self,
        fixed_top_lines: u16,
        fixed_bottom_lines: u16,
    ) -> Result<Scroller> {
        let height = match self.mode.orientation {
            Orientation::Landscape => self.width,
            Orientation::Portrait => self.height,
        } as u16;
        let scroll_lines = height as u16 - fixed_top_lines - fixed_bottom_lines;

        self.command(
            Command::VerticalScrollDefine,
            &[
                (fixed_top_lines >> 8) as u8,
                (fixed_top_lines & 0xff) as u8,
                (scroll_lines >> 8) as u8,
                (scroll_lines & 0xff) as u8,
                (fixed_bottom_lines >> 8) as u8,
                (fixed_bottom_lines & 0xff) as u8,
            ],
        )?;

        Ok(Scroller::new(fixed_top_lines, fixed_bottom_lines, height))
    }

    pub fn scroll_vertically(&mut self, scroller: &mut Scroller, num_lines: u16) -> Result {
        scroller.top_offset += num_lines;
        if scroller.top_offset > (scroller.height - scroller.fixed_bottom_lines) {
            scroller.top_offset = scroller.fixed_top_lines
                + (scroller.top_offset + scroller.fixed_bottom_lines - scroller.height)
        }

        self.command(
            Command::VerticalScrollAddr,
            &[
                (scroller.top_offset >> 8) as u8,
                (scroller.top_offset & 0xff) as u8,
            ],
        )
    }

    /// Draw a rectangle on the screen, represented by top-left corner (x0, y0)
    /// and bottom-right corner (x1, y1).
    ///
    /// The border is included.
    ///
    /// This method accepts an iterator of rgb565 pixel values.
    ///
    /// The iterator is useful to avoid wasting memory by holding a buffer for
    /// the whole screen when it is not necessary.
    pub fn draw_raw_iter<I: IntoIterator<Item=u16>>(
        &mut self,
        x0: u16,
        y0: u16,
        x1: u16,
        y1: u16,
        data: I,
    ) -> Result {
        self.set_window(x0, y0, x1, y1)?;
        self.write_iter(data)
    }

    /// Draw a rectangle on the screen, represented by top-left corner (x0, y0)
    /// and bottom-right corner (x1, y1).
    ///
    /// The border is included.
    ///
    /// This method accepts a raw buffer of words that will be copied to the screen
    /// video memory.
    ///
    /// The expected format is rgb565.
    pub fn draw_raw_slice(&mut self, x0: u16, y0: u16, x1: u16, y1: u16, data: &[u16]) -> Result {
        self.draw_raw_iter(x0, y0, x1, y1, data.iter().copied())
    }

    /// Change the orientation of the screen
    pub fn set_display_mode(&mut self, mode: DisplayMode) -> Result {
        if self.mode.orientation != mode.orientation {
            core::mem::swap(&mut self.height, &mut self.width)
        };
        self.mode = mode;
        self.command(Command::MemoryAccessControl, &[mode.into()])?;
        Ok(())
    }
}

impl<IFACE, RESET> ILI9486<IFACE, RESET> {
    /// Get the current screen width. It can change based on the current orientation
    pub fn width(&self) -> usize {
        self.width
    }

    /// Get the current screen heighth. It can change based on the current orientation
    pub fn height(&self) -> usize {
        self.height
    }
}

/// Scroller must be provided in order to scroll the screen. It can only be obtained
/// by configuring the screen for scrolling.
pub struct Scroller {
    top_offset: u16,
    fixed_bottom_lines: u16,
    fixed_top_lines: u16,
    height: u16,
}

impl Scroller {
    fn new(fixed_top_lines: u16, fixed_bottom_lines: u16, height: u16) -> Scroller {
        Scroller {
            top_offset: fixed_top_lines,
            fixed_top_lines,
            fixed_bottom_lines,
            height,
        }
    }
}

#[derive(Clone, Copy)]
enum Command {
    Nop = 0x00,
    SoftwareReset = 0x01,
    ReadDisplayId = 0x04,
    ReadErrors = 0x05,
    ReadDisplayStatus = 0x09,
    ReadDisplayPowerMode = 0x0a,
    ReadDisplayMADCTL = 0x0b,
    ReadDisplayPixelFormat = 0x0c,
    ReadDisplayImageMode = 0x0d,
    ReadDisplaySignalMode = 0x0e,
    ReadDisplaySelfDiagResult = 0x0f,
    SleepIn = 0x10,
    SleepOut = 0x11,
    PartialModeOn = 0x12,
    NormalDisplayMode = 0x13,
    DisplayInversionOff = 0x20,
    DisplayInversionOn = 0x21,
    DisplayOff = 0x28,
    DisplayOn = 0x29,
    ColumnAddressSet = 0x2a,
    PageAddressSet = 0x2b,
    MemoryWrite = 0x2c,
    MemoryRead = 0x2e,
    PartialArea = 0x30,
    VerticalScrollDefine = 0x33,
    TearingEffectLineOff = 0x34,
    TearingEffectLineOn = 0x35,
    MemoryAccessControl = 0x36,
    VerticalScrollAddr = 0x37,
    IdleModeOff = 0x38,
    IdleModeOn = 0x39,
    PixelFormatSet = 0x3a,
    MemoryWriteContinue = 0x3c,
    MemoryReadContinue = 0x3e,
    WriteTearScanLine = 0x44,
    ReadTearScanLine = 0x45,
    WriteDisplayBrightnessValue = 0x51,
    ReadDisplayBrigthnessValue = 0x52,
    WriteCTRLDisplayValue = 0x53,
    ReadCTRLDisplayValue = 0x54,
    WriteCABrigthnessControl = 0x55,
    ReadCABrigthnessControl = 0x56,
    WriteCABCMinBrigthness = 0x5e,
    ReadCABCMinBrigthness = 0x5f,
    ReadFirstChecksum = 0xaa,
    ReadContinueChecksum = 0xab,
    ReadID1 = 0xda,
    ReadID2 = 0xdb,
    ReadID3 = 0xdc,
    InterfaceModeControl = 0xb0,
    FrameRateControlNormal = 0xb1,
    FrameRateControlIdle = 0xb2,
    FrameRateControlPartial = 0xb3,
    DisplayInversionControl = 0xb4,
    BlankingPorchControl = 0xb5,
    DisplayFunctionControl = 0xb6,
    EntryModeSet = 0xb7,
    PowerControl1 = 0xc0,
    PowerControl2 = 0xc1,
    PowerControl3 = 0xc2,
    PowerControl4 = 0xc3,
    PowerControl5 = 0xc4,
    VCOMControl = 0xc5,
    CABCControl9 = 0xc6,
    CABCControl1 = 0xc8,
    CABCControl2 = 0xc9,
    CABCControl3 = 0xca,
    CABCControl4 = 0xcb,
    CABCControl5 = 0xcc,
    CABCControl6 = 0xcd,
    CABCControl7 = 0xce,
    CABCControl8 = 0xcf,
    NVMemoryWrite = 0xd0,
    NVMemoryProtectionKey = 0xd1,
    NVMemoryStatusRead = 0xd2,
    ReadID4 = 0xd3,
    PGAMCTRL = 0xe0,
    NGAMCTRL = 0xe1,
    DigitalGammaControl1 = 0xe2,
    DigitalGammaControl2 = 0xe3,
    SPIReadCommandSetting = 0xfb,
}
