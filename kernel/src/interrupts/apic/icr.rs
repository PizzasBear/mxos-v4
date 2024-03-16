use super::TriggerMode;

/// The interrupt command register (ICR) is a 64-bit4 local APIC register (see Figure 11-12) that allows software
/// running on the processor to specify and send interprocessor interrupts (IPIs) to other processors in the system.
///
/// To send an IPI, software must set up the ICR to indicate the type of IPI message to be sent and the destination
/// processor or processors. (All fields of the ICR are read-write by software with the exception of the delivery status
/// field, which is read-only.)
pub struct InterruptCommandRegister(pub u64);

/// Specifies the type of IPI to be sent. This field is also know as the IPI message type field.
pub enum ICRDeliveryMode {
    /// Delivers the interrupt specified in the vector field to the target processor or processors.
    Fixed = 0b000,
    /// Same as fixed mode, except that the interrupt is delivered to the processor
    /// executing at the lowest priority among the set of processors specified in
    /// the destination field. The ability for a processor to send a lowest priority
    /// IPI is model specific and should be avoided by BIOS and operating system
    /// software.
    LowestPriority = 0b001,
    /// Delivers an SMI interrupt to the target processor or processors. The vector field must be
    /// programmed to 00H for future compatibility.
    SMI = 0b010,
    /// Delivers an NMI interrupt to the target processor or processors. The vector information is ignored.
    NMI = 0b100,
    /// # INIT
    /// Delivers an INIT request to the target processor or processors, which causes them to perform
    /// an INIT. As a result of this IPI message, all the target processors perform an INIT. The vector
    /// field must be programmed to 00H for future compatibility.
    ///
    /// # INIT Level De-assert
    /// (Not supported in the Pentium 4 and Intel Xeon processors.) Sends a synchronization message to
    /// all the local APICs in the system to set their arbitration IDs (stored in their Arb ID registers)
    /// to the values of their APIC IDs (see Section 11.7, "System and APIC Bus Arbitration"). For this
    /// delivery mode, the level flag must be set to 0 and trigger mode flag to 1. This IPI is sent to all
    /// processors, regardless of the value in the destination field or the destination shorthand field;
    /// however, software should specify the "all including self" shorthand.
    INIT = 0b101,
    /// Sends a special "start-up" IPI (called a SIPI) to the target processor or processors. The vector
    /// typically points to a start-up routine that is part of the BIOS boot-strap code (see Section 9.4,
    /// "Multiple-Processor (MP) Initialization"). IPIs sent with this delivery mode are not automatically
    /// retried if the source APIC is unable to deliver it. It is up to the software to determine if the
    /// SIPI was not successfully delivered and to reissue the SIPI if necessary.
    StartUp = 0b110,
}

/// Indicates the IPI delivery status
pub enum ICRDeliveryStatus {
    /// Indicates that this local APIC has completed sending any previous IPIs.
    Idle = 0,
    /// Indicates that this local APIC has not completed sending the last IPI
    SendPending = 1,
}

pub enum ICRDestinationMode {
    Physical = 0,
    Logical = 1,
}

/// For the INIT level de-assert delivery mode this flag must be set to 0; for all other delivery
/// modes it must be set to 1. (This flag has no meaning in Pentium 4 and Intel Xeon processors,
/// and will always be issued as a 1.)
pub enum ICRLevel {
    DeAssert = 0,
    Assert = 1,
}

/// Indicates whether a shorthand notation is used to specify the destination of the interrupt and,
/// if so, which shorthand is used. Destination shorthands are used in place of the 8-bit destination
/// field, and can be sent by software using a single write to the low doubleword of the ICR. Shorthands
/// are defined for the following cases: software self interrupt, IPIs to all processors in the system
/// including the sender, IPIs to all processors in the system excluding the sender.
pub enum ICRDestinationShorthand {
    /// The destination is specified in the destination field.
    NoShorthand = 0b00,
    /// The issuing APIC is the one and only destination of the IPI. This destination shorthand allows
    /// software to interrupt the processor on which it is executing. An APIC implementation is free to
    /// deliver the self-interrupt message internally or to issue the message to the bus and "snoop" it
    /// as with any other IPI message.
    Myself = 0b01,
    /// The IPI is sent to all processors in the system including the processor sending the IPI. The APIC
    /// will broadcast an IPI message with the destination field set to FH for Pentium and P6 family
    /// processors and to FFH for Pentium 4 and Intel Xeon processors.
    AllIncludingSelf = 0b10,
    /// The IPI is sent to all processors in a system with the exception of the processor sending the IPI.
    /// The APIC broadcasts a message with the physical destination mode and destination field set to FH
    /// for Pentium and P6 family processors and to FFH for Pentium 4 and Intel Xeon processors. Support
    /// for this destination shorthand in conjunction with the lowest-priority delivery mode is model
    /// specific. For Pentium 4 and Intel Xeon processors, when this shorthand is used together with
    /// lowest priority delivery mode, the IPI may be redirected back to the issuing processor.
    AllExcludingSelf = 0b11,
}

