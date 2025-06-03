mod cartridge;
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
	env, fs, thread,
	time::{Duration, SystemTime},
};

const DEBUG_FLAG: bool = false;
const WIDTH: usize = 160;
const HEIGHT: usize = 144;

impl From<Button> for Key {
	fn from(button: Button) -> Self {
		match button {
			Button::A => Key::J,
			Button::B => Key::K,
			Button::SELECT => Key::Backspace,
			Button::START => Key::Enter,
			Button::RIGHT => Key::D,
			Button::LEFT => Key::A,
			Button::UP => Key::W,
			Button::DOWN => Key::S,
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

fn main() {
	let cwd = env::current_dir().expect("unable to get current working directory");
	let cartridge = fs::read(cwd.join("rom.gb")).expect("unable to load cartridge");
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
	let mut mmu = MMU::new(cartridge);
	let mut cpu = CPU::new();
	let mut ppu = PPU::new(&mmu);
	let mut frames = 0;
	let start = SystemTime::now();

	while window.is_open() && !window.is_key_down(Key::Escape) {
		let cycles = cpu.execute_next(&mut mmu);
		(0..cycles).for_each(|_| {
			mmu.update_timers(1);
			ppu.tick(&mut mmu);

			if ppu.is_frame_ready() {
				window.set_title(
					format!(
						"RustBoy - FPS: {}",
						1_000_000 * frames / start.elapsed().unwrap().as_micros()
					)
					.as_str(),
				);
				let _ = window.update_with_buffer(ppu.get_frame_buffer(), WIDTH, HEIGHT);
				frames += 1;
				thread::sleep(Duration::from_millis(12));
				Button::values()
					.iter()
					.for_each(|button| update_joypad_key(&window, &mut mmu, *button));
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
