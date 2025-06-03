use crate::DEBUG_FLAG;
use crate::MMU;
use crate::utils::Checks;
use std::ops::{Shl, Shr};

pub struct CPU {
	a: u8,
	f: u8,
	b: u8,
	c: u8,
	d: u8,
	e: u8,
	h: u8,
	l: u8,
	sp: u16,
	pc: u16,
	ime: bool,
	ime_scheduled: bool,
	low_power_mode: bool,
}

impl CPU {
	pub fn new() -> Self {
		CPU {
			a: 0x01,
			f: 0xB0,
			b: 0x00,
			c: 0x13,
			d: 0x00,
			e: 0xD8,
			h: 0x01,
			l: 0x4D,
			sp: 0xFFFE,
			pc: 0x0100,
			ime: false,
			ime_scheduled: false,
			low_power_mode: false,
		}
	}

	fn af(&self) -> u16 {
		self.f as u16 | (self.a as u16) << 8
	}

	fn bc(&self) -> u16 {
		self.c as u16 | (self.b as u16) << 8
	}

	fn de(&self) -> u16 {
		self.e as u16 | (self.d as u16) << 8
	}

	fn hl(&self) -> u16 {
		self.l as u16 | (self.h as u16) << 8
	}

	fn set_af(&mut self, val: u16) {
		self.f = (val & 0xFFF0) as u8;
		self.a = (val >> 8) as u8;
	}

	fn set_bc(&mut self, val: u16) {
		self.c = val as u8;
		self.b = (val >> 8) as u8;
	}

	fn set_de(&mut self, val: u16) {
		self.e = val as u8;
		self.d = (val >> 8) as u8;
	}

	fn set_hl(&mut self, val: u16) {
		self.l = val as u8;
		self.h = (val >> 8) as u8;
	}

	fn set_flag(&mut self, bit: u8, flag: bool) {
		match flag {
			true => {
				self.f |= 1 << bit;
			}
			false => {
				self.f &= !(1 << bit);
			}
		};
	}

	fn get_z_flag(&self) -> bool {
		(self.f >> 7) & 0x01 == 0x01
	}

	fn set_z_flag(&mut self, flag: bool) {
		self.set_flag(7, flag);
	}

	fn get_n_flag(&self) -> bool {
		(self.f >> 6) & 0x01 == 0x01
	}

	fn set_n_flag(&mut self, flag: bool) {
		self.set_flag(6, flag);
	}

	fn get_h_flag(&self) -> bool {
		(self.f >> 5) & 0x01 == 0x01
	}

	fn set_h_flag(&mut self, flag: bool) {
		self.set_flag(5, flag);
	}

	fn get_c_flag(&self) -> bool {
		(self.f >> 4) & 0x01 == 0x01
	}

	fn set_c_flag(&mut self, flag: bool) {
		self.set_flag(4, flag);
	}

	fn get_byte(&mut self, mmu: &MMU) -> u8 {
		let byte = mmu.read_byte(self.pc);
		self.pc = self.pc.wrapping_add(1);
		byte
	}

	fn push_stack(&mut self, mmu: &mut MMU, val: u16) {
		self.sp = self.sp.wrapping_sub(1);
		mmu.write_byte(self.sp, (val >> 8) as u8);

		self.sp = self.sp.wrapping_sub(1);
		mmu.write_byte(self.sp, val as u8);
	}

	fn pop_stack(&mut self, mmu: &MMU) -> u16 {
		let l = mmu.read_byte(self.sp);
		self.sp = self.sp.wrapping_add(1);

		let h = mmu.read_byte(self.sp);
		self.sp = self.sp.wrapping_add(1);

		u16::from_le_bytes([l, h])
	}

	fn execute_interrupts(&mut self, mmu: &mut MMU) -> u16 {
		let ie_reg = mmu.read_byte(0xFFFF);
		let if_reg = mmu.read_byte(0xFF0F);

		if 0x1F & ie_reg & if_reg > 0 {
			self.low_power_mode = false;
			if self.ime {
				self.ime = false;
				self.push_stack(mmu, self.pc);
				match ie_reg & if_reg {
					x if (x >> 0) & 0x01 == 0x01 => {
						self.pc = 0x0040;
						mmu.write_byte(0xFF0F, if_reg & 0xFE);
					}
					x if (x >> 1) & 0x01 == 0x01 => {
						self.pc = 0x0048;
						mmu.write_byte(0xFF0F, if_reg & 0xFD);
					}
					x if (x >> 2) & 0x01 == 0x01 => {
						self.pc = 0x0050;
						mmu.write_byte(0xFF0F, if_reg & 0xFB);
					}
					x if (x >> 3) & 0x01 == 0x01 => {
						self.pc = 0x0058;
						mmu.write_byte(0xFF0F, if_reg & 0xF7);
					}
					x if (x >> 4) & 0x01 == 0x01 => {
						self.pc = 0x0060;
						mmu.write_byte(0xFF0F, if_reg & 0xEF);
					}
					_ => unreachable!(),
				};
				return 20;
			}
		}

		0
	}

