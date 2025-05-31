use crate::{
	joypad::{Button, Joypad},
	utils::is_bit_set,
};

// fn display(title: &str, vec: &[u8]) {
// 	print!("{}:", title);
// 	for (i, x) in vec.iter().enumerate() {
// 		if i % 8 == 0 {
// 			println!();
// 		}
// 		print!("{:02X}\t", x);
// 	}
// 	println!();
// }
//
// fn display_cartridge_info(header: &[u8]) {
// 	display("entry point", &header[..4]);
//
// 	display("logo", &header[4..52]);
//
// 	// display("title", &header[52..68]);
//
// 	if header[67].is_ascii() || header[67] == 0x80 {
// 		let title = String::from_utf8(
// 			header[52..68]
// 				.iter()
// 				.take_while(|u| u.is_ascii() && **u != 0)
// 				.map(|u| *u)
// 				.collect::<Vec<_>>(),
// 		)
// 		.unwrap();
//
// 		println!("title: {:?}", title);
// 	} else {
// 		panic!("not implemented");
// 	}
//
// 	if header[75] == 0x33 {
// 		display("new licensee code", &header[68..70]);
// 	}
//
// 	display("SGB flag", &header[70..71]);
//
// 	display("cartridge type", &header[71..72]);
//
// 	display("rom size", &header[72..73]);
//
// 	display("ram size", &header[73..74]);
//
// 	display("destination code", &header[74..75]);
//
// 	display("old licensee code", &header[75..76]);
//
// 	display("mask rom version number", &header[76..77]);
//
// 	display("header checksum", &header[77..78]);
//
// 	display("global checksum", &header[78..80]);
// }

pub struct MMU {
	memory: [u8; 0x10000],
	div_counter: u16,
	prev_and_result: bool,
	dma_cycles_counter: u16,
	joypad: Joypad,
}

impl MMU {
	pub fn new(cartridge: &[u8]) -> Self {
		let mut memory = [0_u8; 0x10000];
		memory[0x0000..0x8000].copy_from_slice(&cartridge[0x0000..0x8000]);
		memory[0xFF00] = 0xCF;
		memory[0xFF02] = 0x7E;
		memory[0xFF04] = 0xAB;
		memory[0xFF07] = 0xF8;
		memory[0xFF0F] = 0xE1;
		memory[0xFF10] = 0x80;
		memory[0xFF11] = 0xBF;
		memory[0xFF12] = 0xF3;
		memory[0xFF13] = 0xFF;
		memory[0xFF14] = 0xBF;
		memory[0xFF16] = 0x3F;
		memory[0xFF18] = 0xFF;
		memory[0xFF19] = 0xBF;
		memory[0xFF1A] = 0x7F;
		memory[0xFF1B] = 0xFF;
		memory[0xFF1C] = 0x9F;
		memory[0xFF1D] = 0xFF;
		memory[0xFF1E] = 0xBF;
		memory[0xFF20] = 0xFF;
		memory[0xFF23] = 0xBF;
		memory[0xFF24] = 0x77;
		memory[0xFF25] = 0xF3;
		memory[0xFF26] = 0xF1;
		memory[0xFF40] = 0x91;
		memory[0xFF41] = 0x85;
		memory[0xFF46] = 0xFF;
		memory[0xFF47] = 0xFC;

		MMU {
			memory: memory,
			div_counter: 0xABCC,
			prev_and_result: false,
			dma_cycles_counter: 0,
			joypad: Joypad::new(),
		}
	}

	pub fn read_byte(&self, address: u16) -> u8 {
		match address {
			0xA000..0xC000 => 0x00, // reads not allowed on external ram
			0xE000..0xFE00 => self.memory[address as usize - 0x2000],
			0xFEA0..0xFF00 => 0x00, // reads not allowed on unusable region
			0xFF00 => self.joypad.read(self.memory[0xFF00]),
			0xFF04 => (self.div_counter >> 8) as u8,
			a => self.memory[a as usize],
		}
	}

	pub fn write_byte(&mut self, address: u16, value: u8) {
		if address == 0xFF46 {
			self.dma_cycles_counter = 0x0280;
		}

		match address {
			0x0000..0x8000 => {} // writes not allowed on rom
			0xA000..0xC000 => {} // writes not allowed on external ram
			0xE000..0xFE00 => self.memory[address as usize - 0x2000] = value,
			0xFEA0..0xFF00 => {} // writes not allowed on unusable region
			0xFF00 => {
				self.memory[address as usize] = (self.memory[address as usize] & 0xCF) | (value & 0x30)
			}
			0xFF04 => self.div_counter = 0,
			_ => self.memory[address as usize] = value,
		};
	}

	pub fn press_key(&mut self, button: Button) {
		if self.joypad.pressed(button) && (self.memory[0xFF00] >> 4) & 0x03 < 0x03 {
			self.request_interrupt(4);
		}
	}

	pub fn release_key(&mut self, button: Button) {
		self.joypad.released(button);
	}

	pub fn request_interrupt(&mut self, bit: u8) {
		if bit > 4 {
			unreachable!();
		}
		let if_reg = self.read_byte(0xFF0F);
		self.write_byte(0xFF0F, if_reg | (1 << bit));
	}

	pub fn update_timers(&mut self, cycles: u16) {
		if self.dma_cycles_counter > 0 {
			self.dma_cycles_counter = self.dma_cycles_counter.saturating_sub(cycles);
			if self.dma_cycles_counter == 0 {
				let x = (self.memory[0xFF46] as usize) << 8;
				self.memory.copy_within(x..(x + 0xA0), 0xFE00);
			}
		}

		self.div_counter = self.div_counter.wrapping_add(cycles);

		let tac = self.read_byte(0xFF07);
		let timer_enabled = is_bit_set(tac, 2);
		// Explanation: https://github.com/Hacktix/GBEDG/blob/master/timers/index.md
		let div_counter_bit = match tac & 0x03 {
			0x00 => 9, // 1024 cycle @ 4 MHz ~ 1 cycle @   4 KHz
			0x01 => 3, //   16 cycle @ 4 MHz ~ 1 cycle @ 256 KHz
			0x02 => 5, //   64 cycle @ 4 MHz ~ 1 cycle @  64 KHz
			0x03 => 7, //  256 cycle @ 4 MHz ~ 1 cycle @  16 KHz
			_ => unreachable!(),
		};

		let curr_div_bit_value = (self.div_counter >> div_counter_bit) & 0x01 == 0x01;
		let curr_and_result = curr_div_bit_value & timer_enabled;

		if self.prev_and_result && !curr_and_result {
			let mut tima = self.read_byte(0xFF05).wrapping_add(1);
			if tima == 0x00 {
				tima = self.read_byte(0xFF06);
				self.request_interrupt(2);
			}
			self.write_byte(0xFF05, tima);
		}

		self.prev_and_result = curr_and_result;
	}
}
