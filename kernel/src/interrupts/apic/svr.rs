/// A special situation may occur when a processor raises its task priority to be greater than or equal to the level of the
/// interrupt for which the processor INTR signal is currently being asserted. If at the time the INTA cycle is issued, the
/// interrupt that was to be dispensed has become masked (programmed by software), the local APIC will deliver a
/// spurious-interrupt vector. Dispensing the spurious-interrupt vector does not affect the ISR, so the handler for this
/// vector should return without an EOI.
pub struct SpuriousInterruptVectorRegister(pub u32);

impl SpuriousInterruptVectorRegister {
    pub fn bits(&self) -> u32 {
        self.0
    }
    pub fn from_bits_retain(bits: u32) -> Self {
        Self(bits)
    }

    /// Determines the vector number to be delivered to the processor when the local APIC generates
    /// a spurious vector.
    ///
    /// (Pentium 4 and Intel Xeon processors.) Bits 0 through 7 of the this field are programmable by
    /// software.
    ///
    /// (P6 family and Pentium processors). Bits 4 through 7 of the this field are programmable by
    /// software, and bits 0 through 3 are hardwired to logical ones. Software writes to bits 0 through
    /// 3 have no effect.
    pub fn vector(&self) -> u8 {
        (self.0 & 0xFF) as u8
    }

    /// Determines the vector number to be delivered to the processor when the local APIC generates
    /// a spurious vector.
    ///
    /// (Pentium 4 and Intel Xeon processors.) Bits 0 through 7 of the this field are programmable by
    /// software.
    ///
    /// (P6 family and Pentium processors). Bits 4 through 7 of the this field are programmable by
    /// software, and bits 0 through 3 are hardwired to logical ones. Software writes to bits 0 through
    /// 3 have no effect.
    pub fn set_vector(&mut self, vector: u8) {
        self.0 = (self.0 & !0xFF) | (vector as u32);
    }

    /// Allows software to temporarily enable (1) or disable (0) the local APIC (see Section 11.4.3,
    /// "Enabling or Disabling the Local APIC").
    pub fn apic_enabled(&self) -> bool {
        (self.0 >> 8) & 1 != 0
    }

    /// Allows software to temporarily enable (1) or disable (0) the local APIC (see Section 11.4.3,
    /// "Enabling or Disabling the Local APIC").
    pub fn set_apic_enabled(&mut self, enabled: bool) {
        self.0 &= !(1 << 8);
        self.0 |= (enabled as u32) << 8;
    }

    /// Determines if focus processor checking is enabled (0) or disabled (1) when using the lowest-
    /// priority delivery mode. In Pentium 4 and Intel Xeon processors, this bit is reserved and should
    /// be cleared to 0.
    ///
    /// Not supported in Pentium 4 and Intel Xeon processors.
    pub fn focus_processor_checking(&self) -> bool {
        (self.0 >> 9) & 1 != 0
    }

    /// Determines if focus processor checking is enabled (0) or disabled (1) when using the lowest-
    /// priority delivery mode. In Pentium 4 and Intel Xeon processors, this bit is reserved and should
    /// be cleared to 0.
    ///
    /// Not supported in Pentium 4 and Intel Xeon processors.
    pub fn set_focus_processor_checking(&mut self, enabled: bool) {
        self.0 &= !(1 << 9);
        self.0 |= (enabled as u32) << 9;
    }

    /// Determines whether an EOI for a level-triggered interrupt causes EOI messages to be
    /// broadcast  to the I/O APICs (0) or not (1). See Section 11.8.5. The default value for this
    /// bit is 0, indicating that EOI broadcasts are performed. This bit is reserved to 0 if the
    /// processor does not support EOI-broadcast suppression.
    ///
    /// Not supported on all processors. See bit 24 of Local APIC Version Register.
    pub fn eoi_broadcast_suppression(&self) -> bool {
        (self.0 >> 12) & 1 != 0
    }

    /// Determines whether an EOI for a level-triggered interrupt causes EOI messages to be
    /// broadcast  to the I/O APICs (0) or not (1). See Section 11.8.5. The default value for this
    /// bit is 0, indicating that EOI broadcasts are performed. This bit is reserved to 0 if the
    /// processor does not support EOI-broadcast suppression.
    ///
    /// Not supported on all processors. See bit 24 of Local APIC Version Register.
    pub fn set_eoi_broadcast_suppression(&mut self, enabled: bool) {
        self.0 &= !(1 << 12);
        self.0 |= (enabled as u32) << 12;
    }
}
