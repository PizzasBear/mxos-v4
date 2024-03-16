pub struct LocalAPICVersion(pub u32);

impl LocalAPICVersion {
    pub fn bits(&self) -> u32 {
        self.0
    }
    pub fn from_bits_retain(bits: u32) -> Self {
        LocalAPICVersion(bits)
    }

    pub fn version(&self) -> u8 {
        (self.0 & 0xFF) as u8
    }
    /// Shows the number of LVT entries minus 1. For the Pentium 4 and Intel Xeon processors (which
    /// have 6 LVT entries), the value returned in the Max LVT field is 5; for the P6 family
    /// processors (which have 5 LVT entries), the value returned is 4; for the Pentium processor
    /// (which has 4 LVT entries), the value returned is 3. For processors based on the Nehalem
    /// microarchitecture (which has 7 LVT entries) and onward, the value returned is 6.
    pub fn max_lvt_entry(&self) -> u8 {
        (self.0 >> 16) as u8
    }

    /// The EAS bit when set to 1 indicates the presence of an extended APIC register space,
    /// starting at offset 400h.
    ///
    /// Not supported by us.
    pub fn extended_apic_space(&self) -> bool {
        (self.0 >> 31) & 1 != 0
    }

    /// Indicates whether software can inhibit the broadcast of EOI message by setting bit 12 of the
    /// Spurious Interrupt Vector Register; see Section 11.8.5 and Section 11.9.
    pub fn support_eoi_broadcast(&self) -> bool {
        (self.0 >> 24) & 1 != 0
    }
}
