use crate::{HEIGHT, WIDTH, mmu::MMU, utils::is_bit_set};
use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Modes {
	HBLANK = 0,
	VBLANK = 1,
	OAMSCAN = 2,
	RENDER = 3,
}

//				Colors				:		Calc	|	DMG-NSO	|	2B-GRAY | HOLLOW
// Color 0 (White)			: #FFFFFF | #8cad28 | #ffffff | #fafbf6
// Color 1 (Light Gray)	: #AAAAAA | #6c9421 | #b6b6b6 | #c6b7be
// Color 2 (Dark Gray)	: #555555 | #426b29 | #676767 | #565a75
// Color 3 (Black):			: #000000 | #214231 | #000000 | #0f0f1b
enum Color {
	White = 0x00fafbf6,
	LightGray = 0x00c6b7be,
	DarkGray = 0x00565a75,
	Black = 0x000f0f1b,
}

impl From<u8> for Modes {
	fn from(value: u8) -> Self {
		match value {
			0 => Modes::HBLANK,
			1 => Modes::VBLANK,
			2 => Modes::OAMSCAN,
			3 => Modes::RENDER,
			_ => unreachable!(),
		}
	}
}

#[derive(Debug)]
struct SpriteFifoData {
	color: u8,
	palette_address: u16,
	bg_obj_priority_flag: bool,
}

#[derive(Debug)]
pub struct PPU {
	frame_buffer: [u32; WIDTH * HEIGHT],
	frame_ready: bool,
	background_fifo: VecDeque<u8>,
	sprite_fifo: VecDeque<SpriteFifoData>,
	sprite_buffer: VecDeque<u16>,
	interrupt_triggered: bool,
	cycles_waste: u16,
	cycles_spent: u16,
	mode: Modes,
	wy: u8,
	wx: u8,
	ly: u8,
	lx: u8,
	w_present: bool,
	w_ly: u8,
	w_lx: u8,
}

impl PPU {
	// PPU Hardware Registers
	// 7 - PPU Enable,  6 - Window Tile Map, 5 - Window Enable, 4 - BG & Window Tiles
	// 3 - BG Tile Map, 2 - OBJ Size,        1 - OBJ Enable,    0 - BG & Window Enable
	const LCDC: u16 = 0xFF40; // 0x91 = 1001 0001
	// 7 - 1, 6 - LYC, 5 - Mode 2, 4 - Mode 1, 3 - Mode 0, 2 - LYC == LY, 1 & 0 - PPU Mode
	const STAT: u16 = 0xFF41;
	const SCY: u16 = 0xFF42;
	const SCX: u16 = 0xFF43;
	const LY: u16 = 0xFF44;
	const LYC: u16 = 0xFF45;
	// const DMA: u16 = 0xFF46;
	const BGP: u16 = 0xFF47;
	const OBP0: u16 = 0xFF48;
	const OBP1: u16 = 0xFF49;
	const WY: u16 = 0xFF4A;
	const WX: u16 = 0xFF4B;

	const MAX_CYCLES_PER_SCANLINE: u16 = 456;

	fn get_tile_row(a: u8, b: u8) -> [u8; 8] {
		let mut res = [0_u8; 8];
		(0..res.len()).for_each(|bit| {
			res[res.len() - 1 - bit] = match (is_bit_set(b, bit as u8), is_bit_set(a, bit as u8)) {
				(false, false) => 0,
				(false, true) => 1,
				(true, false) => 2,
				(true, true) => 3,
			}
		});
		res
	}

	fn palette_to_color(palette: u8, color_id: u8) -> Color {
		match (palette >> (2 * color_id)) & 3 {
			0 => Color::White,
			1 => Color::LightGray,
			2 => Color::DarkGray,
			3 => Color::Black,
			_ => unreachable!(),
		}
	}

