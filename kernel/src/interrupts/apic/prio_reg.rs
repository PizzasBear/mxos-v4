pub struct PriorityRegisiter(u32);

impl PriorityRegisiter {
    pub fn new() -> Self {
        Self(0)
    }
    pub fn bits(&self) -> u32 {
        self.0
    }
    pub fn from_bits_retain(bits: u32) -> Self {
        Self(bits)
    }
    pub fn class(&self) -> u8 {
        (self.0 >> 4) as _
    }
    pub fn set_class(&mut self, class: u8) {
        self.0 &= !(15 << 4);
        self.0 |= (15 & class as u32) << 4;
    }
    pub fn subclass(&self) -> u8 {
        (self.0 & 15) as _
    }
    pub fn set_subclass(&mut self, subclass: u8) {
        self.0 &= !15;
        self.0 |= 15 & subclass as u32;
    }
}
