use std::ops::{BitAnd, BitOr, Rem, Shl};

const ROM_SIZE_MAP: [(u8, u16); 12] = [
	(0x00, 2),   //    32 KiB
	(0x01, 4),   //    64 KiB
	(0x02, 8),   //   128 KiB
	(0x03, 16),  //   256 KiB
	(0x04, 32),  //   512 KiB
	(0x05, 64),  //     1 MiB
	(0x06, 128), //     2 MiB
	(0x07, 256), //     4 MiB
	(0x08, 512), //     8 MiB
	(0x52, 72),  // 1.125 MiB
	(0x53, 80),  // 1.250 MiB
	(0x54, 96),  // 1.500 MiB
];
const RAM_SIZE_MAP: [(u8, u8); 6] = [
	(0x00, 0),  //    None
	(0x01, 1),  //   2 KiB
	(0x02, 4),  //   8 KiB
	(0x03, 16), //  32 KiB
	(0x04, 64), // 128 KiB
	(0x05, 32), //  64 KiB
];

pub trait Cartridge {
	fn new(_: Vec<u8>) -> Box<dyn Cartridge>
	where
		Self: Sized;

	fn read_byte(&self, _: u16) -> u8;

	fn write_byte(&mut self, _: u16, _: u8);

	fn get_title(&self) -> String {
		(0x0134..0x0144)
			.map(|a| self.read_byte(a))
			.take_while(|&u| u != 0 && u.is_ascii())
			.map(|u| char::from(u))
			.collect::<String>()
	}

	fn get_total_rom_banks(&self) -> u16 {
		ROM_SIZE_MAP[ROM_SIZE_MAP
			.binary_search_by_key(&self.read_byte(0x0148), |&(a, _)| a)
			.expect("game not supported")]
		.1
	}

	fn get_total_ram_banks(&self) -> u8 {
		RAM_SIZE_MAP[RAM_SIZE_MAP
			.binary_search_by_key(&self.read_byte(0x0149), |&(a, _)| a)
			.expect("game not suppoted")]
		.1
	}
}

struct RomOnly {
	rom_data: Vec<u8>,
}

impl Cartridge for RomOnly {
	fn new(data: Vec<u8>) -> Box<dyn Cartridge> {
		Box::new(RomOnly { rom_data: data })
	}

	fn read_byte(&self, address: u16) -> u8 {
		match address {
			0x0000..0x8000 => self.rom_data[address as usize],
			0xA000..0xC000 => 0xFF,
			_ => unreachable!(),
		}
	}

	fn write_byte(&mut self, _: u16, _: u8) {}
}

// MBC1 Registers:
// - 0000-1FFF: RAM Enable
// - 2000-3FFF: 5 bits of ROM Bank Number
// - 4000-5FFF: RAM Bank Number / upper 2 bits of ROM Bank Number
// - 6000-7FFF: Banking Mode
#[allow(dead_code)]
struct MBC1 {
	banking_mode: bool,
	ram_enable: bool,
	ram_bank_register: u8,
	rom_bank_register: u8,
	ram_data: Vec<u8>,
	rom_data: Vec<u8>,
}

impl Cartridge for MBC1 {
	fn new(data: Vec<u8>) -> Box<dyn Cartridge> {
		let mut c = Box::new(MBC1 {
			banking_mode: false,
			ram_enable: false,
			ram_bank_register: 0x00,
			rom_bank_register: 0x00,
			ram_data: vec![0; 0],
			rom_data: data,
		});
		c.ram_data = vec![0; 0x0800 * c.get_total_ram_banks() as usize];
		c
	}

	fn read_byte(&self, address: u16) -> u8 {
		match address {
			0x0000..0x4000 => match self.banking_mode {
				false => self.rom_data[address as usize],
				true => {
					let rom_bank_number = match self.get_total_rom_banks() {
						0..=32 => 0,
						_ => self
							.rom_bank_register
							.bitand(0x0F)
							.bitor(self.ram_bank_register.bitand(0x03).shl(4) as u8),
					} as usize;
					self.rom_data[0x4000 * rom_bank_number + address as usize]
				}
			},
			0x4000..0x8000 => {
				let rom_bank_number = match self.rom_bank_register {
					0x00 => 0x01,
					val => val.bitand((self.get_total_rom_banks().min(32) - 1) as u8),
				} | match self.get_total_rom_banks() {
					0..=32 => 0x00,
					_ => self.ram_bank_register.bitand(0x03).shl(4) as u8,
				};
				self.rom_data[0x4000 * rom_bank_number as usize + address as usize - 0x4000]
			}
			0xA000..0xC000 if self.ram_enable => {
				let ram_bank_size = 0x2000.min(0x0800 * self.get_total_ram_banks() as usize);
				let ram_bank_number = match self.banking_mode {
					true => self.ram_bank_register.bitand(0x03).shl(2) as usize,
					false => 0,
				};
				self.ram_data[0x0800 * ram_bank_number + (address as usize - 0xA000).rem(ram_bank_size)]
			}
			0xA000..0xC000 => 0xFF,
			_ => unreachable!(),
		}
	}

	fn write_byte(&mut self, address: u16, value: u8) {
		match address {
			0x0000..0x2000 => self.ram_enable = (value & 0x0F) == 0x0A,
			0x2000..0x4000 => self.rom_bank_register = value,
			0x4000..0x6000 => self.ram_bank_register = value,
			0x6000..0x8000 => self.banking_mode = value & 0x01 == 0x01,
			0xA000..0xC000 => {
				if !self.ram_enable {
					return;
				}
				let ram_bank_size = 0x2000.min(0x0800 * self.get_total_ram_banks() as usize);
				let ram_bank_number = match self.banking_mode {
					true => self.ram_bank_register.bitand(0x03).shl(2) as usize,
					false => 0,
				};
				self.ram_data[0x0800 * ram_bank_number + (address as usize - 0xA000).rem(ram_bank_size)] =
					value;
			}
			_ => unreachable!(),
		}
	}
}

