use crate::{
    arch::rvm::flags,
    device::{PciDevice, PCI_DEVICES},
    printk, printlnk,
    ram::{glacier::GLACIER, PAGE_4KIB}
};

use spin::Mutex;

#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
pub struct Colour {
    pub alpha: u8,
    pub red: u8,
    pub green: u8,
    pub blue: u8
}

impl Colour {
    pub const fn new(red: u8, green: u8, blue: u8) -> Self { Self { alpha: 0xff, red, green, blue } }
    pub const fn rgba(red: u8, green: u8, blue: u8, alpha: u8) -> Self { Self { alpha, red, green, blue } }
    pub const BLACK: Self   = Self::new(0x00, 0x00, 0x00);
    pub const WHITE: Self   = Self::new(0xff, 0xff, 0xff);
    pub const RED: Self     = Self::new(0xff, 0x00, 0x00);
    pub const GREEN: Self   = Self::new(0x00, 0xff, 0x00);
    pub const BLUE: Self    = Self::new(0x00, 0x00, 0xff);
    pub const YELLOW: Self  = Self::new(0xff, 0xff, 0x00);
    pub const CYAN: Self    = Self::new(0x00, 0xff, 0xff);
    pub const MAGENTA: Self = Self::new(0xff, 0x00, 0xff);
}

impl From<u32> for Colour {
    fn from(value: u32) -> Self {
        Self {
            alpha: ((value >> 24) & 0xff) as u8,
            red: ((value >> 16) & 0xff) as u8,
            green: ((value >> 8) & 0xff) as u8,
            blue: (value & 0xff) as u8
        }
    }
}

impl From<Colour> for u32 {
    fn from(colour: Colour) -> Self {
        u32::from_be_bytes(unsafe { core::mem::transmute(colour) })
    }
}

pub struct Vga {
    framebuffer: *mut u32,
    edid: *mut u8,
    width: u32,
    height: u32,
    pitch: u32
}

impl Vga {
    const EDID_HEADER: [u8; 8] = [0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00];

    pub fn new(dev: &PciDevice) -> Option<Self> {
        if !dev.is_vga() { return None; }

        let mut fb_addr = dev.bar(0).unwrap() as usize;
        if (fb_addr & 0x6) == 0x4 {
            fb_addr |= (dev.bar(1).unwrap() as usize) << 32;
        }
        fb_addr &= !0xf; // 16 byte alignment

        let mut edid_addr = dev.bar(2).unwrap() as usize;
        if (edid_addr & 0x6) == 0x4 {
            edid_addr |= (dev.bar(3).unwrap() as usize) << 32;
        }
        edid_addr &= !0xf; // 16 byte alignment

        GLACIER.write().map_range(edid_addr, edid_addr, PAGE_4KIB, flags::D_RW);
        let edid_regs = unsafe {
            core::slice::from_raw_parts(edid_addr as *mut u8, PAGE_4KIB)
        };

        if &edid_regs[0..8] != Self::EDID_HEADER { return None; }

        let timing_desc = &edid_regs[54..72];
        let width = timing_desc[2] as u32 | ((timing_desc[4] as u32 & 0xf0) << 4);
        let height = timing_desc[5] as u32 | ((timing_desc[7] as u32 & 0xf0) << 4);
        let width_blanking = timing_desc[3] as u32 | ((timing_desc[4] as u32 & 0x0f) << 8);
        let height_blanking = timing_desc[6] as u32 | ((timing_desc[7] as u32 & 0x0f) << 8);
        let pitch = width * 4;

        let map_size = width as usize * height as usize * pitch as usize;
        GLACIER.write().map_range(fb_addr, fb_addr, map_size, flags::D_RW);
        return Some(Vga {
            framebuffer: fb_addr as *mut u32,
            edid: edid_addr as *mut u8,
            width, height, pitch
        });
    }

    pub fn framebuffer(&self) -> *mut u32 { self.framebuffer }
    pub fn edid(&self) -> *mut u8 { self.edid }
    pub fn width(&self) -> u32 { self.width }
    pub fn height(&self) -> u32 { self.height }
    pub fn pitch(&self) -> u32 { self.pitch }

    pub fn set_pixel(&self, x: u32, y: u32, colour: Colour) {
        if x >= self.width() || y >= self.height() { return; }

        unsafe {
            let offset = (y * self.width() + x) as usize;
            let addr = self.framebuffer().add(offset);
            *addr = colour.into();
        }
    }