	pub fn new(mmu: &MMU) -> Self {
		Self {
			frame_buffer: [0; WIDTH * HEIGHT],
			frame_ready: false,
			background_fifo: VecDeque::with_capacity(8),
			sprite_fifo: VecDeque::with_capacity(8),
			sprite_buffer: VecDeque::with_capacity(10),
			interrupt_triggered: false,
			cycles_waste: 0,
			cycles_spent: 0,
			mode: Modes::from(mmu.read_byte(Self::STAT) & 0x03),
			wy: mmu.read_byte(Self::WY),
			wx: mmu.read_byte(Self::WX),
			ly: mmu.read_byte(Self::LY),
			lx: 0,
			w_present: false,
			w_ly: 0,
			w_lx: 0,
		}
	}

	pub fn is_frame_ready(&self) -> bool {
		self.frame_ready
	}

	pub fn get_frame_buffer(&self) -> &[u32] {
		&self.frame_buffer
	}

	// PPU Modes - State Machine
	// LY = 0        , C = 0      , Mode = VBLANK  => OAMSCAN
	// LY = 0 - 143  , C = 1 - 79 , Mode = OAMSCAN => OAMSCAN
	// LY = 0 - 143  , C = 80     , Mode = OAMSCAN => RENDER
	// LY = 0 - 143  , C = 81 - x , Mode = RENDER  => RENDER
	// LY = 0 - 143  , C = x      , Mode = RENDER  => HBLANK
	// LY = 0 - 143  , C = x - 455, Mode = HBLANK  => HBLANK
	// LY = 144      , C = 0      , Mode = HBLANK  => VBLANK
	// LY = 144      , C = 1 - 455, Mode = VBLANK  => VBLANK
	// LY = 145 - 153, C = 0 - 455, Mode = VBLANK  => VBLANK
	// x == is rendering complete or not for the scanline (i.e. curr_lx > 160)
	fn update_mode(&mut self, mmu: &mut MMU) {
		let prev_mode = self.mode;
		self.mode = match (self.mode, self.ly, self.cycles_spent) {
			(Modes::VBLANK, 0, 0) => Modes::OAMSCAN,
			(Modes::OAMSCAN, ly, c) if ly < 0x90 && c < 0x50 => Modes::OAMSCAN,
			(Modes::OAMSCAN, ly, 80) if ly < 0x90 => Modes::RENDER,
			(Modes::RENDER, ly, _) if ly < 0x90 => {
				if self.lx < 0xA0 {
					Modes::RENDER
				} else {
					Modes::HBLANK
				}
			}
			(Modes::HBLANK, ly, 0) if ly < 0x90 => Modes::OAMSCAN,
			(Modes::HBLANK, _, 0) => Modes::VBLANK,
			(Modes::HBLANK, ly, _) if ly < 0x90 => Modes::HBLANK,
			(Modes::VBLANK, ly, _) if ly >= 0x90 && ly < 0x9A => Modes::VBLANK,
			_ => unreachable!("{:?}", &self),
		};

		if self.mode == prev_mode {
			return;
		}

		let stat = mmu.read_byte(Self::STAT);
		let x = (stat & 0xFC) | (self.mode as u8);
		mmu.write_byte(Self::STAT, x);

		match self.mode {
			Modes::OAMSCAN => self.cycles_waste += 79,
			Modes::RENDER => self.cycles_waste += 12,
			Modes::VBLANK => {
				self.w_ly = 0;
				self.frame_ready = true;
				mmu.request_interrupt(0);
			}
			_ => {}
		};

		if self.mode != Modes::RENDER
			&& !self.interrupt_triggered
			&& (stat >> (3 + self.mode as u8)) & 0x01 == 0x01
		{
			self.interrupt_triggered = true;
			mmu.request_interrupt(1);
		}
	}

	fn find_object_address(&self, mmu: &MMU) -> Option<u16> {
		self
			.sprite_buffer
			.iter()
			.filter(|address| {
				let obj_x = mmu.read_byte(*address + 1);
				if obj_x <= self.lx + 8 && self.lx < obj_x {
					return true;
				}
				false
			})
			.map(|a| *a)
			.take(1)
			.next()
	}

