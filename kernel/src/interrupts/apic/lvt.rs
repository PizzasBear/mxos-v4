use core::fmt;

use super::TriggerMode;

/// The local vector table (LVT) allows software to specify the manner in which the local
/// interrupts are delivered to the processor core.
///
/// # Fields:
/// * `vector`: Interrupt vector number
/// * `delivery_mode`: Specifies the type of interrupt to be sent to the processor.
/// * `delivery_status`: Indicates the interrupt delivery status. Read-only.
/// * `interrupt_input_pin_polarity`: Specifies the polarity of the corresponding interrupt pin:
///   (false) active high or (true) active low.
/// * `remote_irr`: For fixed mode, level-triggered interrupts; this flag is set when the local APIC
///   accepts the interrupt for servicing and is reset when an EOI command is received from the
///   processor. The meaning of this flag is undefined for edge-triggered interrupts and other
///   delivery modes.
/// * `trigger_mode`: Specifies the trigger mode for the corresponding interrupt pin: (false)
///   edge-triggered or (true) level-triggered.
/// * `mask`: Interrupt mask: (false) enables reception of the interrupt and (true) inhibits
///   reception of the interrupt.
/// * `timer_mode`: Available only on Timer.
pub struct LocalVectorTable(pub u32);

impl LocalVectorTable {
    /// Creates a new masked LocalVectorTable.
    pub fn new() -> Self {
        let mut slf = LocalVectorTable(0);
        slf.set_mask(true);
        slf
    }

    pub fn bits(&self) -> u32 {
        self.0
    }
    pub fn from_bits_retain(bits: u32) -> Self {
        Self(bits)
    }
}

/// Specifies the type of interrupt to be sent to the processor.
/// Some delivery modes will only operate as intended when used in conjunction with a specific
/// trigger mode.
#[derive(Debug)]
#[repr(u8)]
pub enum LVTDeliveryMode {
    /// Delivers the interrupt specified in the vector field
    Fixed = 0b000,
    /// Delivers an SMI interrupt to the processor core through
    /// the processor's local SMI signal path. When using this delivery mode,
    /// the vector field should be set to 00H for future compatibility.
    SMI = 0b010,
    /// Delivers an NMI interrupt to the processor. The vector information is ignored.
    NMI = 0b100,
    /// Causes the processor to respond to the interrupt as if the interrupt originated
    /// in an externally connected (8259A-compatible) interrupt controller. A special
    /// INTA bus cycle corresponding to ExtINT, is routed to the external controller.
    /// The external controller is expected to supply the vector information. The APIC
    /// architecture supports only one ExtINT source in a system, usually contained in
    /// the compatibility bridge. Only one processor in the system should have an LVT
    /// entry configured to use the ExtINT delivery mode. Not supported for the LVT CMCI
    /// register, the LVT thermal monitor register, or the LVT performance counter register.
    ExtINT = 0b111,
    /// Delivers an INIT request to the processor core, which causes the processor
    /// to perform an INIT. When using this delivery mode, the vector field should
    /// be set to 00H for future compatibility. Not supported for the LVT CMCI
    /// register, the LVT thermal monitor register, or the LVT performance counter
    /// register.
    INIT = 0b101,
}

/// Indicates the interrupt delivery status. Read-only.
#[derive(Debug)]
pub enum LVTDeliveryStatus {
    /// There is currently no activity for this interrupt source,
    /// or the previous interrupt from this source was delivered to the processor core and accepted.
    Idle = 0,
    /// Indicates that an interrupt from this source has been delivered to the processor core but
    /// has not yet been accepted (see Section 11.5.5, "Local Interrupt Acceptance").
    SendPending = 1,
}

#[derive(Debug)]
pub enum TimerMode {
    /// One-shot mode using a count-down value.
    OneShot = 0b00,
    /// Periodic mode reloading a count-down value.
    Periodic = 0b01,
    /// TSC-Deadline mode using absolute target value in `IA32_TSC_DEADLINE` MSR (see Section 11.5.4.1)
    TSCDeadline = 0b10,
}