    pub fn get_pixel(&self, x: u32, y: u32) -> Colour {
        if x >= self.width() || y >= self.height() { return Colour::BLACK; }


        let offset = (y * self.width() + x) as usize;
        let addr = unsafe { self.framebuffer.add(offset) };
        return unsafe { (*addr).into() };
    }

    pub fn fill_screen(&self, colour: Colour) {
        for y in 0..self.height() {
            for x in 0..self.width() {
                self.set_pixel(x, y, colour);
            }
        }
    }

    pub fn draw_rect(&self, x: u32, y: u32, width: u32, height: u32, colour: Colour) {
        for dy in 0..height {
            for dx in 0..width {
                self.set_pixel(x + dx, y + dy, colour);
            }
        }
    }

    pub fn draw_line(&self, x0: u32, y0: u32, x1: u32, y1: u32, colour: Colour) {
        // Bresenham's line algorithm
        let dx = (x1 as i32 - x0 as i32).abs();
        let dy = -(y1 as i32 - y0 as i32).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        let mut x = x0 as i32;
        let mut y = y0 as i32;

        loop {
            self.set_pixel(x as u32, y as u32, colour);
            if x == x1 as i32 && y == y1 as i32 { break; }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    pub fn edid_regs(&self) -> &[u8] {
        unsafe { core::slice::from_raw_parts(self.edid(), 0x1000) }
    }

    pub fn print_edid_info(&self) {
        let edid = self.edid_regs();

        printlnk!("=== EDID Info ===");

        let manufacturer_id = u16::from_be_bytes([edid[8], edid[9]]);
        let c1 = (((manufacturer_id >> 10) & 0x1f) + b'A' as u16 - 1) as u8 as char;
        let c2 = (((manufacturer_id >> 5) & 0x1f) + b'A' as u16 - 1) as u8 as char;
        let c3 = ((manufacturer_id & 0x1f) + b'A' as u16 - 1) as u8 as char;
        printlnk!("Manufacturer: {}{}{}", c1, c2, c3);

        let product_code = u16::from_le_bytes([edid[10], edid[11]]);
        printlnk!("Product Code: {:#06x}", product_code);

        let year = 1990 + edid[17] as u16;
        printlnk!("Y: {}", year);

        printlnk!("EDID Version: {}.{}", edid[18], edid[19]);
        printlnk!("Resolution: {}x{}", self.width(), self.height());

        printlnk!("RAW EDID:");
        for (i, line) in edid[0..0x80].chunks(16).enumerate() {
            printk!("{:#06x}:", i * 16);
            for byte in line { printk!(" {:02x}", byte); }
            printlnk!();
        }
    }

    pub fn test_pattern(&self) {
        let colors = [
            Colour::WHITE, Colour::YELLOW, Colour::CYAN, Colour::GREEN,
            Colour::MAGENTA, Colour::RED, Colour::BLUE, Colour::BLACK
        ];

        let bar_width = self.width() / colors.len() as u32;

        for (i, &color) in colors.iter().enumerate() {
            let x_start = i as u32 * bar_width;
            let x_end = if i == colors.len() - 1 { self.width } else { (i + 1) as u32 * bar_width };

            for x in x_start..x_end {
                for y in 0..self.height() {
                    self.set_pixel(x, y, color);
                }
            }
        }
    }
}

unsafe impl Send for Vga {}
unsafe impl Sync for Vga {}

pub static VGA_DEVICE: Mutex<Option<Vga>> = Mutex::new(None);

pub fn init_vga() {
    for dev in PCI_DEVICES.lock().iter() {
        if dev.is_vga() {
            let vga = match Vga::new(dev) {
                Some(vga) => vga,
                None => { continue; }
            };
            vga.fill_screen(Colour::WHITE);
            vga.test_pattern();
            *VGA_DEVICE.lock() = Some(vga);
        }
    }
}

pub fn set_pixel(x: u32, y: u32, colour: Colour) {
    if let Some(ref vga) = *VGA_DEVICE.lock() {
        vga.set_pixel(x, y, colour)
    }
}

pub fn get_pixel(x: u32, y: u32) -> Colour {
    if let Some(ref vga) = *VGA_DEVICE.lock() {
        return vga.get_pixel(x, y);
    }
    Colour::BLACK
}

pub fn fill_screen(colour: Colour) {
    if let Some(ref vga) = *VGA_DEVICE.lock() {
        vga.fill_screen(colour);
    }
}

pub fn draw_rect(x: u32, y: u32, width: u32, height: u32, colour: Colour) {
    if let Some(ref vga) = *VGA_DEVICE.lock() {
        vga.draw_rect(x, y, width, height, colour)
    }
}
