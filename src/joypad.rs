use crate::utils::is_bit_set;

#[derive(Clone, Copy, Debug)]
pub enum Button {
	A = 0,
	B = 1,
	SELECT = 2,
	START = 3,
	RIGHT = 4,
	LEFT = 5,
	UP = 6,
	DOWN = 7,
	UNKNOWN,
}

impl Button {
	pub fn values() -> [Button; 9] {
		[
			Button::A,
			Button::B,
			Button::SELECT,
			Button::START,
			Button::RIGHT,
			Button::LEFT,
			Button::UP,
			Button::DOWN,
			Button::UNKNOWN,
		]
	}
}

pub struct Joypad(u8);

impl Joypad {
	pub fn new() -> Joypad {
		Joypad(0xFF)
	}

	pub fn read(&self, r_joypad: u8) -> u8 {
		// println!("joypad register: {:08b}, state: {:08b}", r_joypad, self.0);
		(r_joypad & 0xF0)
			| match (is_bit_set(r_joypad, 4), is_bit_set(r_joypad, 5)) {
				(false, false) => 0x0F & (self.0 | (self.0 >> 4)), // both action & direction buttons
				(false, true) => 0x0F & (self.0 >> 4),             // only direction buttons
				(true, false) => 0x0F & self.0,                    // only action buttons (SsBA)
				(true, true) => 0x0F,                              // none
			}
	}

	pub fn pressed(&mut self, button: Button) -> bool {
		match button {
			Button::UNKNOWN => false,
			b if !is_bit_set(self.0, b as u8) => false,
			b => {
				self.0 &= !(1 << b as u8);
				// println!("button pressed: {:?}, joypad: {:08b}", button, self.0);
				true
			}
		}
	}

	pub fn released(&mut self, button: Button) {
		match button {
			Button::UNKNOWN => (),
			_ => {
				self.0 |= 1 << button as u8;
				// println!("button released: {:?}, joypad: {:08b}", button, self.0);
			}
		}
	}
}