impl InterruptCommandRegister {
    /// The vector number of the interrupt being sent.
    pub fn vector(&self) -> u8 {
        (self.0 & 0xFF) as u8
    }
    /// The vector number of the interrupt being sent.
    pub fn set_vector(&mut self, vector: u8) {
        self.0 = (self.0 & !0xFF) | (vector as u64);
    }

    pub fn delivery_mode(&self) -> ICRDeliveryMode {
        match (self.0 >> 8) & 0b111 {
            0b000 => ICRDeliveryMode::Fixed,
            0b001 => ICRDeliveryMode::LowestPriority,
            0b010 => ICRDeliveryMode::SMI,
            0b100 => ICRDeliveryMode::NMI,
            0b101 => ICRDeliveryMode::INIT,
            0b110 => ICRDeliveryMode::StartUp,
            _ => unimplemented!("reserved"),
        }
    }
    pub fn set_delivery_mode(&mut self, mode: ICRDeliveryMode) {
        self.0 &= !(0b111 << 8);
        self.0 |= (mode as u64) << 8;
    }
    /// (see Section 11.6.2, "Determining IPI Destination")
    pub fn destination_mode(&self) -> ICRDestinationMode {
        match (self.0 >> 11) & 1 != 0 {
            false => ICRDestinationMode::Physical,
            true => ICRDestinationMode::Logical,
        }
    }
    /// (see Section 11.6.2, "Determining IPI Destination")
    pub fn set_destination_mode(&mut self, mode: ICRDestinationMode) {
        self.0 &= !(1 << 11);
        self.0 |= (mode as u64) << 11;
    }
    /// Only available in xAPIC (not x2APIC).
    pub fn delivery_status(&self) -> ICRDeliveryStatus {
        match (self.0 >> 12) & 1 != 0 {
            false => ICRDeliveryStatus::Idle,
            true => ICRDeliveryStatus::SendPending,
        }
    }
    pub fn level(&self) -> ICRLevel {
        match (self.0 >> 14) & 1 != 0 {
            false => ICRLevel::DeAssert,
            true => ICRLevel::Assert,
        }
    }
    pub fn set_level(&mut self, level: ICRLevel) {
        self.0 &= !(1 << 14);
        self.0 |= (level as u64) << 14;
    }
    /// Selects the trigger mode when using the INIT level de-assert delivery mode: edge (0) or level
    /// (1). It is ignored for all other delivery modes. (This flag has no meaning in Pentium 4 and Intel
    /// Xeon processors, and will always be issued as a 0.)
    pub fn trigger_mode(&self) -> TriggerMode {
        match (self.0 >> 15) & 1 != 0 {
            false => TriggerMode::Edge,
            true => TriggerMode::Level,
        }
    }
    /// Selects the trigger mode when using the INIT level de-assert delivery mode: edge (0) or level
    /// (1). It is ignored for all other delivery modes. (This flag has no meaning in Pentium 4 and Intel
    /// Xeon processors, and will always be issued as a 0.)
    pub fn set_trigger_mode(&mut self, mode: TriggerMode) {
        self.0 &= !(1 << 15);
        self.0 |= (mode as u64) << 15;
    }
    pub fn destination_shorthand(&self) -> ICRDestinationShorthand {
        match (self.0 >> 18) & 0b11 {
            0b00 => ICRDestinationShorthand::NoShorthand,
            0b01 => ICRDestinationShorthand::Myself,
            0b10 => ICRDestinationShorthand::AllIncludingSelf,
            0b11 => ICRDestinationShorthand::AllExcludingSelf,
            _ => unreachable!(),
        }
    }
    pub fn set_destination_shorthand(&mut self, shorthand: ICRDestinationShorthand) {
        self.0 &= !(0b11 << 18);
        self.0 |= (shorthand as u64) << 18;
    }
    pub fn destination(&self) -> u32 {
        (self.0 >> 32) as _
    }
    pub fn set_destination(&mut self, destination: u32) {
        self.0 &= !(0xFFFF_FFFF << 32);
        self.0 |= (destination as u64) << 32;
    }
}