	fn fill_sprite_fifo(&mut self, mmu: &MMU) {
		let obj_addr = self.find_object_address(mmu);
		if obj_addr.is_none() {
			self.sprite_fifo.push_back(SpriteFifoData {
				color: 0,
				palette_address: Self::OBP0,
				bg_obj_priority_flag: true,
			});
			return;
		}

		self.cycles_waste += 6;
		let obj_addr = obj_addr.unwrap();
		let lcdc = mmu.read_byte(Self::LCDC);
		let obj_enable_flag = is_bit_set(lcdc, 1);
		let obj_size = is_bit_set(lcdc, 2);
		let obj_y = mmu.read_byte(obj_addr);
		let obj_x = mmu.read_byte(obj_addr + 1);
		let obj_tile_index = mmu.read_byte(obj_addr + 2) as u16;
		let obj_attr = mmu.read_byte(obj_addr + 3);

		let bg_obj_priority_flag = is_bit_set(obj_attr, 7);
		let y_flip = is_bit_set(obj_attr, 6);
		let x_flip = is_bit_set(obj_attr, 5);
		let obj_palette_address = match is_bit_set(obj_attr, 4) {
			true => Self::OBP1,
			false => Self::OBP0,
		};
		let obj_tile_data_address = 0x8000
			+ 16
				* match obj_size {
					true => match y_flip ^ (self.ly + 8 < obj_y) {
						true => obj_tile_index & 0xFE,
						false => obj_tile_index | 0x01,
					},
					false => obj_tile_index,
				};

		let mut obj_data_index = (self.ly + 16 - obj_y) as u16 % 8;
		if y_flip {
			obj_data_index = 7 - obj_data_index;
		}
		let obj_data_address = obj_tile_data_address + (obj_data_index * 2);

		let mut pixels = Self::get_tile_row(
			mmu.read_byte(obj_data_address),
			mmu.read_byte(obj_data_address + 1),
		);

		if x_flip {
			pixels.reverse();
		}

		((self.lx + 8 - obj_x)..8).for_each(|idx| {
			self.sprite_fifo.push_back(SpriteFifoData {
				color: if obj_enable_flag {
					pixels[idx as usize]
				} else {
					0
				},
				palette_address: obj_palette_address,
				bg_obj_priority_flag: bg_obj_priority_flag,
			});
		});
	}

	fn fill_background_fifo(&mut self, mmu: &MMU) {
		let scy = mmu.read_byte(Self::SCY);
		let scx = mmu.read_byte(Self::SCX);
		let lcdc = mmu.read_byte(Self::LCDC);
		let bg_enable = is_bit_set(lcdc, 0);
		let is_window = is_bit_set(lcdc, 5) && self.ly >= self.wy && self.lx + 7 >= self.wx;
		let tile_map_address = match match is_window {
			true => is_bit_set(lcdc, 6),
			false => is_bit_set(lcdc, 3),
		} {
			true => 0x9C00,
			false => 0x9800,
		};
		let tile_y = match is_window {
			true => self.w_ly,
			false => scy.wrapping_add(self.ly),
		} / 8;
		let tile_x = match is_window {
			true => (self.lx + 7).wrapping_sub(self.wx),
			false => scx.wrapping_add(self.lx),
		} / 8;
		let tile_no = mmu.read_byte(tile_map_address + (32 * tile_y as u16) + tile_x as u16);
		let tile_address = match is_bit_set(lcdc, 4) {
			true => 0x8000 + (16 * (tile_no as u16)),
			false => 0x9000u16.wrapping_add_signed(16 * (tile_no as i8) as i16),
		} + (2
			* (match is_window {
				true => self.w_ly,
				false => scy.wrapping_add(self.ly),
			} % 8) as u16);
		let lb = mmu.read_byte(tile_address);
		let hb = mmu.read_byte(tile_address + 1);
		let pixels = Self::get_tile_row(lb, hb);
		pixels.iter().for_each(|p| {
			self
				.background_fifo
				.push_back(if bg_enable { *p } else { 0 });
		});

		if self.lx == 0 {
			let remaining = match is_window {
				true => 1 + self.wx,
				false => 8 - (scx % 8),
			} as usize;
			while self.background_fifo.len() > remaining {
				self.cycles_waste += 1;
				self.background_fifo.pop_front();
			}
		}
	}