// MBC3 Registers:
// - 0000-1FFF: RAM Enable
// - 2000-3FFF: 7 bits of ROM Bank Number
// - 4000-5FFF: RAM Bank Number
struct MBC3 {
	ram_enable: bool,
	ram_bank_register: u8,
	rom_bank_register: u8,
	ram_data: Vec<u8>,
	rom_data: Vec<u8>,
}

impl Cartridge for MBC3 {
	fn new(data: Vec<u8>) -> Box<dyn Cartridge> {
		let mut c = Box::new(MBC3 {
			ram_enable: false,
			ram_bank_register: 0x00,
			rom_bank_register: 0x00,
			ram_data: vec![0; 0],
			rom_data: data,
		});
		c.ram_data = vec![0; 0x0800 * c.get_total_ram_banks() as usize];
		c
	}

	fn read_byte(&self, address: u16) -> u8 {
		match address {
			0x0000..0x4000 => self.rom_data[address as usize],
			0x4000..0x8000 => {
				let rom_bank_number = match self.rom_bank_register.bitand(0x07) {
					0x00 => 0x01,
					val => val,
				} as usize;
				self.rom_data[0x4000 * rom_bank_number + address as usize - 0x4000]
			}
			0xA000..0xC000 if self.ram_enable => {
				let ram_bank_number = match self.ram_bank_register.bitand(0x0F) {
					val if val < 0x04 => val,
					_ => unimplemented!("Real Time Clock!"),
				} as usize;
				self.ram_data[0x2000 * ram_bank_number + address as usize - 0xA000]
			}
			0xA000..0xC000 => 0xFF,
			_ => unreachable!(),
		}
	}

	fn write_byte(&mut self, address: u16, value: u8) {
		match address {
			0x0000..0x2000 => self.ram_enable = (value & 0x0F) == 0x0A,
			0x2000..0x4000 => self.rom_bank_register = value,
			0x4000..0x6000 => self.ram_bank_register = value,
			0x6000..0x8000 => (),
			0xA000..0xC000 => {
				if !self.ram_enable {
					return;
				}
				let ram_bank_number = match self.ram_bank_register.bitand(0x0F) {
					val if val < 0x04 => val,
					_ => unimplemented!("Real Time Clock!"),
				} as usize;
				self.ram_data[0x2000 * ram_bank_number + address as usize - 0xA000] = value;
			}
			_ => unreachable!(),
		}
	}
}

// MBC5 Registers:
// - 0000-1FFF: RAM Enable
// - 2000-2FFF: 8 bits of ROM Bank Number
// - 3000-3FFF: 9th bit of ROM Bank Number
// - 4000-5FFF: RAM Bank Number
struct MBC5 {
	ram_enable: bool,
	ram_bank_register: u8,
	rom_bank_register_lo: u8,
	rom_bank_register_hi: u8,
	ram_data: Vec<u8>,
	rom_data: Vec<u8>,
}

impl Cartridge for MBC5 {
	fn new(data: Vec<u8>) -> Box<dyn Cartridge> {
		let mut c = Box::new(MBC5 {
			ram_enable: false,
			ram_bank_register: 0x00,
			rom_bank_register_lo: 0x00,
			rom_bank_register_hi: 0x00,
			ram_data: vec![0; 0],
			rom_data: data,
		});
		c.ram_data = vec![0; 0x0800 * c.get_total_ram_banks() as usize];
		c
	}

	fn read_byte(&self, address: u16) -> u8 {
		match address {
			0x0000..0x4000 => self.rom_data[address as usize],
			0x4000..0x8000 => {
				let rom_bank_number =
					u16::from_be_bytes([self.rom_bank_register_hi, self.rom_bank_register_lo]).bitand(0x01FF)
						as usize;
				self.rom_data[0x4000 * rom_bank_number + address as usize - 0x4000]
			}
			0xA000..0xC000 if self.ram_enable => {
				let ram_bank_number = self.ram_bank_register.bitand(0x0F) as usize;
				self.ram_data[0x2000 * ram_bank_number + address as usize - 0xA000]
			}
			0xA000..0xC000 => 0xFF,
			_ => unreachable!(),
		}
	}

	fn write_byte(&mut self, address: u16, value: u8) {
		match address {
			0x0000..0x2000 => self.ram_enable = (value & 0x0F) == 0x0A,
			0x2000..0x3000 => self.rom_bank_register_lo = value,
			0x3000..0x4000 => self.rom_bank_register_hi = value,
			0x4000..0x6000 => self.ram_bank_register = value,
			0x6000..0x8000 => (),
			0xA000..0xC000 => {
				if !self.ram_enable {
					return;
				}
				let ram_bank_number = self.ram_bank_register.bitand(0x0F) as usize;
				self.ram_data[0x2000 * ram_bank_number + address as usize - 0xA000] = value;
			}
			_ => unreachable!(),
		}
	}
}

pub fn create(data: Vec<u8>) -> Box<dyn Cartridge> {
	let c = match data[0x0147] {
		0x00 => RomOnly::new(data),
		0x01 | 0x02 | 0x03 => MBC1::new(data),
		0x11 | 0x12 | 0x13 => MBC3::new(data),
		0x19 | 0x1A | 0x1B => MBC5::new(data),
		_ => todo!(),
	};

	println!("title: {:?}", c.get_title());
	println!("rom banks: {}", c.get_total_rom_banks());
	println!("ram banks: {}\n", c.get_total_ram_banks());

	c
}