	fn execute_prefixed(&mut self, mmu: &mut MMU) -> u16 {
		let opcode = self.get_byte(mmu);

		match opcode {
			0x00..=0x07 => {
				let mut cycles = 8;
				let x = match opcode & 0x07 {
					0x00 => self.b,
					0x01 => self.c,
					0x02 => self.d,
					0x03 => self.e,
					0x04 => self.h,
					0x05 => self.l,
					0x06 => {
						cycles += 4;
						mmu.read_byte(self.hl())
					}
					0x07 => self.a,
					_ => unreachable!(),
				};

				let c_flag = x & 0x80 == 0x80;
				let x = (x << 1) | if c_flag { 0x01 } else { 0x00 };
				let z_flag = x == 0x00;

				match opcode & 0x07 {
					0x00 => self.b = x,
					0x01 => self.c = x,
					0x02 => self.d = x,
					0x03 => self.e = x,
					0x04 => self.h = x,
					0x05 => self.l = x,
					0x06 => {
						cycles += 4;
						mmu.write_byte(self.hl(), x)
					}
					0x07 => self.a = x,
					_ => unreachable!(),
				};

				self.set_z_flag(z_flag);
				self.set_n_flag(false);
				self.set_h_flag(false);
				self.set_c_flag(c_flag);
				cycles
			}

			0x08..=0x0F => {
				let mut cycles = 8;
				let x = match opcode & 0x07 {
					0x00 => self.b,
					0x01 => self.c,
					0x02 => self.d,
					0x03 => self.e,
					0x04 => self.h,
					0x05 => self.l,
					0x06 => {
						cycles += 4;
						mmu.read_byte(self.hl())
					}
					0x07 => self.a,
					_ => unreachable!(),
				};

				let c_flag = x & 0x01 == 0x01;
				let x = (x >> 1) | if c_flag { 0x80 } else { 0x00 };
				let z_flag = x == 0x00;

				match opcode & 0x07 {
					0x00 => self.b = x,
					0x01 => self.c = x,
					0x02 => self.d = x,
					0x03 => self.e = x,
					0x04 => self.h = x,
					0x05 => self.l = x,
					0x06 => {
						cycles += 4;
						mmu.write_byte(self.hl(), x)
					}
					0x07 => self.a = x,
					_ => unreachable!(),
				};

				self.set_z_flag(z_flag);
				self.set_n_flag(false);
				self.set_h_flag(false);
				self.set_c_flag(c_flag);
				cycles
			}

			0x10..=0x17 => {
				let mut cycles = 8;
				let x = match opcode & 0x07 {
					0x00 => self.b,
					0x01 => self.c,
					0x02 => self.d,
					0x03 => self.e,
					0x04 => self.h,
					0x05 => self.l,
					0x06 => {
						cycles += 4;
						mmu.read_byte(self.hl())
					}
					0x07 => self.a,
					_ => unreachable!(),
				};

				let c_flag = x & 0x80 == 0x80;
				let x = (x << 1) | if self.get_c_flag() { 1 } else { 0 };
				let z_flag = x == 0x00;

				match opcode & 0x07 {
					0x00 => self.b = x,
					0x01 => self.c = x,
					0x02 => self.d = x,
					0x03 => self.e = x,
					0x04 => self.h = x,
					0x05 => self.l = x,
					0x06 => {
						cycles += 4;
						mmu.write_byte(self.hl(), x)
					}
					0x07 => self.a = x,
					_ => unreachable!(),
				};

				self.set_z_flag(z_flag);
				self.set_n_flag(false);
				self.set_h_flag(false);
				self.set_c_flag(c_flag);
				cycles
			}

			0x18..=0x1F => {
				let mut cycles = 8;
				let x = match opcode & 0x07 {
					0x00 => self.b,
					0x01 => self.c,
					0x02 => self.d,
					0x03 => self.e,
					0x04 => self.h,
					0x05 => self.l,
					0x06 => {
						cycles += 4;
						mmu.read_byte(self.hl())
					}
					0x07 => self.a,
					_ => unreachable!(),
				};

				let c_flag = x & 0x01 == 0x01;
				let x = (x >> 1) | if self.get_c_flag() { 0x80 } else { 0 };
				let z_flag = x == 0x00;

				match opcode & 0x07 {
					0x00 => self.b = x,
					0x01 => self.c = x,
					0x02 => self.d = x,
					0x03 => self.e = x,
					0x04 => self.h = x,
					0x05 => self.l = x,
					0x06 => {
						cycles += 4;
						mmu.write_byte(self.hl(), x)
					}
					0x07 => self.a = x,
					_ => unreachable!(),
				};

				self.set_z_flag(z_flag);
				self.set_n_flag(false);
				self.set_h_flag(false);
				self.set_c_flag(c_flag);
				cycles
			}

			0x20..=0x27 => {
				let mut cycles = 8;
				let x = match opcode & 0x07 {
					0x00 => self.b,
					0x01 => self.c,
					0x02 => self.d,
					0x03 => self.e,
					0x04 => self.h,
					0x05 => self.l,
					0x06 => {
						cycles += 4;
						mmu.read_byte(self.hl())
					}
					0x07 => self.a,
					_ => unreachable!(),
				};

				let c_flag = x & 0x80 == 0x80;
				let x = (x as i8).shl(1) as u8;
				let z_flag = x == 0x00;

				match opcode & 0x07 {
					0x00 => self.b = x,
					0x01 => self.c = x,
					0x02 => self.d = x,
					0x03 => self.e = x,
					0x04 => self.h = x,
					0x05 => self.l = x,
					0x06 => {
						cycles += 4;
						mmu.write_byte(self.hl(), x)
					}
					0x07 => self.a = x,
					_ => unreachable!(),
				};

				self.set_z_flag(z_flag);
				self.set_n_flag(false);
				self.set_h_flag(false);
				self.set_c_flag(c_flag);
				cycles
			}

			0x28..=0x2F => {
				let mut cycles = 8;
				let x = match opcode & 0x07 {
					0x00 => self.b,
					0x01 => self.c,
					0x02 => self.d,
					0x03 => self.e,
					0x04 => self.h,
					0x05 => self.l,
					0x06 => {
						cycles += 4;
						mmu.read_byte(self.hl())
					}
					0x07 => self.a,
					_ => unreachable!(),
				};

				let c_flag = x & 0x01 == 0x01;
				let x = (x as i8).shr(1) as u8;
				let z_flag = x == 0x00;

				match opcode & 0x07 {
					0x00 => self.b = x,
					0x01 => self.c = x,
					0x02 => self.d = x,
					0x03 => self.e = x,
					0x04 => self.h = x,
					0x05 => self.l = x,
					0x06 => {
						cycles += 4;
						mmu.write_byte(self.hl(), x)
					}
					0x07 => self.a = x,
					_ => unreachable!(),
				};

				self.set_z_flag(z_flag);
				self.set_n_flag(false);
				self.set_h_flag(false);
				self.set_c_flag(c_flag);
				cycles
			}

			0x30..=0x37 => {
				let mut cycles = 8;
				let x = match opcode & 0x07 {
					0x00 => self.b,
					0x01 => self.c,
					0x02 => self.d,
					0x03 => self.e,
					0x04 => self.h,
					0x05 => self.l,
					0x06 => {
						cycles += 4;
						mmu.read_byte(self.hl())
					}
					0x07 => self.a,
					_ => unreachable!(),
				};

				let x = ((x & 0x0F) << 4) | (x >> 4);
				let z_flag = x == 0x00;

				match opcode & 0x07 {
					0x00 => self.b = x,
					0x01 => self.c = x,
					0x02 => self.d = x,
					0x03 => self.e = x,
					0x04 => self.h = x,
					0x05 => self.l = x,
					0x06 => {
						cycles += 4;
						mmu.write_byte(self.hl(), x)
					}
					0x07 => self.a = x,
					_ => unreachable!(),
				};

				self.set_z_flag(z_flag);
				self.set_n_flag(false);
				self.set_h_flag(false);
				self.set_c_flag(false);
				cycles
			}

			0x38..=0x3F => {
				let mut cycles = 8;
				let x = match opcode & 0x07 {
					0x00 => self.b,
					0x01 => self.c,
					0x02 => self.d,
					0x03 => self.e,
					0x04 => self.h,
					0x05 => self.l,
					0x06 => {
						cycles += 4;
						mmu.read_byte(self.hl())
					}
					0x07 => self.a,
					_ => unreachable!(),
				};

				let c_flag = x & 0x01 == 0x01;
				let x = x >> 1;
				let z_flag = x == 0x00;

				match opcode & 0x07 {
					0x00 => self.b = x,
					0x01 => self.c = x,
					0x02 => self.d = x,
					0x03 => self.e = x,
					0x04 => self.h = x,
					0x05 => self.l = x,
					0x06 => {
						cycles += 4;
						mmu.write_byte(self.hl(), x)
					}
					0x07 => self.a = x,
					_ => unreachable!(),
				};

				self.set_z_flag(z_flag);
				self.set_n_flag(false);
				self.set_h_flag(false);
				self.set_c_flag(c_flag);
				cycles
			}

			0x40..=0x7F => {
				let mut cycles = 8;
				let bit = (opcode >> 3) & 0x07;
				let z_flag = match opcode & 0x07 {
					0x00 => self.b >> bit,
					0x01 => self.c >> bit,
					0x02 => self.d >> bit,
					0x03 => self.e >> bit,
					0x04 => self.h >> bit,
					0x05 => self.l >> bit,
					0x06 => {
						cycles += 4;
						mmu.read_byte(self.hl()) >> bit
					}
					0x07 => self.a >> bit,
					_ => unreachable!(),
				} & 0x01
					== 0x01;
				self.set_z_flag(!z_flag);
				self.set_n_flag(false);
				self.set_h_flag(true);
				cycles
			}

			0x80..=0xBF => {
				let mut cycles = 8;
				let val = !(1 << ((opcode >> 3) & 0x07));
				match opcode & 0x07 {
					0x00 => self.b &= val,
					0x01 => self.c &= val,
					0x02 => self.d &= val,
					0x03 => self.e &= val,
					0x04 => self.h &= val,
					0x05 => self.l &= val,
					0x06 => {
						mmu.write_byte(self.hl(), mmu.read_byte(self.hl()) & val);
						cycles += 8;
					}
					0x07 => self.a &= val,
					_ => unreachable!(),
				};
				cycles
			}

			0xC0..=0xFF => {
				let mut cycles = 8;
				let val = 1 << ((opcode >> 3) & 0x07);
				match opcode & 0x07 {
					0x00 => self.b |= val,
					0x01 => self.c |= val,
					0x02 => self.d |= val,
					0x03 => self.e |= val,
					0x04 => self.h |= val,
					0x05 => self.l |= val,
					0x06 => {
						mmu.write_byte(self.hl(), mmu.read_byte(self.hl()) | val);
						cycles += 8;
					}
					0x07 => self.a |= val,
					_ => unreachable!(),
				}
				cycles
			}
		}
	}

