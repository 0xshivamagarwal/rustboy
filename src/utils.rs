pub fn is_bit_set(value: u8, bit: u8) -> bool {
	(value >> bit) & 0x01 == 0x01
}

pub trait Checks {
	fn check_half_carry_add(a: Self, b: Self, c: Self) -> bool;

	fn check_half_carry_sub(a: Self, b: Self, c: Self) -> bool;

	fn check_carry_add(a: Self, b: Self, c: Self) -> bool;

	fn check_carry_sub(a: Self, b: Self, c: Self) -> bool;
}

impl Checks for u8 {
	fn check_half_carry_add(a: u8, b: u8, c: u8) -> bool {
		(a & 0x0F) + (b & 0x0F) + (c & 0x0F) > 0x0F
	}

	fn check_half_carry_sub(a: u8, b: u8, c: u8) -> bool {
		(b & 0x0F) + (c & 0x0F) > (a & 0x0F)
	}

	fn check_carry_add(a: u8, b: u8, c: u8) -> bool {
		let (r, o1) = a.overflowing_add(b);
		let (_, o2) = r.overflowing_add(c);
		o1 || o2
	}

	fn check_carry_sub(a: u8, b: u8, c: u8) -> bool {
		if a == b {
			return c == 0x01;
		}
		b > a || b + c > a
	}
}

impl Checks for u16 {
	fn check_half_carry_add(a: u16, b: u16, c: u16) -> bool {
		(a & 0x0FFF) + (b & 0x0FFF) + (c & 0xFFFF) > 0x0FFF
	}

	fn check_half_carry_sub(_: u16, _: u16, _: u16) -> bool {
		unimplemented!();
	}

	fn check_carry_add(a: u16, b: u16, c: u16) -> bool {
		let (r, o1) = a.overflowing_add(b);
		let (_, o2) = r.overflowing_add(c);
		o1 || o2
	}

	fn check_carry_sub(_: u16, _: u16, _: u16) -> bool {
		unimplemented!();
	}
}