impl LocalVectorTable {
    /// Interrupt vector number
    pub fn vector(&self) -> u8 {
        (self.0 & 0xFF) as u8
    }
    /// Interrupt vector number
    pub fn set_vector(&mut self, vector: u8) {
        self.0 = (self.0 & !0xFF) | (vector as u32);
    }
    /// Available on: CMCI, LINT0, LINT1, Performance Counter, and Thermal Sensor.
    pub fn delivery_mode(&self) -> LVTDeliveryMode {
        match (self.0 >> 8) & 0x7 {
            0b000 => LVTDeliveryMode::Fixed,
            0b010 => LVTDeliveryMode::SMI,
            0b100 => LVTDeliveryMode::NMI,
            0b111 => LVTDeliveryMode::ExtINT,
            0b101 => LVTDeliveryMode::INIT,
            _ => unimplemented!("reserved"),
        }
    }
    /// Available on: CMCI, LINT0, LINT1, Performance Counter, and Thermal Sensor.
    pub fn set_delivery_mode(&mut self, mode: LVTDeliveryMode) {
        self.0 &= !(0x7 << 8);
        self.0 |= (mode as u32) << 8;
    }
    pub fn delivery_status(&self) -> LVTDeliveryStatus {
        match (self.0 >> 12) & 1 != 0 {
            false => LVTDeliveryStatus::Idle,
            true => LVTDeliveryStatus::SendPending,
        }
    }
    /// Available on: LINT0, LINT1.
    ///
    /// Specifies the polarity of the corresponding interrupt pin: (false) active high or (true) active low.
    pub fn interrupt_input_pin_polarity(&self) -> bool {
        (self.0 >> 13) & 1 != 0
    }
    /// Available on: LINT0, LINT1.
    ///
    /// Specifies the polarity of the corresponding interrupt pin: (false) active high or (true) active low.
    pub fn set_interrupt_input_pin_polarity(&mut self, polarity: bool) {
        self.0 &= !(1 << 13);
        self.0 |= (polarity as u32) << 13;
    }
    /// Available on: LINT0, LINT1.
    ///
    /// For fixed mode, level-triggered interrupts; this flag is set when the local APIC accepts the
    /// interrupt for servicing and is reset when an EOI command is received from the processor. The
    /// meaning of this flag is undefined for edge-triggered interrupts and other delivery modes.
    pub fn remote_irr(&self) -> bool {
        (self.0 >> 14) & 1 != 0
    }

    /// Available on: LINT0, LINT1.
    pub fn trigger_mode(&self) -> TriggerMode {
        match (self.0 >> 15) & 1 != 0 {
            false => TriggerMode::Edge,
            true => TriggerMode::Level,
        }
    }
    /// Available on: LINT0, LINT1.
    pub fn set_trigger_mode(&mut self, mode: bool) {
        self.0 &= !(1 << 15);
        self.0 |= (mode as u32) << 15;
    }

    /// Interrupt mask: (false) enables reception of the interrupt and (true) inhibits reception of the interrupt.
    /// When the local APIC handles a performance-monitoring counters interrupt, it automatically sets the mask flag
    /// in the LVT performance counter register. This flag is set to `true` on reset. It can be cleared only by software.
    pub fn mask(&self) -> bool {
        (self.0 >> 16) & 1 != 0
    }
    /// Interrupt mask: (false) enables reception of the interrupt and (true) inhibits reception of the interrupt.
    /// When the local APIC handles a performance-monitoring counters interrupt, it automatically sets the mask flag
    /// in the LVT performance counter register. This flag is set to `true` on reset. It can be cleared only by software.
    pub fn set_mask(&mut self, mask: bool) {
        self.0 &= !(1 << 16);
        self.0 |= (mask as u32) << 16;
    }

    /// Available only on Timer.
    pub fn timer_mode(&self) -> TimerMode {
        match (self.0 >> 17) & 0b11 {
            0b00 => TimerMode::OneShot,
            0b01 => TimerMode::Periodic,
            0b10 => TimerMode::TSCDeadline,
            _ => unimplemented!("reserved"),
        }
    }
    /// Available only on Timer.
    pub fn set_timer_mode(&mut self, mode: TimerMode) {
        self.0 &= !(0b11 << 17);
        self.0 |= (mode as u32) << 17;
    }
}

impl fmt::Debug for LocalVectorTable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LocalVectorTable")
            .field("vector", &self.vector())
            .field("delivery_mode", &self.delivery_mode())
            .field("delivery_status", &self.delivery_status())
            .field(
                "interrupt_input_pin_polarity",
                &self.interrupt_input_pin_polarity(),
            )
            .field("remote_irr", &self.remote_irr())
            .field("trigger_mode", &self.trigger_mode())
            .field("mask", &self.mask())
            .field("timer_mode", &self.timer_mode())
            .finish()
    }
}
