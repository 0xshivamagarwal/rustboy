mod cpu;
mod joypad;
mod mmu;
mod ppu;
mod utils;

use cpu::CPU;
use joypad::Button;
use minifb::{Key, Scale, ScaleMode, Window, WindowOptions};
use mmu::MMU;
use ppu::PPU;
use std::{
	env,
	fs::{self, File},
	io::BufWriter,
	thread,
	time::{Duration, SystemTime},
};

const DEBUG_FLAG: bool = false;
const WIDTH: usize = 160;
const HEIGHT: usize = 144;

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

trait Checks {
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

// fn test_ppu_modes() {
// 	let max_cycles_per_scanline = 456;
// 	let mut curr_mode = Modes::VBLANK;
// 	let mut curr_ly = 0;
// 	let mut curr_lx = 0;
// 	let mut cycles_spent = 0;
//
// 	while curr_ly != 154 {
// 		print!(
// 			"{}",
// 			match curr_mode {
// 				Modes::HBLANK => 'H',
// 				Modes::VBLANK => 'V',
// 				Modes::OAMSCAN => 'O',
// 				Modes::RENDER => 'R',
// 			}
// 		);
// 		curr_mode = match (curr_mode, curr_ly, cycles_spent) {
// 			(Modes::VBLANK, 0, 0) => Modes::OAMSCAN,
// 			(Modes::OAMSCAN, ly, c) if ly < 0x90 && c < 0x50 => Modes::OAMSCAN,
// 			(Modes::OAMSCAN, ly, 80) if ly < 0x90 => Modes::RENDER,
// 			(Modes::RENDER, ly, _) if ly < 0x90 => {
// 				if curr_lx < 0xA0 {
// 					Modes::RENDER
// 				} else {
// 					Modes::HBLANK
// 				}
// 			}
// 			(Modes::HBLANK, ly, 0) if ly < 0x90 => Modes::OAMSCAN,
// 			(Modes::HBLANK, _, 0) => Modes::VBLANK,
// 			(Modes::HBLANK, ly, _) if ly < 0x90 => Modes::HBLANK,
// 			(Modes::VBLANK, ly, _) if ly < 0x9A => Modes::VBLANK,
// 			_ => unreachable!(),
// 		};
//
// 		if curr_mode == Modes::RENDER {
// 			curr_lx += 1;
// 		}
// 		cycles_spent = (cycles_spent + 1) % max_cycles_per_scanline;
// 		if cycles_spent == 0 {
// 			println!();
// 			curr_lx = 0;
// 			curr_ly += 1;
// 			// curr_ly = (curr_ly + 1) % 154;
// 		}
// 	}
// }

impl From<Button> for Key {
	fn from(button: Button) -> Self {
		match button {
			Button::RIGHT => Key::D,
			Button::A => Key::J,
			Button::LEFT => Key::A,
			Button::B => Key::K,
			Button::UP => Key::W,
			Button::SELECT => Key::Backspace,
			Button::DOWN => Key::S,
			Button::START => Key::Enter,
			Button::UNKNOWN => Key::Unknown,
		}
	}
}

fn update_joypad_key(window: &Window, mmu: &mut MMU, button: Button) {
	match window.is_key_down(Key::from(button)) {
		true => mmu.press_key(button),
		false => mmu.release_key(button),
	};
}

fn update_joypad(window: &Window, mmu: &mut MMU) {
	update_joypad_key(window, mmu, Button::UP);
	update_joypad_key(window, mmu, Button::DOWN);
	update_joypad_key(window, mmu, Button::LEFT);
	update_joypad_key(window, mmu, Button::RIGHT);
	update_joypad_key(window, mmu, Button::A);
	update_joypad_key(window, mmu, Button::B);
	update_joypad_key(window, mmu, Button::START);
	update_joypad_key(window, mmu, Button::SELECT);
}

fn main() {
	let cwd = env::current_dir().expect("unable to get current working directory");
	let cartridge = fs::read(cwd.join("rom.gb")).expect("unable to load cartridge");
	let debug_file = File::create(cwd.join("debug.log")).expect("unable to create debug file");
	let mut debug_buffer = BufWriter::new(debug_file);
	let mut window = Window::new(
		"RustBoy",
		WIDTH,
		HEIGHT,
		WindowOptions {
			resize: true,
			scale: Scale::X4,
			scale_mode: ScaleMode::AspectRatioStretch,
			..WindowOptions::default()
		},
	)
	.expect("unable to create window");
	let mut mmu = MMU::new(&cartridge[..]);
	let mut cpu = CPU::new();
	let mut ppu = PPU::new(&mmu);

	// display_cartridge_info(memory.get(0x0100..0x0150).unwrap());
	window.update();

	let mut frames = 0;
	let start = SystemTime::now();

	// TODO:
	// -- PPU:
	// 1. add window functionality
	// 2. check lcdc register bits add functionality if any not used
	// 3. check stat register bits add functionality if any noy used
	// 4. enable / disable vram & oam access after mode changes
	//
	// -- CPU:
	// 1. improve timings

	while window.is_open() && !window.is_key_down(Key::Escape) {
		// mmu.read_byte(0xFF00);

		let cycles = cpu.execute_next(&mut mmu, &mut debug_buffer);
		(0..cycles).for_each(|_| {
			mmu.update_timers(1);
			ppu.tick(&mut mmu);
			if ppu.is_frame_ready() {
				let _ = window.update_with_buffer(ppu.get_frame_buffer(), WIDTH, HEIGHT);
				frames += 1;
				thread::sleep(Duration::from_millis(8));
				update_joypad(&window, &mut mmu);
			}
		});
	}

	println!(
		"frames: {}, time elapsed: {:?}, fps: {}",
		frames,
		start.elapsed(),
		1_000_000.0 * (frames as f32) / (start.elapsed().unwrap().as_micros() as f32)
	);
}