	pub fn execute_next(&mut self, mmu: &mut MMU) -> u16 {
		let cycles = self.execute_interrupts(mmu);

		if cycles > 0 {
			return cycles;
		} else if self.low_power_mode {
			return 4;
		}

		if DEBUG_FLAG {
			println!(
				"A:{:02X} F:{:02X} B:{:02X} C:{:02X} D:{:02X} E:{:02X} H:{:02X} L:{:02X} SP:{:04X} PC:{:04X} PCMEM:{:02X},{:02X},{:02X},{:02X}\n",
				self.a,
				self.f,
				self.b,
				self.c,
				self.d,
				self.e,
				self.h,
				self.l,
				self.sp,
				self.pc,
				mmu.read_byte(self.pc),
				mmu.read_byte(self.pc + 1),
				mmu.read_byte(self.pc + 2),
				mmu.read_byte(self.pc + 3),
			);
		}

		let opcode = self.get_byte(mmu);

		let cycles = match opcode {
			0x00 => 4,

			0x01 => {
				let x = u16::from_le_bytes([self.get_byte(mmu), self.get_byte(mmu)]);
				self.set_bc(x);
				12
			}

			0x02 => {
				mmu.write_byte(self.bc(), self.a);
				8
			}

			0x03 => {
				self.set_bc(self.bc().wrapping_add(1));
				8
			}

			0x04 => {
				self.b = self.b.wrapping_add(1);
				if self.b == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(false);
				if self.b & 0x0F == 0x00 {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				4
			}

			0x05 => {
				if {
					let a = self.b;
					u8::check_half_carry_sub(a, 1, 0x00)
				} {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				self.b = self.b.wrapping_sub(1);
				if self.b == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(true);
				4
			}

			0x06 => {
				self.b = self.get_byte(mmu);
				8
			}

			0x07 => {
				let msb = self.a & 0x80 == 0x80;
				self.a <<= 1;
				self.set_z_flag(false);
				self.set_n_flag(false);
				self.set_h_flag(false);
				if msb {
					self.set_c_flag(true);
					self.a |= 0x01;
				} else {
					self.set_c_flag(false);
				}
				4
			}

			0x08 => {
				let address = u16::from_le_bytes([self.get_byte(mmu), self.get_byte(mmu)]);
				mmu.write_byte(address, self.sp as u8);
				mmu.write_byte(address + 1, (self.sp >> 8) as u8);
				20
			}

			0x09 => {
				if u16::check_half_carry_add(self.hl(), self.bc(), 0x0000) {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				if u16::check_carry_add(self.hl(), self.bc(), 0x0000) {
					self.set_c_flag(true);
				} else {
					self.set_c_flag(false);
				}
				self.set_hl(self.hl().wrapping_add(self.bc()));
				self.set_n_flag(false);
				8
			}

			0x0A => {
				self.a = mmu.read_byte(self.bc());
				8
			}

			0x0B => {
				self.set_bc(self.bc().wrapping_sub(1));
				8
			}

			0x0C => {
				self.c = self.c.wrapping_add(1);
				if self.c == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(false);
				if self.c & 0x0F == 0x00 {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				4
			}

			0x0D => {
				if {
					let a = self.c;
					u8::check_half_carry_sub(a, 1, 0x00)
				} {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				self.c = self.c.wrapping_sub(1);
				if self.c == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(true);
				4
			}

			0x0E => {
				self.c = self.get_byte(mmu);
				8
			}

			0x0F => {
				let lsb = self.a & 0x01 == 0x01;
				self.a >>= 1;
				self.set_z_flag(false);
				self.set_n_flag(false);
				self.set_h_flag(false);
				if lsb {
					self.set_c_flag(true);
					self.a |= 0x80;
				} else {
					self.set_c_flag(false);
				}
				4
			}

			0x10 => {
				self.get_byte(mmu);
				8
			}

			0x11 => {
				let x = u16::from_le_bytes([self.get_byte(mmu), self.get_byte(mmu)]);
				self.set_de(x);
				12
			}

			0x12 => {
				mmu.write_byte(self.de(), self.a);
				8
			}

			0x13 => {
				self.set_de(self.de().wrapping_add(1));
				8
			}

			0x14 => {
				self.d = self.d.wrapping_add(1);
				if self.d == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(false);
				if self.d & 0x0F == 0x00 {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				4
			}

			0x15 => {
				if {
					let a = self.d;
					u8::check_half_carry_sub(a, 1, 0x00)
				} {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				self.d = self.d.wrapping_sub(1);
				if self.d == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(true);
				4
			}

			0x16 => {
				self.d = self.get_byte(mmu);
				8
			}

			0x17 => {
				let msb = self.a & 0x80 == 0x80;
				self.a <<= 1;
				self.set_z_flag(false);
				self.set_n_flag(false);
				self.set_h_flag(false);
				if self.get_c_flag() {
					self.a |= 0x01;
				}
				if msb {
					self.set_c_flag(true);
				} else {
					self.set_c_flag(false);
				}
				4
			}

			0x18 => {
				let x = self.get_byte(mmu) as i8;
				self.pc = self.pc.wrapping_add_signed(x as i16);
				12
			}

			0x19 => {
				if u16::check_half_carry_add(self.hl(), self.de(), 0x0000) {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				if u16::check_carry_add(self.hl(), self.de(), 0x0000) {
					self.set_c_flag(true);
				} else {
					self.set_c_flag(false);
				}
				self.set_hl(self.hl().wrapping_add(self.de()));
				self.set_n_flag(false);
				8
			}

			0x1A => {
				self.a = mmu.read_byte(self.de());
				8
			}

			0x1B => {
				self.set_de(self.de().wrapping_sub(1));
				8
			}

			0x1C => {
				self.e = self.e.wrapping_add(1);
				if self.e == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(false);
				if self.e & 0x0F == 0x00 {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				4
			}

			0x1D => {
				if {
					let a = self.e;
					u8::check_half_carry_sub(a, 1, 0x00)
				} {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				self.e = self.e.wrapping_sub(1);
				if self.e == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(true);
				4
			}

			0x1E => {
				self.e = self.get_byte(mmu);
				8
			}

			0x1F => {
				let lsb = self.a & 0x01 == 0x01;
				self.a >>= 1;
				self.set_z_flag(false);
				self.set_n_flag(false);
				self.set_h_flag(false);
				if self.get_c_flag() {
					self.a |= 0x80;
				}
				if lsb {
					self.set_c_flag(true);
				} else {
					self.set_c_flag(false);
				}
				4
			}

			0x20 => {
				let x = self.get_byte(mmu) as i8;
				if !self.get_z_flag() {
					self.pc = self.pc.wrapping_add_signed(x as i16);
					12
				} else {
					8
				}
			}

			0x21 => {
				let x = u16::from_le_bytes([self.get_byte(mmu), self.get_byte(mmu)]);
				self.set_hl(x);
				12
			}

			0x22 => {
				mmu.write_byte(self.hl(), self.a);
				self.set_hl(self.hl().wrapping_add(1));
				8
			}

			0x23 => {
				self.set_hl(self.hl().wrapping_add(1));
				8
			}

			0x24 => {
				self.h = self.h.wrapping_add(1);
				if self.h == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(false);
				if self.h & 0x0F == 0x00 {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				4
			}

			0x25 => {
				if {
					let a = self.h;
					u8::check_half_carry_sub(a, 1, 0x00)
				} {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				self.h = self.h.wrapping_sub(1);
				if self.h == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(true);
				4
			}

			0x26 => {
				self.h = self.get_byte(mmu);
				8
			}

			0x27 => {
				if self.get_n_flag() {
					let mut adjustment = 0;
					if self.get_h_flag() {
						adjustment += 0x06;
					}
					if self.get_c_flag() {
						adjustment += 0x60;
					}
					self.a = self.a.wrapping_sub(adjustment);
				} else {
					let mut adjustment = 0;
					if self.get_h_flag() || self.a & 0x0F > 0x09 {
						adjustment += 0x06;
					}
					if self.get_c_flag() || self.a > 0x99 {
						adjustment += 0x60;
						self.set_c_flag(true);
					}
					self.a = self.a.wrapping_add(adjustment);
				}
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_h_flag(false);
				4
			}

			0x28 => {
				let x = self.get_byte(mmu) as i8;
				if self.get_z_flag() {
					self.pc = self.pc.wrapping_add_signed(x as i16);
					12
				} else {
					8
				}
			}

			0x29 => {
				if u16::check_half_carry_add(self.hl(), self.hl(), 0x0000) {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				if u16::check_carry_add(self.hl(), self.hl(), 0x0000) {
					self.set_c_flag(true);
				} else {
					self.set_c_flag(false);
				}
				self.set_hl(self.hl().wrapping_add(self.hl()));
				self.set_n_flag(false);
				8
			}

			0x2A => {
				let hl = self.hl();
				self.a = mmu.read_byte(hl);
				self.set_hl(hl.wrapping_add(1));
				8
			}

			0x2B => {
				self.set_hl(self.hl().wrapping_sub(1));
				8
			}

			0x2C => {
				self.l = self.l.wrapping_add(1);
				if self.l == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(false);
				if self.l & 0x0F == 0x00 {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				4
			}

			0x2D => {
				if {
					let a = self.l;
					u8::check_half_carry_sub(a, 1, 0x00)
				} {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				self.l = self.l.wrapping_sub(1);
				if self.l == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(true);
				4
			}

			0x2E => {
				self.l = self.get_byte(mmu);
				8
			}

			0x2F => {
				self.a = !self.a;
				self.set_n_flag(true);
				self.set_h_flag(true);
				4
			}

			0x30 => {
				let x = self.get_byte(mmu) as i8;
				if !self.get_c_flag() {
					self.pc = self.pc.wrapping_add_signed(x as i16);
					12
				} else {
					8
				}
			}

			0x31 => {
				self.sp = u16::from_le_bytes([self.get_byte(mmu), self.get_byte(mmu)]);
				12
			}

			0x32 => {
				mmu.write_byte(self.hl(), self.a);
				self.set_hl(self.hl().wrapping_sub(1));
				8
			}

			0x33 => {
				self.sp = self.sp.wrapping_add(1);
				8
			}

			0x34 => {
				let x = mmu.read_byte(self.hl()).wrapping_add(1);
				mmu.write_byte(self.hl(), x);
				if x == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(false);
				if x & 0x0F == 0x00 {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				12
			}

			0x35 => {
				let mut x = mmu.read_byte(self.hl());
				if u8::check_half_carry_sub(x, 1, 0x00) {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				x = x.wrapping_sub(1);
				mmu.write_byte(self.hl(), x);
				if x == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(true);
				12
			}

			0x36 => {
				mmu.write_byte(self.hl(), self.get_byte(mmu));
				12
			}

			0x37 => {
				self.set_n_flag(false);
				self.set_h_flag(false);
				self.set_c_flag(true);
				4
			}

			0x38 => {
				let x = self.get_byte(mmu) as i8;
				if self.get_c_flag() {
					self.pc = self.pc.wrapping_add_signed(x as i16);
					12
				} else {
					8
				}
			}

			0x39 => {
				if u16::check_half_carry_add(self.hl(), self.sp, 0x0000) {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				if u16::check_carry_add(self.hl(), self.sp, 0x0000) {
					self.set_c_flag(true);
				} else {
					self.set_c_flag(false);
				}
				self.set_hl(self.hl().wrapping_add(self.sp));
				self.set_n_flag(false);
				8
			}

			0x3A => {
				let hl = self.hl();
				self.a = mmu.read_byte(hl);
				self.set_hl(hl.wrapping_sub(1));
				8
			}

			0x3B => {
				self.sp = self.sp.wrapping_sub(1);
				8
			}

			0x3C => {
				self.a = self.a.wrapping_add(1);
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(false);
				if self.a & 0x0F == 0x00 {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				4
			}

			0x3D => {
				if {
					let a = self.a;
					u8::check_half_carry_sub(a, 1, 0x00)
				} {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				self.a = self.a.wrapping_sub(1);
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(true);
				4
			}

			0x3E => {
				self.a = self.get_byte(mmu);
				8
			}

			0x3F => {
				self.set_n_flag(false);
				self.set_h_flag(false);
				if self.get_c_flag() {
					self.set_c_flag(false);
				} else {
					self.set_c_flag(true);
				}
				4
			}

			0x40 => {
				self.b = self.b;
				4
			}

			0x41 => {
				self.b = self.c;
				4
			}

			0x42 => {
				self.b = self.d;
				4
			}

			0x43 => {
				self.b = self.e;
				4
			}

			0x44 => {
				self.b = self.h;
				4
			}

			0x45 => {
				self.b = self.l;
				4
			}

			0x46 => {
				self.b = mmu.read_byte(self.hl());
				8
			}

			0x47 => {
				self.b = self.a;
				4
			}

			0x48 => {
				self.c = self.b;
				4
			}

			0x49 => {
				self.c = self.c;
				4
			}

			0x4A => {
				self.c = self.d;
				4
			}

			0x4B => {
				self.c = self.e;
				4
			}

			0x4C => {
				self.c = self.h;
				4
			}

			0x4D => {
				self.c = self.l;
				4
			}

			0x4E => {
				self.c = mmu.read_byte(self.hl());
				8
			}

			0x4F => {
				self.c = self.a;
				4
			}

			0x50 => {
				self.d = self.b;
				4
			}

			0x51 => {
				self.d = self.c;
				4
			}

			0x52 => {
				self.d = self.d;
				4
			}

			0x53 => {
				self.d = self.e;
				4
			}

			0x54 => {
				self.d = self.h;
				4
			}

			0x55 => {
				self.d = self.l;
				4
			}

			0x56 => {
				self.d = mmu.read_byte(self.hl());
				8
			}

			0x57 => {
				self.d = self.a;
				4
			}

			0x58 => {
				self.e = self.b;
				4
			}

			0x59 => {
				self.e = self.c;
				4
			}

			0x5A => {
				self.e = self.d;
				4
			}

			0x5B => {
				self.e = self.e;
				4
			}

			0x5C => {
				self.e = self.h;
				4
			}

			0x5D => {
				self.e = self.l;
				4
			}

			0x5E => {
				self.e = mmu.read_byte(self.hl());
				8
			}

			0x5F => {
				self.e = self.a;
				4
			}

			0x60 => {
				self.h = self.b;
				4
			}

			0x61 => {
				self.h = self.c;
				4
			}

			0x62 => {
				self.h = self.d;
				4
			}

			0x63 => {
				self.h = self.e;
				4
			}

			0x64 => {
				self.h = self.h;
				4
			}

			0x65 => {
				self.h = self.l;
				4
			}

			0x66 => {
				self.h = mmu.read_byte(self.hl());
				8
			}

			0x67 => {
				self.h = self.a;
				4
			}

			0x68 => {
				self.l = self.b;
				4
			}

			0x69 => {
				self.l = self.c;
				4
			}

			0x6A => {
				self.l = self.d;
				4
			}

			0x6B => {
				self.l = self.e;
				4
			}

			0x6C => {
				self.l = self.h;
				4
			}

			0x6D => {
				self.l = self.l;
				4
			}

			0x6E => {
				self.l = mmu.read_byte(self.hl());
				8
			}

			0x6F => {
				self.l = self.a;
				4
			}

			0x70 => {
				mmu.write_byte(self.hl(), self.b);
				8
			}

			0x71 => {
				mmu.write_byte(self.hl(), self.c);
				8
			}

			0x72 => {
				mmu.write_byte(self.hl(), self.d);
				8
			}

			0x73 => {
				mmu.write_byte(self.hl(), self.e);
				8
			}

			0x74 => {
				mmu.write_byte(self.hl(), self.h);
				8
			}

			0x75 => {
				mmu.write_byte(self.hl(), self.l);
				8
			}

			0x76 => {
				self.low_power_mode = true;
				4
			}

			0x77 => {
				mmu.write_byte(self.hl(), self.a);
				8
			}

			0x78 => {
				self.a = self.b;
				4
			}

			0x79 => {
				self.a = self.c;
				4
			}

			0x7A => {
				self.a = self.d;
				4
			}

			0x7B => {
				self.a = self.e;
				4
			}

			0x7C => {
				self.a = self.h;
				4
			}

			0x7D => {
				self.a = self.l;
				4
			}

			0x7E => {
				self.a = mmu.read_byte(self.hl());
				8
			}

			0x7F => {
				self.a = self.a;
				4
			}

			0x80 => {
				if {
					let a = self.a;
					let b = self.b;
					u8::check_half_carry_add(a, b, 0x00)
				} {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				if {
					let a = self.a;
					let b = self.b;
					u8::check_carry_add(a, b, 0x00)
				} {
					self.set_c_flag(true);
				} else {
					self.set_c_flag(false);
				}
				self.a = self.a.wrapping_add(self.b);
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(false);
				4
			}

			0x81 => {
				if {
					let a = self.a;
					let b = self.c;
					u8::check_half_carry_add(a, b, 0x00)
				} {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				if {
					let a = self.a;
					let b = self.c;
					u8::check_carry_add(a, b, 0x00)
				} {
					self.set_c_flag(true);
				} else {
					self.set_c_flag(false);
				}
				self.a = self.a.wrapping_add(self.c);
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(false);
				4
			}

			0x82 => {
				if {
					let a = self.a;
					let b = self.d;
					u8::check_half_carry_add(a, b, 0x00)
				} {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				if {
					let a = self.a;
					let b = self.d;
					u8::check_carry_add(a, b, 0x00)
				} {
					self.set_c_flag(true);
				} else {
					self.set_c_flag(false);
				}
				self.a = self.a.wrapping_add(self.d);
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(false);
				4
			}

			0x83 => {
				if {
					let a = self.a;
					let b = self.e;
					u8::check_half_carry_add(a, b, 0x00)
				} {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				if {
					let a = self.a;
					let b = self.e;
					u8::check_carry_add(a, b, 0x00)
				} {
					self.set_c_flag(true);
				} else {
					self.set_c_flag(false);
				}
				self.a = self.a.wrapping_add(self.e);
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(false);
				4
			}

			0x84 => {
				if {
					let a = self.a;
					let b = self.h;
					u8::check_half_carry_add(a, b, 0x00)
				} {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				if {
					let a = self.a;
					let b = self.h;
					u8::check_carry_add(a, b, 0x00)
				} {
					self.set_c_flag(true);
				} else {
					self.set_c_flag(false);
				}
				self.a = self.a.wrapping_add(self.h);
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(false);
				4
			}

			0x85 => {
				if {
					let a = self.a;
					let b = self.l;
					u8::check_half_carry_add(a, b, 0x00)
				} {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				if {
					let a = self.a;
					let b = self.l;
					u8::check_carry_add(a, b, 0x00)
				} {
					self.set_c_flag(true);
				} else {
					self.set_c_flag(false);
				}
				self.a = self.a.wrapping_add(self.l);
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(false);
				4
			}

			0x86 => {
				let x = mmu.read_byte(self.hl());
				if {
					let a = self.a;
					u8::check_half_carry_add(a, x, 0x00)
				} {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				if {
					let a = self.a;
					u8::check_carry_add(a, x, 0x00)
				} {
					self.set_c_flag(true);
				} else {
					self.set_c_flag(false);
				}
				self.a = self.a.wrapping_add(x);
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(false);
				8
			}

			0x87 => {
				if {
					let a = self.a;
					let b = self.a;
					u8::check_half_carry_add(a, b, 0x00)
				} {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				if {
					let a = self.a;
					let b = self.a;
					u8::check_carry_add(a, b, 0x00)
				} {
					self.set_c_flag(true);
				} else {
					self.set_c_flag(false);
				}
				self.a = self.a.wrapping_add(self.a);
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(false);
				4
			}

			0x88 => {
				let carry = if self.get_c_flag() { 1 } else { 0 };
				if u8::check_half_carry_add(self.a, self.b, carry) {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				if u8::check_carry_add(self.a, self.b, carry) {
					self.set_c_flag(true);
				} else {
					self.set_c_flag(false);
				}
				self.a = self.a.wrapping_add(self.b).wrapping_add(carry);
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(false);
				4
			}

			0x89 => {
				let carry = if self.get_c_flag() { 1 } else { 0 };
				if u8::check_half_carry_add(self.a, self.c, carry) {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				if u8::check_carry_add(self.a, self.c, carry) {
					self.set_c_flag(true);
				} else {
					self.set_c_flag(false);
				}
				self.a = self.a.wrapping_add(self.c).wrapping_add(carry);
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(false);
				4
			}

			0x8A => {
				let carry = if self.get_c_flag() { 1 } else { 0 };
				if u8::check_half_carry_add(self.a, self.d, carry) {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				if u8::check_carry_add(self.a, self.d, carry) {
					self.set_c_flag(true);
				} else {
					self.set_c_flag(false);
				}
				self.a = self.a.wrapping_add(self.d).wrapping_add(carry);
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(false);
				4
			}

			0x8B => {
				let carry = if self.get_c_flag() { 1 } else { 0 };
				if u8::check_half_carry_add(self.a, self.e, carry) {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				if u8::check_carry_add(self.a, self.e, carry) {
					self.set_c_flag(true);
				} else {
					self.set_c_flag(false);
				}
				self.a = self.a.wrapping_add(self.e).wrapping_add(carry);
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(false);
				4
			}

			0x8C => {
				let carry = if self.get_c_flag() { 1 } else { 0 };
				if u8::check_half_carry_add(self.a, self.h, carry) {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				if u8::check_carry_add(self.a, self.h, carry) {
					self.set_c_flag(true);
				} else {
					self.set_c_flag(false);
				}
				self.a = self.a.wrapping_add(self.h).wrapping_add(carry);
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(false);
				4
			}

			0x8D => {
				let carry = if self.get_c_flag() { 1 } else { 0 };
				if u8::check_half_carry_add(self.a, self.l, carry) {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				if u8::check_carry_add(self.a, self.l, carry) {
					self.set_c_flag(true);
				} else {
					self.set_c_flag(false);
				}
				self.a = self.a.wrapping_add(self.l).wrapping_add(carry);
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(false);
				4
			}

			0x8E => {
				let x = mmu.read_byte(self.hl());
				let carry = if self.get_c_flag() { 1 } else { 0 };
				if u8::check_half_carry_add(self.a, x, carry) {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				if u8::check_carry_add(self.a, x, carry) {
					self.set_c_flag(true);
				} else {
					self.set_c_flag(false);
				}
				self.a = self.a.wrapping_add(x).wrapping_add(carry);
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(false);
				8
			}

			0x8F => {
				let carry = if self.get_c_flag() { 1 } else { 0 };
				if u8::check_half_carry_add(self.a, self.a, carry) {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				if u8::check_carry_add(self.a, self.a, carry) {
					self.set_c_flag(true);
				} else {
					self.set_c_flag(false);
				}
				self.a = self.a.wrapping_add(self.a).wrapping_add(carry);
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(false);
				4
			}

			0x90 => {
				if {
					let a = self.a;
					let b = self.b;
					u8::check_half_carry_sub(a, b, 0x00)
				} {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				if {
					let a = self.a;
					let b = self.b;
					u8::check_carry_sub(a, b, 0x00)
				} {
					self.set_c_flag(true);
				} else {
					self.set_c_flag(false);
				}
				self.a = self.a.wrapping_sub(self.b);
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(true);
				4
			}

			0x91 => {
				if {
					let a = self.a;
					let b = self.c;
					u8::check_half_carry_sub(a, b, 0x00)
				} {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				if {
					let a = self.a;
					let b = self.c;
					u8::check_carry_sub(a, b, 0x00)
				} {
					self.set_c_flag(true);
				} else {
					self.set_c_flag(false);
				}
				self.a = self.a.wrapping_sub(self.c);
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(true);
				4
			}

			0x92 => {
				if {
					let a = self.a;
					let b = self.d;
					u8::check_half_carry_sub(a, b, 0x00)
				} {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				if {
					let a = self.a;
					let b = self.d;
					u8::check_carry_sub(a, b, 0x00)
				} {
					self.set_c_flag(true);
				} else {
					self.set_c_flag(false);
				}
				self.a = self.a.wrapping_sub(self.d);
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(true);
				4
			}

			0x93 => {
				if {
					let a = self.a;
					let b = self.e;
					u8::check_half_carry_sub(a, b, 0x00)
				} {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				if {
					let a = self.a;
					let b = self.e;
					u8::check_carry_sub(a, b, 0x00)
				} {
					self.set_c_flag(true);
				} else {
					self.set_c_flag(false);
				}
				self.a = self.a.wrapping_sub(self.e);
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(true);
				4
			}

			0x94 => {
				if {
					let a = self.a;
					let b = self.h;
					u8::check_half_carry_sub(a, b, 0x00)
				} {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				if {
					let a = self.a;
					let b = self.h;
					u8::check_carry_sub(a, b, 0x00)
				} {
					self.set_c_flag(true);
				} else {
					self.set_c_flag(false);
				}
				self.a = self.a.wrapping_sub(self.h);
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(true);
				4
			}

			0x95 => {
				if {
					let a = self.a;
					let b = self.l;
					u8::check_half_carry_sub(a, b, 0x00)
				} {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				if {
					let a = self.a;
					let b = self.l;
					u8::check_carry_sub(a, b, 0x00)
				} {
					self.set_c_flag(true);
				} else {
					self.set_c_flag(false);
				}
				self.a = self.a.wrapping_sub(self.l);
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(true);
				4
			}

			0x96 => {
				let x = mmu.read_byte(self.hl());
				if {
					let a = self.a;
					u8::check_half_carry_sub(a, x, 0x00)
				} {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				if {
					let a = self.a;
					u8::check_carry_sub(a, x, 0x00)
				} {
					self.set_c_flag(true);
				} else {
					self.set_c_flag(false);
				}
				self.a = self.a.wrapping_sub(x);
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(true);
				8
			}

			0x97 => {
				self.set_h_flag(false);
				self.set_c_flag(false);
				self.a = self.a.wrapping_sub(self.a);
				self.set_z_flag(true);
				self.set_n_flag(true);
				4
			}

			0x98 => {
				let carry = if self.get_c_flag() { 1 } else { 0 };
				if u8::check_half_carry_sub(self.a, self.b, carry) {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				if u8::check_carry_sub(self.a, self.b, carry) {
					self.set_c_flag(true);
				} else {
					self.set_c_flag(false);
				}
				self.a = self.a.wrapping_sub(self.b).wrapping_sub(carry);
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(true);
				4
			}

			0x99 => {
				let carry = if self.get_c_flag() { 1 } else { 0 };
				if u8::check_half_carry_sub(self.a, self.c, carry) {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				if u8::check_carry_sub(self.a, self.c, carry) {
					self.set_c_flag(true);
				} else {
					self.set_c_flag(false);
				}
				self.a = self.a.wrapping_sub(self.c).wrapping_sub(carry);
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(true);
				4
			}

			0x9A => {
				let carry = if self.get_c_flag() { 1 } else { 0 };
				if u8::check_half_carry_sub(self.a, self.d, carry) {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				if u8::check_carry_sub(self.a, self.d, carry) {
					self.set_c_flag(true);
				} else {
					self.set_c_flag(false);
				}
				self.a = self.a.wrapping_sub(self.d).wrapping_sub(carry);
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(true);
				4
			}

			0x9B => {
				let carry = if self.get_c_flag() { 1 } else { 0 };
				if u8::check_half_carry_sub(self.a, self.e, carry) {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				if u8::check_carry_sub(self.a, self.e, carry) {
					self.set_c_flag(true);
				} else {
					self.set_c_flag(false);
				}
				self.a = self.a.wrapping_sub(self.e).wrapping_sub(carry);
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(true);
				4
			}

			0x9C => {
				let carry = if self.get_c_flag() { 1 } else { 0 };
				if u8::check_half_carry_sub(self.a, self.h, carry) {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				if u8::check_carry_sub(self.a, self.h, carry) {
					self.set_c_flag(true);
				} else {
					self.set_c_flag(false);
				}
				self.a = self.a.wrapping_sub(self.h).wrapping_sub(carry);
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(true);
				4
			}

			0x9D => {
				let carry = if self.get_c_flag() { 1 } else { 0 };
				if u8::check_half_carry_sub(self.a, self.l, carry) {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				if u8::check_carry_sub(self.a, self.l, carry) {
					self.set_c_flag(true);
				} else {
					self.set_c_flag(false);
				}
				self.a = self.a.wrapping_sub(self.l).wrapping_sub(carry);
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(true);
				4
			}

			0x9E => {
				let x = mmu.read_byte(self.hl());
				let carry = if self.get_c_flag() { 1 } else { 0 };
				if u8::check_half_carry_sub(self.a, x, carry) {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				if u8::check_carry_sub(self.a, x, carry) {
					self.set_c_flag(true);
				} else {
					self.set_c_flag(false);
				}
				self.a = self.a.wrapping_sub(x).wrapping_sub(carry);
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(true);
				8
			}

			0x9F => {
				let carry = if self.get_c_flag() { 1 } else { 0 };
				if u8::check_half_carry_sub(self.a, self.a, carry) {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				if u8::check_carry_sub(self.a, self.a, carry) {
					self.set_c_flag(true);
				} else {
					self.set_c_flag(false);
				}
				self.a = self.a.wrapping_sub(self.a).wrapping_sub(carry);
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(true);
				4
			}

			0xA0 => {
				self.a &= self.b;
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(false);
				self.set_h_flag(true);
				self.set_c_flag(false);
				4
			}

			0xA1 => {
				self.a &= self.c;
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(false);
				self.set_h_flag(true);
				self.set_c_flag(false);
				4
			}

			0xA2 => {
				self.a &= self.d;
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(false);
				self.set_h_flag(true);
				self.set_c_flag(false);
				4
			}

			0xA3 => {
				self.a &= self.e;
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(false);
				self.set_h_flag(true);
				self.set_c_flag(false);
				4
			}

			0xA4 => {
				self.a &= self.h;
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(false);
				self.set_h_flag(true);
				self.set_c_flag(false);
				4
			}

			0xA5 => {
				self.a &= self.l;
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(false);
				self.set_h_flag(true);
				self.set_c_flag(false);
				4
			}

			0xA6 => {
				self.a &= mmu.read_byte(self.hl());
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(false);
				self.set_h_flag(true);
				self.set_c_flag(false);
				8
			}

			0xA7 => {
				self.a &= self.a;
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(false);
				self.set_h_flag(true);
				self.set_c_flag(false);
				4
			}

			0xA8 => {
				self.a ^= self.b;
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(false);
				self.set_h_flag(false);
				self.set_c_flag(false);
				4
			}

			0xA9 => {
				self.a ^= self.c;
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(false);
				self.set_h_flag(false);
				self.set_c_flag(false);
				4
			}

			0xAA => {
				self.a ^= self.d;
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(false);
				self.set_h_flag(false);
				self.set_c_flag(false);
				4
			}

			0xAB => {
				self.a ^= self.e;
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(false);
				self.set_h_flag(false);
				self.set_c_flag(false);
				4
			}

			0xAC => {
				self.a ^= self.h;
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(false);
				self.set_h_flag(false);
				self.set_c_flag(false);
				4
			}

			0xAD => {
				self.a ^= self.l;
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(false);
				self.set_h_flag(false);
				self.set_c_flag(false);
				4
			}

			0xAE => {
				self.a ^= mmu.read_byte(self.hl());
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(false);
				self.set_h_flag(false);
				self.set_c_flag(false);
				8
			}

			0xAF => {
				self.a ^= self.a;
				self.set_z_flag(true);
				self.set_n_flag(false);
				self.set_h_flag(false);
				self.set_c_flag(false);
				4
			}

			0xB0 => {
				self.a |= self.b;
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(false);
				self.set_h_flag(false);
				self.set_c_flag(false);
				4
			}

			0xB1 => {
				self.a |= self.c;
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(false);
				self.set_h_flag(false);
				self.set_c_flag(false);
				4
			}

			0xB2 => {
				self.a |= self.d;
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(false);
				self.set_h_flag(false);
				self.set_c_flag(false);
				4
			}

			0xB3 => {
				self.a |= self.e;
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(false);
				self.set_h_flag(false);
				self.set_c_flag(false);
				4
			}

			0xB4 => {
				self.a |= self.h;
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(false);
				self.set_h_flag(false);
				self.set_c_flag(false);
				4
			}

			0xB5 => {
				self.a |= self.l;
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(false);
				self.set_h_flag(false);
				self.set_c_flag(false);
				4
			}

			0xB6 => {
				self.a |= mmu.read_byte(self.hl());
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(false);
				self.set_h_flag(false);
				self.set_c_flag(false);
				8
			}

			0xB7 => {
				self.a |= self.a;
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(false);
				self.set_h_flag(false);
				self.set_c_flag(false);
				4
			}

			0xB8 => {
				if {
					let a = self.a;
					let b = self.b;
					u8::check_half_carry_sub(a, b, 0x00)
				} {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				if {
					let a = self.a;
					let b = self.b;
					u8::check_carry_sub(a, b, 0x00)
				} {
					self.set_c_flag(true);
				} else {
					self.set_c_flag(false);
				}
				if self.a.wrapping_sub(self.b) == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(true);
				4
			}

			0xB9 => {
				if {
					let a = self.a;
					let b = self.c;
					u8::check_half_carry_sub(a, b, 0x00)
				} {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				if {
					let a = self.a;
					let b = self.c;
					u8::check_carry_sub(a, b, 0x00)
				} {
					self.set_c_flag(true);
				} else {
					self.set_c_flag(false);
				}
				if self.a.wrapping_sub(self.c) == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(true);
				4
			}

			0xBA => {
				if {
					let a = self.a;
					let b = self.d;
					u8::check_half_carry_sub(a, b, 0x00)
				} {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				if {
					let a = self.a;
					let b = self.d;
					u8::check_carry_sub(a, b, 0x00)
				} {
					self.set_c_flag(true);
				} else {
					self.set_c_flag(false);
				}
				if self.a.wrapping_sub(self.d) == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(true);
				4
			}

			0xBB => {
				if {
					let a = self.a;
					let b = self.e;
					u8::check_half_carry_sub(a, b, 0x00)
				} {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				if {
					let a = self.a;
					let b = self.e;
					u8::check_carry_sub(a, b, 0x00)
				} {
					self.set_c_flag(true);
				} else {
					self.set_c_flag(false);
				}
				if self.a.wrapping_sub(self.e) == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(true);
				4
			}

			0xBC => {
				if {
					let a = self.a;
					let b = self.h;
					u8::check_half_carry_sub(a, b, 0x00)
				} {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				if {
					let a = self.a;
					let b = self.h;
					u8::check_carry_sub(a, b, 0x00)
				} {
					self.set_c_flag(true);
				} else {
					self.set_c_flag(false);
				}
				if self.a.wrapping_sub(self.h) == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(true);
				4
			}

			0xBD => {
				if {
					let a = self.a;
					let b = self.l;
					u8::check_half_carry_sub(a, b, 0x00)
				} {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				if {
					let a = self.a;
					let b = self.l;
					u8::check_carry_sub(a, b, 0x00)
				} {
					self.set_c_flag(true);
				} else {
					self.set_c_flag(false);
				}
				if self.a.wrapping_sub(self.l) == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(true);
				4
			}

			0xBE => {
				let x = mmu.read_byte(self.hl());
				if {
					let a = self.a;
					u8::check_half_carry_sub(a, x, 0x00)
				} {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				if {
					let a = self.a;
					u8::check_carry_sub(a, x, 0x00)
				} {
					self.set_c_flag(true);
				} else {
					self.set_c_flag(false);
				}
				if self.a.wrapping_sub(x) == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(true);
				8
			}

			0xBF => {
				self.set_h_flag(false);
				self.set_c_flag(false);
				self.set_z_flag(true);
				self.set_n_flag(true);
				4
			}

			0xC0 => {
				if !self.get_z_flag() {
					self.pc = self.pop_stack(mmu);
					20
				} else {
					8
				}
			}

			0xC1 => {
				let x = self.pop_stack(mmu);
				self.set_bc(x);
				12
			}

			0xC2 => {
				let address = u16::from_le_bytes([self.get_byte(mmu), self.get_byte(mmu)]);
				if !self.get_z_flag() {
					self.pc = address;
					16
				} else {
					12
				}
			}

			0xC3 => {
				self.pc = u16::from_le_bytes([self.get_byte(mmu), self.get_byte(mmu)]);
				16
			}

			0xC4 => {
				let x = u16::from_le_bytes([self.get_byte(mmu), self.get_byte(mmu)]);
				if !self.get_z_flag() {
					self.push_stack(mmu, self.pc);
					self.pc = x;
					24
				} else {
					12
				}
			}

			0xC5 => {
				self.push_stack(mmu, self.bc());
				16
			}

			0xC6 => {
				let x = self.get_byte(mmu);
				if {
					let a = self.a;
					u8::check_half_carry_add(a, x, 0x00)
				} {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				if {
					let a = self.a;
					u8::check_carry_add(a, x, 0x00)
				} {
					self.set_c_flag(true);
				} else {
					self.set_c_flag(false);
				}
				self.a = self.a.wrapping_add(x);
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(false);
				8
			}

			0xC7 => {
				self.push_stack(mmu, self.pc);
				self.pc = 0x0000;
				16
			}

			0xC8 => {
				if self.get_z_flag() {
					self.pc = self.pop_stack(mmu);
					20
				} else {
					8
				}
			}

			0xC9 => {
				self.pc = self.pop_stack(mmu);
				16
			}

			0xCA => {
				let address = u16::from_le_bytes([self.get_byte(mmu), self.get_byte(mmu)]);
				if self.get_z_flag() {
					self.pc = address;
					16
				} else {
					12
				}
			}

			0xCB => self.execute_prefixed(mmu),

			0xCC => {
				let address = u16::from_le_bytes([self.get_byte(mmu), self.get_byte(mmu)]);
				if self.get_z_flag() {
					self.push_stack(mmu, self.pc);
					self.pc = address;
					24
				} else {
					12
				}
			}

			0xCD => {
				let address = u16::from_le_bytes([self.get_byte(mmu), self.get_byte(mmu)]);
				self.push_stack(mmu, self.pc);
				self.pc = address;
				24
			}

			0xCE => {
				let x = self.get_byte(mmu);
				let carry = if self.get_c_flag() { 1 } else { 0 };
				if u8::check_half_carry_add(self.a, x, carry) {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				if u8::check_carry_add(self.a, x, carry) {
					self.set_c_flag(true);
				} else {
					self.set_c_flag(false);
				}
				self.a = self.a.wrapping_add(x).wrapping_add(carry);
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(false);
				8
			}

			0xCF => {
				self.push_stack(mmu, self.pc);
				self.pc = 0x0008;
				16
			}

			0xD0 => {
				if !self.get_c_flag() {
					self.pc = self.pop_stack(mmu);
					20
				} else {
					8
				}
			}

			0xD1 => {
				let x = self.pop_stack(mmu);
				self.set_de(x);
				12
			}

			0xD2 => {
				let address = u16::from_le_bytes([self.get_byte(mmu), self.get_byte(mmu)]);
				if !self.get_c_flag() {
					self.pc = address;
					16
				} else {
					12
				}
			}

			0xD4 => {
				let address = u16::from_le_bytes([self.get_byte(mmu), self.get_byte(mmu)]);
				if !self.get_c_flag() {
					self.push_stack(mmu, self.pc);
					self.pc = address;
					24
				} else {
					12
				}
			}

			0xD5 => {
				self.push_stack(mmu, self.de());
				16
			}

			0xD6 => {
				let x = self.get_byte(mmu);
				if {
					let a = self.a;
					u8::check_half_carry_sub(a, x, 0x00)
				} {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				if {
					let a = self.a;
					u8::check_carry_sub(a, x, 0x00)
				} {
					self.set_c_flag(true);
				} else {
					self.set_c_flag(false);
				}
				self.a = self.a.wrapping_sub(x);
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(true);
				8
			}

			0xD7 => {
				self.push_stack(mmu, self.pc);
				self.pc = 0x0010;
				16
			}

			0xD8 => {
				if self.get_c_flag() {
					self.pc = self.pop_stack(mmu);
					20
				} else {
					8
				}
			}

			0xD9 => {
				self.pc = self.pop_stack(mmu);
				self.ime = true;
				16
			}

			0xDA => {
				let address = u16::from_le_bytes([self.get_byte(mmu), self.get_byte(mmu)]);
				if self.get_c_flag() {
					self.pc = address;
					16
				} else {
					12
				}
			}

			0xDC => {
				let address = u16::from_le_bytes([self.get_byte(mmu), self.get_byte(mmu)]);
				if self.get_c_flag() {
					self.push_stack(mmu, self.pc);
					self.pc = address;
					24
				} else {
					12
				}
			}

			0xDE => {
				let x = self.get_byte(mmu);
				let carry = if self.get_c_flag() { 1 } else { 0 };
				if u8::check_half_carry_sub(self.a, x, carry) {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				if u8::check_carry_sub(self.a, x, carry) {
					self.set_c_flag(true);
				} else {
					self.set_c_flag(false);
				}
				self.a = self.a.wrapping_sub(x).wrapping_sub(carry);
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(true);
				8
			}

			0xDF => {
				self.push_stack(mmu, self.pc);
				self.pc = 0x0018;
				16
			}

			0xE0 => {
				mmu.write_byte(0xFF00 | self.get_byte(mmu) as u16, self.a);
				12
			}

			0xE1 => {
				let x = self.pop_stack(mmu);
				self.set_hl(x);
				12
			}

			0xE2 => {
				mmu.write_byte(0xFF00 | self.c as u16, self.a);
				8
			}

			0xE5 => {
				self.push_stack(mmu, self.hl());
				16
			}

			0xE6 => {
				self.a &= self.get_byte(mmu);
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(false);
				self.set_h_flag(true);
				self.set_c_flag(false);
				8
			}

			0xE7 => {
				self.push_stack(mmu, self.pc);
				self.pc = 0x0020;
				16
			}

			0xE8 => {
				let x = self.get_byte(mmu);
				self.set_z_flag(false);
				self.set_n_flag(false);
				if {
					let a = self.sp as u8;
					u8::check_half_carry_add(a, x, 0x00)
				} {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				if {
					let a = self.sp as u8;
					u8::check_carry_add(a, x, 0x00)
				} {
					self.set_c_flag(true);
				} else {
					self.set_c_flag(false);
				}
				self.sp = self.sp.wrapping_add_signed((x as i8) as i16);
				16
			}

			0xE9 => {
				self.pc = self.hl();
				4
			}

			0xEA => {
				let address = u16::from_le_bytes([self.get_byte(mmu), self.get_byte(mmu)]);
				mmu.write_byte(address, self.a);
				16
			}

			0xEE => {
				self.a ^= self.get_byte(mmu);
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(false);
				self.set_h_flag(false);
				self.set_c_flag(false);
				8
			}

			0xEF => {
				self.push_stack(mmu, self.pc);
				self.pc = 0x0028;
				16
			}

			0xF0 => {
				self.a = mmu.read_byte(0xFF00 | self.get_byte(mmu) as u16);
				12
			}

			0xF1 => {
				let x = self.pop_stack(mmu);
				self.set_af(x);
				12
			}

			0xF2 => {
				self.a = mmu.read_byte(0xFF00 | self.c as u16);
				8
			}

			0xF3 => {
				self.ime = false;
				self.ime_scheduled = false;
				4
			}

			0xF5 => {
				self.push_stack(mmu, self.af());
				16
			}

			0xF6 => {
				self.a |= self.get_byte(mmu);
				if self.a == 0x00 {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(false);
				self.set_h_flag(false);
				self.set_c_flag(false);
				8
			}

			0xF7 => {
				self.push_stack(mmu, self.pc);
				self.pc = 0x0030;
				16
			}

			0xF8 => {
				let x = self.get_byte(mmu);
				self.set_z_flag(false);
				self.set_n_flag(false);
				if {
					let a = self.sp as u8;
					u8::check_half_carry_add(a, x, 0x00)
				} {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				if {
					let a = self.sp as u8;
					u8::check_carry_add(a, x, 0x00)
				} {
					self.set_c_flag(true);
				} else {
					self.set_c_flag(false);
				}
				self.set_hl(self.sp.wrapping_add_signed((x as i8) as i16));
				12
			}

			0xF9 => {
				self.sp = self.hl();
				8
			}

			0xFA => {
				let address = u16::from_le_bytes([self.get_byte(mmu), self.get_byte(mmu)]);
				self.a = mmu.read_byte(address);
				16
			}

			0xFB => {
				self.ime_scheduled = true;
				4
			}

			0xFE => {
				let x = self.get_byte(mmu);
				if self.a == x {
					self.set_z_flag(true);
				} else {
					self.set_z_flag(false);
				}
				self.set_n_flag(true);
				if {
					let a = self.a;
					u8::check_half_carry_sub(a, x, 0x00)
				} {
					self.set_h_flag(true);
				} else {
					self.set_h_flag(false);
				}
				if {
					let a = self.a;
					u8::check_carry_sub(a, x, 0x00)
				} {
					self.set_c_flag(true);
				} else {
					self.set_c_flag(false);
				}
				8
			}

			0xFF => {
				self.push_stack(mmu, self.pc);
				self.pc = 0x0038;
				16
			}

			_ => panic!("opcode: {:02X?}, not implemented", opcode),
		};

		if self.ime_scheduled && opcode != 0xFB {
			self.ime = true;
			self.ime_scheduled = false;
		}

		if mmu.read_byte(0xFF02) == 0x81 {
			print!("{}", char::from_u32(mmu.read_byte(0xFF01) as u32).unwrap());
			mmu.write_byte(0xFF02, 0x00);
		}

		cycles
	}
}