	fn render(&mut self, mmu: &MMU) {
		if !self.w_present
			&& is_bit_set(mmu.read_byte(Self::LCDC), 5)
			&& self.ly >= self.wy
			&& self.lx + 7 >= self.wx
		{
			self.background_fifo.clear();
			self.w_present = true;
			self.cycles_waste += 6;
			self.w_lx = self.lx;
		}

		if self.background_fifo.is_empty() {
			self.fill_background_fifo(mmu);
		}

		if self.sprite_fifo.is_empty() {
			self.fill_sprite_fifo(mmu);
			if self.cycles_waste > 0 {
				if self.lx != 0 && self.background_fifo.len() < 6 {
					self.cycles_waste += 6 - self.background_fifo.len() as u16;
				}
				return;
			}
		}

		let bg_pixel = self.background_fifo.pop_front().unwrap();
		let obj_data = self.sprite_fifo.pop_front().unwrap();
		let color = match obj_data.color == 0 || (obj_data.bg_obj_priority_flag && bg_pixel > 0) {
			true => Self::palette_to_color(mmu.read_byte(Self::BGP), bg_pixel),
			false => Self::palette_to_color(mmu.read_byte(obj_data.palette_address), obj_data.color),
		};
		self.frame_buffer[self.ly as usize * WIDTH + self.lx as usize] = color as u32;
		self.lx += 1;
	}

	fn oamscan(&mut self, mmu: &MMU) {
		let mut address = 0xFE00;
		let obj_size = match is_bit_set(mmu.read_byte(Self::LCDC), 2) {
			true => 16,
			false => 8,
		};

		while self.sprite_buffer.len() < 10 && address < 0xFEA0 {
			let obj_y = mmu.read_byte(address);
			if obj_y <= self.ly + 16 && self.ly + 16 < obj_y + obj_size {
				self.sprite_buffer.push_back(address);
			}
			address += 4;
		}
	}

	fn process(&mut self, mmu: &MMU) {
		if self.cycles_waste > 0 {
			self.cycles_waste -= 1;
			return;
		}

		match self.mode {
			Modes::OAMSCAN => self.oamscan(mmu),
			Modes::RENDER => self.render(mmu),
			_ => {}
		};
	}

	fn setup_for_new_scanline(&mut self, mmu: &mut MMU) {
		self.interrupt_triggered = false;
		self.background_fifo.clear();
		self.sprite_fifo.clear();
		self.sprite_buffer.clear();
		self.w_ly += if self.w_present { 1 } else { 0 };
		self.w_lx = 0;
		self.w_present = false;
		self.wy = mmu.read_byte(Self::WY);
		self.wx = mmu.read_byte(Self::WX);
		let lyc = mmu.read_byte(Self::LYC);
		self.ly = (self.ly + 1) % 0x9A;
		self.lx = 0;

		mmu.write_byte(Self::LY, self.ly);
		if lyc == self.ly {
			self.interrupt_triggered = true;
			mmu.request_interrupt(1);
		}
	}

	pub fn tick(&mut self, mmu: &mut MMU) {
		if self.frame_ready {
			self.frame_ready = false;
		}
		self.update_mode(mmu);
		self.process(mmu);

		self.cycles_spent = (self.cycles_spent + 1) % Self::MAX_CYCLES_PER_SCANLINE;
		if self.cycles_spent == 0 {
			self.setup_for_new_scanline(mmu);
		}
	}
}
