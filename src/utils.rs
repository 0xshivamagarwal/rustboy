pub fn is_bit_set(value: u8, bit: u8) -> bool {
	(value >> bit) & 0x01 == 0x01
}
